//! PRD (Product Requirements Document) to task conversion implementation.
//!
//! Responsibilities:
//! - Parse PRD markdown files to extract structured content.
//! - Generate Ralph tasks from parsed PRD data.
//! - Support both single consolidated task and multi-task (per user story) modes.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::prd`).
//! - Queue persistence details (see `crate::queue`).
//! - Runner execution or external command invocation.
//!
//! Invariants/assumptions:
//! - PRD files use standard markdown format with recognizable sections.
//! - User stories follow `### US-XXX: Title` format when present.
//! - Generated tasks have unique IDs computed from queue state.
//! - Task insertion respects the doing-task-first ordering rule.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::{config, queue, timeutil};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;

/// Options for creating tasks from a PRD file.
pub struct CreateOptions {
    /// Path to the PRD markdown file.
    pub path: std::path::PathBuf,
    /// Create multiple tasks (one per user story) instead of single consolidated task.
    pub multi: bool,
    /// Preview without inserting into queue.
    pub dry_run: bool,
    /// Priority for generated tasks.
    pub priority: Option<TaskPriority>,
    /// Tags to add to all generated tasks.
    pub tags: Vec<String>,
    /// Create as draft status.
    pub draft: bool,
}

/// Parsed PRD content structure.
#[derive(Debug, Clone, Default)]
struct ParsedPrd {
    /// Title from first # heading.
    title: String,
    /// Introduction/overview section content.
    introduction: String,
    /// User stories found in the PRD.
    user_stories: Vec<UserStory>,
    /// Functional requirements (numbered or bulleted).
    functional_requirements: Vec<String>,
    /// Non-goals/out of scope items.
    non_goals: Vec<String>,
}

/// A user story extracted from the PRD.
#[derive(Debug, Clone, Default)]
struct UserStory {
    /// Story ID (e.g., "US-001").
    id: String,
    /// Story title.
    title: String,
    /// Story description (the "As a... I want... so that..." part).
    description: String,
    /// Acceptance criteria lines.
    acceptance_criteria: Vec<String>,
}

