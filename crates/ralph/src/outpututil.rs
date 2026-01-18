pub const OUTPUT_TAIL_LINES: usize = 20;
pub const OUTPUT_TAIL_LINE_MAX_CHARS: usize = 200;

pub fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }
    if chars.next().is_none() {
        return out;
    }
    if max_chars <= 3 {
        return out;
    }
    out.truncate(max_chars - 3);
    out.push_str("...");
    out
}

pub fn tail_lines(text: &str, max_lines: usize, max_chars: usize) -> Vec<String> {
    if max_lines == 0 || text.trim().is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.len() > max_lines {
        lines = lines[lines.len() - max_lines..].to_vec();
    }

    lines
        .into_iter()
        .map(|line| truncate_chars(line.trim(), max_chars))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chars_adds_ellipsis() {
        let value = "abcdefghijklmnopqrstuvwxyz";
        let truncated = truncate_chars(value, 10);
        assert_eq!(truncated, "abcdefg...");
    }

    #[test]
    fn truncate_chars_returns_full_when_short() {
        let value = "hello";
        let truncated = truncate_chars(value, 10);
        assert_eq!(truncated, "hello");
    }

    #[test]
    fn truncate_chars_empty_when_max_zero() {
        let value = "hello";
        let truncated = truncate_chars(value, 0);
        assert_eq!(truncated, "");
    }

    #[test]
    fn truncate_chars_no_ellipsis_for_small_max() {
        let value = "hello";
        let truncated = truncate_chars(value, 2);
        assert_eq!(truncated, "he");
    }

    #[test]
    fn tail_lines_returns_empty_for_zero_max() {
        let text = "line1\nline2\nline3";
        let tail = tail_lines(text, 0, 100);
        assert!(tail.is_empty());
    }

    #[test]
    fn tail_lines_returns_empty_for_empty_text() {
        let tail = tail_lines("", 5, 100);
        assert!(tail.is_empty());
    }

    #[test]
    fn tail_lines_filters_empty_lines() {
        let text = "line1\n\nline2\n\nline3";
        let tail = tail_lines(text, 10, 100);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn tail_lines_takes_last_n() {
        let text = "line1\nline2\nline3\nline4\nline5";
        let tail = tail_lines(text, 3, 100);
        assert_eq!(tail, vec!["line3", "line4", "line5"]);
    }

    #[test]
    fn tail_lines_truncates_each_line() {
        let text = "very long line 1\nvery long line 2";
        let tail = tail_lines(text, 10, 5);
        assert_eq!(tail, vec!["ve...", "ve..."]);
    }

    #[test]
    fn tail_lines_returns_all_when_fewer_than_max() {
        let text = "line1\nline2";
        let tail = tail_lines(text, 10, 100);
        assert_eq!(tail, vec!["line1", "line2"]);
    }
}
