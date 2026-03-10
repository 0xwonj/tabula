# ExecutionChip Evolution: From Universal to Program-Specific

> Status: Design
> Date: 2026-03-09
> Depends on: air-chip-architecture.md, proof-optimization-architecture.md, triton-codesign-analysis.md

---

## Context

The ExecutionChip is a 278-column universal AIR (W=3, MAX_SLOTS=16) that handles all 13 IR operations for every Tabula program. Its column layout breaks down as:

| Group | Columns | Description |
|-------|---------|-------------|
| Control | 3 | `is_real`, `tx_index`, `effect_ordinal_in_tx` |
| Opcode selectors | 20 | 12 one-hot + 2 arith sub-selectors + 6 cmp sub-selectors |
| Clock/access flags | 5 | `is_access`, `clk`, `is_empty_col`, `carry0`, `carry1` |
| Access log | 30 | t, c, r (KeyRangeChecked), is_write, val[3], is_null |
| Operand witness | 7 | src1_val[3], src2_val[3], cond_val |
| Operand-to-slot selectors | 49 | src1_sel[16], src2_sel[16], cond_sel[16], src1_is_null |
| SSA slots | 64 | slots[16][3], slot_is_null[16], slot_written[16] |
| Cmp witness | 27 | 6 sub-selectors, lt/eq witnesses, StrictIneq, LimbHalves, Limb2Bits, 3x IsZero |
| Hash permutation | 24 | hash_perm_input[16], hash_perm_output[8] |
| Mul carry | 5 | c0, c0_halves(2), c1_lo, c1_hi |
| DivMod witness | 36 | q_sel[16], c0, c0_halves(2), c1_lo, c1_hi, StrictIneq(5), 2x LimbHalves(4), Limb2Bits(4), IsZero(2) |
| **Total** | **278** | |

Every row carries all 278 columns regardless of which opcode it represents. A Read instruction pays for DivMod's 36 witness columns; a simple Add pays for Hash's 24 permutation columns. This universal layout is correct but wasteful.

The fundamental property that enables optimization: **Tabula programs are fixed at registration time.** `Program::register()` validates the instruction sequence, and `BodyTypeInfo` exposes slot types, `max_slot`, and the set of used opcodes at compile time. This is a structural advantage over general-purpose VMs (Triton, SP1, RISC Zero) where the program is unknown at circuit setup.

---

## IR-to-AIR Mapping: Where the Waste Lives

### Selector Overhead

The 12 one-hot opcode selectors plus 8 sub-selectors consume 20 columns. Compare with Triton VM's 7-bit decomposition approach: 7 selector columns, but degree-7 deselector polynomials push combined constraint degree to 19, requiring 130-230 auxiliary columns for degree lowering to degree 4. Tabula's one-hot approach keeps degree at most 4 without degree lowering, but pays 13 extra columns.

Neither approach is optimal when the program is known. A program using only `{Read, Write, Add, Assert}` needs 4 selectors (degree 1 gating, degree 3 combined), not 12 or 7.

### Operand-to-Slot Linkage

The `src1_sel[16]`, `src2_sel[16]`, `cond_sel[16]` arrays enforce that operand witness values match actual SSA slot contents. Each array is a one-hot selector over `MAX_SLOTS`. Three observations:

1. Only opcodes that read source operands need `src1_sel`. Assert, Not, And, Or, Arith, Cmp, DivMod, Select, Hash all use `src1_sel`, but Read, Write, and Lookup do not.
2. `cond_sel` is used only by Select. Every non-Select row wastes 16 columns.
3. A program with `max_slot=4` wastes 12 entries per selector array -- 36 columns total.

### SSA Slot Carry

Layout A (full carry) maintains the complete SSA state in every row: `slots[16][3]` (48 columns), `slot_is_null[16]`, `slot_written[16]`. The forward carry constraint copies the previous row's slot values unless the current instruction writes to that slot.

A program with `max_slot=4` wastes `12 * (3 + 1 + 1) = 60` slot columns per row. Furthermore, the carry constraint evaluates 16 slot-forwarding checks per row even when only 4 slots exist.

### Opcode-Specific Witness

The 92 columns dedicated to Cmp (27), Mul (5), DivMod (36), and Hash (24) are non-zero only for their respective opcode. In a batch of 1000 transactions with 20 instructions each, if only 5% of instructions are Cmp, then 95% of rows carry 27 dead Cmp columns. The waste is proportional to `(1 - opcode_frequency) * witness_width`.

---

## Design Principle: Exploit Program Knowledge

Tabula's invariant I5 (trusted compiler) guarantees that `Program::register()` validates the Normal Form at registration time. The compiler produces `BodyTypeInfo` containing:

