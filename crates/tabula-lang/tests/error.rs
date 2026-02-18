use tabula_lang::error::{CompileError, ErrorKind};
use tabula_lang::span::Span;

#[test]
fn test_error_display_with_source() {
    let err = CompileError::new(
        ErrorKind::TypeMismatch,
        Span::new(10, 15),
        "expected u64, found bool",
    );
    let source = "let x =\n  foo + bar";
    let display = format!("{}", err.display_with_source(source));
    assert!(display.contains("TypeMismatch"));
    assert!(display.contains("2:")); // line 2
    assert!(display.contains("expected u64, found bool"));
}
