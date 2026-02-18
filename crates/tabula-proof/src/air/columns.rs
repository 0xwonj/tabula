//! Column struct utilities: zero-copy borrow of flat slices as typed column structs.
//!
//! Uses the SP1/Valida `AlignedBorrow` pattern: a `#[repr(C)]` struct of `T` fields
//! is safely reinterpreted from a `&[T]` slice of matching length.

use core::mem;

/// Safely borrow a `&[T]` as `&C` where `C` is a `#[repr(C)]` column struct.
///
/// # Panics
/// Panics if the slice length does not equal `num_cols::<C, T>()`.
///
/// # Safety justification
/// - `C` must be `#[repr(C)]` (caller's responsibility — enforced by convention).
/// - Size check guarantees the slice covers exactly `size_of::<C>()` bytes.
/// - `T` has no alignment requirement beyond what `[T]` already satisfies.
pub fn borrow_cols<T, C>(slice: &[T]) -> &C {
    let expected = num_cols::<C, T>();
    assert_eq!(
        slice.len(),
        expected,
        "borrow_cols: expected {} elements, got {}",
        expected,
        slice.len()
    );
    // SAFETY: `#[repr(C)]` guarantees field layout matches flat slice of T.
    // Size assertion above ensures slice covers exactly `size_of::<C>()` bytes.
    // Alignment: for `#[repr(C)]` structs whose fields are all `T`, align(C) == align(T).
    assert_eq!(
        mem::align_of::<C>(),
        mem::align_of::<T>(),
        "borrow_cols: alignment mismatch between struct and element"
    );
    unsafe { &*(slice.as_ptr() as *const C) }
}

/// Safely borrow a `&mut [T]` as `&mut C` where `C` is a `#[repr(C)]` column struct.
///
/// # Panics
/// Panics if the slice length does not equal `num_cols::<C, T>()`.
///
/// # Safety justification
/// Same as [`borrow_cols`], but for exclusive references.
pub fn borrow_cols_mut<T, C>(slice: &mut [T]) -> &mut C {
    let expected = num_cols::<C, T>();
    assert_eq!(
        slice.len(),
        expected,
        "borrow_cols_mut: expected {} elements, got {}",
        expected,
        slice.len()
    );
    // SAFETY: see `borrow_cols`.
    assert_eq!(
        mem::align_of::<C>(),
        mem::align_of::<T>(),
        "borrow_cols_mut: alignment mismatch between struct and element"
    );
    unsafe { &mut *(slice.as_mut_ptr() as *mut C) }
}

/// Number of `T`-typed fields in a `#[repr(C)]` column struct `C`.
///
/// # Panics
/// Panics (at const-eval or runtime) if `T` is a ZST or if `C` has padding.
pub const fn num_cols<C, T>() -> usize {
    assert!(
        mem::size_of::<T>() > 0,
        "ZST not supported as column element"
    );
    assert!(
        mem::size_of::<C>().is_multiple_of(mem::size_of::<T>()),
        "column struct has padding — all fields must be T or [T; N]"
    );
    mem::size_of::<C>() / mem::size_of::<T>()
}