- `max_slot`: highest SSA slot index used
- `slot_types`: type of each slot (Bool, U64, I64, Digest)
- Instruction sequence: the exact opcode list

This compile-time information feeds directly into circuit specialization. The evolution proceeds through four levels of increasing specialization, each building on the previous and compatible with the existing LogUp bus architecture.

---

## Level 0: Universal ExecutionChip (Current)

The current design. All programs use the same 278-column layout. The PCS commits to all columns for every row. Constraint evaluation checks all opcode groups.

This is the correct baseline: it handles any valid Tabula program with a single circuit. Its cost is proportional to `num_instructions * 278`, regardless of program complexity.

---

## Level 1: Constraint Mask

At `Program::register()`, a `ConstraintMask` records which opcode groups appear in the program:

```rust
struct ConstraintMask {
    has_arith: bool,
    has_cmp: bool,
    has_mul: bool,
    has_divmod: bool,
    has_hash: bool,
    has_select: bool,
    has_lookup: bool,
    has_logic: bool,    // Not, And, Or
}
```

In `eval()`, each opcode-specific constraint block is gated by its mask flag. When `has_cmp` is false, the 27 Cmp witness columns exist but are constrained to be zero (or the constraint block is simply skipped since the opcode selector is constrained to zero by the one-hot sum).

**Column layout**: Unchanged. The PCS still commits to 278 columns per row. The savings come exclusively from constraint evaluation: the verifier and prover skip constraint polynomial evaluation for absent opcode groups.

**Estimated savings**: 15-25% reduction in constraint evaluation cost, depending on program complexity. Zero reduction in PCS commitment cost.

**Architectural impact**: None. The `ConstraintMask` is a prover/verifier optimization that does not change the AIR, the trace layout, or the bus interactions. It is compatible with all existing infrastructure.

---

## Level 2: Column Subsetting

Column subsetting generates a program-specific column layout where unused opcode groups and excess slots are physically removed from the trace.

### Dynamic MAX_SLOTS

Replace the global `MAX_SLOTS = 16` with a per-program `max_slot` derived from `BodyTypeInfo`. The column savings are:

| Component | Per-excess-slot savings | max_slot=4 savings |
|-----------|----------------------|-------------------|
| Slot values (`slots[s][W]`) | 3 columns | 36 columns |
| Slot null flags (`slot_is_null[s]`) | 1 column | 12 columns |
| Slot written flags (`slot_written[s]`) | 1 column | 12 columns |
| `src1_sel[s]` | 1 column | 12 columns |
| `src2_sel[s]` | 1 column | 12 columns |
| `cond_sel[s]` | 1 column | 12 columns |
| **Total per excess slot** | **8 columns** | **96 columns** |

A program with `max_slot=4` saves 96 columns from slot subsetting alone.

### Opcode Group Removal

When an opcode group is absent from the program, its witness columns and sub-selectors are removed entirely:

| Absent opcodes | Columns removed |
|---------------|----------------|
| Cmp (all 6 sub-ops) | 27 (CmpWitness) + 6 (sub-selectors in CmpWitness) |
| Mul | 5 (MulCarry) + 1 (arith_is_mul selector) |
| DivMod | 36 (DivModWitness) + 1 (op_divmod selector) |
| Hash | 24 (hash_perm_input + hash_perm_output) + 1 (op_hash selector) |
| Select | 16 (cond_sel) + 1 (cond_val) + 1 (op_select selector) |
| Lookup | 1 (op_lookup selector) |

**Combined example**: A transfer program using `{Read, Write, Add, Assert}` with `max_slot=4`:
- Slot subsetting: -96 columns
- Cmp removal: -27 columns
- Mul removal: -5 columns (arith_is_mul remains if Add/Sub present)
- DivMod removal: -37 columns
- Hash removal: -25 columns
- Select removal: -18 columns (cond_sel already counted in slot subsetting)
- Lookup removal: -1 column
- Unused selectors: -8 columns (op_cmp, op_divmod, op_not, op_and, op_or, op_select, op_hash, op_lookup)
- **Result**: 278 - 96 - 27 - 5 - 37 - 25 - 2 - 1 - 8 = ~77 columns

### Implementation Strategy

Column subsetting requires generating per-program column structs. Two approaches:

**Const generics**: Parameterize `ExecutionCols<T, W, S, HAS_CMP, HAS_MUL, ...>` with const bool flags. Rust's const generics support this pattern, but the combinatorial explosion of flag combinations makes monomorphization expensive.

