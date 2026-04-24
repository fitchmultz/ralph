//! CI failure pattern detection helpers.
//!
//! Purpose:
//! - CI failure pattern detection helpers.
//!
//! Responsibilities:
//! - Detect common CI failure signatures from stdout/stderr.
//! - Extract structured details for operator-facing remediation messages.
//!
//! Non-scope:
//! - Running the CI gate command.
//! - Formatting markdown compliance messages.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

#[derive(Debug, Clone)]
pub(crate) struct DetectedErrorPattern {
    pub pattern_type: &'static str,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub invalid_value: Option<String>,
    pub valid_values: Option<String>,
    pub guidance: &'static str,
}

const TOML_PARSE_ERROR_GUIDANCE: &str =
    "Read the TOML file at the mentioned line and fix the syntax error or invalid value.";
const UNKNOWN_VARIANT_GUIDANCE: &str =
    "Replace the invalid value with one of the valid options listed in the error message.";
const RUFF_PYPROJECT_GUIDANCE: &str = "Check pyproject.toml for invalid ruff configuration. Common issues: invalid target-version, unknown lint rules.";
const FORMAT_CHECK_GUIDANCE: &str = "Run the formatter directly to see what needs changing.";
const LINT_CHECK_GUIDANCE: &str = "Run the linter directly to see the specific errors.";
pub(crate) const LOCK_CONTENTION_GUIDANCE: &str = "A build or test process is waiting on a file lock. Identify and stop stale `cargo`/`rustc`/`make` processes, then retry.";

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    };
    if haystack.len() < needle.len() {
        return None;
    };

    for (idx, _) in haystack.char_indices() {
        if let Some(candidate) = haystack.get(idx..idx + needle.len())
            && candidate.eq_ignore_ascii_case(needle)
        {
            return Some(idx);
        }
    }

    None
}

pub(super) fn extract_line_number(output: &str) -> Option<u32> {
    let lower = output.to_lowercase();

    if let Some(pos) = lower.find("line ") {
        let after = &lower[pos + 5..];
        if let Some(token) = after.split_whitespace().next() {
            let cleaned = token.trim_end_matches(':').trim_end_matches(',');
            if let Ok(num) = cleaned.parse::<u32>() {
                return Some(num);
            }
        }
    }

    for part in lower.split_whitespace() {
        if let Some(first_colon) = part.find(':') {
            let after_first = &part[first_colon + 1..];
            if let Some(line_str) = after_first.split(':').next()
                && let Ok(num) = line_str.parse::<u32>()
                && num > 0
                && num < 100000
            {
                return Some(num);
            }
        }
    }

    None
}

pub(super) fn extract_invalid_value(output: &str) -> Option<String> {
    if let Some(pos) = find_ascii_case_insensitive(output, "unknown variant") {
        let after = &output[pos..];
        if let Some(start) = after.find('`') {
            let rest = &after[start + 1..];
            if let Some(end) = rest.find('`') {
                return Some(rest[..end].to_string());
            }
        }
    }

    None
}

pub(super) fn extract_valid_values(output: &str) -> Option<String> {
    const PREFIX: &str = "expected one of";
    if let Some(pos) = find_ascii_case_insensitive(output, PREFIX) {
        let after = &output[pos + PREFIX.len()..];
        let end_pos = after
            .find('\n')
            .or_else(|| after.find('.'))
            .unwrap_or(after.len());
        let values = after[..end_pos].trim();
        if !values.is_empty() {
            return Some(values.to_string());
        }
    }

    None
}

pub(super) fn infer_file_path(output: &str) -> Option<String> {
    let lower = output.to_lowercase();

    for filename in &["pyproject.toml", "cargo.toml", "rustfmt.toml", ".toml"] {
        if lower.contains(filename) {
            for word in lower.split_whitespace() {
                if word.contains(".toml") || word.ends_with(".toml") {
                    let cleaned = word.trim_end_matches(':').trim_end_matches(',');
                    return Some(cleaned.to_string());
                }
            }
            return Some(filename.to_string());
        }
    }

    if lower.contains("ruff") && lower.contains("parse") {
        return Some("pyproject.toml".to_string());
    }

    None
}

pub(super) fn detect_toml_parse_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if !lower.contains("toml") || !lower.contains("parse") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "TOML parse error",
        file_path: infer_file_path(output),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: TOML_PARSE_ERROR_GUIDANCE,
    })
}

pub(super) fn detect_unknown_variant_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if !lower.contains("unknown variant") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Unknown variant error",
        file_path: infer_file_path(output),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: UNKNOWN_VARIANT_GUIDANCE,
    })
}

pub(super) fn detect_ruff_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if !lower.contains("ruff") {
        return None;
    }
    if lower.contains("toml") && lower.contains("parse") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Ruff error",
        file_path: Some("pyproject.toml".to_string()),
        line_number: extract_line_number(output),
        invalid_value: extract_invalid_value(output),
        valid_values: extract_valid_values(output),
        guidance: RUFF_PYPROJECT_GUIDANCE,
    })
}

pub(super) fn detect_format_check_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if !lower.contains("format") || !lower.contains("failed") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Format check failure",
        file_path: None,
        line_number: None,
        invalid_value: None,
        valid_values: None,
        guidance: FORMAT_CHECK_GUIDANCE,
    })
}

pub(super) fn detect_lint_check_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if !lower.contains("lint") || !lower.contains("failed") {
        return None;
    }

    Some(DetectedErrorPattern {
        pattern_type: "Lint check failure",
        file_path: None,
        line_number: None,
        invalid_value: None,
        valid_values: None,
        guidance: LINT_CHECK_GUIDANCE,
    })
}

pub(super) fn detect_lock_contention_error(output: &str) -> Option<DetectedErrorPattern> {
    let lower = output.to_lowercase();
    if lower.contains("waiting for file lock") || lower.contains("file lock on build directory") {
        return Some(DetectedErrorPattern {
            pattern_type: "Lock contention",
            file_path: None,
            line_number: None,
            invalid_value: None,
            valid_values: None,
            guidance: LOCK_CONTENTION_GUIDANCE,
        });
    }
    None
}

pub(crate) fn detect_ci_error_pattern(stdout: &str, stderr: &str) -> Option<DetectedErrorPattern> {
    let combined = format!("{}\n{}", stderr, stdout);
    detect_toml_parse_error(&combined)
        .or_else(|| detect_unknown_variant_error(&combined))
        .or_else(|| detect_ruff_error(&combined))
        .or_else(|| detect_lock_contention_error(&combined))
        .or_else(|| detect_format_check_error(&combined))
        .or_else(|| detect_lint_check_error(&combined))
}
