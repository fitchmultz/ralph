//! Visual rendering tests for TUI using ratatui's TestBackend.
//!
//! These tests validate that the TUI renders correctly by:
//! - Setting up a mock terminal with TestBackend
//! - Drawing the UI with the test app state
//! - Verifying the rendered output contains expected content

use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use ralph::tui::{self, App, AppMode};
use ratatui::{backend::TestBackend, Terminal};

/// Helper to create a test task.
fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test evidence".to_string()],
        plan: vec![
            "test plan step 1".to_string(),
            "test plan step 2".to_string(),
        ],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    }
}

/// Helper to create a test queue with multiple tasks.
fn make_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

/// Helper to setup a test terminal with the given dimensions.
fn setup_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    Terminal::new(backend).expect("failed to create terminal")
}

/// Helper to get the rendered buffer as a string.
fn get_rendered_output(terminal: &mut Terminal<TestBackend>, app: &mut App) -> String {
    terminal
        .draw(|f| {
            // Update detail width from current terminal size
            app.detail_width = f.area().width.saturating_sub(4);
            tui::draw_ui(f, app)
        })
        .expect("failed to draw");

    let buffer = terminal.backend().buffer();
    let area = buffer.area();

    let mut output = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let pos = ratatui::layout::Position { x, y };
            let cell = &buffer[pos];
            output.push(cell.symbol().chars().next().unwrap_or(' '));
        }
        output.push('\n');
    }
    output
}

/// Helper to check if the rendered output contains a specific string.
fn output_contains(terminal: &mut Terminal<TestBackend>, app: &mut App, text: &str) -> bool {
    let output = get_rendered_output(terminal, app);
    output.contains(text)
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
    assert!(output.contains("ralph task build"));
}

#[test]
fn test_render_task_list_shows_task_ids() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "RQ-0001"));
    assert!(output_contains(&mut terminal, &mut app, "RQ-0002"));
    assert!(output_contains(&mut terminal, &mut app, "RQ-0003"));
}

#[test]
fn test_render_task_list_shows_task_titles() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "First Task"));
    assert!(output_contains(&mut terminal, &mut app, "Second Task"));
    assert!(output_contains(&mut terminal, &mut app, "Third Task"));
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

    assert!(output_contains(&mut terminal, &mut app, "medium"));
}

#[test]
fn test_render_task_details_shows_id() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Should show ID in details panel
    assert!(output.contains("ID:"));
    assert!(output.contains("RQ-0001"));
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
    assert!(output.contains("Status:"));
    assert!(output.contains("todo"));
}

#[test]
fn test_render_task_details_shows_priority() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Priority:"));
    assert!(output.contains("medium"));
}

#[test]
fn test_render_task_details_shows_tags() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "Tags:"));
    assert!(output_contains(&mut terminal, &mut app, "test"));
}

#[test]
fn test_render_task_details_shows_scope() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "Scope:"));
    assert!(output_contains(&mut terminal, &mut app, "crates/ralph"));
}

#[test]
fn test_render_task_details_shows_evidence() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Evidence"));
    assert!(output.contains("•"));
    assert!(output.contains("test evidence"));
}

#[test]
fn test_render_task_details_shows_plan() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Plan"));
    assert!(output.contains("1."));
    assert!(output.contains("2."));
    assert!(output.contains("test plan step 1"));
    assert!(output.contains("test plan step 2"));
}

#[test]
fn test_render_task_details_shows_timestamps() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Created"));
    assert!(output.contains("Updated"));
    assert!(output.contains("2026-01-19T00:00:00Z"));
}

#[test]
fn test_render_editing_title_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::EditingTitle("Modified Title".to_string());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Edit Title:"));
    assert!(output.contains("Modified Title"));
}

#[test]
fn test_render_confirm_delete_dialog() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDelete;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Delete this task?"));
    assert!(output.contains("(y/n)"));
}

#[test]
fn test_render_executing_mode_shows_task_id() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
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

#[test]
fn test_render_help_footer_normal_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // Check for keybinding hints
    assert!(output.contains("q:quit"));
    assert!(output.contains(":nav"));
    assert!(output.contains(":run"));
    assert!(output.contains(":del"));
    assert!(output.contains(":edit"));
    assert!(output.contains(":status"));
}

#[test]
fn test_render_help_footer_editing_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::EditingTitle("test".to_string());
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Enter:save"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_confirm_delete_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::ConfirmDelete;
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("y:yes"));
    assert!(output.contains("n:no"));
    assert!(output.contains("Esc:cancel"));
}

#[test]
fn test_render_help_footer_executing_mode() {
    let queue = make_test_queue();
    let mut app = App::new(queue);
    app.mode = AppMode::Executing {
        task_id: "RQ-0001".to_string(),
    };
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    // In executing mode, the help footer is in the execution view title
    assert!(output.contains("Esc to return"));
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
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Notes"));
    assert!(output.contains("- First note"));
    assert!(output.contains("- Second note"));
}

#[test]
fn test_render_task_with_dependencies() {
    let mut queue = make_test_queue();
    queue.tasks[0].depends_on = vec!["RQ-0000".to_string(), "RQ-0002".to_string()];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        })
        .collect();

    let queue = QueueFile { version: 1, tasks };
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

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
    // Details should show selected task
    assert!(output.contains("Second Task"));
}

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
fn test_render_with_high_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::High;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "high"));
}

#[test]
fn test_render_with_critical_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::Critical;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "critical"));
}

#[test]
fn test_render_with_low_priority() {
    let mut queue = make_test_queue();
    queue.tasks[0].priority = TaskPriority::Low;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "low"));
}

#[test]
fn test_render_with_rejected_status() {
    let mut queue = make_test_queue();
    queue.tasks[0].status = TaskStatus::Rejected;
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    assert!(output_contains(&mut terminal, &mut app, "rejected"));
}

#[test]
fn test_render_with_empty_evidence() {
    let mut queue = make_test_queue();
    queue.tasks[0].evidence = vec![];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    // Should render without evidence section when empty
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Title")); // Should still show other fields
}

#[test]
fn test_render_with_empty_plan() {
    let mut queue = make_test_queue();
    queue.tasks[0].plan = vec![];
    let mut app = App::new(queue);
    let mut terminal = setup_test_terminal(80, 24);

    // Should render without plan section when empty
    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Title")); // Should still show other fields
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
    let mut terminal = setup_test_terminal(80, 24);

    let output = get_rendered_output(&mut terminal, &mut app);
    assert!(output.contains("Evidence"));
    // Should show bullet points for each item
    assert!(output.contains("First evidence item"));
    assert!(output.contains("Second evidence item"));
    assert!(output.contains("Third evidence item"));
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
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
