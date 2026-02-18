# Tabula — Execution Plan

> Concrete implementation roadmap from current state to working proof system.
> Each task includes files to change, dependencies, and acceptance criteria.
> Aligned with proof-spec v0.9, semantics-spec v0.2.1, and architecture v0.4.5.

---

## Status Overview

| Milestone | Status | Description |
|-----------|--------|-------------|
| Phase 1 + 1.5 | ✅ DONE | Reference interpreter, overlay, DSL compiler, CLI, trait polish |
| M1: IR Housekeeping | ✅ DONE | SSA enforcement, CellKey order, Lookup rename, Select, Hash encoding |
| M2: 2-Slot Migration | ✅ DONE | 2-slot Read/Write, Value::Null removal, budgets, statement alignment |
| M3: NF Validation | ✅ DONE | NF-1~NF-4 compile-time enforcement, canonicalization passes |
| M4: Plonky3 Foundation | ✅ DONE | Poseidon2, BabyBear codec, SMT, SSMC, Hybrid VC |
| M5: Witness Generation | ✅ DONE | WitnessGenerator, BatchWitness, key routing, M5↔M6/M7 integration |
| M6: AIR Foundation | ✅ DONE | Chip/gadget patterns, ColumnMetaChip, debug checker, InteractionKind |
| M7: Gadgets + Memory Layer | ✅ DONE | Integer gadgets (U64Limbs, IsZero, StrictIneq), memory gadgets, GlobalSortedMemChip, RangeCheckChip |
| M8: Execution + Hashing | ✅ DONE | ExecutionChip, PoseidonChip, GlobalSSMCChip, GlobalMergeChip, ColumnMeta finalization |
| M9: LogUp Wiring | **Next** | Cross-chip LogUp buses, operand-slot linkage, hash chain constraints |

**Current state**: ~22,500 LOC across 7 crates, 512 tests, `cargo clippy` zero warnings.

### Structural changes since original plan

The original plan described M5 as "Single-Tx Proof" and M6 as "Batch Proof" with end-to-end proving. In practice, the work was restructured:

- **M5** became witness generation (executor → proof bridge) without end-to-end proving
- **M6** became AIR foundation (chip patterns, gadget library, ColumnMetaChip, debug checker)
- **M7-M8** were added for the remaining chips (not in original plan)
- **tabula-ir** was extracted as a new crate from tabula-core (not in original plan)
- End-to-end proving (original B5/C5) is deferred until after M9 (LogUp wiring)

The original task IDs (B1-B7, C1-C5) below are kept for traceability but their milestone grouping has changed.

---

## Completed Work

<details>
<summary>Phase 1 + 1.5 (click to expand)</summary>

- **tabula-core**: types, traits, events, errors, schema, mock
- **tabula-executor**: interpreter, overlay (ExecutionState + TraceRecorder), batch, consistency, resolve
- **tabula-commitment**: mock implementations (Blake3 hasher, InMemoryState)
- **tabula-proof**: statement types
- **tabula-lang**: full DSL compiler (lex → parse → lower), zero parser deps
- **tabula-cli**: JSON-based execute/inspect/example commands
- **T1**: `SigVerifier::verify` / `NoncePolicy::validate` → `Result<()>`
- **T2**: `Hasher::hash_many` default method
- **T3**: `StateSnapshot::column_len` removed

**Spec patches applied** (proof-spec v0.6.1 → v0.9, semantics-spec v0.2.1, architecture v0.4.5):