**Code generation**: A `build.rs` or proc-macro generates a specialized `ExecutionCols` for each registered program. The generated struct includes only the relevant fields, and the generated `eval()` includes only the relevant constraint blocks. This approach scales linearly with the number of distinct programs.

**ChipSpec impact**: Each program produces a different `ExecutionChip` with a different `trace_width()`. The `ChipRegistry` and `TabulaMachine` must handle per-program chip widths. The `ChipId::Execution` variant carries a program identifier, or the registry maps programs to their specialized chip instances.

**Bus compatibility**: Column subsetting does not change LogUp bus fingerprints. The Memory bus still carries `(t, c, r, tau, is_write, val, val_is_null)` regardless of which columns are present in the execution trace. The fingerprint is computed from the operand values, not the column indices.

---

## Level 3: Coprocessor Factoring

Coprocessor factoring extracts witness-heavy opcodes into dedicated chips connected via LogUp buses. The ExecutionChip retains control flow, SSA state, and lightweight opcodes; each coprocessor handles its own witness columns with a trace height proportional to actual usage.

### Architecture

```
ExecutionChip (~100 cols)
├── Control: is_real, tx_index, effect_ordinal_in_tx
├── Selectors: reduced set (opcodes present in program)
├── Clock/access: is_access, clk, is_empty_col, carry0, carry1
├── Access log: t, c, r, is_write, val, is_null
├── Operand witness: src1_val, src2_val
├── Operand-to-slot selectors: src1_sel, src2_sel (no cond_sel)
├── SSA slots: slots[max_slot][W], slot_is_null, slot_written
├── Lightweight opcodes inline: Add/Sub carry, Not/And/Or, Assert
└── Delegation stubs: bus send for Cmp, Mul, DivMod, Hash, Select

CmpChip (~30 cols, height = num_cmp_instructions)
├── src1_val, src2_val (or received via bus)
├── CmpWitness (27 cols)
└── Result: single boolean

MulChip (~12 cols, height = num_mul_instructions)
├── src1_val, src2_val
├── MulCarry (5 cols)
└── Result: product value

DivModChip (~25 cols, height = num_divmod_instructions)
├── lhs_val, rhs_val
├── Carry chain, StrictIneq, IsZero
└── Result: quotient + remainder values

HashDelegationChip: absorbed into existing PoseidonChip
├── ExecutionChip sends slot values via PoseidonPerm bus (already exists)
├── 24 inline columns (hash_perm_input/output) removed from ExecutionChip
└── PoseidonChip receives and processes as before
```

### Bus Definitions

Each coprocessor connects via a new LogUp bus:

| Bus | Sender | Receiver | Fingerprint |
|-----|--------|----------|-------------|
| CmpDelegate | ExecutionChip | CmpChip | `(src1_val, src2_val, cmp_sub_op)` |
| CmpResult | CmpChip | ExecutionChip | `(row_id, result_bool)` |
| MulDelegate | ExecutionChip | MulChip | `(src1_val, src2_val)` |
| MulResult | MulChip | ExecutionChip | `(row_id, product_val)` |
| DivModDelegate | ExecutionChip | DivModChip | `(lhs_val, rhs_val)` |
| DivModResult | DivModChip | ExecutionChip | `(row_id, quotient_val, remainder_val)` |

The `row_id` (or equivalent unique identifier such as `tx_index || clk`) binds each delegation to its result, preventing the prover from mismatching results across rows.

### Trace Height Savings

In a batch of 1000 transactions with 20 instructions each (20,000 total rows), if Cmp appears in 10% of instructions:
- Without coprocessors: CmpWitness occupies 27 columns x 20,000 rows = 540,000 cells
- With coprocessors: CmpChip has 30 columns x 2,048 rows (next power of two above 2,000) = 61,440 cells

The savings grow with opcode rarity. For a DivMod that appears in 1% of instructions: 36 cols x 20,000 rows = 720,000 cells reduced to 25 cols x 256 rows = 6,400 cells.

### Relationship to Existing Coprocessors

Tabula already uses the coprocessor pattern for three chips:

- **PoseidonChip** (bus 5): Receives permutation requests, 93 columns, height proportional to hash count
- **RangeCheckChip** (bus 8): Preprocessed lookup table, 2 columns
- **StaticTableChip** (bus 9): Static table lookups

Level 3 extends this proven pattern to arithmetic operations. The bus protocol is identical: LogUp with shared challenge pair (alpha, beta) over BabyBear^4.

---

## Level 4: Template AIR (Program-Specific Circuit)

Template AIR generation produces a complete program-specific circuit at registration time. The instruction sequence is known, so opcode dispatch, SSA carry, and slot forwarding are eliminated entirely.

### How It Works

