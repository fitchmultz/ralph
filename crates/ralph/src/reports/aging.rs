//! Task aging report implementation.
//!
//! Responsibilities:
//! - Categorize tasks by age into buckets (fresh, warning, stale, rotten, unknown).
//! - Provide shared aging logic for CLI reports and UI indicators.
//!
//! Not handled here:
//! - Output formatting (see shared.rs).
//! - UI color/styling mapping (callers decide how to render buckets).
//!
//! Invariants/assumptions:
//! - Thresholds must satisfy: warning_days < stale_days < rotten_days.
//! - Age is computed as now_utc - anchor_timestamp based on task status.

use anyhow::{Result, bail};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::contracts::{QueueConfig, QueueFile, Task, TaskStatus};
use crate::timeutil;

use super::shared::{ReportFormat, format_duration, print_json};

/// Default aging threshold values (in days).
pub(crate) const DEFAULT_WARNING_DAYS: u32 = 7;
pub(crate) const DEFAULT_STALE_DAYS: u32 = 14;
pub(crate) const DEFAULT_ROTTEN_DAYS: u32 = 30;

/// Validated aging thresholds with guaranteed ordering.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct AgingThresholds {
    pub warning_days: u32,
    pub stale_days: u32,
    pub rotten_days: u32,
}

impl AgingThresholds {
    /// Create thresholds from queue config, validating ordering.
    pub(crate) fn from_queue_config(cfg: &QueueConfig) -> Result<Self> {
        let t = cfg.aging_thresholds.clone();
        let warning = t
            .as_ref()
            .and_then(|t| t.warning_days)
            .unwrap_or(DEFAULT_WARNING_DAYS);
        let stale = t
            .as_ref()
            .and_then(|t| t.stale_days)
            .unwrap_or(DEFAULT_STALE_DAYS);
        let rotten = t
            .as_ref()
            .and_then(|t| t.rotten_days)
            .unwrap_or(DEFAULT_ROTTEN_DAYS);

        if !(warning < stale && stale < rotten) {
            bail!(
                "Invalid queue.aging_thresholds ordering: require warning_days < stale_days < rotten_days (got warning_days={}, stale_days={}, rotten_days={})",
                warning,
                stale,
                rotten
            );
        }
        Ok(Self {
            warning_days: warning,
            stale_days: stale,
            rotten_days: rotten,
        })
    }

    fn warning_dur(self) -> Duration {
        Duration::days(self.warning_days as i64)
    }
    fn stale_dur(self) -> Duration {
        Duration::days(self.stale_days as i64)
    }
    fn rotten_dur(self) -> Duration {
        Duration::days(self.rotten_days as i64)
    }
}

impl Default for AgingThresholds {
    fn default() -> Self {
        Self {
            warning_days: DEFAULT_WARNING_DAYS,
            stale_days: DEFAULT_STALE_DAYS,
            rotten_days: DEFAULT_ROTTEN_DAYS,
        }
    }
}

/// Age bucket for a task.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgingBucket {
    /// Cannot determine age (missing/invalid timestamp).
    Unknown,
    /// Age <= warning threshold.
    Fresh,
    /// Warning < age <= stale threshold.
    Warning,
    /// Stale < age <= rotten threshold.
    Stale,
    /// Age > rotten threshold.
    Rotten,
}

/// Aging information for a single task.
#[derive(Debug, Clone)]
pub(crate) struct TaskAging {
    pub bucket: AgingBucket,
    /// The computed age duration.
    pub age: Option<Duration>,
}

/// Get the anchor timestamp field name and value for a task based on its status.
///
/// Anchor selection by status:
/// - `draft`, `todo`: `created_at`
/// - `doing`: `started_at` if present, else `created_at`
/// - `done`, `rejected`: `completed_at` if present, else `updated_at`, else `created_at`
fn anchor_for_task(task: &Task) -> Option<(&'static str, &str)> {
    match task.status {
        TaskStatus::Draft | TaskStatus::Todo => {
            task.created_at.as_deref().map(|ts| ("created_at", ts))
        }
        TaskStatus::Doing => task
            .started_at
            .as_deref()
            .map(|ts| ("started_at", ts))
            .or_else(|| task.created_at.as_deref().map(|ts| ("created_at", ts))),
        TaskStatus::Done | TaskStatus::Rejected => task
            .completed_at
            .as_deref()
            .map(|ts| ("completed_at", ts))
            .or_else(|| task.updated_at.as_deref().map(|ts| ("updated_at", ts)))
            .or_else(|| task.created_at.as_deref().map(|ts| ("created_at", ts))),
    }
}

