use tabula_core::TableId;
use tabula_core::error::TabulaError;

#[test]
fn test_error_display_table_not_found() {
    let e = TabulaError::TableNotFound(TableId(7));
    assert!(e.to_string().contains("TableId(7)"));
}

#[test]
fn test_error_display_arithmetic_overflow() {
    let e = TabulaError::ArithmeticOverflow;
    assert_eq!(e.to_string(), "arithmetic overflow");
}

#[test]
fn test_error_display_slot_out_of_bounds() {
    let e = TabulaError::SlotOutOfBounds { index: 10, max: 5 };
    assert_eq!(e.to_string(), "slot out of bounds: 10 (max 5)");
}

#[test]
fn test_error_display_custom() {
    let e = TabulaError::Custom("something went wrong".into());
    assert_eq!(e.to_string(), "something went wrong");
}
