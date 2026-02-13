//! Source location tracking for error reporting.

/// Byte offset range in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start byte offset (inclusive).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

impl Span {
    /// Create a new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Merge two spans into the smallest span covering both.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Compute (line, column) from a byte offset in source text.
/// Both are 1-indexed.
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_merge() {
        let a = Span::new(5, 10);
        let b = Span::new(8, 15);
        assert_eq!(a.merge(b), Span::new(5, 15));
    }

    #[test]
    fn test_line_col_first_char() {
        assert_eq!(line_col("hello", 0), (1, 1));
    }

    #[test]
    fn test_line_col_second_line() {
        assert_eq!(line_col("ab\ncd", 3), (2, 1));
    }

    #[test]
    fn test_line_col_mid_line() {
        assert_eq!(line_col("ab\ncde", 5), (2, 3));
    }
}
