use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Local, LocalResult, NaiveTime, TimeZone, Utc};
use serde::Serialize;
use std::{
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    cleanup::{self, AutoCleanError, AutoCleanResult, AutoCleanState},
    config::{self, AutoCleanPolicy, Config},
    db::DbState,
    error::report_background_error,
    privacy::PrivacyManager,
    quick_access::QuickAccessCache,
};

pub const AUTO_CLEAN_FINISHED_EVENT: &str = "auto-clean-finished";

const ERROR_RETRY_DELAY: Duration = Duration::from_secs(60);

pub struct AutoCleanScheduler {
    sender: Sender<SchedulerMessage>,
}

pub struct AutoCleanMonitor {
    sender: Sender<()>,
}

impl AutoCleanScheduler {
    pub fn start<R: Runtime>(app: AppHandle<R>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("scourgify-auto-clean".to_string())
            .spawn(move || run_worker(app, receiver))
            .context("failed to start auto-clean scheduler")?;
        Ok(Self { sender })
    }

    pub fn reschedule(&self) -> Result<()> {
        self.sender
            .send(SchedulerMessage::Reschedule)
            .context("auto-clean scheduler is unavailable")
    }
}

impl AutoCleanMonitor {
    pub fn start<R: Runtime>(app: AppHandle<R>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("scourgify-auto-clean-monitor".to_string())
            .spawn(move || run_monitor_worker(app, receiver))
            .context("failed to start auto-clean monitor")?;
        Ok(Self { sender })
    }