| Patch | Section |
|-------|---------|
| True SSA invariant | §6.2, §10.6 |
| (t,c) MUST + Lemma | §3.2 |
| Source enum 2-bit encoding | §4.2 merge |
| Integer encoding layer (u64 3-limb) | §4.2.R |
| Streaming commitment strategy | §4.2 SSMC |
| same_key zero-test gadget | §8.6 |
| Break-even model | §9 |
| Threshold TBD (100-300 est.) | §4.2, §10.1, §12 |
| Schema-Typed Digest-Native Two-Tier encoding | §10.3 |
| val_is_null / mem_is_null columns | §8.2, §8.3, §8.8 |
| LogUp fingerprint with val_is_null | §4.5, §8.4, §8.8 |
| architecture.md AppliedTxDigest + True SSA + ValueCodec | v0.4.2 |
| Read-cache semantics (read dedup via overlay) | §6.1 (v0.9) |
| SSA trace layout choice (Layout A/B) | §6.2 (v0.9) |
| Program budgets (max_ops, max_slots, max_accesses) | §6.2 (v0.9) |
| Overlay correctness split (state cells vs locals) | §6.3, §10.6 (v0.9) |
| Canonical hash semantics (Poseidon = hash_id) | §10.7 (v0.9) |
| Ok-gating canonical form (optional upgrade) | §6.3.5 (v0.9) |
| Document split: semantics-spec.md extracted from proof-spec | v0.1 (new doc) |
| NF rules as IR invariants; overlay removed from normative spec | semantics-spec v0.2 |
| IR type `T` (not `𝔽^{w(T)}`); canonical zero MUST; RowExpr equality §2.4 | semantics-spec v0.2.1 |
| CellKey struct notation (unordered fields) | proof-spec v0.9 |
| Overlay → NF language in proof-spec (§4.3, §7 title, §7.2) | proof-spec v0.9 |
| Hash input encoding §1.5.5; Select op §1.5.6; Batch semantics §2.2 | semantics-spec v0.2.1 |
| IR normative link §7; overlay intra/inter split §8.2; proof interface note | architecture v0.4.3 |
| NF-2 coalescing scope (intra vs inter-tx); Lookup null policy; Read/opening amortization | semantics-spec v0.2.1 |
| StaticTableRoot in §5.1; public-input checklist (incl/excl); Lookup verification pinned | proof-spec v0.9 |
| Stage 1/2/3 naming; receiptsDigest out-of-protocol note; Layer B/C disambiguation | architecture v0.4.4 |
| WriteSet terminology; Lookup access-row heading fix; budgets §1.7→§1.8; StaticTableRoot in §5.1 | proof-spec v0.9 |
| StaticTableRoot + budgets in ApplyBatchStatement; CellKey canonical order note | architecture v0.4.4 |
| Module listings, types, deps updated; Value::Null removed from §5.2; 2-slot R/W in §7.3 | architecture v0.4.5 |

</details>

<details>
<summary>M1 — IR Housekeeping (click to expand)</summary>

- **S1**: True SSA enforcement — `Program::register()` rejects duplicate dst slots
- **CK**: CellKey canonical order — fields reordered to `(table, col, row)` for `(t,c,r)` sort
- **LK**: Lookup field naming — `key` → `row`, fields reordered to `(dst, static_table, col, row)`
- **S4**: Select instruction — `Select(dst, cond, if_true, if_false)` in IR + interpreter + DSL
- **S5**: Hash input encoding — domain-tagged `hash_ir()` method on Hasher trait

</details>

<details>
<summary>M2 — 2-Slot Migration (click to expand)</summary>

- **S2**: Two-slot Read/Write + Value::Null removal
  - `Read { dst_val, dst_is_null, table, col, row }` — produces 2 SSA slots
  - `Write { table, col, row, src_val, src_is_null }` — takes 2 source exprs
  - `Value` enum: 4 variants (U64, I64, Bool, Bytes32) — no Null
  - Canonical zero enforced when `val_is_null = true`
- **BG**: ProgramBudgets — `max_ops`, `max_slots`, `max_accesses`
- **ST**: ApplyBatchStatement alignment — 6 public input fields

</details>

<details>
<summary>M3 — NF Validation (click to expand)</summary>

- **S3**: NF-1~NF-4 compile-time enforcement in `Program::register()`
  - NF-1 (Unique-Read): deduplicated via canonicalization pass
  - NF-2 (Unique-Write): validated
  - NF-3 (No-Read-After-Write): validated
  - NF-4 (Key-Alias Resolvability): validated per §2.5 RowExpr equality
- **tabula-ir** extracted as new crate with pass pipeline: canonicalize → typecheck → validate

</details>

<details>
<summary>M4 — Plonky3 Foundation (click to expand)</summary>

- **P1**: Plonky3 workspace deps (p3-field, p3-baby-bear, p3-poseidon2, p3-air, p3-matrix) behind `stark` feature
- **P2**: BabyBear ValueCodec — `field.rs` + `codec.rs` in tabula-commitment
- **P3**: Poseidon2 Hasher — `poseidon.rs` + `hasher.rs`, domain-separated sponge
- **P4**: SMT — `smt.rs`, 64-level with Poseidon2, membership/non-membership proofs
- **P5**: SSMC — `ssmc.rs`, sorted sparse list, merge trace, hash chain
- **P6**: Hybrid VC — `hybrid.rs`, per-column SSMC/SMT dispatch, two-level state root

</details>

<details>
<summary>M5 — Witness Generation (click to expand)</summary>