/// Compute the aging bucket for a task.
///
/// Returns `Unknown` if:
/// - The anchor timestamp is missing
/// - The anchor timestamp is invalid RFC3339
/// - The anchor timestamp is in the future relative to `now`
pub(crate) fn compute_task_aging(
    task: &Task,
    thresholds: AgingThresholds,
    now: OffsetDateTime,
) -> TaskAging {
    let Some((_basis, raw)) = anchor_for_task(task) else {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    };

    let Some(anchor) = timeutil::parse_rfc3339_opt(raw) else {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    };

    if anchor > now {
        return TaskAging {
            bucket: AgingBucket::Unknown,
            age: None,
        };
    }

    let age = now - anchor;
    let bucket = if age > thresholds.rotten_dur() {
        AgingBucket::Rotten
    } else if age > thresholds.stale_dur() {
        AgingBucket::Stale
    } else if age > thresholds.warning_dur() {
        AgingBucket::Warning
    } else {
        AgingBucket::Fresh
    };

    TaskAging {
        bucket,
        age: Some(age),
    }
}

/// Task entry in the aging report.
#[derive(Debug, Serialize)]
struct AgingTaskEntry {
    id: String,
    title: String,
    status: TaskStatus,
    age_seconds: i64,
    age_human: String,
    basis: String,
    anchor_ts: String,
}

/// Bucket entry in the aging report.
#[derive(Debug, Serialize)]
struct AgingBucketEntry {
    bucket: String,
    count: usize,
    tasks: Vec<AgingTaskEntry>,
}

/// Summary counts for each bucket.
#[derive(Debug, Serialize)]
struct AgingTotals {
    total: usize,
    fresh: usize,
    warning: usize,
    stale: usize,
    rotten: usize,
    unknown: usize,
}

/// Aging thresholds in the report output.
#[derive(Debug, Serialize)]
struct AgingThresholdsOutput {
    warning_days: u32,
    stale_days: u32,
    rotten_days: u32,
}

/// Filters applied to the report.
#[derive(Debug, Serialize)]
struct AgingFilters {
    statuses: Vec<String>,
}

/// Full aging report structure.
#[derive(Debug, Serialize)]
struct AgingReport {
    as_of: String,
    thresholds: AgingThresholdsOutput,
    filters: AgingFilters,
    totals: AgingTotals,
    buckets: Vec<AgingBucketEntry>,
}

