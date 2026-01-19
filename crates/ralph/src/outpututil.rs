use colored::Colorize;

use crate::contracts::{Task, TaskStatus};

pub const OUTPUT_TAIL_LINES: usize = 20;
pub const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;

pub fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }
    if chars.next().is_none() {
        return out;
    }
    if max_chars <= 3 {
        return out;
    }
    out.truncate(max_chars - 3);
    out.push_str("...");
    out
}

pub fn tail_lines(text: &str, max_lines: usize, max_chars: usize) -> Vec<String> {
    if max_lines == 0 || text.trim().is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.len() > max_lines {
        lines = lines[lines.len() - max_lines..].to_vec();
    }

    lines
        .into_iter()
        .map(|line| truncate_chars(line.trim(), max_chars))
        .collect()
}

pub fn style_status(status: TaskStatus) -> colored::ColoredString {
    match status {
        TaskStatus::Todo => "todo".blue(),
        TaskStatus::Doing => "doing".yellow().bold(),
        TaskStatus::Done => "done".green(),
    }
}

pub fn join_csv_trimmed(values: &[String]) -> String {
    values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<&str>>()
        .join(",")
}

pub fn format_task_id(id: &str) -> String {
    id.trim().to_string()
}

pub fn format_task_id_title(id: &str, title: &str) -> String {
    format!("{}\t{}", id.trim(), title.trim())
}

pub fn format_task_commit_message(task_id: &str, title: &str) -> String {
    let mut raw = format!("{}: {}", task_id.trim(), title.trim());
    raw = raw.replace(['\n', '\r', '\t'], " ");
    let squashed = raw.split_whitespace().collect::<Vec<&str>>().join(" ");
    truncate_chars(&squashed, 100)
}

pub fn format_task_compact(task: &Task) -> String {
    format!(
        "{}\t{}\t{}",
        task.id.trim(),
        style_status(task.status),
        task.title.trim()
    )
}

pub fn format_task_detailed(task: &Task) -> String {
    let tags = join_csv_trimmed(&task.tags);
    let scope = join_csv_trimmed(&task.scope);
    let updated_at = task.updated_at.as_deref().unwrap_or("").trim();
    let completed_at = task.completed_at.as_deref().unwrap_or("").trim();

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        task.id.trim(),
        style_status(task.status),
        task.title.trim(),
        tags,
        scope,
        updated_at,
        completed_at
    )
}

#[allow(dead_code)]
pub fn print_success(msg: &str) {
    println!("{} {}", "SUCCESS:".green().bold(), msg);
}

#[allow(dead_code)]
pub fn print_info(msg: &str) {
    println!("{} {}", "INFO:".blue().bold(), msg);
}

#[allow(dead_code)]
pub fn print_warn(msg: &str) {
    eprintln!("{} {}", "WARN:".yellow().bold(), msg);
}

#[allow(dead_code)]
pub fn print_error(msg: &str) {
    eprintln!("{} {}", "ERROR:".red().bold(), msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_adds_ellipsis() {
        let value = "abcdefghijklmnopqrstuvwxyz";
        let truncated = truncate_chars(value, 10);
        assert_eq!(truncated, "abcdefg...");
    }

    #[test]
    fn truncate_chars_returns_full_when_short() {
        let value = "hello";
        let truncated = truncate_chars(value, 10);
        assert_eq!(truncated, "hello");
    }

    #[test]
    fn truncate_chars_empty_when_max_zero() {
        let value = "hello";
        let truncated = truncate_chars(value, 0);
        assert_eq!(truncated, "");
    }

    #[test]
    fn truncate_chars_no_ellipsis_for_small_max() {
        let value = "hello";
        let truncated = truncate_chars(value, 2);
        assert_eq!(truncated, "he");
    }

    #[test]
    fn tail_lines_returns_empty_for_zero_max() {
        let text = "line1\nline2\nline3";
        let tail = tail_lines(text, 0, 100);
        assert!(tail.is_empty());
    }

    #[test]
    fn tail_lines_returns_empty_for_empty_text() {
        let tail = tail_lines("", 5, 100);
        assert!(tail.is_empty());
    }

    #[test]
    fn tail_lines_filters_empty_lines() {
        let text = "line1\n\nline2\n\nline3";
        let tail = tail_lines(text, 10, 100);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn tail_lines_takes_last_n() {
        let text = "line1\nline2\nline3\nline4\nline5";
        let tail = tail_lines(text, 3, 100);
        assert_eq!(tail, vec!["line3", "line4", "line5"]);
    }

    #[test]
    fn tail_lines_truncates_each_line() {
        let text = "very long line 1\nvery long line 2";
        let tail = tail_lines(text, 10, 5);
        assert_eq!(tail, vec!["ve...", "ve..."]);
    }

    #[test]
    fn tail_lines_returns_all_when_fewer_than_max() {
        let text = "line1\nline2";
        let tail = tail_lines(text, 10, 100);
        assert_eq!(tail, vec!["line1", "line2"]);
    }

    #[test]
    fn format_task_compact_contains_styled_status() {
        // We can't easily assert exact ANSI codes without being brittle,
        // but we can check the plain text parts are there.
        let task = Task {
            id: "RQ-123".into(),
            status: TaskStatus::Todo,
            title: "My Task".into(),
            ..Default::default()
        };
        let out = format_task_compact(&task);
        assert!(out.contains("RQ-123"));
        assert!(out.contains("My Task"));
        assert!(out.contains("todo")); // ANSI codes surround "todo"
    }

    #[test]
    fn format_task_detailed_formatting() {
        let task = Task {
            id: "RQ-123".into(),
            status: TaskStatus::Done,
            title: "My Task".into(),
            tags: vec!["t1".into(), "t2".into()],
            scope: vec!["s1".into()],
            completed_at: Some("2026-01-01".into()),
            ..Default::default()
        };
        let out = format_task_detailed(&task);
        assert!(out.contains("RQ-123"));
        assert!(out.contains("done"));
        assert!(out.contains("t1,t2"));
        assert!(out.contains("s1"));
        assert!(out.contains("2026-01-01"));
    }
}
