//! Rendering tests for layout/scaling behavior.
//!
//! These tests validate rendering across terminal sizes and scrolling behavior.

mod test_support;
mod tui_rendering_support;

use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use ralph::tui::App;
use test_support::make_render_test_queue as make_test_queue;
use tui_rendering_support::{get_rendered_output, setup_test_terminal};

#[test]
fn test_render_narrow_terminal() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(40, 20);

    // Should render without panicking on narrow terminal
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Tasks"));
    assert!(output.contains("Task Details"));
}

#[test]
fn test_render_wide_terminal() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 30);

    // Should render without panicking on wide terminal
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Tasks"));
    assert!(output.contains("Task Details"));
}

#[test]
fn test_render_scrolling_hides_top_tasks() {
    let tasks: Vec<Task> = (0..20)
        .map(|i| Task {
            id: format!("RQ-{:04}", i),
            title: format!("Task Number {}", i),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        })
        .collect();

    let queue = QueueFile { version: 1, tasks };
    let mut app = App::new(queue);

    // Set scroll to 5, so RQ-0000 to RQ-0004 should be hidden
    app.scroll = 5;
    // Also select a visible task so RQ-0000 doesn't show up in details panel
    app.selected = 5;

    let mut terminal = setup_test_terminal(80, 24);
    let output = get_rendered_output(&mut terminal, &mut app);

    assert!(!output.contains("RQ-0000"));
    assert!(!output.contains("RQ-0004"));
    assert!(output.contains("RQ-0005"));
}

#[test]
fn test_render_scrolling_shows_bottom_tasks() {
    let tasks: Vec<Task> = (0..30)
        .map(|i| Task {
            id: format!("RQ-{:04}", i),
            title: format!("Task Number {}", i),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        })
        .collect();

    let queue = QueueFile { version: 1, tasks };
    let mut app = App::new(queue);

    // Scroll down to show later tasks
    app.scroll = 20;

    let mut terminal = setup_test_terminal(80, 24);
    let output = get_rendered_output(&mut terminal, &mut app);

    assert!(output.contains("RQ-0020"));
    assert!(output.contains("RQ-0025"));
}
