//! Palette-related helper functions for the TUI.
//!
//! Responsibilities:
//! - Provide pure functions for palette entry fuzzy matching and scoring.
//!
//! Not handled here:
//! - Palette command execution (handled in app.rs).
//! - Palette state management (handled in app.rs).

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