    pub fn trigger(&self) -> Result<()> {
        self.sender
            .send(())
            .context("auto-clean monitor is unavailable")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AutoCleanFinished {
    pub completed_at: DateTime<Utc>,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub warnings: usize,
    pub section_errors: usize,
    pub history_errors: usize,
}

pub fn run_now<R: Runtime>(app: &AppHandle<R>) -> Result<AutoCleanResult> {
    let config = config_snapshot(app)?;
    let result = execute(app, &config)?;
    finish(app, &config, &result)?;
    Ok(result)
}

fn execute<R: Runtime>(app: &AppHandle<R>, config: &Config) -> Result<AutoCleanResult> {
    let database = app.state::<DbState>();
    let privacy = app.state::<PrivacyManager>();
    let auto_clean = app.state::<AutoCleanState>();
    let result = cleanup::run_auto_clean(
        database.inner(),
        config.history_retention,
        privacy.inner(),
        auto_clean.inner(),
    )?;
    if let Some(cache) = app.try_state::<QuickAccessCache>() {
        cache.refresh_after_write(app, "all");
    }
    Ok(result)
}

fn finish<R: Runtime>(app: &AppHandle<R>, config: &Config, result: &AutoCleanResult) -> Result<()> {
    super::notifier::notify_auto_clean(app, config, result);
    record_completion(app, result)?;
    Ok(())
}

fn run_monitor_worker<R: Runtime>(app: AppHandle<R>, receiver: Receiver<()>) {
    while receiver.recv().is_ok() {
        while receiver.try_recv().is_ok() {}
        loop {
            match run_monitor_once(&app) {
                Ok(()) => break,
                Err(error)
                    if matches!(
                        error.downcast_ref::<AutoCleanError>(),
                        Some(AutoCleanError::AlreadyRunning)
                    ) =>
                {
                    std::thread::sleep(Duration::from_secs(1));
                }
                Err(error) => {
                    let incident_id = report_background_error("monitored_auto_clean", &error);
                    log::warn!("monitored auto-clean skipped incident_id={incident_id}");
                    break;
                }
            }
        }
    }
}

fn run_monitor_once<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let config = config_snapshot(app)?;
    if config.auto_clean != AutoCleanPolicy::Monitor {
        return Ok(());
    }
    let result = execute(app, &config)?;
    if result.total > 0 || result.has_issues() {
        finish(app, &config, &result)?;
    }
    Ok(())
}

fn run_worker<R: Runtime>(app: AppHandle<R>, receiver: Receiver<SchedulerMessage>) {
    let mut last_interval_attempt = None;

    loop {
        let config = match config_snapshot(&app) {
            Ok(config) => config,
            Err(error) => {
                report_background_error("scheduler_read_config", &error);
                match wait_for_next(&receiver, Some(ERROR_RETRY_DELAY)) {
                    WaitOutcome::Run | WaitOutcome::Reschedule => continue,
                    WaitOutcome::Stop => return,
                }
            }
        };
        let delay = match next_delay(
            &config.auto_clean,
            config.auto_clean_last_run,
            last_interval_attempt,
            Utc::now(),
        ) {
            Ok(delay) => delay,
            Err(error) => {
                report_background_error("scheduler_calculate_next_run", &error);
                match wait_for_next(&receiver, Some(ERROR_RETRY_DELAY)) {
                    WaitOutcome::Run | WaitOutcome::Reschedule => continue,
                    WaitOutcome::Stop => return,
                }
            }
        };

        match wait_for_next(&receiver, delay) {
            WaitOutcome::Reschedule => continue,
            WaitOutcome::Stop => return,
            WaitOutcome::Run => {}
        }

        let current_schedule = match config_snapshot(&app) {
            Ok(config) => config.auto_clean,
            Err(error) => {
                report_background_error("scheduler_verify_config", &error);
                continue;
            }
        };
        if current_schedule != config.auto_clean {
            continue;
        }
        match config.auto_clean {
            AutoCleanPolicy::EveryHours { .. } => last_interval_attempt = Some(Utc::now()),
            AutoCleanPolicy::Disabled
            | AutoCleanPolicy::Monitor
            | AutoCleanPolicy::DailyAt { .. } => {}
        }

        if let Err(error) = run_now(&app) {
            let incident_id = report_background_error("scheduled_auto_clean", &error);
            log::warn!("scheduled auto-clean skipped incident_id={incident_id}");
        }
    }
}

fn next_delay(
    schedule: &AutoCleanPolicy,
    last_run: Option<DateTime<Utc>>,
    last_interval_attempt: Option<DateTime<Utc>>,
    now_utc: DateTime<Utc>,
) -> Result<Option<Duration>> {
    match schedule {
        AutoCleanPolicy::Disabled | AutoCleanPolicy::Monitor => Ok(None),
        AutoCleanPolicy::EveryHours { hours } => {
            let next = next_interval_run(now_utc, last_run, last_interval_attempt, *hours);
            Ok(Some(duration_until(now_utc, next)))
        }
        AutoCleanPolicy::DailyAt { hour, minute } => {
            let local_now = now_utc.with_timezone(&Local);
            let next = next_daily_run(&local_now, *hour, *minute)?;
            Ok(Some(duration_until(now_utc, next)))
        }
    }
}

fn next_interval_run(
    now: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
    last_attempt: Option<DateTime<Utc>>,
    hours: u32,
) -> DateTime<Utc> {
    let Some(anchor) = [last_run, last_attempt].into_iter().flatten().max() else {
        return now + ChronoDuration::hours(i64::from(hours));
    };
    (anchor + ChronoDuration::hours(i64::from(hours))).max(now)
}

fn next_daily_run<Tz: TimeZone>(now: &DateTime<Tz>, hour: u8, minute: u8) -> Result<DateTime<Utc>> {
    let time = NaiveTime::from_hms_opt(u32::from(hour), u32::from(minute), 0)
        .context("invalid daily auto-clean time")?;
    let now_utc = now.with_timezone(&Utc);
    let timezone = now.timezone();
    let mut date = now.date_naive();

    for _ in 0..=366 {
        let local = date.and_time(time);
        let candidates = match timezone.from_local_datetime(&local) {
            LocalResult::Single(value) => vec![value],
            LocalResult::Ambiguous(first, second) => vec![first, second],
            LocalResult::None => Vec::new(),
        };
        if let Some(next) = candidates
            .into_iter()
            .map(|value| value.with_timezone(&Utc))
            .filter(|value| *value > now_utc)
            .min()
        {
            return Ok(next);
        }
        date = date
            .succ_opt()
            .context("daily auto-clean date is out of range")?;
    }

    anyhow::bail!("no valid daily auto-clean time found")
}

fn duration_until(now: DateTime<Utc>, next: DateTime<Utc>) -> Duration {
    next.signed_duration_since(now)
        .to_std()
        .unwrap_or(Duration::ZERO)
}

fn wait_for_next(receiver: &Receiver<SchedulerMessage>, delay: Option<Duration>) -> WaitOutcome {
    match delay {
        Some(delay) => match receiver.recv_timeout(delay) {
            Ok(SchedulerMessage::Reschedule) => WaitOutcome::Reschedule,
            Err(RecvTimeoutError::Timeout) => WaitOutcome::Run,
            Err(RecvTimeoutError::Disconnected) => WaitOutcome::Stop,
        },
        None => match receiver.recv() {
            Ok(SchedulerMessage::Reschedule) => WaitOutcome::Reschedule,
            Err(_) => WaitOutcome::Stop,
        },
    }
}

fn record_completion<R: Runtime>(app: &AppHandle<R>, result: &AutoCleanResult) -> Result<()> {
    let completed_at = Utc::now();
    {
        let state = app.state::<Mutex<Config>>();
        let mut current = state
            .lock()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut next = current.clone();
        next.auto_clean_last_run = Some(completed_at);
        config::save(app, &next)?;
        *current = next;
    }

    let event = AutoCleanFinished {
        completed_at,
        total: result.total,
        succeeded: result.succeeded,
        failed: result.failed,
        warnings: result.warnings,
        section_errors: result.section_errors,
        history_errors: result.history_errors,
    };
    if let Err(error) = app.emit(AUTO_CLEAN_FINISHED_EVENT, event) {
        log::warn!("failed to emit auto-clean completion: {error}");
    }
    if let Some(scheduler) = app.try_state::<AutoCleanScheduler>() {
        if let Err(error) = scheduler.reschedule() {
            log::warn!("failed to reschedule auto-clean after completion: {error:#}");
        }
    }
    Ok(())
}

fn config_snapshot<R: Runtime>(app: &AppHandle<R>) -> Result<Config> {
    app.state::<Mutex<Config>>()
        .lock()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .map(|config| config.clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchedulerMessage {
    Reschedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitOutcome {
    Run,
    Reschedule,
    Stop,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn interval_uses_last_run_or_attempt_and_catches_up_once() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap();

        assert_eq!(
            next_interval_run(now, None, None, 6),
            now + ChronoDuration::hours(6)
        );
        assert_eq!(
            next_interval_run(now, Some(now - ChronoDuration::hours(8)), None, 6),
            now
        );
        assert_eq!(
            next_interval_run(
                now,
                Some(now - ChronoDuration::hours(8)),
                Some(now - ChronoDuration::hours(1)),
                6,
            ),
            now + ChronoDuration::hours(5)
        );
    }

    #[test]
    fn daily_schedule_uses_the_next_local_occurrence() {
        let timezone = FixedOffset::east_opt(8 * 60 * 60).unwrap();
        let before = timezone.with_ymd_and_hms(2026, 7, 13, 7, 30, 0).unwrap();
        let after = timezone.with_ymd_and_hms(2026, 7, 13, 9, 30, 0).unwrap();

        assert_eq!(
            next_daily_run(&before, 8, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 13, 0, 0, 0).unwrap()
        );
        assert_eq!(
            next_daily_run(&after, 8, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 14, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn disabled_and_monitor_policies_do_not_schedule_runs() {
        let now_utc = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap();

        assert_eq!(
            next_delay(&AutoCleanPolicy::Disabled, None, None, now_utc).unwrap(),
            None
        );
        assert_eq!(
            next_delay(&AutoCleanPolicy::Monitor, None, None, now_utc).unwrap(),
            None
        );
    }

    #[test]
    fn wake_message_interrupts_scheduled_wait() {
        let (sender, receiver) = mpsc::channel();
        sender.send(SchedulerMessage::Reschedule).unwrap();

        assert_eq!(
            wait_for_next(&receiver, Some(Duration::from_secs(60))),
            WaitOutcome::Reschedule
        );
    }
}