Each instruction in the program becomes a fixed row (or group of rows) in the trace with a known column meaning. There are no opcode selectors because the row's opcode is a compile-time constant embedded in the constraint polynomial.

For a transfer program:

```
Row 0: Read(s0, s1, t0, c0, Param(0))    →  columns: key, old_val, old_null
Row 1: Read(s2, s3, t0, c0, Param(1))    →  columns: key, old_val, old_null
Row 2: Sub(s4, s0, Param(2))             →  columns: (none, constraint on val)
Row 3: Add(s5, s2, Param(2))             →  columns: (none, constraint on val)
Row 4: Assert(Cmp(Gte, s4, 0))           →  columns: ineq witness
Row 5: Write(t0, c0, Param(0), s4, s1)   →  columns: key, new_val, new_null
Row 6: Write(t0, c0, Param(1), s5, s3)   →  columns: key, new_val, new_null
```

Each row's constraints are specific to its instruction. Row 2 constrains `val[row2] = val[row0] - param[2]` directly -- no selector multiplication, no SSA carry from a previous row's slot array.

### Constraint Degree Reduction

Without selector multiplication, the maximum constraint degree drops:

| Constraint type | Universal (Level 0) | Template (Level 4) |
|----------------|--------------------|--------------------|
| Arithmetic (Add/Sub) | degree 2 (`op_arith * (dst - src1 - src2)`) | degree 1 (`dst - src1 - src2`) |
| Cmp ordering | degree 3 (`op_cmp * is_lt * ineq_constraint`) | degree 2 (`is_lt * ineq_constraint`) |
| Slot carry | degree 2 (`(1 - slot_written) * (slot - slot_prev)`) | degree 0 (eliminated) |
| Access constraint | degree 2 (`is_access * (...)`) | degree 1 (no gating) |

Lower degree means fewer FRI queries for the same security level, or equivalently, a smaller blowup factor.

### Column Width

For the transfer program above: ~60-80 columns total, compared to 278 at Level 0 or ~77 at Level 2. The remaining columns are:

- Transaction parameters (keys, amount): ~9 columns
- Read values (2 reads x 4 = val + null): ~8 columns
- Computed values (sub result, add result): ~6 columns
- Write values (carried from computed): ~0 (reuse computed columns)
- Assert witness (Cmp + StrictIneq): ~10 columns
- Access log fields (t, c, r, is_write per access): ~24 columns
- Control (is_real, tx_index, clk): ~3 columns

The exact width depends on column reuse opportunities across rows.

### Implementation

Template AIR generation is an IR-to-AIR compiler. It takes a validated `TxTypeDef` and produces:

1. A column struct with exactly the needed fields
2. An `eval()` function with per-row constraint expressions
3. A trace builder that populates the matrix from execution results
4. LogUp bus interactions matching the universal ExecutionChip's protocol

The generated chip emits identical bus fingerprints to the universal ExecutionChip. GlobalSortedMem, GlobalSSMC, GlobalMerge, and ColumnMeta operate identically regardless of whether events originate from a template chip or the universal chip.

This design is described in proof-optimization-architecture.md Section 3 (Template Chips), where the TransferTemplate serves as the reference implementation pattern.

### Mixed-Mode Execution

A batch containing multiple transaction types can use template chips for some types and the universal ExecutionChip for others. The `ChipRegistry` holds both the template chip (for type A) and the universal chip (for type B). Both emit to the same Memory bus. LogUp balance is maintained across all chips.

---

## Type-Aware Constraint Elision

Orthogonal to the level hierarchy, type information from `BodyTypeInfo` enables constraint elision within any level.

### Width-Class Specialization

The value encoding uses type-dependent widths: `w(Bool) = 1`, `w(U64) = w(I64) = 3`, `w(Digest) = 8`. When `BodyTypeInfo` reveals that slot `s` holds a Bool:

- Slot value columns: 1 instead of W (saves 2 columns per Bool slot)
- Range check sends: none (Bool is a single constrained bit)
- Null flag: if the slot is the output of a comparison or logic op, `slot_is_null[s]` is provably always 0 and can be constrained as constant

### Never-Null Elision

Outputs of Arith, Cmp, Not, And, Or, Select, and Hash are never null (the operations are total over non-null inputs, and null propagation is handled by Assert). For these slots, `slot_is_null` contributes a constant-zero column that need not be committed.

### Range-Bounded Carry Elision

When abstract interpretation over the program's value ranges proves that an arithmetic operation cannot overflow a single BabyBear limb, the carry columns (`carry0`, `carry1`) are provably zero and can be elided. For example, adding a Bool (0 or 1) to a U64 whose range is known to be below `2^30 - 1` produces no carry.

