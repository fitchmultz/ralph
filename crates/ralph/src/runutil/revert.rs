//! Git revert prompting and application helpers.
//!
//! Responsibilities:
//! - Apply `GitRevertMode` policies (Enabled/Disabled/Ask).
//! - Provide prompt context/types for interactive UIs (CLI + TUI).
//! - Parse prompt responses in a deterministic, testable way.
//!
//! Not handled here:
//! - Runner execution or abort classification.
//!
//! Invariants/assumptions:
//! - `apply_git_revert_mode*` never mutates repo state unless mode=Enabled or user chooses Revert.
//! - Non-interactive stdin (non-TTY) in Ask mode defaults to "keep changes".

use anyhow::Result;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;

use crate::contracts::GitRevertMode;
use crate::git;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertSource {
    Auto,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertOutcome {
    Reverted { source: RevertSource },
    Skipped { reason: String },
    Continue { message: String },
    Proceed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertDecision {
    Revert,
    Keep,
    Continue { message: String },
    Proceed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertPromptContext {
    pub label: String,
    pub allow_proceed: bool,
    pub preface: Option<String>,
}

impl RevertPromptContext {
    pub fn new(label: &str, allow_proceed: bool) -> Self {
        Self {
            label: label.to_string(),
            allow_proceed,
            preface: None,
        }
    }

    pub fn with_preface(mut self, preface: impl Into<String>) -> Self {
        let preface = preface.into();
        if preface.trim().is_empty() {
            return self;
        }
        self.preface = Some(preface);
        self
    }
}

pub type RevertPromptHandler = Arc<dyn Fn(&RevertPromptContext) -> RevertDecision + Send + Sync>;

pub fn apply_git_revert_mode(
    repo_root: &Path,
    mode: GitRevertMode,
    prompt_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
) -> Result<RevertOutcome> {
    apply_git_revert_mode_with_context(
        repo_root,
        mode,
        RevertPromptContext::new(prompt_label, false),
        revert_prompt,
    )
}

pub fn apply_git_revert_mode_with_context(
    repo_root: &Path,
    mode: GitRevertMode,
    prompt_context: RevertPromptContext,
    revert_prompt: Option<&RevertPromptHandler>,
) -> Result<RevertOutcome> {
    match mode {
        GitRevertMode::Enabled => {
            git::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted {
                source: RevertSource::Auto,
            })
        }
        GitRevertMode::Disabled => Ok(RevertOutcome::Skipped {
            reason: "git_revert_mode=disabled".to_string(),
        }),
        GitRevertMode::Ask => {
            if let Some(prompt) = revert_prompt {
                return apply_revert_decision(
                    repo_root,
                    prompt(&prompt_context),
                    prompt_context.allow_proceed,
                );
            }
            let stdin = std::io::stdin();
            if !stdin.is_terminal() {
                return Ok(RevertOutcome::Skipped {
                    reason: "stdin is not a TTY; keeping changes".to_string(),
                });
            }
            let choice = prompt_revert_choice(&prompt_context)?;
            apply_revert_decision(repo_root, choice, prompt_context.allow_proceed)
        }
    }
}

fn apply_revert_decision(
    repo_root: &Path,
    decision: RevertDecision,
    allow_proceed: bool,
) -> Result<RevertOutcome> {
    match decision {
        RevertDecision::Revert => {
            git::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted {
                source: RevertSource::User,
            })
        }
        RevertDecision::Keep => Ok(RevertOutcome::Skipped {
            reason: "user chose to keep changes".to_string(),
        }),
        RevertDecision::Continue { message } => Ok(RevertOutcome::Continue {
            message: message.trim_end_matches(['\n', '\r']).to_string(),
        }),
        RevertDecision::Proceed => {
            if allow_proceed {
                Ok(RevertOutcome::Proceed {
                    reason: "user chose to proceed".to_string(),
                })
            } else {
                Ok(RevertOutcome::Skipped {
                    reason: "proceed not allowed; keeping changes".to_string(),
                })
            }
        }
    }
}

pub fn format_revert_failure_message(base: &str, outcome: RevertOutcome) -> String {
    match outcome {
        RevertOutcome::Reverted { .. } => format!("{base} Uncommitted changes were reverted."),
        RevertOutcome::Skipped { reason } => format!("{base} Revert skipped ({reason})."),
        RevertOutcome::Continue { .. } => {
            format!("{base} Continue requested. No changes were reverted.")
        }
        RevertOutcome::Proceed { .. } => {
            format!("{base} Proceed requested. No changes were reverted.")
        }
    }
}

fn prompt_revert_choice(prompt_context: &RevertPromptContext) -> Result<RevertDecision> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stderr = std::io::stderr();
    prompt_revert_choice_with_io(prompt_context, &mut reader, &mut stderr)
}

pub fn prompt_revert_choice_with_io<R: BufRead, W: Write>(
    prompt_context: &RevertPromptContext,
    reader: &mut R,
    writer: &mut W,
) -> Result<RevertDecision> {
    if let Some(preface) = prompt_context.preface.as_ref()
        && !preface.trim().is_empty()
    {
        write!(writer, "{preface}")?;
        if !preface.ends_with('\n') {
            writeln!(writer)?;
        }
        writer.flush().ok();
    }

    let mut prompt = format!(
        "{}: action? [1=keep (default), 2=revert, 3=other",
        prompt_context.label
    );
    if prompt_context.allow_proceed {
        prompt.push_str(", 4=keep+continue");
    }
    prompt.push_str("]: ");
    write!(writer, "{prompt}")?;
    writer.flush().ok();

    let mut input = String::new();
    reader.read_line(&mut input)?;

    let mut decision = parse_revert_response(&input, prompt_context.allow_proceed);

    if matches!(decision, RevertDecision::Continue { ref message } if message.is_empty()) {
        write!(
            writer,
            "{}: enter message to send (empty => keep): ",
            prompt_context.label
        )?;
        writer.flush().ok();

        let mut msg = String::new();
        reader.read_line(&mut msg)?;
        let msg = msg.trim_end_matches(['\n', '\r']);
        if msg.trim().is_empty() {
            decision = RevertDecision::Keep;
        } else {
            decision = RevertDecision::Continue {
                message: msg.to_string(),
            };
        }
    }

    Ok(decision)
}

pub fn parse_revert_response(input: &str, allow_proceed: bool) -> RevertDecision {
    let raw = input.trim_end_matches(['\n', '\r']);
    let normalized = raw.trim().to_lowercase();

    match normalized.as_str() {
        "" => RevertDecision::Keep,
        "1" | "k" | "keep" => RevertDecision::Keep,
        "2" | "r" | "revert" => RevertDecision::Revert,
        "3" => RevertDecision::Continue {
            message: String::new(),
        },
        "4" if allow_proceed => RevertDecision::Proceed,
        _ => RevertDecision::Continue {
            message: raw.to_string(),
        },
    }
}
