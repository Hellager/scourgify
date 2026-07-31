use std::collections::BTreeMap;

use chrono::{DateTime, Duration, TimeZone, Utc};

use super::{QaActivityRange, QaActivityTrendPoint, QaItemMetadata};

pub(crate) fn activity_trend<Tz>(
    recent_entries: impl IntoIterator<Item = QaItemMetadata>,
    frequent_entries: impl IntoIterator<Item = QaItemMetadata>,
    range: QaActivityRange,
    now: DateTime<Tz>,
) -> Vec<QaActivityTrendPoint>
where
    Tz: TimeZone,
{
    let start_date = (now.clone() - Duration::days(range.day_span() as i64)).date_naive();
    let end_date = now.date_naive();
    let cutoff_ms = (now.timestamp_millis() - Duration::hours(24).num_milliseconds()) as i128;
    let now_ms = i128::from(now.timestamp_millis());
    let timezone = now.timezone();
    let mut counts = BTreeMap::new();
    let mut date = start_date;

    while date <= end_date {
        counts.insert(date, (0_u64, 0_u64));
        date = match date.succ_opt() {
            Some(next) => next,
            None => break,
        };
    }

    count_entries(
        &mut counts,
        recent_entries,
        range,
        cutoff_ms,
        now_ms,
        &timezone,
        true,
    );
    count_entries(
        &mut counts,
        frequent_entries,
        range,
        cutoff_ms,
        now_ms,
        &timezone,
        false,
    );

    counts
        .into_iter()
        .map(
            |(date, (recent_files, frequent_folders))| QaActivityTrendPoint {
                date: date.format("%Y-%m-%d").to_string(),
                recent_files,
                frequent_folders,
            },
        )
        .collect()
}