### Algebraic Identities

The one-hot constraint `sum(op_i) = 1` implies `op_i * op_j = 0` for `i != j`. Cross-opcode constraint terms that contain such products are algebraically zero and can be eliminated during constraint compilation. This is a form of dead constraint elimination enabled by the selector structure.

---

## Comparison with Triton VM

| Metric | Triton VM | Tabula Level 0 | Tabula Level 2 | Tabula Level 4 |
|--------|-----------|----------------|----------------|----------------|
| Opcodes supported | 47 (universal) | 13 (universal) | Per-program subset | Per-program, no dispatch |
| Selector encoding | 7-bit decomposition | 12-way one-hot | Subset one-hot | None (fixed rows) |
| Combined constraint degree | 19 (before lowering) | 4 | 4 | 3 |
| Degree lowering columns | 130-230 | 0 | 0 | 0 |
| Total trace width (transfer) | ~643 (degree 4) | 278 | ~77 | ~60-80 |
| Program knowledge | None | Full (unused) | Partial exploitation | Full exploitation |
| Per-opcode coprocessors | Hash Table, U32 Table | PoseidonChip, RangeCheckChip | Same | Same + template fusion |

Triton's architectural constraints stem from its role as a general-purpose VM: it must support any program with a single universal constraint set. Bit decomposition, deselector polynomials, and degree lowering are necessary consequences of universality over a large opcode space.

Tabula's program-specific knowledge makes three of Triton's core techniques unnecessary:

1. **Bit decomposition**: Not needed because the opcode set is known and small per program.
2. **Degree lowering**: Not needed because one-hot selectors (or no selectors at Level 4) keep degree at most 4.
3. **Universal constraint set**: Not needed because the constraint set is generated per program.

The cost Tabula pays is per-program circuit generation and key generation. This cost is amortized over all batches that execute the same program -- a favorable tradeoff for programs that process many transactions.

---

## Recommended Evolution Path

**Level 1 first.** Constraint masking requires no architectural change. It is a localized optimization in `eval()` gated by a `ConstraintMask` computed at registration. Immediate benefit with zero risk to existing infrastructure.

**Level 2 next.** Column subsetting delivers the highest return on investment. Slot subsetting alone (dynamic `MAX_SLOTS`) saves up to 96 columns, and the implementation is a straightforward parameterization of the existing column struct. Opcode group removal follows naturally. The main engineering cost is adapting `ChipRegistry` to handle per-program chip widths.

**Level 3 for production workloads.** Coprocessor factoring is the proven pattern from Triton (Hash Table, U32 Table) and SP1 (precompiles). It eliminates the fixed cost of carrying witness columns for rare opcodes. The existing PoseidonChip and RangeCheckChip demonstrate that Tabula's LogUp bus infrastructure supports this pattern.

**Level 4 for high-throughput transaction types.** Template AIR generation delivers the maximum reduction but requires building an IR-to-AIR compiler. This investment is justified for transaction types that dominate batch volume (e.g., transfers). Less-common types can remain on the universal chip or Level 2/3 variants.

---

## Relationship to Other Optimizations

**Constraint CSE** (compiler-optimization-research.md Section 3): Common subexpression elimination over the constraint DAG benefits all levels. A smaller column set produces a smaller DAG, making CSE more effective. At Level 4, the absence of selector products simplifies the DAG structure.

**Prover pipeline acceleration** (proof-optimization-architecture.md): The prover pipeline (NTT, FRI, commitment) is orthogonal to ExecutionChip evolution. All levels produce standard polynomial commitment inputs. Pipeline optimizations (parallelism, GPU offloading) compose with any level.

**Memory-layer optimization** (proof-optimization-architecture.md Section 2): KeyRoute classification and ReadOnlyOpeningChip/ShortRunChip are independent of execution-layer specialization. Template chips emit the same LogUp bus fingerprints as the universal chip, so memory-layer optimizations compose without interference.

**Full sharding** (sharded-protocol-design.md): Per-column sharding is compatible with all levels. Each shard's execution sub-proof uses whatever ExecutionChip variant is appropriate for the program.

---

## References

- `docs/research/triton-codesign-analysis.md` -- Sections 1, 3, 5, 6
- `docs/research/compiler-optimization-research.md` -- Sections 1, 3, 4
- `docs/design/proof-optimization-architecture.md` -- Section 3 (Template Chips)
- `docs/design/air-chip-architecture.md` -- Chip complexity table, code organization
- `crates/chips/src/execution/columns.rs` -- Current `ExecutionCols` struct
- OpenVM "no-CPU" architecture (external)