fn build_aging_report(
    queue: &QueueFile,
    statuses: &[TaskStatus],
    thresholds: AgingThresholds,
    now: OffsetDateTime,
) -> AgingReport {
    // Filter tasks by status
    let filtered_tasks: Vec<&Task> = queue
        .tasks
        .iter()
        .filter(|t| statuses.contains(&t.status))
        .collect();

    // Compute aging for each task
    let mut bucketed: std::collections::HashMap<AgingBucket, Vec<(AgingBucket, &Task, Duration)>> =
        std::collections::HashMap::new();

    for task in &filtered_tasks {
        let aging = compute_task_aging(task, thresholds, now);
        if let Some(age) = aging.age {
            bucketed
                .entry(aging.bucket)
                .or_default()
                .push((aging.bucket, task, age));
        } else {
            bucketed.entry(aging.bucket).or_default();
        }
    }

    // Sort tasks within each bucket by age descending
    let mut fresh_tasks: Vec<AgingTaskEntry> = bucketed
        .get(&AgingBucket::Fresh)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, task, age)| AgingTaskEntry {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            age_seconds: age.whole_seconds(),
            age_human: format_duration(age),
            basis: "created_at".to_string(),
            anchor_ts: task.created_at.clone().unwrap_or_default(),
        })
        .collect();
    fresh_tasks.sort_by(|a, b| b.age_seconds.cmp(&a.age_seconds));

    let mut warning_tasks: Vec<AgingTaskEntry> = bucketed
        .get(&AgingBucket::Warning)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, task, age)| AgingTaskEntry {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            age_seconds: age.whole_seconds(),
            age_human: format_duration(age),
            basis: "created_at".to_string(),
            anchor_ts: task.created_at.clone().unwrap_or_default(),
        })
        .collect();
    warning_tasks.sort_by(|a, b| b.age_seconds.cmp(&a.age_seconds));

    let mut stale_tasks: Vec<AgingTaskEntry> = bucketed
        .get(&AgingBucket::Stale)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, task, age)| AgingTaskEntry {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            age_seconds: age.whole_seconds(),
            age_human: format_duration(age),
            basis: "created_at".to_string(),
            anchor_ts: task.created_at.clone().unwrap_or_default(),
        })
        .collect();
    stale_tasks.sort_by(|a, b| b.age_seconds.cmp(&a.age_seconds));

    let mut rotten_tasks: Vec<AgingTaskEntry> = bucketed
        .get(&AgingBucket::Rotten)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, task, age)| AgingTaskEntry {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
            age_seconds: age.whole_seconds(),
            age_human: format_duration(age),
            basis: "created_at".to_string(),
            anchor_ts: task.created_at.clone().unwrap_or_default(),
        })
        .collect();
    rotten_tasks.sort_by(|a, b| b.age_seconds.cmp(&a.age_seconds));

    let unknown_count = bucketed
        .get(&AgingBucket::Unknown)
        .map(|v| v.len())
        .unwrap_or(0);

    let totals = AgingTotals {
        total: filtered_tasks.len(),
        fresh: fresh_tasks.len(),
        warning: warning_tasks.len(),
        stale: stale_tasks.len(),
        rotten: rotten_tasks.len(),
        unknown: unknown_count,
    };

    // Build buckets in severity order: rotten, stale, warning, fresh, unknown
    // For fresh, we omit the task list to keep output small
    let mut buckets = Vec::new();

    if !rotten_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "rotten".to_string(),
            count: rotten_tasks.len(),
            tasks: rotten_tasks,
        });
    }

    if !stale_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "stale".to_string(),
            count: stale_tasks.len(),
            tasks: stale_tasks,
        });
    }

    if !warning_tasks.is_empty() {
        buckets.push(AgingBucketEntry {
            bucket: "warning".to_string(),
            count: warning_tasks.len(),
            tasks: warning_tasks,
        });
    }

    // Fresh bucket always shown but without task list
    buckets.push(AgingBucketEntry {
        bucket: "fresh".to_string(),
        count: fresh_tasks.len(),
        tasks: Vec::new(), // Omit task list for fresh
    });

    if unknown_count > 0 {
        buckets.push(AgingBucketEntry {
            bucket: "unknown".to_string(),
            count: unknown_count,
            tasks: Vec::new(),
        });
    }

    AgingReport {
        as_of: timeutil::format_rfc3339(now).unwrap_or_else(|_| now.to_string()),
        thresholds: AgingThresholdsOutput {
            warning_days: thresholds.warning_days,
            stale_days: thresholds.stale_days,
            rotten_days: thresholds.rotten_days,
        },
        filters: AgingFilters {
            statuses: statuses.iter().map(|s| s.as_str().to_string()).collect(),
        },
        totals,
        buckets,
    }
}

