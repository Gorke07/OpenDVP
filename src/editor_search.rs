//! Search the editable source, independently of whether it compiles.

/// A zero-based source position. Column is a UTF-8 byte offset, as required
/// by Iced's text editor; it is always on a character boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// Half-open byte range in the original source.
    pub start: usize,
    pub end: usize,
    pub start_position: Position,
    pub end_position: Position,
}

// Match stepping is covered by tests; the editor currently steps through its own search state.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Clone, Copy)]
struct FoldedChar {
    value: char,
    start: usize,
    end: usize,
    first: bool,
    last: bool,
}

fn position(line_starts: &[usize], offset: usize) -> Position {
    let line = line_starts.partition_point(|&start| start <= offset) - 1;
    Position {
        line,
        column: offset - line_starts[line],
    }
}

fn token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find occurrences in source order. `whole_token` excludes
/// occurrences adjacent to a letter, digit, or underscore (e.g. D1 in D10).
/// Matches may overlap and may span lines. Unicode lowercase expansion is
/// accepted only at complete original-character boundaries.
pub fn find_matches(
    source: &str,
    query: &str,
    whole_token: bool,
    case_sensitive: bool,
) -> Vec<Match> {
    let needle: Vec<char> = if case_sensitive {
        query.chars().collect()
    } else {
        query.chars().flat_map(char::to_lowercase).collect()
    };
    if needle.is_empty() {
        return Vec::new();
    }

    let mut folded = Vec::new();
    let mut line_starts = vec![0];
    for (start, ch) in source.char_indices() {
        let end = start + ch.len_utf8();
        if ch == '\n' {
            line_starts.push(end);
        }
        let lower: Vec<char> = if case_sensitive {
            vec![ch]
        } else {
            ch.to_lowercase().collect()
        };
        let last = lower.len() - 1;
        folded.extend(lower.into_iter().enumerate().map(|(i, value)| FoldedChar {
            value,
            start,
            end,
            first: i == 0,
            last: i == last,
        }));
    }

    if needle.len() > folded.len() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for i in 0..=folded.len() - needle.len() {
        let first = folded[i];
        let last = folded[i + needle.len() - 1];
        if !first.first
            || !last.last
            || !folded[i..i + needle.len()]
                .iter()
                .map(|c| c.value)
                .eq(needle.iter().copied())
        {
            continue;
        }
        if whole_token
            && (source[..first.start]
                .chars()
                .next_back()
                .is_some_and(token_char)
                || source[last.end..].chars().next().is_some_and(token_char))
        {
            continue;
        }
        matches.push(Match {
            start: first.start,
            end: last.end,
            start_position: position(&line_starts, first.start),
            end_position: position(&line_starts, last.end),
        });
    }
    matches
}

/// Advance among ordered matches. An absent or stale index starts at the
/// first match when moving forward and the last when moving backward.
#[allow(dead_code)]
pub fn next_index(count: usize, current: Option<usize>, direction: Direction) -> Option<usize> {
    if count == 0 {
        return None;
    }
    match (current.filter(|&index| index < count), direction) {
        (None, Direction::Forward) => Some(0),
        (None, Direction::Backward) => Some(count - 1),
        (Some(index), Direction::Forward) => Some((index + 1) % count),
        (Some(0), Direction::Backward) => Some(count - 1),
        (Some(index), Direction::Backward) => Some(index - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_token_excludes_longer_addresses_and_identifiers() {
        let source = "LD D1\nMOV D10 D1\nD1_suffix d1\n";
        let exact = find_matches(source, "d1", true, false);
        assert_eq!(
            exact
                .iter()
                .map(|m| m.start_position.line)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(find_matches(source, "D1", false, false).len(), 5);
    }

    #[test]
    fn unicode_offsets_preserve_original_character_boundaries() {
        let source = "Çıkış\nİSTANBUL ve istanbul\n";
        let found = find_matches(source, "İSTANBUL", false, false);
        assert_eq!(found.len(), 1);
        assert_eq!(&source[found[0].start..found[0].end], "İSTANBUL");
        assert_eq!(found[0].start_position, Position { line: 1, column: 0 });
        assert_eq!(found[0].end_position.column, "İSTANBUL".len());
        // A partial lowercase expansion must not select only part of İ.
        assert!(find_matches("İ", "i", false, false).is_empty());
        assert_eq!(find_matches("ÇIKIŞ", "çikiş", false, false).len(), 1);
    }

    #[test]
    fn newline_and_overlapping_matches_have_precise_positions() {
        let source = "aa\r\naa\n";
        let m = find_matches(source, "a\r\na", false, false);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_position, Position { line: 0, column: 1 });
        assert_eq!(m[0].end_position, Position { line: 1, column: 1 });
        assert_eq!(find_matches("aaa", "aa", false, false).len(), 2);
    }

    #[test]
    fn empty_and_wrap_navigation() {
        assert!(find_matches("abc", "", false, false).is_empty());
        assert_eq!(next_index(0, None, Direction::Forward), None);
        assert_eq!(next_index(3, None, Direction::Forward), Some(0));
        assert_eq!(next_index(3, None, Direction::Backward), Some(2));
        assert_eq!(next_index(3, Some(2), Direction::Forward), Some(0));
        assert_eq!(next_index(3, Some(0), Direction::Backward), Some(2));
        assert_eq!(next_index(3, Some(9), Direction::Forward), Some(0));
    }

    #[test]
    fn case_sensitive_matching_respects_case_for_ascii_and_unicode() {
        let source = "D1 d1 ÇIKIŞ çikiş İ i";
        assert_eq!(find_matches(source, "D1", true, true).len(), 1);
        assert_eq!(find_matches(source, "D1", true, false).len(), 2);
        assert_eq!(find_matches(source, "çikiş", true, true).len(), 1);
        assert_eq!(find_matches(source, "çikiş", true, false).len(), 2);
        assert_eq!(find_matches(source, "İ", true, true).len(), 1);
    }
}
