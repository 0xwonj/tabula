use tabula_stark::air::{borrow_cols, num_cols};

#[repr(C)]
struct TestCols<T> {
    a: T,
    b: T,
    c: [T; 3],
}

#[test]
fn num_cols_matches_fields() {
    assert_eq!(num_cols::<TestCols<u32>, u32>(), 5);
}

#[test]
fn borrow_cols_correct_size() {
    let data: Vec<u32> = vec![1, 2, 3, 4, 5];
    let cols: &TestCols<u32> = borrow_cols(&data);
    assert_eq!(cols.a, 1);
    assert_eq!(cols.b, 2);
    assert_eq!(cols.c, [3, 4, 5]);
}

#[test]
#[should_panic(expected = "borrow_cols: expected 5 elements, got 3")]
fn borrow_cols_wrong_size_panics() {
    let data: Vec<u32> = vec![1, 2, 3];
    let _: &TestCols<u32> = borrow_cols(&data);
}
