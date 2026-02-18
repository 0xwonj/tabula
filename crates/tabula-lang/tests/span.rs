use tabula_lang::span::{Span, line_col};

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
