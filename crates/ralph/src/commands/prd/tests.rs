//! Tests for PRD parsing and generation seams.
//!
//! Purpose:
//! - Tests for PRD parsing and generation seams.
//!
//! Responsibilities:
//! - Verify PRD markdown parsing and task generation remain stable after modularization.
//! - Keep seam-level tests adjacent to parser and generator helpers.
//!
//! Not handled here:
//! - CLI command integration.
//! - Queue persistence workflow tests.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Parser and generator behavior are deterministic for identical input.

use crate::contracts::{QueueFile, TaskPriority, TaskStatus};

use super::generate::{generate_multi_tasks, generate_single_task};
use super::parse::{ParsedPrd, UserStory, parse_prd};

#[test]
fn parse_prd_extracts_title() {
    let content = "# My Feature PRD\n\nSome introduction text.\n";
    let parsed = parse_prd(content);
    assert_eq!(parsed.title, "My Feature PRD");
}

#[test]
fn parse_prd_extracts_introduction() {
    let content = "# My Feature PRD\n\n## Introduction\n\nThis is the introduction paragraph.\nIt continues on the next line.\n";
    let parsed = parse_prd(content);
    assert!(
        parsed
            .introduction
            .contains("This is the introduction paragraph")
    );
}

#[test]
fn parse_prd_extracts_user_stories_with_following_sections() {
    let content = "# Test Feature PRD\n\n## User Stories\n\n### US-001: First Story\n**Description:** As a user, I want X.\n\n**Acceptance Criteria:**\n- [ ] Criterion 1\n\n### US-002: Second Story\n**Description:** As an admin, I want Z.\n\n**Acceptance Criteria:**\n- [ ] Criterion A\n\n## Functional Requirements\n\n1. First requirement\n";
    let parsed = parse_prd(content);
    assert_eq!(parsed.user_stories.len(), 2);
    assert_eq!(parsed.user_stories[1].id, "US-002");
    assert_eq!(parsed.functional_requirements.len(), 1);
}

#[test]
fn parse_prd_extracts_numbered_requirements() {
    let content = "# My Feature PRD\n\n## Functional Requirements\n\n1. First requirement\n2. Second requirement\n";
    let parsed = parse_prd(content);
    assert_eq!(
        parsed.functional_requirements,
        vec!["First requirement", "Second requirement"]
    );
}

#[test]
fn generate_single_task_includes_all_data() {
    let parsed = ParsedPrd {
        title: "Test PRD".to_string(),
        introduction: "Intro text".to_string(),
        user_stories: vec![UserStory {
            id: "US-001".to_string(),
            title: "Story One".to_string(),
            description: "As a user...".to_string(),
            acceptance_criteria: vec!["AC1".to_string(), "AC2".to_string()],
        }],
        functional_requirements: vec!["FR1".to_string(), "FR2".to_string()],
        non_goals: vec!["NG1".to_string()],
    };

    let task = generate_single_task(
        &parsed,
        "2026-01-28T12:00:00Z",
        TaskPriority::High,
        TaskStatus::Todo,
        &["feature".to_string()],
        &QueueFile::default(),
        None,
        "RQ",
        4,
        10,
    )
    .unwrap();

    assert_eq!(task.title, "Test PRD");
    assert!(task.tags.contains(&"prd".to_string()));
    assert!(task.tags.contains(&"feature".to_string()));
    assert!(task.request.as_ref().unwrap().contains("Intro text"));
    assert!(task.plan.contains(&"FR1".to_string()));
    assert!(task.notes.contains(&"NG1".to_string()));
}

#[test]
fn generate_multi_tasks_creates_per_story() {
    let parsed = ParsedPrd {
        title: "Test PRD".to_string(),
        introduction: "Intro".to_string(),
        user_stories: vec![
            UserStory {
                id: "US-001".to_string(),
                title: "Story One".to_string(),
                description: "As a user...".to_string(),
                acceptance_criteria: vec!["AC1".to_string()],
            },
            UserStory {
                id: "US-002".to_string(),
                title: "Story Two".to_string(),
                description: "As an admin...".to_string(),
                acceptance_criteria: vec!["AC2".to_string()],
            },
        ],
        functional_requirements: vec![],
        non_goals: vec![],
    };

    let tasks = generate_multi_tasks(
        &parsed,
        "2026-01-28T12:00:00Z",
        TaskPriority::Medium,
        TaskStatus::Todo,
        &[],
        &QueueFile::default(),
        None,
        "RQ",
        4,
        10,
    )
    .unwrap();

    assert_eq!(tasks.len(), 2);
    assert!(tasks[0].title.contains("Story One"));
    assert_eq!(tasks[1].depends_on, vec![tasks[0].id.clone()]);
}