- **B1** (adapted): WitnessGenerator — ExecutionResult → BatchWitness
  - `witness/types.rs`: InitRow, AccessRow, ColumnWitness, BatchWitness
  - `witness/generator.rs`: value encoding, column grouping, state updates, root computation
  - `witness/route.rs`: KeyRoute (ReadOnly | ShortRun | SortedMemory), route_keys()
  - `witness/program_info.rs`: ProgramInfo, TemplateId, LiteralCell (types only)
  - 60+ tests including M5↔M6 (ColumnMeta AIR) and M5↔M7 (GlobalSortedMem AIR) integration

</details>

<details>
<summary>M6 — AIR Foundation (click to expand)</summary>

- AIR chip 3-file pattern: columns.rs / air.rs / trace.rs
- `air/columns.rs`: `#[repr(C)]` column struct + `borrow_cols()` zero-copy
- `air/bus.rs`: InteractionKind enum (8 variants: Memory, SsmcMembership, MergeCompleteness, ColumnMetaJoin, RangeCheck, ReadOnlyOpening, PoseidonPermutation)
- `air/debug.rs`: DebugConstraintBuilder — evaluate AIR constraints on concrete trace
- `air/gadgets/boolean.rs`: `constrain_is_real_prefix()`
- `air/chips/column_meta/`: ColumnMetaChip (25 cols) — lex ordering, boundary, is_touched consistency
- Design doc: air-chip-architecture.md

</details>

<details>
<summary>M7 — Gadgets + Memory Layer (click to expand)</summary>

- `air/gadgets/integer.rs`: U64Limbs (30+30+4 split), IsZero (inverse gadget), StrictIneq (gap decomposition)
- `air/gadgets/mem.rs`: null canonicality, mem_read, mem_write constraints
- `air/chips/sorted_mem/`: GlobalSortedMemChip<W> (32 cols, W=3) — segment-first init, memory R/W, ordering, write-set extraction
- `air/chips/range_check.rs`: RangeCheckChip (2 cols, 2^16 preprocessed table)
- Design doc: [archive/m8-design.md](./archive/m8-design.md)

</details>

<details>
<summary>M8 — Execution + Hashing (click to expand)</summary>

- `air/chips/execution/`: ExecutionChip<W> (118 cols, W=3, S=16) — 12 opcode one-hot, SSA slot carry, access log, arith limb carry
- `air/chips/poseidon/`: PoseidonChip (69 cols) — width-16 Poseidon2, S-box x^7, 21 rows/perm
- `air/chips/ssmc/`: GlobalSSMCChip<W> (27 cols) — sorted entries, hash chain accumulator, boundary flags
- `air/chips/merge/`: GlobalMergeChip<W> (34 cols) — 3-way merge, source encoding, in_new flag
- ColumnMeta finalization: is_touched consistency, empty→non-empty transition
- Design doc: [archive/m8-design.md](./archive/m8-design.md)

</details>

---

## M9 — LogUp Wiring (Next)

> Wire all chip interactions via LogUp cross-table arguments.
> This is the critical step between "chips with local constraints" and "a sound proof system."

### L1. LogUp Framework

**What**: Implement the LogUp running-sum argument infrastructure.

Each chip declares its interactions as `(kind, direction, fingerprint_columns, multiplicity)`. The framework accumulates running sums across all chips and asserts that send/receive totals match.

**Fingerprint**: Random-challenge linear combination `Φ(row) = Σ_j α^j · col_j` over interaction columns.

| Interaction | Send | Receive |
|-------------|------|---------|
| Memory | ExecutionChip (is_access rows) | GlobalSortedMemChip (non-init rows) |
| SsmcMembership | GlobalSortedMemChip (init rows) | GlobalSSMCChip |
| MergeCompleteness | GlobalSSMCChip + WriteSet | GlobalMergeChip |
| ColumnMetaJoin | Chips with (t,c) columns | ColumnMetaChip |
| RangeCheck | Execution/gadget limbs | RangeCheckChip |
| PoseidonPermutation | SSMC/Merge/Hash ops | PoseidonChip |

### L2. Operand-to-Slot Linkage

**What**: Constrain `src1_val`, `src2_val`, `cond_val` in ExecutionChip to match actual SSA slot values.

Currently these are unconstrained witness columns. Linkage options:
- **Direct column equality**: `src1_val[i] = slots[src1_idx][i]` (requires decode of src operand)
- **LogUp def-use**: (Layout B alternative, deferred)

### L3. Hash Chain Constraints

**What**: Constrain `hash_acc` in GlobalSSMC and GlobalMerge via PoseidonPermutation bus.