/// Create task(s) from a PRD file.
pub fn create_from_prd(
    resolved: &config::Resolved,
    opts: &CreateOptions,
    force: bool,
) -> Result<()> {
    // Validate file exists and is readable
    if !opts.path.exists() {
        bail!(
            "PRD file not found: {}. Check the path and try again.",
            opts.path.display()
        );
    }

    let content = std::fs::read_to_string(&opts.path)
        .with_context(|| format!("Failed to read PRD file: {}", opts.path.display()))?;

    if content.trim().is_empty() {
        bail!("PRD file is empty: {}", opts.path.display());
    }

    // Parse the PRD
    let parsed = parse_prd(&content);

    if parsed.title.is_empty() {
        bail!(
            "Could not extract title from PRD: {}. Ensure the file has a # Heading at the start.",
            opts.path.display()
        );
    }

    // Load queue for ID generation and insertion
    let _queue_lock = if !opts.dry_run {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "prd create",
            force,
        )?)
    } else {
        None
    };

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };

    // Calculate insertion index
    let insert_index = queue::suggest_new_task_insert_index(&queue_file);

    // Generate tasks
    let now = timeutil::now_utc_rfc3339()?;
    let priority = opts.priority.unwrap_or(TaskPriority::Medium);
    let status = if opts.draft {
        TaskStatus::Draft
    } else {
        TaskStatus::Todo
    };

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let tasks = if opts.multi {
        generate_multi_tasks(
            &parsed,
            &now,
            priority,
            status,
            &opts.tags,
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?
    } else {
        vec![generate_single_task(
            &parsed,
            &now,
            priority,
            status,
            &opts.tags,
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?]
    };

    if tasks.is_empty() {
        bail!(
            "No tasks generated from PRD: {}. Check the file format.",
            opts.path.display()
        );
    }

    if opts.dry_run {
        println!("Dry run - would create {} task(s):", tasks.len());
        for task in &tasks {
            println!("\n  ID: {}", task.id);
            println!("  Title: {}", task.title);
            println!("  Priority: {}", task.priority);
            println!("  Status: {}", task.status);
            if !task.tags.is_empty() {
                println!("  Tags: {}", task.tags.join(", "));
            }
            if let Some(req) = &task.request {
                println!("  Request: {}", req.lines().next().unwrap_or(req));
            }
        }
        return Ok(());
    }

    // Insert tasks into queue
    let new_task_ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    for task in tasks {
        queue_file.tasks.insert(insert_index, task);
    }

    // Save queue
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    println!("Created {} task(s) from PRD:", new_task_ids.len());
    for id in &new_task_ids {
        println!("  {}", id);
    }

    Ok(())
}

/// Parse PRD markdown content into structured data.
fn parse_prd(content: &str) -> ParsedPrd {
    let mut parsed = ParsedPrd::default();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    // Extract title from first # heading
    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(title) = line.strip_prefix("# ") {
            parsed.title = title.trim().to_string();
            i += 1;
            break;
        }
        i += 1;
    }

    // Skip blank lines after title
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }

    // Parse sections
    let mut current_section = String::new();
    let mut in_user_story = false;
    let mut current_story: Option<UserStory> = None;
    let mut in_acceptance_criteria = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check for section headers
        if let Some(section) = trimmed.strip_prefix("## ") {
            // Save current user story before switching sections
            if let Some(story) = current_story.take()
                && !story.title.is_empty()
            {
                parsed.user_stories.push(story);
            }
            current_section = section.trim().to_lowercase();
            in_user_story = false;
            in_acceptance_criteria = false;
        } else if trimmed.starts_with("### ") && current_section == "user stories" {
            // Save previous story if exists
            if let Some(story) = current_story.take()
                && !story.title.is_empty()
            {
                parsed.user_stories.push(story);
            }

            // Parse user story header: "### US-001: Story Title"
            let header = trimmed[4..].trim();
            let mut story = UserStory::default();

            if let Some(colon_pos) = header.find(':') {
                story.id = header[..colon_pos].trim().to_string();
                story.title = header[colon_pos + 1..].trim().to_string();
            } else {
                story.title = header.to_string();
            }

            current_story = Some(story);
            in_user_story = true;
            in_acceptance_criteria = false;
        } else if in_user_story {
            let Some(story) = current_story.as_mut() else {
                i += 1;
                continue;
            };

            if let Some(desc) = trimmed.strip_prefix("**Description:**") {
                in_acceptance_criteria = false;
                let desc = desc.trim();
                if !desc.is_empty() {
                    story.description = desc.to_string();
                }
            } else if let Some(desc) = trimmed.strip_prefix("Description:") {
                in_acceptance_criteria = false;
                let desc = desc.trim();
                if !desc.is_empty() {
                    story.description = desc.to_string();
                }
            } else if trimmed.starts_with("**Story:**") {
                in_acceptance_criteria = false;
            } else if trimmed.starts_with("**Acceptance Criteria:**")
                || trimmed.starts_with("Acceptance Criteria:")
            {
                in_acceptance_criteria = true;
            } else if trimmed.starts_with("- [ ]") && in_acceptance_criteria {
                let criterion = trimmed[5..].trim().to_string();
                if !criterion.is_empty() {
                    story.acceptance_criteria.push(criterion);
                }
            } else if trimmed.starts_with("-") && in_acceptance_criteria {
                let criterion = trimmed[1..].trim().to_string();
                if !criterion.is_empty() {
                    story.acceptance_criteria.push(criterion);
                }
            } else if !trimmed.is_empty()
                && !trimmed.starts_with("#")
                && !trimmed.starts_with("**")
                && story.description.is_empty()
            {
                // First non-empty, non-header line after story title is the description
                story.description = trimmed.to_string();
            } else if !trimmed.is_empty()
                && !trimmed.starts_with("#")
                && !trimmed.starts_with("**")
                && !story.description.is_empty()
            {
                // Continue description on subsequent lines
                story.description.push(' ');
                story.description.push_str(trimmed);
            }
        } else if current_section == "introduction" || current_section == "overview" {
            if !trimmed.is_empty() && !trimmed.starts_with("#") {
                if !parsed.introduction.is_empty() {
                    parsed.introduction.push(' ');
                }
                parsed.introduction.push_str(trimmed);
            }
        } else if current_section == "functional requirements" {
            if trimmed.starts_with("-") || trimmed.starts_with("*") {
                let req = trimmed[1..].trim().to_string();
                if !req.is_empty() {
                    parsed.functional_requirements.push(req);
                }
            } else if trimmed.len() > 2
                && trimmed.starts_with(|c: char| c.is_ascii_digit())
                && trimmed.chars().nth(1) == Some('.')
            {
                // Numbered list: "1. Requirement text"
                let req = trimmed[2..].trim().to_string();
                if !req.is_empty() {
                    parsed.functional_requirements.push(req);
                }
            }
        } else if (current_section == "non-goals" || current_section == "out of scope")
            && (trimmed.starts_with('-') || trimmed.starts_with('*'))
        {
            let item = trimmed[1..].trim().to_string();
            if !item.is_empty() {
                parsed.non_goals.push(item);
            }
        }

        i += 1;
    }

    // Save last user story if exists
    if let Some(story) = current_story
        && !story.title.is_empty()
    {
        parsed.user_stories.push(story);
    }

    parsed
}

