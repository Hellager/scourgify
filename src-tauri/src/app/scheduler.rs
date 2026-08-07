use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Local, LocalResult, NaiveTime, TimeZone, Utc};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    backend::QuickAccessBackendState,
    cleanup::{self, AutoCleanError, AutoCleanResult, AutoCleanState},
    config::{self, AutoCleanPolicy, Config},
    db::{history_runs::CleanupTrigger, DbState},
    error::{command_warning, report_background_error, CommandWarning},
    privacy::PrivacyManager,
    quick_access::QuickAccessCache,
};

pub const AUTO_CLEAN_FINISHED_EVENT: &str = "auto-clean-finished";

const ERROR_RETRY_DELAY: Duration = Duration::from_secs(60);

pub struct AutoCleanScheduler {
    sender: Sender<SchedulerMessage>,
    alive: Arc<AtomicBool>,
}

pub struct AutoCleanMonitor {
    sender: Sender<()>,
    alive: Arc<AtomicBool>,
}

struct WorkerAliveGuard(Arc<AtomicBool>);

impl Drop for WorkerAliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl AutoCleanScheduler {
    pub fn start<R: Runtime>(app: AppHandle<R>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        std::thread::Builder::new()
            .name("scourgify-auto-clean".to_string())
            .spawn(move || {
                let _alive_guard = WorkerAliveGuard(worker_alive);
                run_worker(app, receiver);
            })
            .context("failed to start auto-clean scheduler")?;
        Ok(Self { sender, alive })
    }