fn count_entries<Tz>(
    counts: &mut BTreeMap<chrono::NaiveDate, (u64, u64)>,
    entries: impl IntoIterator<Item = QaItemMetadata>,
    range: QaActivityRange,
    cutoff_ms: i128,
    now_ms: i128,
    timezone: &Tz,
    recent: bool,
) where
    Tz: TimeZone,
{
    for entry in entries {
        let Some(timestamp) = entry.last_interaction_at else {
            continue;
        };
        let timestamp = i128::from(timestamp);
        if timestamp > now_ms || (range == QaActivityRange::Last24Hours && timestamp < cutoff_ms) {
            continue;
        }
        let Ok(timestamp) = i64::try_from(timestamp) else {
            continue;
        };
        let Some(interaction) = DateTime::<Utc>::from_timestamp_millis(timestamp) else {
            continue;
        };
        let interaction_date = interaction.with_timezone(timezone).date_naive();
        if let Some((recent_files, frequent_folders)) = counts.get_mut(&interaction_date) {
            if recent {
                *recent_files += 1;
            } else {
                *frequent_folders += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};
    use serde_json::json;

    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 31, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn metadata_at(timestamp: Option<DateTime<Utc>>) -> QaItemMetadata {
        QaItemMetadata {
            path: String::new(),
            name: String::new(),
            last_interaction_at: timestamp.map(|value| value.timestamp_millis() as u64),
            access_count: 0,
            score: None,
            recent_rank: 0,
            mru_position: 0,
            pinned: false,
            pin_order: None,
            warning_count: 0,
        }
    }

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn activity_range_deserializes_supported_values() {
        let cases = [
            ("24h", QaActivityRange::Last24Hours),
            ("7d", QaActivityRange::Last7Days),
            ("30d", QaActivityRange::Last30Days),
        ];

        for (value, expected) in cases {
            assert_eq!(
                serde_json::from_value::<QaActivityRange>(json!(value)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn activity_range_rejects_unsupported_value() {
        assert!(serde_json::from_value::<QaActivityRange>(json!("14d")).is_err());
    }

    #[test]
    fn activity_trend_returns_expected_bucket_count_for_each_range() {
        let cases = [
            (QaActivityRange::Last24Hours, 2),
            (QaActivityRange::Last7Days, 7),
            (QaActivityRange::Last30Days, 30),
        ];

        for (range, expected_count) in cases {
            let result = activity_trend(Vec::new(), Vec::new(), range, fixed_now());

            assert_eq!(result.len(), expected_count);
            assert!(result
                .iter()
                .all(|point| point.recent_files == 0 && point.frequent_folders == 0));
            assert_eq!(result.last().unwrap().date, "2026-07-31");
        }
    }

    #[test]
    fn activity_trend_point_serializes_only_split_series() {
        let point = QaActivityTrendPoint {
            date: "2026-07-31".to_string(),
            recent_files: 3,
            frequent_folders: 2,
        };

        assert_eq!(
            serde_json::to_value(point).unwrap(),
            json!({
                "date": "2026-07-31",
                "recent_files": 3,
                "frequent_folders": 2
            })
        );
    }

    #[test]
    fn activity_trend_filters_missing_future_and_out_of_range_timestamps() {
        let mut invalid = metadata_at(None);
        invalid.last_interaction_at = Some(u64::MAX);
        let recent_entries = vec![
            metadata_at(Some(at(2026, 7, 30, 12, 0))),
            metadata_at(Some(at(2026, 7, 30, 11, 59))),
            metadata_at(Some(at(2026, 7, 31, 13, 0))),
            metadata_at(None),
            invalid,
        ];
        let frequent_entries = vec![
            metadata_at(Some(fixed_now())),
            metadata_at(Some(at(2026, 7, 29, 12, 0))),
            metadata_at(Some(at(2026, 7, 31, 13, 0))),
            metadata_at(None),
        ];

        let result = activity_trend(
            recent_entries,
            frequent_entries,
            QaActivityRange::Last24Hours,
            fixed_now(),
        );

        assert_eq!(
            result,
            vec![
                QaActivityTrendPoint {
                    date: "2026-07-30".to_string(),
                    recent_files: 1,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-31".to_string(),
                    recent_files: 0,
                    frequent_folders: 1,
                },
            ]
        );
    }

    #[test]
    fn activity_trend_filters_entries_before_multiday_range() {
        let recent_entries = vec![
            metadata_at(Some(at(2026, 7, 25, 0, 1))),
            metadata_at(Some(at(2026, 7, 24, 23, 59))),
        ];
        let frequent_entries = vec![
            metadata_at(Some(at(2026, 7, 31, 1, 0))),
            metadata_at(Some(at(2026, 7, 24, 23, 58))),
        ];

        let result = activity_trend(
            recent_entries,
            frequent_entries,
            QaActivityRange::Last7Days,
            fixed_now(),
        );

        assert_eq!(result.first().unwrap().date, "2026-07-25");
        assert_eq!(result.first().unwrap().recent_files, 1);
        assert_eq!(result.first().unwrap().frequent_folders, 0);
        assert_eq!(
            result.iter().map(|point| point.recent_files).sum::<u64>(),
            1
        );
        assert_eq!(
            result
                .iter()
                .map(|point| point.frequent_folders)
                .sum::<u64>(),
            1
        );
    }

    #[test]
    fn activity_trend_groups_timestamps_by_the_requested_timezone() {
        let timezone = FixedOffset::east_opt(8 * 60 * 60).unwrap();
        let now = timezone
            .with_ymd_and_hms(2026, 7, 31, 12, 0, 0)
            .single()
            .unwrap();
        let recent_entries = vec![metadata_at(Some(at(2026, 7, 30, 16, 30)))];
        let frequent_entries = vec![metadata_at(Some(at(2026, 7, 30, 15, 30)))];

        let result = activity_trend(
            recent_entries,
            frequent_entries,
            QaActivityRange::Last24Hours,
            now,
        );

        assert_eq!(result[0].recent_files, 0);
        assert_eq!(result[0].frequent_folders, 1);
        assert_eq!(result[1].date, "2026-07-31");
        assert_eq!(result[1].recent_files, 1);
        assert_eq!(result[1].frequent_folders, 0);
    }

    #[test]
    fn activity_trend_keeps_recent_files_and_frequent_folders_separate() {
        let recent_entries = vec![
            metadata_at(Some(at(2026, 7, 25, 8, 0))),
            metadata_at(Some(at(2026, 7, 28, 9, 0))),
            metadata_at(Some(at(2026, 7, 28, 18, 0))),
        ];
        let frequent_entries = vec![
            metadata_at(Some(at(2026, 7, 28, 10, 0))),
            metadata_at(Some(at(2026, 7, 31, 11, 0))),
            metadata_at(Some(at(2026, 7, 31, 11, 30))),
        ];

        let result = activity_trend(
            recent_entries,
            frequent_entries,
            QaActivityRange::Last7Days,
            fixed_now(),
        );

        assert_eq!(
            result,
            vec![
                QaActivityTrendPoint {
                    date: "2026-07-25".to_string(),
                    recent_files: 1,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-26".to_string(),
                    recent_files: 0,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-27".to_string(),
                    recent_files: 0,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-28".to_string(),
                    recent_files: 2,
                    frequent_folders: 1,
                },
                QaActivityTrendPoint {
                    date: "2026-07-29".to_string(),
                    recent_files: 0,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-30".to_string(),
                    recent_files: 0,
                    frequent_folders: 0,
                },
                QaActivityTrendPoint {
                    date: "2026-07-31".to_string(),
                    recent_files: 0,
                    frequent_folders: 2,
                },
            ]
        );
    }
}