/// Generate a single consolidated task from the PRD.
#[allow(clippy::too_many_arguments)]
fn generate_single_task(
    parsed: &ParsedPrd,
    now: &str,
    priority: TaskPriority,
    status: TaskStatus,
    extra_tags: &[String],
    queue: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Task> {
    let id = queue::next_id_across(queue, done, id_prefix, id_width, max_dependency_depth)?;

    // Build plan from functional requirements and user story acceptance criteria
    let mut plan: Vec<String> = parsed.functional_requirements.clone();

    for story in &parsed.user_stories {
        if !story.acceptance_criteria.is_empty() {
            plan.push(format!("{}: {}", story.id, story.title));
            for criterion in &story.acceptance_criteria {
                plan.push(format!("  - {}", criterion));
            }
        }
    }

    // Build request from introduction or summary
    let request = if parsed.introduction.is_empty() {
        format!("Created from PRD: {}", parsed.title)
    } else {
        format!(
            "{}\n\nCreated from PRD: {}",
            parsed.introduction, parsed.title
        )
    };

    // Build notes from non-goals
    let notes = parsed.non_goals.clone();

    // Combine tags
    let mut tags = vec!["prd".to_string()];
    for tag in extra_tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }

    Ok(Task {
        id,
        title: parsed.title.clone(),
        status,
        priority,
        tags,
        scope: Vec::new(),
        evidence: Vec::new(),
        plan,
        notes,
        request: Some(request),
        agent: None,
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    })
}

