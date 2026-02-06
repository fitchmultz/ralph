//! Rendering tests for the main TUI panels (task list + task details).
//!
//! These tests validate that primary panels render expected task fields and
//! content variations (timestamps, evidence/plan, priorities, selection, etc.).

mod test_support;
mod tui_rendering_support;

use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use ralph::tui::App;
use test_support::make_render_test_queue as make_test_queue;
use tui_rendering_support::{get_rendered_output, setup_test_terminal};

#[track_caller]
fn line_containing<'a>(output: &'a str, needle: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| {
            panic!("expected a line containing {needle:?}\n--- output ---\n{output}\n--- end ---")
        })
}

#[test]
fn test_render_empty_queue_shows_message() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![],
    };
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);

    assert!(output.contains("No tasks in queue."));
    assert!(output.contains("ralph task"));
}

#[test]
fn test_render_task_list_shows_task_ids() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("RQ-0001"));
    assert!(output.contains("RQ-0002"));
    assert!(output.contains("RQ-0003"));
}

#[test]
fn test_render_task_list_shows_task_titles() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("First Task"));
    assert!(output.contains("Second Task"));
    assert!(output.contains("Third Task"));
}

#[test]
fn test_render_task_list_shows_task_count() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should show "(3)" task count in title
    assert!(output.contains("Tasks"));
    assert!(output.contains("(3)"));
}

#[test]
fn test_render_task_list_shows_status() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("todo"));
    assert!(output.contains("doing"));
    assert!(output.contains("done"));
}

#[test]
fn test_render_task_list_shows_priority() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("medium"));
}

#[test]
fn test_render_task_details_shows_id() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Ensure ID label and selected task ID appear on the same line (details panel).
    let id_line = line_containing(&output, "ID:");
    assert!(
        id_line.contains("RQ-0001"),
        "expected details panel ID line to include RQ-0001, got: {id_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_task_details_shows_title() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should show Title label and the actual title
    assert!(output.contains("Title"));
    assert!(output.contains("First Task"));
}

#[test]
fn test_render_task_details_shows_status() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let status_line = line_containing(&output, "Status:");
    assert!(
        status_line.contains("todo"),
        "expected details panel status line to include `todo`, got: {status_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_task_details_shows_priority() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let priority_line = line_containing(&output, "Priority:");
    assert!(
        priority_line.contains("medium"),
        "expected details panel priority line to include `medium`, got: {priority_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_task_details_shows_tags() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Tags:"));
    assert!(output.contains("test"));
}

#[test]
fn test_render_task_details_shows_scope() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Scope:"));
    assert!(output.contains("crates/ralph"));
}

#[test]
fn test_render_task_details_shows_evidence() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Evidence"));
    assert!(output.contains("•"));
    assert!(output.contains("test evidence"));
}

#[test]
fn test_render_task_details_shows_plan() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Plan"));
    assert!(output.contains("1."));
    assert!(output.contains("2."));
    assert!(output.contains("test plan step 1"));
    assert!(output.contains("test plan step 2"));
}

#[test]
fn test_render_task_details_shows_scroll_indicator_when_truncated() {
    let mut queue = make_test_queue();
    queue.tasks[0].evidence = (0..24).map(|i| format!("long evidence line {i}")).collect();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 12);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(
        output.contains("Task Details ("),
        "expected scroll indicator in details title when content overflows"
    );
}

#[test]
fn test_render_task_details_shows_timestamps() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Created"));
    assert!(output.contains("Updated"));
    assert!(output.contains("2026-01-19T00:00:00Z"));
}

