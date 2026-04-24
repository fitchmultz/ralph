//! Interactive session-recovery prompts.
//!
//! Purpose:
//! - Interactive session-recovery prompts.
//!
//! Responsibilities:
//! - Render session recovery prompts for valid and timed-out sessions.
//! - Short-circuit to safe defaults in non-interactive environments.
//!
//! Not handled here:
//! - Session persistence.
//! - Session validation/classification.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Non-interactive mode never resumes automatically.
//! - Prompt formatting stays stable enough for humans; tests assert only non-interactive behavior.

use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result};

use crate::contracts::SessionState;

pub fn prompt_session_recovery(session: &SessionState, non_interactive: bool) -> Result<bool> {
    if non_interactive || !std::io::stdin().is_terminal() {
        log::info!(
            "Non-interactive environment detected; skipping session resume for {}",
            session.task_id
        );
        return Ok(false);
    }

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Incomplete session detected                                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Task:        {}", pad_right(&session.task_id, 45));
    println!("║  Started:     {}", pad_right(&session.run_started_at, 45));
    println!(
        "║  Iterations:  {}/{}",
        session.iterations_completed, session.iterations_planned
    );
    println!(
        "║  Phase:       {}",
        pad_right(&format!("{}", session.current_phase), 45)
    );

    maybe_print_phase_settings(session);

    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    print!("Resume this session? [Y/n]: ");
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).context("read stdin")?;

    let input = input.trim().to_lowercase();
    Ok(input.is_empty() || input == "y" || input == "yes")
}

pub fn prompt_session_recovery_timeout(
    session: &SessionState,
    hours: u64,
    threshold_hours: u64,
    non_interactive: bool,
) -> Result<bool> {
    if non_interactive || !std::io::stdin().is_terminal() {
        log::info!(
            "Non-interactive environment detected; skipping stale session resume for {} ({} hours old)",
            session.task_id,
            hours
        );
        return Ok(false);
    }

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!(
        "║  STALE session detected ({} hours old)",
        pad_right(&hours.to_string(), 27)
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Task:        {}", pad_right(&session.task_id, 45));
    println!("║  Started:     {}", pad_right(&session.run_started_at, 45));
    println!(
        "║  Last update: {}",
        pad_right(&session.last_updated_at, 45)
    );
    println!(
        "║  Iterations:  {}/{}",
        session.iterations_completed, session.iterations_planned
    );

    maybe_print_phase_settings(session);

    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!(
        "Warning: This session is older than {} hour{}.",
        threshold_hours,
        if threshold_hours == 1 { "" } else { "s" }
    );
    print!("Resume anyway? [y/N]: ");
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).context("read stdin")?;

    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

fn maybe_print_phase_settings(session: &SessionState) {
    if session.phase1_settings.is_none()
        && session.phase2_settings.is_none()
        && session.phase3_settings.is_none()
    {
        return;
    }

    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Phase Settings:                                             ║");

    if let Some(ref settings) = session.phase1_settings {
        println!(
            "║    Phase 1:   {}",
            pad_right(&format_settings(settings), 41)
        );
    }

    if let Some(ref settings) = session.phase2_settings {
        println!(
            "║    Phase 2:   {}",
            pad_right(&format_settings(settings), 41)
        );
    }

    if let Some(ref settings) = session.phase3_settings {
        println!(
            "║    Phase 3:   {}",
            pad_right(&format_settings(settings), 41)
        );
    }
}

fn format_settings(settings: &crate::contracts::PhaseSettingsSnapshot) -> String {
    let effort = settings
        .reasoning_effort
        .map(|effort| format!(", effort={:?}", effort))
        .unwrap_or_default();
    format!("{:?}/{}{}", settings.runner, settings.model, effort)
}

fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - s.len()))
    }
}
