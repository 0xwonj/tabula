# tabula-proof Code Quality Review

Date: 2026-02-15
Score: 8/10 → 9/10 (after fixes)

## Findings

### CRITICAL

| ID | Location | Issue | Status |
|----|----------|-------|--------|
| C1 | `air/columns.rs` | `borrow_cols` unsafe cast: no padding/alignment assertions | DONE — padding const assert + alignment debug_assert |

### HIGH

| ID | Location | Issue | Status |
|----|----------|-------|--------|
| H1 | `sorted_mem/air.rs` | `eval()` 286 lines — split into constraint group helpers | DONE — 9 private helpers + reconstruct_u64 |
| H2 | `sorted_mem/air.rs` + `integer.rs` | `shift_30`/`shift_60` duplicated — extract `expr_from_u32` helper | DONE — pub(crate) expr_from_u32 + SHIFT_30_U32 |
| H3 | `gadgets/integer.rs` | `StrictIneq.borrow0`/`borrow1` dead columns | DONE — removed, width 34→32 |
| H4 | `gadgets/integer.rs` | `IneqTestChip` doesn't call `constrain_strict_ineq()` | DONE — now calls the actual gadget |
| H5 | `gadgets/mem.rs` vs `sorted_mem/air.rs` | `constrain_mem_read`/`write` not wired to sorted_mem | WONTFIX — sorted_mem uses conditional gating (is_real × is_read × ...) incompatible with simple gadget API |
| H6 | `sorted_mem/trace.rs` | `generate_sorted_mem_trace()` 230 lines | DONE — extracted populate_ordering_witnesses() |

### MEDIUM

| ID | Location | Issue | Status |
|----|----------|-------|--------|
| M1 | `witness/generator.rs` | `generate()` 100 lines — extract `build_column_witnesses()` | DONE — extracted + ColumnWitnessResult type alias |
| M2 | `witness/generator.rs` | Null encoding duplicated | DONE — encode_value_with_null_flag() helper |
| M3 | `gadgets/mem.rs` | zip without length assertion | DONE — assert_eq! guards |
| M4 | trace generators | `bool → BabyBear` repeated 8+ times | DONE — bool_fe() in gadgets/mod.rs |
| M5 | `README.md` | Module layout references deleted files | DONE — full rewrite |
| M6 | `air/mod.rs` | Re-exports incomplete | DONE — added sorted_mem types |
| M7 | `sorted_mem/trace.rs` | No negative ordering tests | DONE — invalid_tau_regression + invalid_ordering_witness_corrupted |
| M8 | `sorted_mem/mod.rs` | `SortedMemRow`, `SORTED_MEM_STANDARD_WIDTH` not re-exported | DONE |
| M9 | `lib.rs` | `warn(missing_docs)` → `deny(missing_docs)` | SKIPPED — reverted by linter; kept as warn |
| M10 | `chips/mod.rs` | dispatch macro can't handle `BaseAir::width()` | WONTFIX — inherent Rust limitation (generic F param requires manual match) |

### LOW

| ID | Location | Issue | Status |
|----|----------|-------|--------|
| L1 | `integer.rs` | Magic mask `0x3FFF_FFFF` | DONE — const MASK_30 |
| L2 | `sorted_mem/air.rs` | Comment says "(7)" boolean fields | DONE — corrected to "(9)" |
| L3 | `sorted_mem/air.rs` | `both_real2` naming | DONE — eliminated by eval() split (single both_real passed to helpers) |
| L4 | `witness/types.rs` | `InitRow`/`AccessRow` missing `PartialEq` | DONE |
| L5 | `debug.rs` | Only reports first violation | DONE — debug_check_all() added |
| L6 | `route.rs` | `or_insert` correctness comment | DONE |
| L7 | `debug.rs` | Cyclic wrap semantics not commented | DONE |
| L8 | chip structs | Missing `#[derive(Debug)]` | DONE — all 4 chip structs + TabulaAir |
| L9 | `bus.rs` | `column_indices: Vec<usize>` → SmallVec | DEFER to M9 |
| L10 | `statement.rs` | Missing serde derives | DEFER to prover integration |
| L11 | `Cargo.toml` | `tabula-commitment` unconditional dep | DONE — optional, gated on stark |

## Summary

- **28 findings** total: 1 CRITICAL, 6 HIGH, 10 MEDIUM, 11 LOW
- **24 DONE**, 2 WONTFIX (with rationale), 1 SKIPPED, 2 DEFERRED
- Test count: 107 → 109 (+2 ordering negative tests)
- Zero clippy warnings, zero build errors
