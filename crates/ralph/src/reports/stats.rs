//! Stats report implementation.
//!
//! Responsibilities:
//! - Calculate task statistics (completion rate, avg duration, tag breakdown).
//! - Compute velocity and slow group breakdowns.
//! - Generate execution history ETA reports.
//!
//! Not handled here:
//! - Output formatting (see shared.rs).
//! - CLI argument parsing.
//!
//! Invariants/assumptions:
//! - Queue files are validated before reporting.
//! - Execution history ETA requires cache directory access.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use time::Duration;

use crate::constants::custom_fields::RUNNER_USED;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::eta_calculator::{EtaCalculator, format_eta};
use crate::runner::resolve_agent_settings;
use crate::timeutil;

use super::shared::{avg_duration, format_duration};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct StatsSummary {
    pub total: usize,
    pub done: usize,
    pub rejected: usize,
    pub terminal: usize,
    pub active: usize,
    pub terminal_rate: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct StatsFilters {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DurationStats {
    pub count: usize,
    pub average_seconds: i64,
    pub median_seconds: i64,
    pub average_human: String,
    pub median_human: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TimeTrackingStats {
    pub lead_time: Option<DurationStats>,
    pub work_time: Option<DurationStats>,
    pub start_lag: Option<DurationStats>,
}

#[derive(Debug, Serialize)]
pub(crate) struct VelocityBreakdownEntry {
    pub key: String,
    pub last_7_days: u32,
    pub last_30_days: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct VelocityBreakdowns {
    pub by_tag: Vec<VelocityBreakdownEntry>,
    pub by_runner: Vec<VelocityBreakdownEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SlowGroupEntry {
    pub key: String,
    pub count: usize,
    pub median_seconds: i64,
    pub median_human: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SlowGroups {
    pub by_tag: Vec<SlowGroupEntry>,
    pub by_runner: Vec<SlowGroupEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TagBreakdown {
    pub tag: String,
    pub count: usize,
    pub percentage: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionHistoryEtaReport {
    pub runner: String,
    pub model: String,
    pub phase_count: u8,
    pub sample_count: usize,
    pub estimated_total_seconds: u64,
    pub estimated_total_human: String,
    pub confidence: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct StatsReport {
    pub summary: StatsSummary,
    pub durations: Option<DurationStats>,
    pub time_tracking: TimeTrackingStats,
    pub velocity: VelocityBreakdowns,
    pub slow_groups: SlowGroups,
    pub tag_breakdown: Vec<TagBreakdown>,
    pub filters: StatsFilters,
    pub execution_history_eta: Option<ExecutionHistoryEtaReport>,
}

/// Extract the runner group key for a task, preferring observational data over intent.
/// Falls back to task.agent.runner if custom_fields.runner_used is not present.
fn task_runner_group_key(task: &Task) -> Option<String> {
    task.custom_fields
        .get(RUNNER_USED)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_ascii_lowercase())
        .or_else(|| {
            task.agent
                .as_ref()
                .and_then(|a| a.runner.as_ref())
                .map(|r| r.id().to_ascii_lowercase())
        })
}

fn summarize_tasks(tasks: &[&Task]) -> StatsSummary {
    let total = tasks.len();
    let done = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .count();
    let rejected = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Rejected)
        .count();
    let terminal = done + rejected;
    let active = total.saturating_sub(terminal);
    let terminal_rate = if total == 0 {
        0.0
    } else {
        (terminal as f64 / total as f64) * 100.0
    };

    StatsSummary {
        total,
        done,
        rejected,
        terminal,
        active,
        terminal_rate,
    }
}

fn collect_all_tasks<'a>(queue: &'a QueueFile, done: Option<&'a QueueFile>) -> Vec<&'a Task> {
    let mut all_tasks: Vec<&Task> = queue.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks.extend(done_file.tasks.iter().collect::<Vec<&Task>>());
    }
    all_tasks
}

fn filter_tasks_by_tags<'a>(tasks: Vec<&'a Task>, tags: &[String]) -> Vec<&'a Task> {
    if tags.is_empty() {
        return tasks;
    }

    tasks
        .into_iter()
        .filter(|t| {
            let task_tags_lower: Vec<String> = t.tags.iter().map(|s| s.to_lowercase()).collect();
            tags.iter()
                .any(|tag| task_tags_lower.contains(&tag.to_lowercase()))
        })
        .collect()
}

fn calc_duration_stats(durations: &[Duration]) -> Option<DurationStats> {
    if durations.is_empty() {
        return None;
    }
    let avg_duration = avg_duration(durations);
    let mut sorted_durations = durations.to_vec();
    sorted_durations.sort();
    let median = sorted_durations[sorted_durations.len() / 2];

    Some(DurationStats {
        count: durations.len(),
        average_seconds: avg_duration.whole_seconds(),
        median_seconds: median.whole_seconds(),
        average_human: format_duration(avg_duration),
        median_human: format_duration(median),
    })
}

fn calc_velocity_breakdowns(tasks: &[&Task]) -> VelocityBreakdowns {
    use time::OffsetDateTime;

    let now = OffsetDateTime::now_utc();
    let seven_days_ago = now - Duration::days(7);
    let thirty_days_ago = now - Duration::days(30);

    let mut tag_counts_7: HashMap<String, u32> = HashMap::new();
    let mut tag_counts_30: HashMap<String, u32> = HashMap::new();
    let mut runner_counts_7: HashMap<String, u32> = HashMap::new();
    let mut runner_counts_30: HashMap<String, u32> = HashMap::new();

    for task in tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
    {
        if let Some(completed_at) = &task.completed_at
            && let Ok(completed_dt) = timeutil::parse_rfc3339(completed_at)
        {
            // By tag
            for tag in &task.tags {
                let normalized = tag.to_lowercase();
                if completed_dt >= seven_days_ago {
                    *tag_counts_7.entry(normalized.clone()).or_insert(0) += 1;
                }
                if completed_dt >= thirty_days_ago {
                    *tag_counts_30.entry(normalized).or_insert(0) += 1;
                }
            }

            // By runner (prefer observational custom_fields.runner_used over task.agent.runner)
            if let Some(runner_key) = task_runner_group_key(task) {
                if completed_dt >= seven_days_ago {
                    *runner_counts_7.entry(runner_key.clone()).or_insert(0) += 1;
                }
                if completed_dt >= thirty_days_ago {
                    *runner_counts_30.entry(runner_key).or_insert(0) += 1;
                }
            }
        }
    }

    let mut by_tag: Vec<VelocityBreakdownEntry> = tag_counts_30
        .keys()
        .map(|k| VelocityBreakdownEntry {
            key: k.clone(),
            last_7_days: *tag_counts_7.get(k).unwrap_or(&0),
            last_30_days: *tag_counts_30.get(k).unwrap_or(&0),
        })
        .collect();
    by_tag.sort_by(|a, b| b.last_30_days.cmp(&a.last_30_days));

    let mut by_runner: Vec<VelocityBreakdownEntry> = runner_counts_30
        .keys()
        .map(|k| VelocityBreakdownEntry {
            key: k.clone(),
            last_7_days: *runner_counts_7.get(k).unwrap_or(&0),
            last_30_days: *runner_counts_30.get(k).unwrap_or(&0),
        })
        .collect();
    by_runner.sort_by(|a, b| b.last_30_days.cmp(&a.last_30_days));

    VelocityBreakdowns { by_tag, by_runner }
}

fn calc_slow_groups(tasks: &[&Task]) -> SlowGroups {
    let mut by_tag_work_times: HashMap<String, Vec<Duration>> = HashMap::new();
    let mut by_runner_work_times: HashMap<String, Vec<Duration>> = HashMap::new();

    for task in tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
    {
        if let (Some(started), Some(completed)) = (&task.started_at, &task.completed_at)
            && let (Ok(start), Ok(end)) = (
                timeutil::parse_rfc3339(started),
                timeutil::parse_rfc3339(completed),
            )
            && end > start
        {
            let work_time = end - start;

            // By tag
            for tag in &task.tags {
                by_tag_work_times
                    .entry(tag.to_lowercase())
                    .or_default()
                    .push(work_time);
            }

            // By runner (prefer observational custom_fields.runner_used over task.agent.runner)
            if let Some(runner_key) = task_runner_group_key(task) {
                by_runner_work_times
                    .entry(runner_key)
                    .or_default()
                    .push(work_time);
            }
        }
    }

    fn calc_median(durations: &[Duration]) -> Duration {
        let mut sorted = durations.to_vec();
        sorted.sort();
        sorted[sorted.len() / 2]
    }

    let mut by_tag: Vec<SlowGroupEntry> = by_tag_work_times
        .into_iter()
        .filter(|(_, durations)| !durations.is_empty())
        .map(|(key, durations)| {
            let median = calc_median(&durations);
            SlowGroupEntry {
                key,
                count: durations.len(),
                median_seconds: median.whole_seconds(),
                median_human: format_duration(median),
            }
        })
        .collect();
    by_tag.sort_by(|a, b| b.median_seconds.cmp(&a.median_seconds));

    let mut by_runner: Vec<SlowGroupEntry> = by_runner_work_times
        .into_iter()
        .filter(|(_, durations)| !durations.is_empty())
        .map(|(key, durations)| {
            let median = calc_median(&durations);
            SlowGroupEntry {
                key,
                count: durations.len(),
                median_seconds: median.whole_seconds(),
                median_human: format_duration(median),
            }
        })
        .collect();
    by_runner.sort_by(|a, b| b.median_seconds.cmp(&a.median_seconds));

    SlowGroups { by_tag, by_runner }
}

pub(crate) fn build_stats_report(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    tags: &[String],
) -> StatsReport {
    let all_tasks = collect_all_tasks(queue, done);
    let filtered_tasks = filter_tasks_by_tags(all_tasks, tags);

    let summary = summarize_tasks(&filtered_tasks);

    // Calculate lead times (created_at -> completed_at)
    let mut lead_times: Vec<Duration> = Vec::new();
    // Calculate work times (started_at -> completed_at)
    let mut work_times: Vec<Duration> = Vec::new();
    // Calculate start lag (created_at -> started_at)
    let mut start_lags: Vec<Duration> = Vec::new();

    for task in filtered_tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done || t.status == TaskStatus::Rejected)
    {
        // Lead time: created -> completed
        if let (Some(created), Some(completed)) = (&task.created_at, &task.completed_at)
            && let (Ok(start), Ok(end)) = (
                timeutil::parse_rfc3339(created),
                timeutil::parse_rfc3339(completed),
            )
            && end > start
        {
            lead_times.push(end - start);
        }

        // Work time: started -> completed
        if let (Some(started), Some(completed)) = (&task.started_at, &task.completed_at)
            && let (Ok(start), Ok(end)) = (
                timeutil::parse_rfc3339(started),
                timeutil::parse_rfc3339(completed),
            )
            && end > start
        {
            work_times.push(end - start);
        }

        // Start lag: created -> started
        if let (Some(created), Some(started)) = (&task.created_at, &task.started_at)
            && let (Ok(created_dt), Ok(started_dt)) = (
                timeutil::parse_rfc3339(created),
                timeutil::parse_rfc3339(started),
            )
            && started_dt > created_dt
        {
            start_lags.push(started_dt - created_dt);
        }
    }

    let durations = calc_duration_stats(&lead_times);
    let work_time_stats = calc_duration_stats(&work_times);
    let start_lag_stats = calc_duration_stats(&start_lags);

    let time_tracking = TimeTrackingStats {
        lead_time: durations.clone(),
        work_time: work_time_stats,
        start_lag: start_lag_stats,
    };

    // Calculate velocity breakdowns
    let velocity = calc_velocity_breakdowns(&filtered_tasks);
    let slow_groups = calc_slow_groups(&filtered_tasks);

    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for task in &filtered_tasks {
        for tag in &task.tags {
            let normalized = tag.to_lowercase();
            *tag_counts.entry(normalized).or_insert(0) += 1;
        }
    }
    let mut sorted_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

    let total = summary.total as f64;
    let tag_breakdown = sorted_tags
        .into_iter()
        .map(|(tag, count)| TagBreakdown {
            tag,
            count,
            percentage: if total == 0.0 {
                0.0
            } else {
                (count as f64 / total) * 100.0
            },
        })
        .collect();

    StatsReport {
        summary,
        durations,
        time_tracking,
        velocity,
        slow_groups,
        tag_breakdown,
        filters: StatsFilters {
            tags: tags.to_vec(),
        },
        execution_history_eta: None, // Will be populated separately
    }
}

/// Build execution history ETA report from resolved config and cache.
fn build_execution_history_eta(
    resolved_config: &crate::contracts::AgentConfig,
    cache_dir: &Path,
) -> Option<ExecutionHistoryEtaReport> {
    // Resolve runner/model from config (no task overrides, no CLI overrides)
    let empty_cli_patch = crate::contracts::RunnerCliOptionsPatch::default();
    let settings = resolve_agent_settings(
        None, // runner_override
        None, // model_override
        None, // effort_override
        &empty_cli_patch,
        None, // task_agent
        resolved_config,
    )
    .ok()?;

    let phase_count = resolved_config.phases.unwrap_or(3);
    let calculator = EtaCalculator::load(cache_dir);

    let estimate = calculator.estimate_new_task_total(
        settings.runner.as_str(),
        settings.model.as_str(),
        phase_count,
    )?;

    let sample_count = calculator.count_entries_for_key(
        settings.runner.as_str(),
        settings.model.as_str(),
        phase_count,
    );

    let confidence_str = match estimate.confidence {
        crate::eta_calculator::EtaConfidence::High => "high",
        crate::eta_calculator::EtaConfidence::Medium => "medium",
        crate::eta_calculator::EtaConfidence::Low => "low",
    };

    Some(ExecutionHistoryEtaReport {
        runner: settings.runner.as_str().to_string(),
        model: settings.model.as_str().to_string(),
        phase_count,
        sample_count,
        estimated_total_seconds: estimate.remaining.as_secs(),
        estimated_total_human: format_eta(estimate.remaining),
        confidence: confidence_str.to_string(),
    })
}

/// Print summary statistics for tasks.
///
/// # Arguments
/// * `queue` - Active queue tasks
/// * `done` - Completed tasks (optional)
/// * `tags` - Optional tag filter (case-insensitive)
/// * `format` - Output format (text or json)
/// * `queue_file_size_kb` - Size of the queue file in KB for display
/// * `config_agent` - Agent config for resolving runner/model/phase_count
/// * `cache_dir` - Optional cache directory for execution history (if None, ETA section is skipped)
pub(crate) fn print_stats(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    tags: &[String],
    format: super::ReportFormat,
    queue_file_size_kb: u64,
    config_agent: &crate::contracts::AgentConfig,
    cache_dir: Option<&Path>,
) -> Result<()> {
    use super::shared::print_json;

    let mut report = build_stats_report(queue, done, tags);

    // Build execution history ETA if cache_dir is provided
    if let Some(cache) = cache_dir {
        report.execution_history_eta = build_execution_history_eta(config_agent, cache);
    }

    match format {
        super::ReportFormat::Json => {
            print_json(&report)?;
        }
        super::ReportFormat::Text => {
            if report.summary.total == 0 {
                println!("No tasks found.");
                return Ok(());
            }

            println!("Task Statistics");
            println!("================");
            println!();

            println!("Total tasks: {}", report.summary.total);
            println!(
                "Terminal (done/rejected): {} ({:.1}%)",
                report.summary.terminal, report.summary.terminal_rate
            );
            println!("Done: {}", report.summary.done);
            println!("Rejected: {}", report.summary.rejected);
            println!("Active: {}", report.summary.active);
            println!("Queue file size: {}KB", queue_file_size_kb);
            println!();

            if let Some(durations) = &report.durations {
                println!(
                    "Lead Time (created -> completed) for {} terminal task{}:",
                    durations.count,
                    if durations.count == 1 { "" } else { "s" }
                );
                println!("  Average: {}", durations.average_human);
                println!("  Median:  {}", durations.median_human);
                println!();
            }

            if let Some(work_time) = &report.time_tracking.work_time {
                println!(
                    "Work Time (started -> completed) for {} terminal task{}:",
                    work_time.count,
                    if work_time.count == 1 { "" } else { "s" }
                );
                println!("  Average: {}", work_time.average_human);
                println!("  Median:  {}", work_time.median_human);
                println!();
            }

            if let Some(start_lag) = &report.time_tracking.start_lag {
                println!(
                    "Start Lag (created -> started) for {} task{}:",
                    start_lag.count,
                    if start_lag.count == 1 { "" } else { "s" }
                );
                println!("  Average: {}", start_lag.average_human);
                println!("  Median:  {}", start_lag.median_human);
                println!();
            }

            if !report.velocity.by_tag.is_empty() {
                println!("Velocity by Tag (7d / 30d):");
                for entry in report.velocity.by_tag.iter().take(10) {
                    println!(
                        "  {}: {} / {}",
                        entry.key, entry.last_7_days, entry.last_30_days
                    );
                }
                println!();
            }

            if !report.velocity.by_runner.is_empty() {
                println!("Velocity by Runner (7d / 30d):");
                for entry in &report.velocity.by_runner {
                    println!(
                        "  {}: {} / {}",
                        entry.key, entry.last_7_days, entry.last_30_days
                    );
                }
                println!();
            }

            if !report.slow_groups.by_tag.is_empty() {
                println!("Slow Task Types by Tag (median work time):");
                for entry in report.slow_groups.by_tag.iter().take(5) {
                    println!(
                        "  {}: {} ({} tasks)",
                        entry.key, entry.median_human, entry.count
                    );
                }
                println!();
            }

            if !report.tag_breakdown.is_empty() {
                println!("Tag Breakdown:");
                for entry in &report.tag_breakdown {
                    println!(
                        "  {} ({}: {:.1}%)",
                        entry.count, entry.tag, entry.percentage
                    );
                }
                println!();
            }

            // Execution History ETA section
            if let Some(ref eta) = report.execution_history_eta {
                println!(
                    "Execution History ETA (runner={}, model={}, phases={}):",
                    eta.runner, eta.model, eta.phase_count
                );
                println!("  Samples: {}", eta.sample_count);
                println!(
                    "  Estimated new task: {} (confidence: {})",
                    eta.estimated_total_human, eta.confidence
                );
            } else if cache_dir.is_some() {
                println!("Execution History ETA: n/a (no samples for current runner/model/phases)");
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

    #[test]
    fn test_task_runner_group_key_prefers_custom_fields_runner_used() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Done);
        task.custom_fields
            .insert(RUNNER_USED.to_string(), "CoDeX ".to_string());
        task.agent = Some(crate::contracts::TaskAgent {
            runner: Some(crate::contracts::Runner::Claude),
            model: None,
            model_effort: crate::contracts::ModelEffort::Default,
            phases: None,
            iterations: None,
            followup_reasoning_effort: None,
            runner_cli: None,
            phase_overrides: None,
        });

        assert_eq!(task_runner_group_key(&task), Some("codex".to_string()));
    }

    #[test]
    fn test_task_runner_group_key_falls_back_to_agent_runner() {
        let mut task = task_with_status("RQ-0001", TaskStatus::Done);
        task.agent = Some(crate::contracts::TaskAgent {
            runner: Some(crate::contracts::Runner::Claude),
            model: None,
            model_effort: crate::contracts::ModelEffort::Default,
            phases: None,
            iterations: None,
            followup_reasoning_effort: None,
            runner_cli: None,
            phase_overrides: None,
        });

        assert_eq!(task_runner_group_key(&task), Some("claude".to_string()));
    }

    #[test]
    fn test_calc_velocity_breakdowns_groups_by_custom_fields_runner_used() {
        let now = time::OffsetDateTime::now_utc();
        let completed_at = crate::timeutil::format_rfc3339(now).unwrap();

        let mut t1 = task_with_status("RQ-0001", TaskStatus::Done);
        t1.completed_at = Some(completed_at.clone());
        t1.custom_fields
            .insert(RUNNER_USED.to_string(), "codex".to_string());

        let mut t2 = task_with_status("RQ-0002", TaskStatus::Rejected);
        t2.completed_at = Some(completed_at.clone());
        t2.custom_fields
            .insert(RUNNER_USED.to_string(), "codex".to_string());

        let mut t3 = task_with_status("RQ-0003", TaskStatus::Done);
        t3.completed_at = Some(completed_at);
        t3.custom_fields
            .insert(RUNNER_USED.to_string(), "claude".to_string());

        let refs: Vec<&Task> = vec![&t1, &t2, &t3];
        let breakdowns = calc_velocity_breakdowns(&refs);

        assert_eq!(breakdowns.by_runner.len(), 2);
        assert_eq!(breakdowns.by_runner[0].key, "codex");
        assert_eq!(breakdowns.by_runner[0].last_7_days, 2);
        assert_eq!(breakdowns.by_runner[0].last_30_days, 2);
        assert_eq!(breakdowns.by_runner[1].key, "claude");
        assert_eq!(breakdowns.by_runner[1].last_7_days, 1);
        assert_eq!(breakdowns.by_runner[1].last_30_days, 1);
    }

    #[test]
    fn test_calc_slow_groups_groups_by_custom_fields_runner_used() {
        let end = time::OffsetDateTime::now_utc();
        let start = end - Duration::hours(1);

        let mut task = task_with_status("RQ-0001", TaskStatus::Done);
        task.started_at = Some(crate::timeutil::format_rfc3339(start).unwrap());
        task.completed_at = Some(crate::timeutil::format_rfc3339(end).unwrap());
        task.custom_fields
            .insert(RUNNER_USED.to_string(), "codex".to_string());

        let refs: Vec<&Task> = vec![&task];
        let slow = calc_slow_groups(&refs);

        assert_eq!(slow.by_runner.len(), 1);
        assert_eq!(slow.by_runner[0].key, "codex");
        assert_eq!(slow.by_runner[0].median_seconds, 3600);
    }

    #[test]
    fn test_summarize_tasks_terminal_counts_rejected() {
        let tasks = [
            task_with_status("RQ-0001", TaskStatus::Todo),
            task_with_status("RQ-0002", TaskStatus::Doing),
            task_with_status("RQ-0003", TaskStatus::Done),
            task_with_status("RQ-0004", TaskStatus::Rejected),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        let summary = summarize_tasks(&refs);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.done, 1);
        assert_eq!(summary.rejected, 1);
        assert_eq!(summary.terminal, 2);
        assert_eq!(summary.active, 2);
        assert!((summary.terminal_rate - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summarize_tasks_empty() {
        let tasks: Vec<Task> = Vec::new();
        let refs: Vec<&Task> = tasks.iter().collect();
        let summary = summarize_tasks(&refs);

        assert_eq!(summary.total, 0);
        assert_eq!(summary.done, 0);
        assert_eq!(summary.rejected, 0);
        assert_eq!(summary.terminal, 0);
        assert_eq!(summary.active, 0);
        assert_eq!(summary.terminal_rate, 0.0);
    }

    #[test]
    fn test_filter_tasks_by_tags_is_case_insensitive() {
        let mut t1 = task_with_status("RQ-001", TaskStatus::Done);
        t1.tags = vec!["Important".to_string()];

        let mut t2 = task_with_status("RQ-002", TaskStatus::Done);
        t2.tags = vec!["urgent".to_string()];

        let tasks: Vec<&Task> = vec![&t1, &t2];

        let filtered = filter_tasks_by_tags(tasks.clone(), &["IMPORTANT".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "RQ-001");

        let filtered2 = filter_tasks_by_tags(tasks, &["urgent".to_string()]);
        assert_eq!(filtered2.len(), 1);
        assert_eq!(filtered2[0].id, "RQ-002");
    }

    #[test]
    fn test_filter_tasks_by_tags_empty_filter_returns_all() {
        let t1 = task_with_status("RQ-001", TaskStatus::Done);
        let t2 = task_with_status("RQ-002", TaskStatus::Done);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        let filtered = filter_tasks_by_tags(tasks, &[]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_calc_duration_stats_empty_returns_none() {
        let durations: Vec<Duration> = vec![];
        let stats = calc_duration_stats(&durations);
        assert!(stats.is_none());
    }

    #[test]
    fn test_calc_duration_stats_even_count_uses_upper_middle_median() {
        let durations = vec![
            Duration::hours(1),
            Duration::hours(2),
            Duration::hours(3),
            Duration::hours(4),
        ];

        let stats = calc_duration_stats(&durations).expect("stats expected");
        assert_eq!(stats.count, 4);
        assert_eq!(stats.median_seconds, Duration::hours(3).whole_seconds());
    }

    #[test]
    fn test_build_stats_report_respects_tag_filter_and_time_tracking() {
        let now = time::OffsetDateTime::from_unix_timestamp(1700000000).unwrap();
        let start = now - Duration::hours(2);
        let created = now - Duration::hours(3);

        let created_str = crate::timeutil::format_rfc3339(created).unwrap();
        let started_str = crate::timeutil::format_rfc3339(start).unwrap();
        let completed_str = crate::timeutil::format_rfc3339(now).unwrap();

        let mut t1 = task_with_status("RQ-001", TaskStatus::Done);
        t1.tags = vec!["A".to_string()];
        t1.created_at = Some(created_str.clone());
        t1.started_at = Some(started_str.clone());
        t1.completed_at = Some(completed_str.clone());

        let mut t2 = task_with_status("RQ-002", TaskStatus::Done);
        t2.tags = vec!["B".to_string()];
        t2.created_at = Some(created_str);
        t2.started_at = Some(started_str);
        t2.completed_at = Some(completed_str);

        let queue = QueueFile {
            version: 1,
            tasks: vec![t1, t2],
        };

        let report = build_stats_report(&queue, None, &["A".to_string()]);

        assert_eq!(report.summary.total, 1);
        assert!(report.time_tracking.lead_time.is_some());
        assert!(report.time_tracking.work_time.is_some());
        assert!(report.time_tracking.start_lag.is_some());
        assert_eq!(report.filters.tags, vec!["A".to_string()]);
    }

    #[test]
    fn test_build_stats_report_merges_queue_and_done() {
        let t1 = task_with_status("RQ-001", TaskStatus::Todo);
        let t2 = task_with_status("RQ-002", TaskStatus::Done);
        let t3 = task_with_status("RQ-003", TaskStatus::Done);

        let queue = QueueFile {
            version: 1,
            tasks: vec![t1],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![t2, t3],
        };

        let report = build_stats_report(&queue, Some(&done), &[]);

        assert_eq!(report.summary.total, 3);
        assert_eq!(report.summary.done, 2);
        assert_eq!(report.summary.terminal, 2);
    }
}
