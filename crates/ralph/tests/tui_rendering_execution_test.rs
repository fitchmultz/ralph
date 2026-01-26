//! Rendering tests for the execution/log view.
//!
//! These tests validate executing UI mode: header, hints, log lines,
//! status line isolation, and auto-scroll toggles.

mod test_support;
mod tui_rendering_support;

use ralph::tui::{App, AppMode};
use test_support::make_render_test_queue as make_test_queue;
use tui_rendering_support::{get_rendered_output, setup_test_terminal};

#[test]
fn test_render_executing_mode_shows_task_id() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.running_task_id = Some("RQ-0001".to_string());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Executing:"));
    assert!(output.contains("RQ-0001"));
}

#[test]
fn test_render_executing_mode_shows_esc_hint() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("(Esc to return)"));
}

#[test]
fn test_render_executing_mode_shows_waiting_message() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.logs.clear();
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Waiting for output..."));
}

#[test]
fn test_render_executing_mode_shows_logs() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.logs.push("Log line 1".to_string());
    app.logs.push("Log line 2".to_string());
    app.logs.push("Log line 3".to_string());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Log line 1"));
    assert!(output.contains("Log line 2"));
    assert!(output.contains("Log line 3"));
}

#[test]
fn test_render_executing_mode_clears_shorter_lines() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.running_task_id = Some("RQ-0001".to_string());
    let mut terminal = setup_test_terminal(40, 10);

    app.logs = vec!["XXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string()];
    let _ = get_rendered_output(&mut terminal, &mut app);

    app.logs = vec!["short".to_string()];
    let output = get_rendered_output(&mut terminal, &mut app);

    assert!(output.contains("short"));
    assert!(!output.contains('X'));
}

#[test]
fn test_render_executing_mode_status_line_is_isolated() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.running_task_id = Some("RQ-0001".to_string());
    app.logs = vec![
        "LOG-ONE".to_string(),
        "LOG-TWO".to_string(),
        "LOG-THREE".to_string(),
        "LOG-FOUR".to_string(),
    ];
    let mut terminal = setup_test_terminal(50, 10);

    let output = get_rendered_output(&mut terminal, &mut app);
    let status_line = output
        .lines()
        .find(|line| line.contains("Lines:"))
        .expect("status line");
    assert!(!status_line.contains("LOG-ONE"));
    assert!(!status_line.contains("LOG-TWO"));
    assert!(!status_line.contains("LOG-THREE"));
    assert!(!status_line.contains("LOG-FOUR"));
}

#[test]
fn test_render_executing_mode_shows_log_stats() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    for i in 0..10 {
        app.logs.push(format!("Log line {}", i));
    }
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Lines:"));
    assert!(output.contains("10"));
    assert!(output.contains("Scroll:"));
}

#[test]
fn test_render_executing_mode_shows_autoscroll_status() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    app.logs.push("Test log".to_string());
    app.autoscroll = true;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Auto-scroll:"));
    assert!(output.contains("ON"));

    app.autoscroll = false;
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("OFF"));
}
