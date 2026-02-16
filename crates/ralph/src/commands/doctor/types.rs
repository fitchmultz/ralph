//! Types for doctor checks and reports.
//!
//! Responsibilities:
//! - Define severity levels, check results, and report structures
//! - Provide factory methods for creating check results
//!
//! Not handled here:
//! - Actual check implementations (see submodules)
//! - Output formatting (see output.rs)
//!
//! Invariants/assumptions:
//! - CheckResult factories are pure functions with no side effects
//! - DoctorReport maintains consistent summary statistics

use serde::Serialize;

/// Severity level for a doctor check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum CheckSeverity {
    /// Check passed successfully.
    Success,
    /// Non-critical issue, operation can continue.
    Warning,
    /// Critical issue, operation should not proceed.
    Error,
}

/// A single check result.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// Category of the check (git, queue, runner, project, lock).
    pub category: String,
    /// Specific check name (e.g., "git_binary", "queue_valid").
    pub check: String,
    /// Severity level of the result.
    pub severity: CheckSeverity,
    /// Human-readable message describing the result.
    pub message: String,
    /// Whether a fix is available for this issue.
    pub fix_available: bool,
    /// Whether a fix was applied (None if not attempted, Some(true/false) if attempted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_applied: Option<bool>,
    /// Suggested fix or action for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

impl CheckResult {
    /// Create a successful check result.
    pub fn success(category: &str, check: &str, message: &str) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Success,
            message: message.to_string(),
            fix_available: false,
            fix_applied: None,
            suggested_fix: None,
        }
    }

    /// Create a warning check result.
    pub fn warning(
        category: &str,
        check: &str,
        message: &str,
        fix_available: bool,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Warning,
            message: message.to_string(),
            fix_available,
            fix_applied: None,
            suggested_fix: suggested_fix.map(|s| s.to_string()),
        }
    }

    /// Create an error check result.
    pub fn error(
        category: &str,
        check: &str,
        message: &str,
        fix_available: bool,
        suggested_fix: Option<&str>,
    ) -> Self {
        Self {
            category: category.to_string(),
            check: check.to_string(),
            severity: CheckSeverity::Error,
            message: message.to_string(),
            fix_available,
            fix_applied: None,
            suggested_fix: suggested_fix.map(|s| s.to_string()),
        }
    }

    /// Mark that a fix was applied to this check.
    pub fn with_fix_applied(mut self, applied: bool) -> Self {
        self.fix_applied = Some(applied);
        self
    }
}

/// Summary of all checks.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    /// Total number of checks performed.
    pub total: usize,
    /// Number of successful checks.
    pub passed: usize,
    /// Number of warnings.
    pub warnings: usize,
    /// Number of errors.
    pub errors: usize,
    /// Number of fixes applied.
    pub fixes_applied: usize,
    /// Number of fixes that failed.
    pub fixes_failed: usize,
}

/// Full doctor report (for JSON output).
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// Overall success status (true if no errors).
    pub success: bool,
    /// Individual check results.
    pub checks: Vec<CheckResult>,
    /// Summary statistics.
    pub summary: Summary,
}

impl DoctorReport {
    /// Create a new empty report.
    pub fn new() -> Self {
        Self {
            success: true,
            checks: Vec::new(),
            summary: Summary {
                total: 0,
                passed: 0,
                warnings: 0,
                errors: 0,
                fixes_applied: 0,
                fixes_failed: 0,
            },
        }
    }

    /// Add a check result to the report.
    pub fn add(&mut self, result: CheckResult) {
        self.summary.total += 1;
        match result.severity {
            CheckSeverity::Success => self.summary.passed += 1,
            CheckSeverity::Warning => self.summary.warnings += 1,
            CheckSeverity::Error => {
                self.summary.errors += 1;
                self.success = false;
            }
        }
        if result.fix_applied == Some(true) {
            self.summary.fixes_applied += 1;
        } else if result.fix_applied == Some(false) {
            self.summary.fixes_failed += 1;
        }
        self.checks.push(result);
    }
}

impl Default for DoctorReport {
    fn default() -> Self {
        Self::new()
    }
}
