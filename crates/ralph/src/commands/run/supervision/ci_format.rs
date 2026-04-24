//! Formatting helpers for CI gate messages and logs.
//!
//! Purpose:
//! - Formatting helpers for CI gate messages and logs.
//!
//! Responsibilities:
//! - Format detected CI patterns into actionable guidance.
//! - Produce concise CI output snippets for logs and continue messages.
//!
//! Non-scope:
//! - CI command execution or retry policy.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::ci_patterns::DetectedErrorPattern;

pub(crate) fn format_detected_pattern(pattern: &DetectedErrorPattern) -> String {
    let mut guidance = format!("\n## DETECTED ERROR: {}\n", pattern.pattern_type);

    if let Some(file) = &pattern.file_path {
        guidance.push_str(&format!("- **File**: `{}`\n", file));
    }
    if let Some(line) = pattern.line_number {
        guidance.push_str(&format!("- **Line**: {}\n", line));
    }
    if let Some(invalid) = &pattern.invalid_value {
        guidance.push_str(&format!("- **Invalid value**: `{}`\n", invalid));
    }
    if let Some(valid) = &pattern.valid_values {
        guidance.push_str(&format!("- **Valid options**: {}\n", valid));
    }

    guidance.push_str(&format!("\n**Action**: {}\n", pattern.guidance));
    guidance
}

pub(crate) fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let skip = char_count.saturating_sub(max_chars);
        let truncated: String = s.chars().skip(skip).collect();
        format!("...{truncated}")
    }
}

pub(crate) fn format_ci_output_for_message(
    stdout: &str,
    stderr: &str,
    max_head_lines: usize,
    max_tail_lines: usize,
) -> String {
    let mut lines: Vec<&str> = Vec::new();
    lines.extend(stderr.lines());
    lines.extend(stdout.lines());

    let total_lines = lines.len();
    if total_lines == 0 {
        return "No output captured.".to_string();
    }

    let budget = max_head_lines.saturating_add(max_tail_lines);
    if total_lines <= budget {
        return format!(
            "CI output ({} lines):\n```\n{}\n```",
            total_lines,
            lines.join("\n")
        );
    }

    let head_count = max_head_lines.min(total_lines);
    let tail_count = max_tail_lines.min(total_lines.saturating_sub(head_count));
    let head: Vec<&str> = lines.iter().take(head_count).copied().collect();
    let tail_start = total_lines.saturating_sub(tail_count);
    let tail: Vec<&str> = lines.iter().skip(tail_start).copied().collect();
    let omitted = total_lines.saturating_sub(head.len() + tail.len());

    if head.is_empty() && tail.is_empty() {
        return format!(
            "CI output ({} lines total; snippet budget is 0 lines).\n\n... {} lines omitted ...",
            total_lines, omitted
        );
    }

    if tail.is_empty() {
        let head_range = format!("1-{}", head.len());
        return format!(
            "CI output ({} lines total; showing lines {}):\n\
             ```
             {}
             ```

             ... {} lines omitted ...",
            total_lines,
            head_range,
            head.join("\n"),
            omitted,
        );
    }

    if head.is_empty() {
        let tail_range = format!("{}-{}", tail_start + 1, total_lines);
        return format!(
            "CI output ({} lines total; showing lines {}):\n\
             ```
             {}
             ```

             ... {} lines omitted ...",
            total_lines,
            tail_range,
            tail.join("\n"),
            omitted,
        );
    }

    let head_range = format!("1-{}", head.len());
    let tail_range = format!("{}-{}", tail_start + 1, total_lines);

    format!(
        "CI output ({} lines total; showing lines {} and {}):\n\
         ```
         {}
         ```

         ... {} lines omitted ...

         ```
         {}
         ```",
        total_lines,
        head_range,
        tail_range,
        head.join("\n"),
        omitted,
        tail.join("\n")
    )
}
