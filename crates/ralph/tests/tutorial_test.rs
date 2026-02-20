//! Integration tests for ralph tutorial command.

use ralph::commands::tutorial::{
    ScriptedResponse, ScriptedTutorialPrompter, run_tutorial_with_prompter,
};
use serial_test::serial;

#[test]
#[serial]
fn tutorial_completes_with_scripted_prompter() {
    // Create scripted responses for all pause points
    let prompter = ScriptedTutorialPrompter::new(vec![
        ScriptedResponse::Pause, // welcome
        ScriptedResponse::Pause, // setup
        ScriptedResponse::Pause, // init
        ScriptedResponse::Pause, // create_task
        ScriptedResponse::Pause, // dry_run
                                 // review doesn't pause
                                 // cleanup doesn't pause
    ]);

    let result = run_tutorial_with_prompter(&prompter, false);
    assert!(
        result.is_ok(),
        "tutorial should complete: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn tutorial_keeps_sandbox_when_requested() {
    let prompter = ScriptedTutorialPrompter::new(vec![
        ScriptedResponse::Pause,
        ScriptedResponse::Pause,
        ScriptedResponse::Pause,
        ScriptedResponse::Pause,
        ScriptedResponse::Pause,
    ]);

    // Run with keep_sandbox=true - we can't easily verify the path persists
    // but we can verify it doesn't error
    let result = run_tutorial_with_prompter(&prompter, true);
    assert!(
        result.is_ok(),
        "tutorial with keep_sandbox should complete: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn tutorial_handles_user_quit_gracefully() {
    // This tests that if responses run out, we get an error (simulating quit)
    let prompter = ScriptedTutorialPrompter::new(vec![
        ScriptedResponse::Pause, // only one response, tutorial needs more
    ]);

    let result = run_tutorial_with_prompter(&prompter, false);
    assert!(
        result.is_err(),
        "tutorial should fail when responses exhausted"
    );
}