/// Print task aging report showing tasks grouped by age buckets.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `statuses` - Statuses to include in the report
/// * `thresholds` - Aging threshold configuration
/// * `format` - Output format (text or json)
pub(crate) fn print_aging(
    queue: &QueueFile,
    statuses: &[TaskStatus],
    thresholds: AgingThresholds,
    format: ReportFormat,
) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    let report = build_aging_report(queue, statuses, thresholds, now);

    match format {
        ReportFormat::Json => {
            print_json(&report)?;
        }
        ReportFormat::Text => {
            println!("Task Aging Report");
            println!("=================");
            println!();

            println!(
                "Thresholds: warning > {}d, stale > {}d, rotten > {}d",
                report.thresholds.warning_days,
                report.thresholds.stale_days,
                report.thresholds.rotten_days
            );
            println!(
                "Filtering by status: {}",
                report.filters.statuses.join(", ")
            );
            println!();

            println!("Totals ({} tasks)", report.totals.total);
            println!("  Fresh:    {}", report.totals.fresh);
            if report.totals.warning > 0 {
                println!("  Warning:  {}", report.totals.warning);
            }
            if report.totals.stale > 0 {
                println!("  Stale:    {}", report.totals.stale);
            }
            if report.totals.rotten > 0 {
                println!("  Rotten:   {}", report.totals.rotten);
            }
            if report.totals.unknown > 0 {
                println!("  Unknown:  {}", report.totals.unknown);
            }
            println!();

            // Show tasks for non-fresh buckets
            for bucket in &report.buckets {
                if bucket.bucket == "fresh" {
                    continue;
                }
                if bucket.tasks.is_empty() {
                    continue;
                }

                let title = match bucket.bucket.as_str() {
                    "rotten" => "🟥 Rotten Tasks",
                    "stale" => "🟧 Stale Tasks",
                    "warning" => "🟨 Warning Tasks",
                    "unknown" => "❓ Unknown Age",
                    _ => &bucket.bucket,
                };
                println!("{}", title);
                println!("{}", "-".repeat(title.len()));

                for task in &bucket.tasks {
                    println!(
                        "  {}  {:10}  {:12}  {}",
                        task.id,
                        task.status.as_str(),
                        task.age_human,
                        task.title
                    );
                }
                println!();
            }

            if report.totals.total == 0 {
                println!("No tasks match the selected filters.");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn task_with_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: crate::contracts::TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    fn fixed_now() -> OffsetDateTime {
        // Fixed point in time for deterministic tests
        OffsetDateTime::from_unix_timestamp(1704153600).unwrap() // 2024-01-02T00:00:00Z
    }

    #[test]
    fn test_anchor_for_task_todo_uses_created_at() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        task.created_at = Some("2024-01-01T00:00:00Z".to_string());

        let result = anchor_for_task(&task);
        assert_eq!(result, Some(("created_at", "2024-01-01T00:00:00Z")));
    }

    #[test]
    fn test_anchor_for_task_doing_prefers_started_at() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Doing);
        task.created_at = Some("2024-01-01T00:00:00Z".to_string());
        task.started_at = Some("2024-01-01T12:00:00Z".to_string());

        let result = anchor_for_task(&task);
        assert_eq!(result, Some(("started_at", "2024-01-01T12:00:00Z")));
    }

    #[test]
    fn test_anchor_for_task_doing_fallback_to_created_at() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Doing);
        task.created_at = Some("2024-01-01T00:00:00Z".to_string());

        let result = anchor_for_task(&task);
        assert_eq!(result, Some(("created_at", "2024-01-01T00:00:00Z")));
    }

    #[test]
    fn test_anchor_for_task_done_prefers_completed_at() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Done);
        task.created_at = Some("2024-01-01T00:00:00Z".to_string());
        task.updated_at = Some("2024-01-02T00:00:00Z".to_string());
        task.completed_at = Some("2024-01-03T00:00:00Z".to_string());

        let result = anchor_for_task(&task);
        assert_eq!(result, Some(("completed_at", "2024-01-03T00:00:00Z")));
    }

    #[test]
    fn test_anchor_for_task_done_fallback_to_updated_at() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Done);
        task.created_at = Some("2024-01-01T00:00:00Z".to_string());
        task.updated_at = Some("2024-01-02T00:00:00Z".to_string());

        let result = anchor_for_task(&task);
        assert_eq!(result, Some(("updated_at", "2024-01-02T00:00:00Z")));
    }

    #[test]
    fn test_compute_task_aging_fresh() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        // 6 days old (less than warning threshold of 7)
        task.created_at = Some("2023-12-27T00:00:00Z".to_string());

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Fresh);
        assert!(aging.age.is_some());
    }

    #[test]
    fn test_compute_task_aging_warning() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        // 8 days old (greater than warning threshold of 7)
        task.created_at = Some("2023-12-25T00:00:00Z".to_string());

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Warning);
    }

    #[test]
    fn test_compute_task_aging_stale() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        // 15 days old (greater than stale threshold of 14)
        task.created_at = Some("2023-12-18T00:00:00Z".to_string());

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Stale);
    }

    #[test]
    fn test_compute_task_aging_rotten() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        // 31 days old (greater than rotten threshold of 30)
        task.created_at = Some("2023-12-02T00:00:00Z".to_string());

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Rotten);
    }

    #[test]
    fn test_compute_task_aging_exact_boundary_fresh() {
        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        // Exactly 7 days old - should be Fresh (strict > comparison)
        let seven_days_ago = now - Duration::days(7);

        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        task.created_at = Some(timeutil::format_rfc3339(seven_days_ago).unwrap());

        let aging = compute_task_aging(&task, thresholds, now);
        assert_eq!(aging.bucket, AgingBucket::Fresh);
    }

    #[test]
    fn test_compute_task_aging_exact_boundary_warning() {
        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        // Exactly 14 days old - should be Warning (strict > comparison)
        let fourteen_days_ago = now - Duration::days(14);

        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        task.created_at = Some(timeutil::format_rfc3339(fourteen_days_ago).unwrap());

        let aging = compute_task_aging(&task, thresholds, now);
        assert_eq!(aging.bucket, AgingBucket::Warning);
    }

    #[test]
    fn test_compute_task_aging_future_timestamp_unknown() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Todo);
        // Future timestamp
        task.created_at = Some("2025-01-01T00:00:00Z".to_string());

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Unknown);
        assert!(aging.age.is_none());
    }

    #[test]
    fn test_compute_task_aging_missing_timestamp_unknown() {
        let task = task_with_status("RQ-0001", TaskStatus::Todo);
        // No timestamps set

        let thresholds = AgingThresholds::default();
        let now = fixed_now();
        let aging = compute_task_aging(&task, thresholds, now);

        assert_eq!(aging.bucket, AgingBucket::Unknown);
        assert!(aging.age.is_none());
    }

    #[test]
    fn test_thresholds_from_queue_config_valid() {
        let cfg = QueueConfig {
            aging_thresholds: Some(crate::contracts::QueueAgingThresholds {
                warning_days: Some(5),
                stale_days: Some(10),
                rotten_days: Some(20),
            }),
            ..Default::default()
        };

        let thresholds = AgingThresholds::from_queue_config(&cfg).unwrap();
        assert_eq!(thresholds.warning_days, 5);
        assert_eq!(thresholds.stale_days, 10);
        assert_eq!(thresholds.rotten_days, 20);
    }

    #[test]
    fn test_thresholds_from_queue_config_uses_defaults() {
        let cfg = QueueConfig::default();

        let thresholds = AgingThresholds::from_queue_config(&cfg).unwrap();
        assert_eq!(thresholds.warning_days, DEFAULT_WARNING_DAYS);
        assert_eq!(thresholds.stale_days, DEFAULT_STALE_DAYS);
        assert_eq!(thresholds.rotten_days, DEFAULT_ROTTEN_DAYS);
    }

    #[test]
    fn test_thresholds_from_queue_config_invalid_ordering() {
        let cfg = QueueConfig {
            aging_thresholds: Some(crate::contracts::QueueAgingThresholds {
                warning_days: Some(30),
                stale_days: Some(14),
                rotten_days: Some(7),
            }),
            ..Default::default()
        };

        let result = AgingThresholds::from_queue_config(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid queue.aging_thresholds ordering")
        );
    }

    #[test]
    fn test_thresholds_from_queue_config_equal_values_invalid() {
        let cfg = QueueConfig {
            aging_thresholds: Some(crate::contracts::QueueAgingThresholds {
                warning_days: Some(7),
                stale_days: Some(7),
                rotten_days: Some(14),
            }),
            ..Default::default()
        };

        let result = AgingThresholds::from_queue_config(&cfg);
        assert!(result.is_err());
    }
}
