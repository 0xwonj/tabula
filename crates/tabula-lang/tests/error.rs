use tabula_lang::error::{CompileError, ErrorKind};
use tabula_lang::span::Span;

#[test]
fn test_rustc_style_multichar_span() {
    let source = "let x =\n  foo + bar";
    let err = CompileError::new(
        ErrorKind::TypeMismatch,
        Span::new(10, 13), // "foo"
        "expected u64, found bool",
    );
    let display = format!("{}", err.display_with_source(source));
    // Should contain rustc-style format
    assert!(display.contains("error[TypeMismatch]: expected u64, found bool"));
    assert!(display.contains("--> 2:3"));
    assert!(display.contains("2 |   foo + bar"));
    assert!(display.contains("^^^")); // 3-char underline
}

#[test]
fn test_rustc_style_single_byte_span() {
    let source = "abc";
    let err = CompileError::new(
        ErrorKind::UnexpectedChar,
        Span::new(1, 2), // "b"
        "unexpected character",
    );
    let display = format!("{}", err.display_with_source(source));
    assert!(display.contains("error[UnexpectedChar]: unexpected character"));
    assert!(display.contains("--> 1:2"));
    assert!(display.contains("1 | abc"));
    assert!(display.contains("^ unexpected character"));
}

#[test]
fn test_rustc_style_multiline_span() {
    // Span crosses line boundary — underline only to end of first line
    let source = "line one\nline two";
    let err = CompileError::new(
        ErrorKind::UnexpectedToken,
        Span::new(5, 14), // "one\nline " — crosses newline
        "unexpected token",
    );
    let display = format!("{}", err.display_with_source(source));
    assert!(display.contains("--> 1:6"));
    assert!(display.contains("1 | line one"));
    // Should underline only "one" (3 chars, to end of line 1)
    assert!(display.contains("^^^ unexpected token"));
}

#[test]
fn test_rustc_style_zero_width_span() {
    let source = "hello";
    let err = CompileError::new(
        ErrorKind::ExpectedToken,
        Span::new(3, 3), // zero-width
        "expected ';'",
    );
    let display = format!("{}", err.display_with_source(source));
    assert!(display.contains("--> 1:4"));
    // Should show at least 1 caret even for zero-width
    assert!(display.contains("^ expected ';'"));
}

#[test]
fn test_display_without_source() {
    let err = CompileError::new(ErrorKind::TypeMismatch, Span::new(0, 5), "type mismatch");
    // Plain Display (no source context)
    let display = format!("{}", err);
    assert_eq!(display, "TypeMismatch: type mismatch");
}
