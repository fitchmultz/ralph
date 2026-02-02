//! Centralized color theme for Ralph CLI and TUI.
//!
//! Provides semantic color mappings that work across both CLI (colored crate)
//! and TUI (ratatui) surfaces. Respects NO_COLOR and --color settings.
//!
//! Color Philosophy:
//! - Use colors semantically (success, error, warning) not decoratively.
//! - Runner output gets distinct colors for different message types.
//! - Maintain readability on both light and dark terminal backgrounds.
//! - Avoid "preschool art class" syndrome - colors should guide, not distract.

use ratatui::style::Color;

/// Semantic color palette for the application.
///
/// These constants define the semantic meaning of colors used throughout Ralph.
/// The actual color values are chosen to work well on both light and dark
/// terminal backgrounds while maintaining sufficient contrast.
#[allow(dead_code)]
pub struct Theme;

impl Theme {
    // Core semantic colors
    /// Success messages, completed tasks, positive indicators
    pub const SUCCESS: Color = Color::Green;
    /// Error messages, failed tasks, critical issues
    pub const ERROR: Color = Color::Red;
    /// Warning messages, in-progress tasks, caution indicators
    pub const WARNING: Color = Color::Yellow;
    /// Informational messages, pending tasks
    pub const INFO: Color = Color::Blue;
    /// Emphasis, highlights, important labels
    pub const EMPHASIS: Color = Color::Cyan;
    /// Secondary text, muted content, draft status
    pub const MUTED: Color = Color::DarkGray;

    // Runner output colors
    /// Agent reasoning/thinking blocks - subtle blue that works on both backgrounds
    pub const REASONING: Color = Color::LightBlue;
    /// Tool calls - bright cyan for visibility
    pub const TOOL_CALL: Color = Color::LightCyan;
    /// Successful tool results
    pub const TOOL_RESULT_SUCCESS: Color = Color::Green;
    /// Failed tool results
    pub const TOOL_RESULT_ERROR: Color = Color::Red;
    /// Command execution - bright magenta for distinction
    pub const COMMAND: Color = Color::LightMagenta;
    /// Supervisor/system messages - bright magenta for visibility
    pub const SUPERVISOR: Color = Color::LightMagenta;
}

/// CLI color helpers using the colored crate.
///
/// These functions provide styled strings for CLI output.
/// They automatically respect the NO_COLOR environment variable
/// and any --color flag settings via colored::control.
#[allow(dead_code)]
pub mod cli {
    use colored::{ColoredString, Colorize};

    /// Style text as success (green)
    pub fn success(text: &str) -> ColoredString {
        text.green()
    }

    /// Style text as error (red)
    pub fn error(text: &str) -> ColoredString {
        text.red()
    }

    /// Style text as warning (yellow)
    pub fn warning(text: &str) -> ColoredString {
        text.yellow()
    }

    /// Style text as info (blue)
    pub fn info(text: &str) -> ColoredString {
        text.blue()
    }

    /// Style text as emphasis (cyan)
    pub fn emphasis(text: &str) -> ColoredString {
        text.cyan()
    }

    /// Style text as muted/dimmed
    pub fn muted(text: &str) -> ColoredString {
        text.dimmed()
    }

    // Runner-specific styling

    /// Style reasoning/thinking block prefix and content
    pub fn reasoning(text: &str) -> ColoredString {
        text.bright_blue()
    }

    /// Style tool call prefix and name
    pub fn tool_call(text: &str) -> ColoredString {
        text.bright_cyan()
    }

    /// Style successful tool result
    pub fn tool_result_success(text: &str) -> ColoredString {
        text.green()
    }

    /// Style failed tool result
    pub fn tool_result_error(text: &str) -> ColoredString {
        text.red()
    }

    /// Style command execution
    pub fn command(text: &str) -> ColoredString {
        text.bright_magenta()
    }

    /// Style supervisor/system message
    pub fn supervisor(text: &str) -> ColoredString {
        text.bright_magenta()
    }

    /// Format a reasoning line with colored prefix
    pub fn format_reasoning(content: &str) -> String {
        format!("{} {}", "[Reasoning]".bright_blue().bold(), content)
    }

    /// Format a tool call line with colored prefix and optional details
    pub fn format_tool_call(name: &str, details: Option<&str>) -> String {
        let prefix = "[Tool]".bright_cyan().bold();
        let name = name.bright_cyan();
        match details {
            Some(d) => match split_status_details(d) {
                Some((status, rest)) => {
                    let status = format_status_paren(&status);
                    if rest.is_empty() {
                        format!("{} {} {}", prefix, name, status)
                    } else {
                        format!("{} {} {} {}", prefix, name, status, rest.dimmed())
                    }
                }
                None => format!("{} {} {}", prefix, name, d.dimmed()),
            },
            None => format!("{} {}", prefix, name),
        }
    }

