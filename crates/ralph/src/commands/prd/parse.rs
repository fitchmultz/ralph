//! PRD markdown parsing.
//!
//! Purpose:
//! - PRD markdown parsing.
//!
//! Responsibilities:
//! - Parse PRD markdown into structured title, introduction, user stories, and requirements.
//! - Preserve parsing semantics independent of queue insertion policy.
//!
//! Not handled here:
//! - Queue I/O and locking.
//! - Task generation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - User stories use `###` headings inside the `User Stories` section.
//! - Functional requirements and non-goals are list-driven sections.

#[derive(Debug, Clone, Default)]
pub(super) struct ParsedPrd {
    pub(super) title: String,
    pub(super) introduction: String,
    pub(super) user_stories: Vec<UserStory>,
    pub(super) functional_requirements: Vec<String>,
    pub(super) non_goals: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct UserStory {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) acceptance_criteria: Vec<String>,
}

pub(super) fn parse_prd(content: &str) -> ParsedPrd {
    let mut parsed = ParsedPrd::default();
    let lines: Vec<&str> = content.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        if let Some(title) = line.strip_prefix("# ") {
            parsed.title = title.trim().to_string();
            index += 1;
            break;
        }
        index += 1;
    }

    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }

    let mut current_section = String::new();
    let mut in_user_story = false;
    let mut current_story: Option<UserStory> = None;
    let mut in_acceptance_criteria = false;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();

        if let Some(section) = trimmed.strip_prefix("## ") {
            push_story(&mut parsed, &mut current_story);
            current_section = section.trim().to_lowercase();
            in_user_story = false;
            in_acceptance_criteria = false;
        } else if trimmed.starts_with("### ") && current_section == "user stories" {
            push_story(&mut parsed, &mut current_story);

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
            if let Some(story) = current_story.as_mut() {
                update_user_story(story, trimmed, &mut in_acceptance_criteria);
            }
        } else if current_section == "introduction" || current_section == "overview" {
            append_section_line(&mut parsed.introduction, trimmed);
        } else if current_section == "functional requirements" {
            append_list_item(&mut parsed.functional_requirements, trimmed);
        } else if current_section == "non-goals" || current_section == "out of scope" {
            append_bullet_item(&mut parsed.non_goals, trimmed);
        }

        index += 1;
    }

    push_story(&mut parsed, &mut current_story);
    parsed
}

fn push_story(parsed: &mut ParsedPrd, current_story: &mut Option<UserStory>) {
    if let Some(story) = current_story.take()
        && !story.title.is_empty()
    {
        parsed.user_stories.push(story);
    }
}

fn update_user_story(story: &mut UserStory, trimmed: &str, in_acceptance_criteria: &mut bool) {
    if let Some(description) = trimmed.strip_prefix("**Description:**") {
        *in_acceptance_criteria = false;
        let description = description.trim();
        if !description.is_empty() {
            story.description = description.to_string();
        }
    } else if let Some(description) = trimmed.strip_prefix("Description:") {
        *in_acceptance_criteria = false;
        let description = description.trim();
        if !description.is_empty() {
            story.description = description.to_string();
        }
    } else if trimmed.starts_with("**Story:**") {
        *in_acceptance_criteria = false;
    } else if trimmed.starts_with("**Acceptance Criteria:**")
        || trimmed.starts_with("Acceptance Criteria:")
    {
        *in_acceptance_criteria = true;
    } else if *in_acceptance_criteria {
        if let Some(criterion) = trimmed.strip_prefix("- [ ]") {
            push_if_present(&mut story.acceptance_criteria, criterion);
        } else if let Some(criterion) = trimmed.strip_prefix('-') {
            push_if_present(&mut story.acceptance_criteria, criterion);
        }
    } else if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("**") {
        if story.description.is_empty() {
            story.description = trimmed.to_string();
        } else {
            story.description.push(' ');
            story.description.push_str(trimmed);
        }
    }
}

fn append_section_line(section: &mut String, trimmed: &str) {
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return;
    }
    if !section.is_empty() {
        section.push(' ');
    }
    section.push_str(trimmed);
}

fn append_list_item(items: &mut Vec<String>, trimmed: &str) {
    if let Some(item) = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('*'))
    {
        push_if_present(items, item);
        return;
    }

    if trimmed.len() > 2
        && trimmed.starts_with(|ch: char| ch.is_ascii_digit())
        && trimmed.chars().nth(1) == Some('.')
    {
        push_if_present(items, &trimmed[2..]);
    }
}

fn append_bullet_item(items: &mut Vec<String>, trimmed: &str) {
    if let Some(item) = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('*'))
    {
        push_if_present(items, item);
    }
}

fn push_if_present(items: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        items.push(trimmed.to_string());
    }
}