/// Generate multiple tasks (one per user story) from the PRD.
#[allow(clippy::too_many_arguments)]
fn generate_multi_tasks(
    parsed: &ParsedPrd,
    now: &str,
    priority: TaskPriority,
    status: TaskStatus,
    extra_tags: &[String],
    queue: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Vec<Task>> {
    let mut tasks: Vec<Task> = Vec::new();
    let mut prev_ids: Vec<String> = Vec::new();

    // If no user stories, fall back to single task
    if parsed.user_stories.is_empty() {
        return Ok(vec![generate_single_task(
            parsed,
            now,
            priority,
            status,
            extra_tags,
            queue,
            done,
            id_prefix,
            id_width,
            max_dependency_depth,
        )?]);
    }

    // Generate one task per user story
    for (idx, story) in parsed.user_stories.iter().enumerate() {
        // Create a temporary queue with existing tasks plus generated ones so far
        let mut temp_queue: QueueFile = queue.clone();
        for task in &tasks {
            temp_queue.tasks.push(task.clone());
        }

        let id =
            queue::next_id_across(&temp_queue, done, id_prefix, id_width, max_dependency_depth)?;

        let title = if parsed.title.is_empty() {
            story.title.clone()
        } else {
            format!("[{}] {}", parsed.title, story.title)
        };

        let request = if story.description.is_empty() {
            format!("User story {} from PRD: {}", story.id, parsed.title)
        } else {
            story.description.clone()
        };

        let plan = story.acceptance_criteria.clone();

        let mut tags = vec!["prd".to_string(), "user-story".to_string()];
        for tag in extra_tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }

        // Build depends_on from previous story IDs for sequential dependency
        let depends_on = if idx > 0 {
            prev_ids.last().cloned().into_iter().collect()
        } else {
            Vec::new()
        };

        prev_ids.push(id.clone());

        tasks.push(Task {
            id,
            title,
            status,
            priority,
            tags,
            scope: Vec::new(),
            evidence: Vec::new(),
            plan,
            notes: Vec::new(),
            request: Some(request),
            agent: None,
            created_at: Some(now.to_string()),
            updated_at: Some(now.to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on,
            blocks: Vec::new(),
            relates_to: Vec::new(),
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        });
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prd_extracts_title() {
        let content = r#"# My Feature PRD

Some introduction text.
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.title, "My Feature PRD");
    }

    #[test]
    fn parse_prd_extracts_introduction() {
        let content = r#"# My Feature PRD

## Introduction

This is the introduction paragraph.
It continues on the next line.

## User Stories

### US-001: First Story
**Description:** As a user, I want X.

**Acceptance Criteria:**
- [ ] Criterion 1
- [ ] Criterion 2
"#;
        let parsed = parse_prd(content);
        assert!(
            parsed
                .introduction
                .contains("This is the introduction paragraph")
        );
    }

    #[test]
    fn parse_prd_extracts_user_stories() {
        let content = r#"# My Feature PRD

## User Stories

### US-001: First Story
**Description:** As a user, I want X so that Y.

**Acceptance Criteria:**
- [ ] Criterion 1
- [ ] Criterion 2

### US-002: Second Story
**Description:** As an admin, I want Z.

**Acceptance Criteria:**
- [ ] Criterion A
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.user_stories.len(), 2);
        assert_eq!(parsed.user_stories[0].id, "US-001");
        assert_eq!(parsed.user_stories[0].title, "First Story");
        assert_eq!(
            parsed.user_stories[0].description,
            "As a user, I want X so that Y."
        );
        assert_eq!(parsed.user_stories[0].acceptance_criteria.len(), 2);
        assert_eq!(parsed.user_stories[1].id, "US-002");
    }

    #[test]
    fn parse_prd_extracts_user_stories_with_following_sections() {
        // Regression test: user stories followed by other sections (Functional Requirements, Non-Goals)
        // should not lose the last story when a new ## section is encountered.
        let content = r#"# Test Feature PRD

## Introduction

This is the introduction.

## User Stories

### US-001: First Story
**Description:** As a user, I want X.

**Acceptance Criteria:**
- [ ] Criterion 1
- [ ] Criterion 2

### US-002: Second Story
**Description:** As an admin, I want Z.

**Acceptance Criteria:**
- [ ] Criterion A

## Functional Requirements

1. First requirement
2. Second requirement

## Non-Goals

- Out of scope item
"#;
        let parsed = parse_prd(content);
        assert_eq!(
            parsed.user_stories.len(),
            2,
            "Should parse both user stories"
        );
        assert_eq!(parsed.user_stories[0].id, "US-001");
        assert_eq!(parsed.user_stories[1].id, "US-002");
        assert_eq!(parsed.functional_requirements.len(), 2);
        assert_eq!(parsed.non_goals.len(), 1);
    }

    #[test]
    fn parse_prd_extracts_functional_requirements() {
        let content = r#"# My Feature PRD

## Functional Requirements

- Requirement one
- Requirement two
- Requirement three
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.functional_requirements.len(), 3);
        assert_eq!(parsed.functional_requirements[0], "Requirement one");
    }

    #[test]
    fn parse_prd_extracts_numbered_requirements() {
        let content = r#"# My Feature PRD

## Functional Requirements

1. First requirement
2. Second requirement
3. Third requirement
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.functional_requirements.len(), 3);
        assert_eq!(parsed.functional_requirements[0], "First requirement");
    }

    #[test]
    fn parse_prd_extracts_non_goals() {
        let content = r#"# My Feature PRD

## Non-Goals

- Out of scope item one
- Out of scope item two
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.non_goals.len(), 2);
        assert_eq!(parsed.non_goals[0], "Out of scope item one");
    }

    #[test]
    fn parse_prd_handles_minimal_content() {
        let content = r#"# Simple PRD

Just some content.
"#;
        let parsed = parse_prd(content);
        assert_eq!(parsed.title, "Simple PRD");
        assert!(parsed.user_stories.is_empty());
        assert!(parsed.functional_requirements.is_empty());
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

        let queue = QueueFile::default();
        let now = "2026-01-28T12:00:00Z";

        let task = generate_single_task(
            &parsed,
            now,
            TaskPriority::High,
            TaskStatus::Todo,
            &["feature".to_string()],
            &queue,
            None,
            "RQ",
            4,
            10,
        )
        .unwrap();

        assert_eq!(task.title, "Test PRD");
        assert_eq!(task.priority, TaskPriority::High);
        assert_eq!(task.status, TaskStatus::Todo);
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

        let queue = QueueFile::default();
        let now = "2026-01-28T12:00:00Z";

        let tasks = generate_multi_tasks(
            &parsed,
            now,
            TaskPriority::Medium,
            TaskStatus::Todo,
            &[],
            &queue,
            None,
            "RQ",
            4,
            10,
        )
        .unwrap();

        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].title.contains("Story One"));
        assert!(tasks[1].title.contains("Story Two"));
        // Check dependency chain
        assert!(tasks[0].depends_on.is_empty());
        assert_eq!(tasks[1].depends_on, vec![tasks[0].id.clone()]);
    }

    #[test]
    fn generate_multi_tasks_falls_back_when_no_stories() {
        let parsed = ParsedPrd {
            title: "Test PRD".to_string(),
            introduction: "Intro".to_string(),
            user_stories: vec![],
            functional_requirements: vec!["FR1".to_string()],
            non_goals: vec![],
        };

        let queue = QueueFile::default();
        let now = "2026-01-28T12:00:00Z";

        let tasks = generate_multi_tasks(
            &parsed,
            now,
            TaskPriority::Medium,
            TaskStatus::Todo,
            &[],
            &queue,
            None,
            "RQ",
            4,
            10,
        )
        .unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Test PRD");
    }
}
