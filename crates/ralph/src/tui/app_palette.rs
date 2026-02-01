//! Palette-related functions for the TUI.
//!
//! Responsibilities:
//! - Provide pure functions for palette entry fuzzy matching and scoring.
//! - Build the palette entries list based on current app state.
//! - Filter and score palette entries based on query.
//!
//! Not handled here:
//! - Palette command execution (handled in app.rs).
//! - Palette state management (handled in app.rs).

use crate::tui::events::{PaletteCommand, PaletteEntry, ScoredPaletteEntry};

/// Score a palette entry title against a query using fuzzy matching.
///
/// Returns a positive score for matches, 0 for no match.
/// Higher scores indicate better matches.
///
/// Scoring algorithm:
/// - Exact match: +1000
/// - Prefix match: +500 (minus position penalty)
/// - Word boundary match: +300 (minus position penalty)
/// - Substring match: +100 (minus position penalty)
/// - Fuzzy match: +5 per character matched, +10 per consecutive match
pub fn score_palette_entry(title: &str, query: &str) -> i32 {
    let title_lower = title.to_lowercase();

    // Exact match (case-insensitive)
    if title_lower == query {
        return 1000;
    }

    // Prefix match
    if title_lower.starts_with(query) {
        return 500 - (query.len() as i32);
    }

    // Word boundary match (after space, hyphen, colon)
    let word_boundaries: Vec<usize> = title_lower
        .chars()
        .enumerate()
        .filter(|(i, c)| *i == 0 || *c == ' ' || *c == '-' || *c == ':' || *c == '/')
        .map(|(i, _)| i)
        .collect();

    for start in word_boundaries {
        let check_start = if start == 0 { 0 } else { start + 1 };
        if title_lower[check_start..].starts_with(query) {
            return 300 - (check_start as i32);
        }
    }

    // Substring match
    if let Some(pos) = title_lower.find(query) {
        return 100 - (pos as i32);
    }

    // Fuzzy match: check if query chars appear in order
    let mut score = 0;
    let mut title_chars = title_lower.chars().peekable();
    let mut last_match_pos: Option<usize> = None;
    let mut consecutive_bonus = 0;

    for (q_idx, q_char) in query.chars().enumerate() {
        let mut found = false;

        for (current_pos, t_char) in title_chars.by_ref().enumerate() {
            if t_char == q_char {
                found = true;
                // Base score for matching character
                score += 5;

                // Bonus for consecutive matches
                if let Some(last) = last_match_pos
                    && current_pos == last + 1
                    && q_idx > 0
                {
                    consecutive_bonus += 10;
                }

                last_match_pos = Some(current_pos);
                break;
            }
        }

        if !found {
            return 0; // All query chars must match for fuzzy
        }
    }

    // Penalty for distance from start
    let distance_penalty = last_match_pos.unwrap_or(0) as i32;

    score + consecutive_bonus - distance_penalty
}

/// Build a scan label from a focus string.
pub fn scan_label(focus: &str) -> String {
    let trimmed = focus.trim();
    if trimmed.is_empty() {
        "scan: (all)".to_string()
    } else {
        format!("scan: {}", trimmed)
    }
}

/// Build the static palette entries list.
///
/// This creates the base list of available commands. The loop_active parameter
/// determines the label for the loop toggle command.
pub fn build_palette_entries(loop_active: bool) -> Vec<PaletteEntry> {
    let toggle_label = if loop_active {
        "Stop loop"
    } else {
        "Start loop"
    };

    vec![
        PaletteEntry {
            cmd: PaletteCommand::RunSelected,
            title: "Run selected task".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::RunNextRunnable,
            title: "Run next runnable task".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ToggleLoop,
            title: toggle_label.to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ArchiveTerminal,
            title: "Archive done/rejected tasks".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::NewTask,
            title: "Create new task".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::BuildTaskAgent,
            title: "Build task with agent".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::EditTask,
            title: "Edit selected task".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::EditConfig,
            title: "Edit project config".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ScanRepo,
            title: "Scan repository for tasks".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::Search,
            title: "Search tasks".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::FilterTags,
            title: "Filter by tags".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::FilterScopes,
            title: "Filter by scope".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ClearFilters,
            title: "Clear filters".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::CycleStatus,
            title: "Cycle selected task status".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::CyclePriority,
            title: "Cycle selected task priority".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetStatusDraft,
            title: "Set status: Draft".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetStatusTodo,
            title: "Set status: Todo".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetStatusDoing,
            title: "Set status: Doing".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetStatusDone,
            title: "Set status: Done".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetStatusRejected,
            title: "Set status: Rejected".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetPriorityCritical,
            title: "Set priority: Critical".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetPriorityHigh,
            title: "Set priority: High".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetPriorityMedium,
            title: "Set priority: Medium".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::SetPriorityLow,
            title: "Set priority: Low".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ToggleCaseSensitive,
            title: "Toggle case-sensitive search".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ToggleRegex,
            title: "Toggle regex search".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ToggleFuzzy,
            title: "Toggle fuzzy search".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::ReloadQueue,
            title: "Reload queue from disk".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::MoveTaskUp,
            title: "Move selected task up".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::MoveTaskDown,
            title: "Move selected task down".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::JumpToTask,
            title: "Jump to task by ID".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::RepairQueue,
            title: "Repair queue".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::RepairQueueDryRun,
            title: "Repair queue (dry run)".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::UnlockQueue,
            title: "Unlock queue".to_string(),
        },
        PaletteEntry {
            cmd: PaletteCommand::Quit,
            title: "Quit".to_string(),
        },
    ]
}

/// Filter and score palette entries based on a query.
///
/// Returns entries that match the query, sorted by relevance.
pub fn filter_and_score_entries(entries: Vec<PaletteEntry>, query: &str) -> Vec<PaletteEntry> {
    let q = query.trim();
    if q.is_empty() {
        return entries;
    }

    let q_lower = q.to_lowercase();

    // Score and filter entries using fuzzy matching
    let mut scored: Vec<ScoredPaletteEntry> = entries
        .into_iter()
        .enumerate()
        .map(|(idx, entry)| {
            let score = score_palette_entry(&entry.title, &q_lower);
            ScoredPaletteEntry {
                entry,
                score,
                original_index: idx,
            }
        })
        .filter(|s| s.score > 0)
        .collect();

    // Sort by score (desc), then title length (asc), then original index (asc)
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.entry.title.len().cmp(&b.entry.title.len()))
            .then_with(|| a.original_index.cmp(&b.original_index))
    });

    scored.into_iter().map(|s| s.entry).collect()
}