#[test]
fn test_render_with_long_title_wraps() {
    let mut queue = make_test_queue();
    queue.tasks[0].title = "This is a very long task title that should wrap when displayed in the details panel because it exceeds the available width".to_string();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should still show the task (content shouldn't be lost)
    assert!(output.contains("This is a very long task title"));
}

#[test]
fn test_render_with_special_characters_in_title() {
    let mut queue = make_test_queue();
    queue.tasks[0].title = "Task with <special> & \"characters\" and 'quotes'".to_string();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    // Should handle special characters without panicking
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Task with"));
}

#[test]
fn test_render_task_with_notes() {
    let mut queue = make_test_queue();
    queue.tasks[0].notes = vec!["First note".to_string(), "Second note".to_string()];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Notes"));
    // Notes are now rendered as Markdown bullet lists (using • instead of -)
    assert!(output.contains("First note"));
    assert!(output.contains("Second note"));
}

#[test]
fn test_render_task_with_dependencies() {
    let mut queue = make_test_queue();
    queue.tasks[0].depends_on = vec!["RQ-0000".to_string(), "RQ-0002".to_string()];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Depends On"));
    assert!(output.contains("RQ-0000"));
    assert!(output.contains("RQ-0002"));
}

#[test]
fn test_render_with_multiple_tasks_in_list() {
    let tasks: Vec<Task> = (0..10)
        .map(|i| Task {
            id: format!("RQ-{:04}", i),
            title: format!("Task Number {}", i),
            description: None,
            status: if i % 2 == 0 {
                TaskStatus::Todo
            } else {
                TaskStatus::Doing
            },
            priority: TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
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
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should show count of 10
    assert!(output.contains("(10)"));
    // Should show first and last tasks
    assert!(output.contains("RQ-0000"));
    assert!(output.contains("RQ-0009"));
}

#[test]
fn test_render_selection_highlighting() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.selected = 1; // Select second task
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should show all tasks
    assert!(output.contains("RQ-0001"));
    assert!(output.contains("RQ-0002"));
    assert!(output.contains("RQ-0003"));

    // Details should reflect the selected task (not just list contents).
    let id_line = line_containing(&output, "ID:");
    assert!(
        id_line.contains("RQ-0002"),
        "expected details panel ID line to include selected task id RQ-0002, got: {id_line:?}\n--- output ---\n{output}\n--- end ---"
    );

    let status_line = line_containing(&output, "Status:");
    assert!(
        status_line.contains("doing"),
        "expected details panel status line to include `doing`, got: {status_line:?}\n--- output ---\n{output}\n--- end ---"
    );

    let selected_line = output
        .lines()
        .find(|line| line.contains("RQ-0002") && line.contains("doing"))
        .unwrap_or_else(|| {
            panic!(
                "expected list row for RQ-0002 to be visible\n--- output ---\n{output}\n--- end ---"
            )
        });
    assert!(
        selected_line.contains("»"),
        "expected highlight symbol on selected row, got: {selected_line:?}\n--- output ---\n{output}\n--- end ---"
    );

    let unselected_line = output
        .lines()
        .find(|line| line.contains("RQ-0001") && line.contains("todo"))
        .unwrap_or_else(|| {
            panic!(
                "expected list row for RQ-0001 to be visible\n--- output ---\n{output}\n--- end ---"
            )
        });
    assert!(
        !unselected_line.contains("»"),
        "expected unselected row to omit highlight symbol, got: {unselected_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_high_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::High;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let priority_line = line_containing(&output, "Priority:");
    assert!(
        priority_line.contains("high"),
        "expected details panel priority line to include `high`, got: {priority_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_critical_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::Critical;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let priority_line = line_containing(&output, "Priority:");
    assert!(
        priority_line.contains("critical"),
        "expected details panel priority line to include `critical`, got: {priority_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_low_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::Low;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let priority_line = line_containing(&output, "Priority:");
    assert!(
        priority_line.contains("low"),
        "expected details panel priority line to include `low`, got: {priority_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_rejected_status() {
    let mut queue = make_test_queue();
    queue.tasks[0].status = TaskStatus::Rejected;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    let status_line = line_containing(&output, "Status:");
    assert!(
        status_line.contains("rejected"),
        "expected details panel status line to include `rejected`, got: {status_line:?}\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_empty_evidence() {
    let mut queue = make_test_queue();
    queue.tasks[0].evidence = vec![];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    // Should render without panicking when evidence is empty.
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Title")); // Sanity check that details render.
    assert!(
        !output.contains("test evidence"),
        "expected evidence items to be absent when evidence is empty\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_empty_plan() {
    let mut queue = make_test_queue();
    queue.tasks[0].plan = vec![];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    // Should render without panicking when plan is empty.
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Title")); // Sanity check that details render.
    assert!(
        !output.contains("test plan step 1") && !output.contains("test plan step 2"),
        "expected plan items to be absent when plan is empty\n--- output ---\n{output}\n--- end ---"
    );
}

#[test]
fn test_render_with_multiline_evidence() {
    let mut queue = make_test_queue();
    queue.tasks[0].evidence = vec![
        "First evidence item".to_string(),
        "Second evidence item".to_string(),
        "Third evidence item".to_string(),
    ];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(120, 40);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Evidence"));
    // Should show bullet points for each item
    assert!(output.contains("First evidence item"));
    assert!(output.contains("Second evidence item"));
    assert!(output.contains("Third evidence item"));
}