    pub fn reschedule(&self) -> Result<()> {
        self.sender
            .send(SchedulerMessage::Reschedule)
            .context("auto-clean scheduler is unavailable")
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl AutoCleanMonitor {
    pub fn start<R: Runtime>(app: AppHandle<R>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        std::thread::Builder::new()
            .name("scourgify-auto-clean-monitor".to_string())
            .spawn(move || {
                let _alive_guard = WorkerAliveGuard(worker_alive);
                run_monitor_worker(app, receiver);
            })
            .context("failed to start auto-clean monitor")?;
        Ok(Self { sender, alive })
    }

    pub fn trigger(&self) -> Result<()> {
        self.sender
            .send(())
            .context("auto-clean monitor is unavailable")
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
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
    pub post_process_warnings: Vec<CommandWarning>,
    pub last_run_persisted: bool,
}

pub fn run_now<R: Runtime>(app: &AppHandle<R>) -> Result<AutoCleanResult> {
    let config = config_snapshot(app)?;
    let mut result = execute(app, &config, CleanupTrigger::Manual)?;
    finish(app, &config, &mut result);
    Ok(result)
}

fn execute<R: Runtime>(
    app: &AppHandle<R>,
    config: &Config,
    trigger: CleanupTrigger,
) -> Result<AutoCleanResult> {
    let database = app.state::<DbState>();
    let backend = app.state::<QuickAccessBackendState>();
    let privacy = app.state::<PrivacyManager>();
    let auto_clean = app.state::<AutoCleanState>();
    let result = cleanup::run_auto_clean(
        database.inner(),
        backend.inner(),
        config.history_retention,
        privacy.inner(),
        auto_clean.inner(),
        trigger,
    )?;
    if let Some(cache) = app.try_state::<QuickAccessCache>() {
        cache.refresh_after_write(app, backend.inner(), "all");
    }
    Ok(result)
}

fn finish<R: Runtime>(app: &AppHandle<R>, config: &Config, result: &mut AutoCleanResult) {
    let completed_at = Utc::now();
    match persist_last_run(app, completed_at) {
        Ok(()) => result.last_run_persisted = true,
        Err(error) => record_post_process_warning(
            result,
            "auto_clean_last_run_persist_failed",
            "config",
            "Automatic cleanup completed, but its last-run time could not be saved.",
            error,
        ),
    }
    if let Err(error) = super::notifier::notify_auto_clean(app, config, result) {
        record_post_process_warning(
            result,
            "auto_clean_completion_notification_failed",
            "notification",
            "Automatic cleanup completed, but its system notification could not be sent.",
            error,
        );
    }
    if let Some(scheduler) = app.try_state::<AutoCleanScheduler>() {
        if let Err(error) = scheduler.reschedule() {
            record_post_process_warning(
                result,
                "auto_clean_reschedule_failed",
                "reschedule",
                "Automatic cleanup completed, but its schedule could not be recalculated.",
                error,
            );
        }
    }
    let event = completion_event(completed_at, result);
    if let Err(error) = app.emit(AUTO_CLEAN_FINISHED_EVENT, event) {
        record_post_process_warning(
            result,
            "auto_clean_completion_event_failed",
            "event",
            "Automatic cleanup completed, but its completion event could not be delivered.",
            error,
        );
    }
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
    let mut result = execute(app, &config, CleanupTrigger::Monitor)?;
    if result.total > 0 || result.has_issues() {
        finish(app, &config, &mut result);
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
        let last_run = match config.auto_clean {
            AutoCleanPolicy::EveryHours { .. } => {
                effective_scheduled_last_run(&app, config.auto_clean_last_run)
            }
            AutoCleanPolicy::Disabled
            | AutoCleanPolicy::Monitor
            | AutoCleanPolicy::DailyAt { .. } => config.auto_clean_last_run,
        };
        let delay = match next_delay(
            &config.auto_clean,
            last_run,
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

        if let Err(error) = run_scheduled(&app) {
            let incident_id = report_background_error("scheduled_auto_clean", &error);
            log::warn!("scheduled auto-clean skipped incident_id={incident_id}");
        }
    }
}

fn run_scheduled<R: Runtime>(app: &AppHandle<R>) -> Result<AutoCleanResult> {
    let config = config_snapshot(app)?;
    let mut result = execute(app, &config, CleanupTrigger::Scheduled)?;
    finish(app, &config, &mut result);
    Ok(result)
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

fn effective_scheduled_last_run<R: Runtime>(
    app: &AppHandle<R>,
    config_last_run: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let database = app.state::<DbState>();
    let history_last_run = database
        .with_connection(|connection| {
            crate::db::history_runs::latest_terminal_auto_clean_completion(
                connection,
                CleanupTrigger::Scheduled,
            )
        })
        .map_err(|error| {
            let incident_id = report_background_error("scheduler_read_last_run", &error);
            log::warn!("using configured auto-clean last-run time incident_id={incident_id}");
        })
        .ok()
        .flatten();
    [config_last_run, history_last_run]
        .into_iter()
        .flatten()
        .max()
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

fn persist_last_run<R: Runtime>(app: &AppHandle<R>, completed_at: DateTime<Utc>) -> Result<()> {
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

    Ok(())
}

fn completion_event(completed_at: DateTime<Utc>, result: &AutoCleanResult) -> AutoCleanFinished {
    AutoCleanFinished {
        completed_at,
        total: result.total,
        succeeded: result.succeeded,
        failed: result.failed,
        warnings: result.warnings,
        section_errors: result.section_errors,
        history_errors: result.history_errors,
        post_process_warnings: result.post_process_warnings.clone(),
        last_run_persisted: result.last_run_persisted,
    }
}

fn record_post_process_warning(
    result: &mut AutoCleanResult,
    code: &'static str,
    step: &'static str,
    message: &'static str,
    error: impl std::fmt::Display,
) {
    result.post_process_warnings.push(command_warning(
        "finish_auto_clean",
        code,
        step,
        message,
        error,
    ));
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

    #[test]
    fn completion_event_preserves_post_process_state() {
        let completed_at = Utc.with_ymd_and_hms(2026, 8, 7, 12, 0, 0).unwrap();
        let result = AutoCleanResult {
            recent: crate::cleanup::AutoCleanSectionResult {
                result: None,
                error: None,
            },
            frequent: crate::cleanup::AutoCleanSectionResult {
                result: None,
                error: None,
            },
            total: 2,
            succeeded: 2,
            failed: 0,
            warnings: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
            post_process_warnings: vec![CommandWarning {
                code: "auto_clean_last_run_persist_failed",
                step: "config",
                message: "completion warning".to_string(),
                incident_id: "incident-1".to_string(),
            }],
            last_run_persisted: false,
        };

        let event = completion_event(completed_at, &result);

        assert_eq!(event.completed_at, completed_at);
        assert_eq!(event.succeeded, 2);
        assert!(!event.last_run_persisted);
        assert_eq!(event.post_process_warnings, result.post_process_warnings);
    }

    #[test]
    fn post_process_failure_preserves_main_cleanup_summary() {
        let mut result = AutoCleanResult {
            recent: crate::cleanup::AutoCleanSectionResult {
                result: None,
                error: None,
            },
            frequent: crate::cleanup::AutoCleanSectionResult {
                result: None,
                error: None,
            },
            total: 3,
            succeeded: 2,
            failed: 1,
            warnings: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
            post_process_warnings: Vec::new(),
            last_run_persisted: false,
        };

        record_post_process_warning(
            &mut result,
            "auto_clean_last_run_persist_failed",
            "config",
            "completion warning",
            "save failed",
        );

        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 1);
        assert_eq!(result.post_process_warnings.len(), 1);
    }
}