Currently `hash_acc` values are populated in trace but NOT constrained. Wiring them to PoseidonChip ensures commitment integrity.

### L4. Poseidon Round Constant Verification

**What**: Constrain `rc` witness columns in PoseidonChip against known round constants (preprocessed or verified via boundary constraints).

### Acceptance

- All 8 InteractionKind buses wired with correct multiplicities
- `debug_check()` extended to verify LogUp running-sum balance
- Operand values provably match slot contents
- Hash chain commitments provably correct
- All existing 177 proof tests pass + ~30 new LogUp tests

---

## Remaining Work (post-M9)

### End-to-End Proving

After M9, the constraint system is complete. Remaining steps for end-to-end proofs:

| Task | Original ID | Description |
|------|-------------|-------------|
| SmtPathChip | B4 | In-circuit SMT Merkle path verification |
| StarkProver | B5 | Wire constraints → Plonky3 STARK → proof generation |
| StarkVerifier | B5 | Proof verification |
| Proof chaining | B6 | `root0 → root1 → root2` via sequential proofs |
| Benchmark | B7 | SSMC vs SMT threshold calibration |

### Proof Optimizations

See [proof-optimization-architecture.md](./design/proof-optimization-architecture.md) for phases 2-4.

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1: Rename + Scaffold | KeyRoute, ProgramInfo types | ✅ DONE |
| Phase 2: §2+§3 Chips | ReadOnlyOpeningChip, ShortRunChip | Planned (after M9) |
| Phase 3: Template Chips | Transfer, ReadComputeWrite fused chips | Planned |
| Phase 4: Literal-Key Elision | Carry columns internal to templates | Planned |

---

## Parallel Tracks (independent of proof system)

> Can start after M3, run alongside proof system work.

### CB1. IR: Basic Block CFG

> See [conditional-branching.md](./research/conditional-branching.md)

**What**: Replace `TxTypeDef.body: Vec<Instruction>` with `TxBody` — CFG of `BasicBlock`s with `Terminator`s (Jump/Branch/Return/Abort). DAG-only.

**Scope**: `tabula-ir/src/instruction.rs`, `tabula-ir/src/tx.rs`, `tabula-executor/src/interpreter.rs`, `tabula-ir/src/program.rs`, `tabula-lang/src/lower.rs`, serialization, all tests.

### CB2. DSL: `if`/`else` Syntax

**What**: Add `if`/`else` to `tabula-lang`. Lower to CFG basic blocks.

**Depends on**: CB1.

### CLI1. Accept `.tab` Files

**What**: Detect file extension in `--program` flag. `.tab` → compile via `tabula-lang`.

**Scope**: `tabula-cli/Cargo.toml`, `commands/execute.rs`.

### DSL1-3. Compiler Improvements

- **DSL1**: Cross-type arithmetic validation at compile time
- **DSL2**: Error message formatting with `^~~~` column pointer
- **DSL3**: Example `.tab` files in `examples/`

---

## Deferred (not blocking proof system)

| Item | Why deferred |
|------|-------------|
| T4: PCS trait split | Only MockPCS exists. Do when second real impl starts. |
| T5: ColumnCommitment::to_bytes | Decide after hybrid VC proves out. |
| T6: ResourcePolicy trait | Needs proving cost estimates from B7. |
| T7: NonceStore trait | Resolves naturally if nonces move to system table. |
| T8: ConsistencyChecker trait | Only one strategy exists. |
| CB3: AIR constraints for block transitions | After proof system works for straight-line code. |
| BL1: Bounded loops | After CB1 (array params prerequisite). |
| Sponge optimization | SSMC hash chain → sponge. After B7 cost data. |

---

## Dependency Graph (updated)

```
✅ M1 — IR Housekeeping
✅ M2 — 2-Slot Migration
✅ M3 — NF Validation + tabula-ir extraction
✅ M4 — Plonky3 Foundation (P1-P6)
✅ M5 — Witness Generation (B1 adapted)
✅ M6 — AIR Foundation (chip patterns, ColumnMetaChip)
✅ M7 — Gadgets + Memory Layer (GlobalSortedMem, RangeCheck)
✅ M8 — Execution + Hashing (Execution, Poseidon, SSMC, Merge)
     │
     ▼
► M9 — LogUp Wiring (L1-L4)
     │
     ├──► SmtPathChip (B4)
     │
     ├──► End-to-End Proving (B5-B7)
     │
     └──► Proof Optimizations (Phase 2-4)

Parallel: CB1 → CB2, CLI1, DSL1-3
```

**Critical path**: M9 → B4 → B5 (end-to-end proof)
