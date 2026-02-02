//! Tests for palette fuzzy matching and command ranking.
//!
//! Responsibilities:
//! - Validate palette entry filtering, ranking, and scoring algorithms.
//! - Test fuzzy matching behavior for abbreviations and word boundaries.
//!
//! Not handled here:
//! - App state initialization, filtering, or phase tracking (see other modules).

use super::super::app::*;
use super::super::events::PaletteCommand;
use super::QueueFile;

#[test]
fn palette_entries_empty_query_returns_all_in_order() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("");
    assert!(
        entries.len() >= 20,
        "Expected at least 20 palette entries, got {}",
        entries.len()
    );

    // First few should be in fixed order
    assert!(
        matches!(entries[0].cmd, PaletteCommand::RunSelected),
        "First entry should be RunSelected, got {:?}",
        entries[0].cmd
    );
    assert!(
        matches!(entries[1].cmd, PaletteCommand::RunNextRunnable),
        "Second entry should be RunNextRunnable, got {:?}",
        entries[1].cmd
    );
}

#[test]
fn palette_entries_exact_match_ranks_highest() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("quit");

    // "Quit" should be first (exact match)
    assert!(
        matches!(entries[0].cmd, PaletteCommand::Quit),
        "Exact match 'quit' should rank Quit first, got {:?}",
        entries[0].cmd
    );
}

#[test]
fn palette_entries_prefix_match_beats_substring() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("set status");

    // All "Set status: X" should rank high due to prefix match
    let status_commands: Vec<_> = entries
        .iter()
        .filter(|e| e.title.to_lowercase().starts_with("set status"))
        .collect();
    assert!(
        status_commands.len() >= 5,
        "Expected at least 5 'Set status' commands, got {}",
        status_commands.len()
    );

    // First should be exact or strong prefix match
    assert!(
        entries[0].title.to_lowercase().contains("set status"),
        "First result should contain 'set status', got '{}'",
        entries[0].title
    );
}

#[test]
fn palette_entries_fuzzy_match_finds_abbreviations() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("ss"); // "Set status" abbreviation

    // Should find "Set status" commands via fuzzy match
    let has_set_status = entries
        .iter()
        .any(|e| e.title.to_lowercase().starts_with("set status"));
    assert!(
        has_set_status,
        "Fuzzy match 'ss' should find 'Set status' commands, got: {:?}",
        entries.iter().map(|e| &e.title).collect::<Vec<_>>()
    );
}

#[test]
fn palette_entries_fuzzy_match_finds_filtered_view() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("fb"); // "Filter by" abbreviation

    // Should find "Filter" commands
    let has_filter = entries
        .iter()
        .any(|e| e.title.to_lowercase().contains("filter"));
    assert!(
        has_filter,
        "Fuzzy match 'fb' should find filter commands, got: {:?}",
        entries.iter().map(|e| &e.title).collect::<Vec<_>>()
    );
}

#[test]
fn palette_entries_case_insensitive() {
    let app = App::new(QueueFile::default());
    let lower = app.palette_entries("quit");
    let upper = app.palette_entries("QUIT");
    let mixed = app.palette_entries("QuIt");

    assert_eq!(
        lower.len(),
        upper.len(),
        "Case should not affect result count"
    );
    assert_eq!(
        lower.len(),
        mixed.len(),
        "Case should not affect result count"
    );

    for i in 0..lower.len().min(3) {
        assert_eq!(
            lower[i].cmd, upper[i].cmd,
            "Case should not affect ranking at position {}",
            i
        );
        assert_eq!(
            lower[i].cmd, mixed[i].cmd,
            "Case should not affect ranking at position {}",
            i
        );
    }
}

#[test]
fn palette_entries_no_match_returns_empty() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("xyznonexistent");
    assert!(
        entries.is_empty(),
        "Non-matching query should return empty, got {:?}",
        entries
    );
}

#[test]
fn palette_entries_shorter_matches_rank_higher() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("run");

    // "Run selected" and "Run next runnable" should both match
    // Shorter title should rank higher when scores are equal
    let run_selected_pos = entries
        .iter()
        .position(|e| matches!(e.cmd, PaletteCommand::RunSelected));
    let run_next_pos = entries
        .iter()
        .position(|e| matches!(e.cmd, PaletteCommand::RunNextRunnable));

    if let (Some(s), Some(n)) = (run_selected_pos, run_next_pos) {
        assert!(
            s < n,
            "'Run selected' (shorter) should rank before 'Run next runnable'"
        );
    }
}

#[test]
fn palette_entries_word_boundary_match() {
    let app = App::new(QueueFile::default());
    let entries = app.palette_entries("archive");

    // "Archive done/rejected tasks" should match via word boundary
    let has_archive = entries
        .iter()
        .any(|e| matches!(e.cmd, PaletteCommand::ArchiveTerminal));
    assert!(
        has_archive,
        "Should find 'Archive' command via word boundary match"
    );
}

#[test]
fn palette_entries_consecutive_chars_boost_score() {
    let app = App::new(QueueFile::default());
    // "rst" should match "Run selected task" with consecutive bonus
    let entries = app.palette_entries("rst");

    // Should find "Run selected task"
    let has_run_selected = entries
        .iter()
        .any(|e| matches!(e.cmd, PaletteCommand::RunSelected));
    assert!(
        has_run_selected,
        "Fuzzy match 'rst' should find 'Run selected task'"
    );
}

#[test]
fn palette_entries_stable_ordering_for_equal_scores() {
    let app = App::new(QueueFile::default());
    // Use a query that will give multiple results with similar scores
    let entries1 = app.palette_entries("set");
    let entries2 = app.palette_entries("set");

    assert_eq!(
        entries1.len(),
        entries2.len(),
        "Same query should return same number of results"
    );

    // Results should be in the same order
    for (a, b) in entries1.iter().zip(entries2.iter()) {
        assert_eq!(a.cmd, b.cmd, "Equal scores should produce stable ordering");
    }
}

#[test]
fn palette_entries_filters_out_non_matches() {
    let app = App::new(QueueFile::default());
    let all_entries = app.palette_entries("");
    let filtered = app.palette_entries("quit");

    assert!(
        filtered.len() < all_entries.len(),
        "Filtered results should be fewer than all entries"
    );
    assert!(
        !filtered.is_empty(),
        "Should find at least the Quit command"
    );
}

#[test]
fn palette_entries_whitespace_query_treated_as_empty() {
    let app = App::new(QueueFile::default());
    let empty = app.palette_entries("");
    let whitespace = app.palette_entries("   ");
    let mixed_whitespace = app.palette_entries("  \t\n  ");

    assert_eq!(
        empty.len(),
        whitespace.len(),
        "Whitespace-only query should behave like empty query"
    );
    assert_eq!(
        empty.len(),
        mixed_whitespace.len(),
        "Mixed whitespace query should behave like empty query"
    );
}