    /// Format a command line with colored prefix and optional status
    pub fn format_command(name: &str, status: Option<&str>) -> String {
        let prefix = "[Command]".bright_magenta().bold();
        let name = name.bright_magenta();
        match status {
            Some(s) => format!("{} {} {}", prefix, name, format_status_paren(s)),
            None => format!("{} {}", prefix, name),
        }
    }

    /// Format a permission denied message
    pub fn format_permission_denied(tool_name: &str) -> String {
        format!("[Permission denied: {}]", tool_name.red())
    }

    #[derive(Clone, Copy, Debug)]
    enum StatusTone {
        Success,
        Warning,
        Error,
        Neutral,
    }

    fn format_status_paren(status: &str) -> ColoredString {
        let display = format!("({})", status);
        match classify_status(status) {
            StatusTone::Success => display.green(),
            StatusTone::Warning => display.yellow(),
            StatusTone::Error => display.red(),
            StatusTone::Neutral => display.dimmed(),
        }
    }

    fn classify_status(status: &str) -> StatusTone {
        if let Some(code) = extract_exit_code(status) {
            return if code == 0 {
                StatusTone::Success
            } else {
                StatusTone::Error
            };
        }

        let status_lower = status.to_ascii_lowercase();
        if contains_any(
            &status_lower,
            &[
                "error", "fail", "failed", "denied", "timeout", "cancel", "canceled",
            ],
        ) {
            return StatusTone::Error;
        }
        if contains_any(
            &status_lower,
            &[
                "running",
                "started",
                "pending",
                "queued",
                "in_progress",
                "working",
            ],
        ) {
            return StatusTone::Warning;
        }
        if contains_any(
            &status_lower,
            &[
                "completed",
                "success",
                "succeeded",
                "ok",
                "done",
                "finished",
            ],
        ) {
            return StatusTone::Success;
        }

        StatusTone::Neutral
    }

    fn contains_any(haystack: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| haystack.contains(needle))
    }

    fn extract_exit_code(status: &str) -> Option<i32> {
        let tokens: Vec<&str> = status
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect();
        for pair in tokens.windows(2) {
            if pair[0].eq_ignore_ascii_case("exit")
                && let Ok(code) = pair[1].parse::<i32>()
            {
                return Some(code);
            }
        }
        None
    }

    fn split_status_details(details: &str) -> Option<(String, String)> {
        let trimmed = details.trim_start();
        let inner = trimmed.strip_prefix('(')?;
        let end = inner.find(')')?;
        let status = inner[..end].trim();
        if status.is_empty() {
            return None;
        }
        let remainder = inner[end + 1..].trim_start();
        Some((status.to_string(), remainder.to_string()))
    }
}

/// TUI color helpers for ratatui.
///
/// These functions provide ratatui Color values for TUI rendering.
#[allow(dead_code)]
pub mod tui {
    use super::Theme;
    use ratatui::style::Color;

    /// Get the color for success states
    pub fn success() -> Color {
        Theme::SUCCESS
    }

    /// Get the color for error states
    pub fn error() -> Color {
        Theme::ERROR
    }

    /// Get the color for warning states
    pub fn warning() -> Color {
        Theme::WARNING
    }

    /// Get the color for info states
    pub fn info() -> Color {
        Theme::INFO
    }

    /// Get the color for emphasis
    pub fn emphasis() -> Color {
        Theme::EMPHASIS
    }

    /// Get the muted color
    pub fn muted() -> Color {
        Theme::MUTED
    }

    /// Get the color for reasoning blocks
    pub fn reasoning() -> Color {
        Theme::REASONING
    }

    /// Get the color for tool calls
    pub fn tool_call() -> Color {
        Theme::TOOL_CALL
    }

    /// Get the color for successful tool results
    pub fn tool_result_success() -> Color {
        Theme::TOOL_RESULT_SUCCESS
    }

    /// Get the color for failed tool results
    pub fn tool_result_error() -> Color {
        Theme::TOOL_RESULT_ERROR
    }

    /// Get the color for command execution
    pub fn command() -> Color {
        Theme::COMMAND
    }

