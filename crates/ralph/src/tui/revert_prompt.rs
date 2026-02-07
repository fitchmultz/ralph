//! TUI-side implementation of the runner revert prompt handler.
//!
//! Responsibilities:
//! - Translate runner-side revert prompt requests into `RunnerEvent::RevertPrompt` messages.
//! - Wait for a user decision via a reply channel.
//! - Fail fast with an error if coordination fails (send fails or reply channel closes).
//!
//! Not handled here:
//! - Rendering, input handling, or UI state transitions.
//! - Applying the revert decision to git state.
//!
//! Invariants/assumptions:
//! - A closed reply channel indicates the prompt could not complete; this is treated as an error.

use anyhow::anyhow;
use std::sync::{Arc, mpsc};

use crate::runutil;
use crate::tui::RunnerEvent;

/// Creates a TUI revert prompt handler that sends a `RunnerEvent::RevertPrompt` to the UI
/// and waits for a decision via a reply channel.
///
/// # Errors
///
/// Returns an error if:
/// - Sending the `RevertPrompt` event to the UI fails (e.g., UI thread has shut down).
/// - The reply channel closes before a decision is received (e.g., user cancelled or UI crashed).
pub(in crate::tui) fn make_tui_revert_prompt_handler(
    tx: mpsc::Sender<RunnerEvent>,
) -> runutil::RevertPromptHandler {
    Arc::new(move |context: &runutil::RevertPromptContext| {
        let (reply_tx, reply_rx) = mpsc::channel();
        tx.send(RunnerEvent::RevertPrompt {
            label: context.label.clone(),
            preface: context.preface.clone(),
            allow_proceed: context.allow_proceed,
            reply: reply_tx,
        })
        .map_err(|_e| anyhow!("TUI revert prompt: failed to send RevertPrompt event to UI"))?;

        reply_rx
            .recv()
            .map_err(|_e| anyhow!("TUI revert prompt: reply channel closed before decision"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn tui_revert_prompt_errors_if_send_fails() {
        // Create a channel and immediately drop the receiver to simulate a dead UI
        let (tx, rx) = mpsc::channel::<RunnerEvent>();
        drop(rx);

        let prompt = make_tui_revert_prompt_handler(tx);
        let context = runutil::RevertPromptContext::new("test", false);

        let result = (prompt)(&context);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failed to send RevertPrompt event to UI"),
            "expected 'failed to send' error, got: {}",
            err_msg
        );
    }

    #[test]
    fn tui_revert_prompt_errors_if_reply_channel_closes() {
        let (tx, rx) = mpsc::channel::<RunnerEvent>();
        let prompt = make_tui_revert_prompt_handler(tx);

        let context = runutil::RevertPromptContext::new("test", false);

        // Spawn a thread that calls the prompt
        let join_handle = thread::spawn(move || (prompt)(&context));

        // Receive the event and drop the reply sender without sending a decision
        match rx.recv().expect("should receive RevertPrompt event") {
            RunnerEvent::RevertPrompt { reply: _reply, .. } => {
                // Drop reply without sending to simulate closed channel
                drop(_reply);
            }
            other => panic!("expected RevertPrompt event, got {:?}", other),
        }

        // Join the thread and verify it returned an error
        let result = join_handle.join().expect("thread should join successfully");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("reply channel closed before decision"),
            "expected 'reply channel closed' error, got: {}",
            err_msg
        );
    }

    #[test]
    fn tui_revert_prompt_succeeds_if_decision_received() {
        let (tx, rx) = mpsc::channel::<RunnerEvent>();
        let prompt = make_tui_revert_prompt_handler(tx);

        let context = runutil::RevertPromptContext::new("test", false);

        // Spawn a thread that calls the prompt
        let join_handle = thread::spawn(move || (prompt)(&context));

        // Receive the event and send a decision
        match rx.recv().expect("should receive RevertPrompt event") {
            RunnerEvent::RevertPrompt { reply, .. } => {
                reply
                    .send(runutil::RevertDecision::Keep)
                    .expect("should send decision");
            }
            other => panic!("expected RevertPrompt event, got {:?}", other),
        }

        // Join the thread and verify it returned the correct decision
        let result = join_handle.join().expect("thread should join successfully");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), runutil::RevertDecision::Keep);
    }
}