    /// Get the color for supervisor messages
    pub fn supervisor() -> Color {
        Theme::SUPERVISOR
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colored::Colorize;

    #[test]
    fn theme_colors_are_defined() {
        // Just verify the colors are accessible
        let _ = Theme::SUCCESS;
        let _ = Theme::ERROR;
        let _ = Theme::WARNING;
        let _ = Theme::INFO;
        let _ = Theme::REASONING;
        let _ = Theme::TOOL_CALL;
        let _ = Theme::COMMAND;
        let _ = Theme::SUPERVISOR;
    }

    #[test]
    fn cli_format_reasoning_includes_prefix() {
        let formatted = cli::format_reasoning("test reasoning");
        assert!(formatted.contains("[Reasoning]"));
        assert!(formatted.contains("test reasoning"));
    }

    #[test]
    fn cli_format_reasoning_only_prefix_colored() {
        // Force colors on for consistent testing
        colored::control::set_override(true);

        let formatted = cli::format_reasoning("test reasoning");
        // The colored prefix should emit ANSI codes (1m = bold, 94m = bright blue, 0m = reset)
        let prefix_colored = "[Reasoning]".bright_blue().bold().to_string();
        assert!(formatted.starts_with(&prefix_colored));

        // Content should be plain (no ANSI codes after the reset)
        // The format is: "\x1B[1;94m[Reasoning]\x1B[0m test reasoning"
        assert!(
            formatted.contains("\x1B[0m test reasoning"),
            "reset code should be followed by plain content"
        );

        // Content portion (after the last reset) should not contain any ANSI codes
        let after_last_reset = formatted.rfind("\x1B[0m").map(|i| i + 4).unwrap_or(0);
        let content_part = &formatted[after_last_reset..];
        assert!(
            !content_part.contains("\x1B["),
            "content should not contain any ANSI codes"
        );
        assert_eq!(content_part, " test reasoning");

        // The content "test reasoning" appears only once and is plain
        assert!(formatted.contains("test reasoning"));
        // Verify the string ends with plain content (not colored)
        assert!(formatted.ends_with("test reasoning"));

        // Reset color override
        colored::control::unset_override();
    }

    #[test]
    fn cli_format_tool_call_with_details() {
        let formatted = cli::format_tool_call("read_file", Some("path=foo.rs"));
        assert!(formatted.contains("[Tool]"));
        assert!(formatted.contains("read_file"));
        assert!(formatted.contains("path=foo.rs"));
    }

    #[test]
    fn cli_format_tool_call_without_details() {
        let formatted = cli::format_tool_call("read_file", None);
        assert!(formatted.contains("[Tool]"));
        assert!(formatted.contains("read_file"));
    }

    #[test]
    fn cli_format_command_with_status() {
        let formatted = cli::format_command("cargo test", Some("running"));
        assert!(formatted.contains("[Command]"));
        assert!(formatted.contains("cargo test"));
        assert!(formatted.contains("(running)"));
    }

    #[test]
    fn cli_format_command_without_status() {
        let formatted = cli::format_command("cargo test", None);
        assert!(formatted.contains("[Command]"));
        assert!(formatted.contains("cargo test"));
    }

    #[test]
    fn cli_format_tool_call_colors_status() {
        colored::control::set_override(true);

        let formatted = cli::format_tool_call("read_file", Some("(completed) path=foo.rs"));
        let expected_status = "(completed)".green().to_string();
        assert!(formatted.contains(&expected_status));
        assert!(formatted.contains("path=foo.rs"));

        let error_formatted = cli::format_tool_call("read_file", Some("(error)"));
        let expected_error = "(error)".red().to_string();
        assert!(error_formatted.contains(&expected_error));

        colored::control::unset_override();
    }

    #[test]
    fn cli_format_command_colors_status() {
        colored::control::set_override(true);

        let formatted = cli::format_command("cargo test", Some("running"));
        let expected_status = "(running)".yellow().to_string();
        assert!(formatted.contains(&expected_status));

        let failed = cli::format_command("cargo test", Some("exit 2"));
        let expected_failed = "(exit 2)".red().to_string();
        assert!(failed.contains(&expected_failed));

        colored::control::unset_override();
    }

    #[test]
    fn cli_format_permission_denied() {
        let formatted = cli::format_permission_denied("bash");
        assert!(formatted.contains("Permission denied"));
        assert!(formatted.contains("bash"));
    }

    #[test]
    fn tui_color_helpers_return_colors() {
        assert_eq!(tui::success(), Color::Green);
        assert_eq!(tui::error(), Color::Red);
        assert_eq!(tui::warning(), Color::Yellow);
        assert_eq!(tui::info(), Color::Blue);
        assert_eq!(tui::reasoning(), Color::LightBlue);
        assert_eq!(tui::tool_call(), Color::LightCyan);
        assert_eq!(tui::command(), Color::LightMagenta);
        assert_eq!(tui::supervisor(), Color::LightMagenta);
    }
}
