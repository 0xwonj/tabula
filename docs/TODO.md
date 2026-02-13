# Tabula — Execution Plan

> Concrete implementation roadmap from current state to working proof system.
> Each task includes files to change, dependencies, and acceptance criteria.
> Aligned with proof-spec v0.9, semantics-spec v0.2.1, and architecture v0.4.4.

---

## Status Overview

| Milestone | Status | Description |
|-----------|--------|-------------|
| Phase 1 + 1.5 | ✅ DONE | Reference interpreter, overlay, DSL compiler, CLI, trait polish |
| M1: IR Housekeeping | ✅ DONE | SSA enforcement, CellKey order, Lookup rename, Select, Hash encoding |
| M2: 2-Slot Migration | ✅ DONE | 2-slot Read/Write, Value::Null removal, budgets, statement alignment |
| M3: NF Validation | ✅ DONE | NF-1~NF-4 compile-time enforcement |
| M4: Plonky3 Foundation | Planned | Poseidon2, BabyBear codec, SMT, SSMC, Hybrid VC |
| M5: Single-Tx Proof | Planned | Witness generator, AIR, constraints, end-to-end Phase B proof |
| M6: Batch Proof | Planned | GlobalSortedMem, LogUp, write coalescing, end-to-end Phase C proof |

**Current state**: ~9,400 LOC across 6 crates, 203 tests, `cargo clippy` zero warnings.

---

## Completed Work

<details>
<summary>Phase 1 + 1.5 (click to expand)</summary>

- **tabula-core** (1,433 LOC): types, IR, traits, events, errors, schema
- **tabula-executor** (2,909 LOC): interpreter, overlay, batch, consistency, resolve, program
- **tabula-commitment** (958 LOC): mock implementations for all traits (Blake3 hasher, MockPCS, InMemoryState)
- **tabula-proof** (116 LOC): statement types, mock prover/verifier (stubs for Phase 2)
- **tabula-lang** (3,304 LOC): full DSL compiler (lex → parse → lower), zero parser deps
- **tabula-cli** (674 LOC): JSON-based execute/inspect/example commands
- **T1**: `SigVerifier::verify` / `NoncePolicy::validate` → `Result<()>`
- **T2**: `Hasher::hash_many` default method
- **T3**: `StateSnapshot::column_len` removed

**Spec patches applied** (proof-spec v0.6.1 → v0.9, semantics-spec v0.2.1, architecture v0.4.4):

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

</details>

---

## M1 — IR Housekeeping

> Clean up all independent spec divergences before the M2 big migration.
> All 5 tasks are independent and can be done in parallel.
> Rationale: resolving these first reduces the file-touch count during M2.

### S1. True SSA Enforcement

**What**: Enforce that each destination slot index appears at most once in a transaction body. Multi-result instructions (`Read` dst_val/dst_is_null, `DivMod` dst_q/dst_r) must have distinct destination slots.

**Why**: Spec (§6.2, §10.6) claims "IR is SSA, therefore no intra-tx memory argument is needed." Current code allows slot reuse via `set_slot`. Without enforcement, SSA claim is vacuous.

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/ir.rs` | Add `validate_ssa(instructions: &[Instruction]) -> Result<(), TabulaError>` |
| `tabula-executor/src/program.rs` | Call `validate_ssa()` in `Program::register()` |
| `tabula-lang/src/lower.rs` | Add test asserting lowered IR is SSA |
| Tests | `[Read{dst:0,...}, Add{dst:0,...}]` rejected; 203 existing tests unchanged |

**Acceptance**:
- `Program::register()` rejects duplicate dst slots
- All existing tests pass (existing programs are already SSA)
- `DivMod { dst_q: 0, dst_r: 0 }` rejected (multi-result distinctness)

**Depends on**: Nothing.

---

### CK. CellKey Canonical Order

**What**: Fix `CellKey` field order from `(table, row, col)` to `(table, col, row)` — canonical `(t, c, r)`.

**Why**: Current `#[derive(Ord)]` produces sort order `(t, r, c)`. Spec requires `(t, c, r)` for GlobalSortedMem sorting, Poseidon domain hashing, and Merkle leaf encoding. Fixing this now avoids a painful double-migration during M2.

**Current**:
```rust
pub struct CellKey {
    pub table: TableId,
    pub row: RowKey,     // ← second
    pub col: ColId,      // ← third
}
// derive(Ord) → sorts (t, r, c)
```

**Target**:
```rust
pub struct CellKey {
    pub table: TableId,
    pub col: ColId,      // ← second
    pub row: RowKey,     // ← third
}
// derive(Ord) → sorts (t, c, r) ✓
```

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/types.rs` | Reorder `CellKey` fields to `(table, col, row)` |
| All call sites | Update struct literal field order (compiler will catch all) |

**Acceptance**:
- `CellKey { table: t(0), col: c(1), row: r(10) } < CellKey { table: t(0), col: c(2), row: r(5) }` (col before row in sort)
- All existing tests pass
- `BTreeMap<CellKey, _>` iteration order is `(t, c, r)` lexicographic

**Depends on**: Nothing.

---

### LK. Lookup Field Naming

**What**: Rename `Lookup::key` to `Lookup::row` for consistency with Read/Write and the spec convention `Lookup(dst, static_table, c, r)`.

**Current**:
```rust
Lookup { dst: Slot, static_table: TableId, key: RowExpr, col: ColId }
```

**Target**:
```rust
Lookup { dst: Slot, static_table: TableId, col: ColId, row: RowExpr }
```

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/ir.rs` | Rename field `key` → `row`, reorder fields |
| `tabula-executor/src/interpreter.rs` | Update pattern match |
| `tabula-executor/src/program.rs` | Update validation |
| `tabula-lang/src/lower.rs` | Update IR emission |
| Tests | Update struct literals |

**Acceptance**:
- Field name is `row: RowExpr`, not `key: RowExpr`
- All existing tests pass

**Depends on**: Nothing.

---

### S4. Select Instruction

**What**: Add `Select` instruction to the IR, interpreter, and DSL compiler.

**Semantics** (semantics-spec §1.5.6):
```
Select(dst: Slot, cond: ValueExpr(Bool), if_true: ValueExpr(T), if_false: ValueExpr(T))
```
Constraint: `dst = cond · if_true + (1 - cond) · if_false`

**Why**: Required for canonical-zero enforcement on dynamic deletes, ok-gating, and future `if/else` lowering.

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/ir.rs` | Add `Select { dst, cond, if_true, if_false }` variant |
| `tabula-executor/src/interpreter.rs` | Execute: evaluate cond, pick branch |
| `tabula-executor/src/program.rs` | SSA validation, type checking (cond=Bool, if_true/if_false same type T) |
| `tabula-lang/src/parser.rs` | Parse `select(cond, a, b)` expression |
| `tabula-lang/src/lower.rs` | Lower to `Select` instruction |
| Tests | Unit + integration + DSL round-trip |

**Acceptance**:
- `Select` with `cond=true` returns `if_true`; `cond=false` returns `if_false`
- Type mismatch between `if_true`/`if_false` rejected at registration
- Non-Bool `cond` rejected at registration
- DSL: `let x = select(flag, a, b)` compiles and executes correctly

**Depends on**: Nothing.

---

### S5. Hash Input Encoding

**What**: Align the IR `Hash` instruction with the normative input encoding from semantics-spec §1.5.5.

**Normative encoding**:
```
Hash(inputs) → Poseidon(domain_tag_hash || n || ComEnc(x_0) || ... || ComEnc(x_{n-1}))
```
where `n` = number of inputs, `ComEnc` = Tier 1 value encoding.

**Current**: The interpreter uses `borsh_encode_value()` → `hash_many()` with raw bytes. No domain tag. No length prefix. No ComEnc.

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/traits.rs` | Add `hash_ir(inputs: &[Value]) -> Digest` with default impl using `hash_many` |
| `tabula-executor/src/interpreter.rs` | Use `hash_ir()` for the `Hash` instruction |

**Acceptance**:
- `Hash` instruction uses length-prefixed, domain-tagged encoding
- Mock hasher: borsh encoding as before (compatible default impl)
- Future Poseidon hasher will use native BabyBear ComEnc per §1.5.5

**Depends on**: Nothing.

---

## M2 — 2-Slot Migration

> The core architectural alignment. Reconciles the biggest divergence between code and spec.
> All S2 sub-tasks are atomic — executed as a single coordinated change.
> BG and ST are bundled here because they touch the same files.

### S2. Two-Slot Read/Write + Value::Null Removal

**What**: Migrate Read/Write from single-slot form to the normative two-slot form. Remove `Value::Null` from the value enum — null is a separate boolean flag, not a value type.

**Current** (implementation):
```rust
Read { dst: Slot, table: TableId, row: RowExpr, col: ColId }
Write { table: TableId, row: RowExpr, col: ColId, src: ValueExpr }
// Value::Null represents absence
```

**Target** (semantics-spec §1.5.1–§1.5.2):
```rust
Read { dst_val: Slot, dst_is_null: Slot, table: TableId, col: ColId, row: RowExpr }
Write { table: TableId, col: ColId, row: RowExpr, src_val: ValueExpr, src_is_null: ValueExpr }
// Value has 4 variants only: U64, I64, Bool, Bytes32
// dst_val MUST be canonical zero when dst_is_null = true
```

**Sub-tasks** (executed atomically):

| Sub-task | What |
|----------|------|
| **S2a** | Remove `Value::Null` variant. `Value` becomes 4-variant enum. Add `fn zero_value(ty: ValueType) -> Value` canonical zero constructor. |
| **S2b** | Update `Instruction::Read` to `{ dst_val, dst_is_null, table, col, row }`. |
| **S2c** | Update `Instruction::Write` to `{ table, col, row, src_val, src_is_null }`. Delete = `src_is_null=true` + `src_val=canonical_zero`. |
| **S2d** | Update `Overlay::read()` return type to `(Value, bool)`. Enforce canonical zero when absent. |
| **S2e** | Add `val_is_null: bool` field to `ExecutionEvent`. Update consistency checker. |
| **S2f** | Update lowerer to emit two-slot IR. Allocate `dst_is_null` slot for each Read. Emit `src_is_null` for each Write. |
| **S2g** | Update CLI JSON types (`ProgramFile`, `StateFile`, `ExecutionOutput`). |

**Changes** (all files touched):

| File | What |
|------|------|
| `tabula-core/src/types.rs` | Remove `Value::Null`, add `zero_value()` |
| `tabula-core/src/ir.rs` | Update `Read` and `Write` variants |
| `tabula-core/src/event.rs` | Add `val_is_null: bool` to `ExecutionEvent` |
| `tabula-executor/src/interpreter.rs` | Two-slot execute loop |
| `tabula-executor/src/resolve.rs` | Remove Null handling from expression resolution |
| `tabula-executor/src/program.rs` | SSA validates both dst slots; type-check `src_is_null` as Bool |
| `tabula-executor/src/overlay.rs` | `read()` → `Result<(Value, bool), TabulaError>` |
| `tabula-executor/src/batch.rs` | Update event construction |
| `tabula-executor/src/consistency.rs` | Account for `val_is_null` in consistency checks |
| `tabula-lang/src/lower.rs` | Emit two-slot IR |
| `tabula-cli/src/io.rs` | Update JSON serialization |
| `tabula-cli/src/commands/execute.rs` | Update output formatting |
| `tabula-commitment/src/mock.rs` | Update `InMemoryState::read()` |
| All test files | Update IR construction |

**Acceptance**:
- `Value` enum has exactly 4 variants (U64, I64, Bool, Bytes32) — no Null
- `Read` produces two distinct slots (validated by SSA)
- `Write` with `src_is_null=true` stores canonical zero + marks key absent
- `dst_val` is canonical zero when `dst_is_null = true` (interpreter enforced)
- `grep -r "Value::Null" crates/` returns 0 matches
- All tests pass, `cargo clippy` zero warnings

**Depends on**: M1 complete (especially S1 for SSA validation of both dst slots).

---

### BG. Program Budgets

**What**: Add budget fields for DoS prevention per semantics-spec §1.8.

**Target**:
```rust
pub struct ProgramBudgets {
    pub max_ops: u32,
    pub max_slots: u16,
    pub max_accesses: u32,
}
```

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/tx.rs` (or new `budget.rs`) | Add `ProgramBudgets` struct |
| `tabula-executor/src/program.rs` | Validate budgets: instruction count ≤ max_ops, max(dst) < max_slots, access count ≤ max_accesses |
| `tabula-cli/src/io.rs` | Add budgets to `ProgramFile` JSON |

**Acceptance**:
- Program exceeding any budget rejected at registration
- Budgets optional for backward compatibility (serde default = unchecked)

**Depends on**: S2 (access counting needs 2-slot Read/Write to be in place).

---

### ST. ApplyBatchStatement Alignment

**What**: Align `ApplyBatchStatement` public inputs with architecture v0.4.4.

**Current**:
```rust
pub struct ApplyBatchStatement {
    pub old_state_root: Digest,
    pub new_state_root: Digest,
    pub program_root: Digest,
    pub batch_digest: Digest,
}
```

**Target**:
```rust
pub struct ApplyBatchStatement {
    pub old_state_root: StateRoot,
    pub new_state_root: StateRoot,
    pub program_root: Digest,
    pub applied_tx_digest: Digest,
    pub static_table_root: Digest,
    pub budgets: ProgramBudgets,
}
```

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/statement.rs` | Update struct fields |
| `tabula-proof/src/mock.rs` | Update mock prover/verifier |

**Acceptance**:
- All 6 public input fields present
- `batch_digest` renamed to `applied_tx_digest`

**Depends on**: BG (needs `ProgramBudgets` type).

---

## M3 — NF Validation

> Enforce all four normal-form rules as compile-time IR structural invariants.
> Guarantees: no intra-tx RAM argument needed, SSA wiring is sufficient.

### S3. NF-1~NF-4 in `Program::register()`

**What**: Add compile-time validation for all four normal-form rules from semantics-spec §2.3.

**Rules**:

| Rule | Name | Definition |
|------|------|------------|
| NF-1 | Unique-Read | At most one `Read` per `(t, c, r)` per tx body |
| NF-2 | Unique-Write | At most one `Write` per `(t, c, r)` per tx body |
| NF-3 | No-Read-After-Write | No `Read` to a `(t, c, r)` that has a prior `Write` |
| NF-4 | Key-Alias Resolvability | For any two state accesses to same `(t, c)`, row exprs must be provably equal or provably distinct per §2.5 |

**RowExpr equality** (§2.5):
- **Provably equal**: `Lit(a)==Lit(a)`, `Param(p)==Param(p)`, `Slot(s)==Slot(s)`
- **Provably distinct**: `Lit(a)` vs `Lit(b)` where `a ≠ b`
- **Ambiguous** (→ reject): all other combinations

**Changes**:

| File | What |
|------|------|
| `tabula-executor/src/program.rs` | Add `validate_normal_form()` called from `register()` |
| `tabula-core/src/error.rs` | Add NF violation error variants (NfUniqueRead, NfUniqueWrite, NfReadAfterWrite, NfAmbiguousAlias) |
| Tests | Rejection tests for each NF violation |

**Acceptance**:
- Each NF violation produces descriptive error with instruction indices
- All existing programs pass (they already satisfy NF)
- Ambiguous alias `Read(t,0, Slot(1))` + `Write(t,0, Param(0))` → rejected

**Depends on**: M2 complete (needs 2-slot IR to correctly identify Read/Write targets and row expressions).

---

## M4 — Plonky3 Foundation

> Build crypto primitives independently testable before STARK integration.
> All items use Schema-Typed Digest-Native Two-Tier encoding (proof-spec §10.3).
> Can start P1 in parallel with M2.

### P1. Plonky3 Workspace Dependencies

**What**: Add `p3-*` crates to workspace behind feature flags.

| File | What |
|------|------|
| `Cargo.toml` (workspace) | Add `p3-field`, `p3-baby-bear`, `p3-poseidon2`, `p3-symmetric`, `p3-uni-stark`, `p3-fri`, `p3-air`, `p3-matrix`, `p3-commit`, `p3-merkle-tree` |
| `tabula-commitment/Cargo.toml` | `p3-field`, `p3-baby-bear`, `p3-poseidon2`, `p3-symmetric` under `[features] stark` |
| `tabula-proof/Cargo.toml` | All `p3-*` under `[features] stark` |

**Acceptance**: `cargo check --features stark` compiles.

**Depends on**: Nothing.

---

### P2. BabyBear ValueCodec

**What**: Implement `ValueCodec` for BabyBear using Schema-Typed Digest-Native Two-Tier encoding (§10.3).

**Encoding** — per-type variable width, no type tag:
- `Bool` → `w=1`: single boolean FE `{0,1}`
- `U64` → `w=3`: 3 BabyBear limbs `(x0, x1 ∈ [0, 2^30), x2 ∈ [0, 16))`
- `I64` → `w=3`: offset encoding `x + 2^63` → same 3 limbs as U64
- `Digest` → `w=8`: 8 native BabyBear FE (Poseidon2 squeeze, NOT byte-decomposed)

**Two tiers**:
- **Tier 1 (ComEnc)**: `w(T)` FE only — SSMC/SMT commitments (non-null)
- **Tier 2 (TraceEnc)**: `w(T)` FE + `val_is_null` boolean — memory traces

| File | What |
|------|------|
| `tabula-commitment/src/baby_bear_codec.rs` (NEW) | `BabyBearCodec` implementing `ValueCodec<FieldRepr = BabyBear>` |

**Acceptance**: Round-trip encode/decode. `field_elements_per()` returns 1/3/3/8. I64 offset encoding preserves ordering.

**Depends on**: P1.

---

### P3. Poseidon2 Hasher

**What**: Implement `Hasher` trait using Poseidon2 over BabyBear.

Poseidon2 output = 8 BabyBear FE (~248 bits, ~124-bit collision resistance). `Digest = [u8; 32]` maps to `8 × 4 LE bytes`. Maintain native `NativeDigest([BabyBear; 8])` for in-circuit composability.

| File | What |
|------|------|
| `tabula-commitment/src/poseidon_hasher.rs` (NEW) | `PoseidonHasher` + `NativeDigest` |

**Key methods**:
- `hash(data: &[u8])` → sponge absorb, squeeze 8 FE → 32 bytes
- `hash_pair(left, right)` → Merkle building block
- `hash_domain(tag: u8, ...)` → domain-separated (0x00=SSMC, 0x01=SMT, 0x10=leaf, 0x11=tables, 0x12=cols)

**Acceptance**: Deterministic, known test vectors.

**Depends on**: P1.

---

### P4. Sparse Merkle Tree (SMT)

**What**: 64-level SMT with Poseidon2. Two uses:
1. **Cell-level** (Strategy B): large columns, leaves = `ComEnc(T)` (variable width)
2. **Meta-level** (root structure): `SMT_cols` per table + `SMT_tables` global, leaves = `Digest` (8 FE)

**Operations**: `new(domain_tag, depth)`, `insert`, `root`, `open`, `verify`, `update`

**Domain separation**: `NodeHash(level, left, right) = Poseidon(domain_tag || level || left || right)`

| File | What |
|------|------|
| `tabula-commitment/src/smt.rs` (NEW) | `SparseMerkleTree`, `MerkleProof` |

**Acceptance**: Empty root deterministic. Insert/open round-trip. Non-membership valid. Domain separation works. Depth parameterization (64 cell, 16-24 meta). 1000+ key stress test.

**Depends on**: P3.

---

### P5. SSMC (Small Sparse Map Commitment)

**What**: Sorted sparse list commitment for small columns. Witness-based AIR sub-table. Membership/non-membership via LogUp. Update via 3-way merge proof. All values use **Tier 1 (ComEnc)** encoding.

**Data structures**:
- `SsmcTable` — sorted `(RowKey, Vec<BabyBear>, next_key, is_first, is_last)`
- `SsmcCommitment` — Poseidon streaming: `Poseidon(0x00 || t || c || k_0 || v_0 || ...)`
- `MergeTrace` — `(key, source, old_val, write_val, new_val, in_new)`

**Operations**: `commit`, `open_membership`, `open_non_membership`, `merge_update`

| File | What |
|------|------|
| `tabula-commitment/src/ssmc.rs` (NEW) | `SsmcTable`, `SsmcCommitment`, `MergeTrace` |

**Acceptance**: Commitment deterministic. Membership/non-membership verified (interior, before-first, after-last, empty). Strict inequality decomposition. Merge completeness. Delete removes key. Unsorted/duplicate-key rejected.

**Depends on**: P3.

---

### P6. Hybrid State Commitment Layer

**What**: Per-column strategy selection (SSMC ≤ threshold, SMT > threshold). Two-level root structure. ColumnMeta wiring.

**Architecture**:
```
LeafDigest(t,c) = Poseidon(0x10 || t || c || tag_c || Com[t,c])
TableRoot[t]    = SMT_cols.Root(key=c, value=LeafDigest(t,c))
oldRoot         = SMT_tables.Root(key=t, value=TableRoot[t])
```

**ColumnMeta**: `(t, c, tag, Com_old, Com_new, is_empty_old, is_empty_new, is_touched)`

| File | What |
|------|------|
| `tabula-commitment/src/hybrid.rs` (NEW) | `HybridVC`, `ColumnProof`, `ColumnMeta`, strategy dispatch |
| `tabula-commitment/src/table.rs` | Refactor: use `SMT_cols` for table root |
| `tabula-commitment/src/root.rs` | Refactor: use `SMT_tables` for global root |

**Acceptance**: Strategy selection by size. Both membership/non-membership for both strategies. Two-level root. Inclusion proofs. Meta-level update proofs `oldRoot → newRoot`. Round-trip commit → open → verify.

**Depends on**: P4, P5.

---

## M5 — Single-Tx Proof (Phase B)

> Execute tx → generate witness → build AIR trace → prove → verify.
> Value encoding: Tier 1 (ComEnc) in commitments, Tier 2 (TraceEnc) in execution trace.

### B1. Witness Generator

**What**: Run executor, collect trace, format as AIR witness columns.

**Output**: `WitnessData` — instruction trace, slot values (Tier 2), access events, SSMC/SMT witnesses (Tier 1), ColumnMeta, state root transition.

| File | What |
|------|------|
| `tabula-proof/src/witness.rs` (NEW) | `WitnessGenerator`, `WitnessData` |

**Depends on**: M2 (2-slot IR), M3 (NF guarantees), P2 (ValueCodec), P6 (Hybrid VC).

---

### B2. AIR Trace Layout

**What**: Define Plonky3 AIR column layout with width-class chips (Narrow w=1, Standard w=3, Wide w=8).

**Tables**: instruction trace, GlobalSSMC (Tier 1), GlobalMerge (Tier 1), GlobalSortedMem (Tier 2), ColumnMeta. All with `is_real` prefix constraint, `same_group` boundaries, strict lexicographic `(t,c)` ordering.

| File | What |
|------|------|
| `tabula-proof/src/air.rs` (NEW) | `TabulaAir` implementing `p3_air::Air` |

**Depends on**: B1.

---

### B3. Instruction Constraints

**What**: Per-opcode AIR constraints.

| Opcode | Constraint |
|--------|-----------|
| `Read` | `slot[dst] == access_val`, `val_is_null` propagated, `is_write == 0` |
| `Write` | `access_val == slot[src]`, `val_is_null` propagated, `is_write == 1` |
| `Add/Sub/Mul` | Type-specific arithmetic (u64: 3-limb + carry; i64: offset-encoded) |
| `DivMod` | `dst_q * den + dst_r == num` with range checks |
| `Assert` | `predicate_value == 1` |
| `Hash` | Poseidon2 chain: ComEnc inputs → 8 FE Digest output |
| `Select` | `dst = cond · if_true + (1-cond) · if_false` per FE |
| `Lookup` | Static table LogUp |
| `Emit` | No constraint |

| File | What |
|------|------|
| `tabula-proof/src/constraints/` (NEW dir) | Per-opcode constraint modules |

**Depends on**: B2.

---

### B4. State Commitment Verification Constraints

**What**: In-circuit verification of state openings/updates using Tier 1 (ComEnc) encoding.

- **SSMC**: sorted uniqueness (borrow-chain), hash chain, LogUp membership/non-membership, merge constraints
- **SMT**: 64-level Merkle path, domain-separated Poseidon per level
- **Meta-level**: ColumnMeta constraints, root inclusion, `SMT_tables` update proof

| File | What |
|------|------|
| `tabula-proof/src/state_constraints.rs` (NEW) | SSMC, SMT, meta-level constraints |

**Depends on**: B2, P4, P5.

---

### B5. End-to-End Single-Tx Proof

**What**: Wire witness → AIR → Plonky3 STARK → verify.

**Test case**: `Read(balance) → Sub(balance, amount) → Write(balance)`.

| File | What |
|------|------|
| `tabula-proof/src/prover.rs` (NEW) | `StarkProver` implementing `Prover` trait |
| `tabula-proof/src/verifier.rs` (NEW) | `StarkVerifier` implementing `Verifier` trait |
| `tabula-proof/tests/single_tx.rs` (NEW) | End-to-end proof test |

**Depends on**: B3, B4.

---

### B6. Proof Chaining Test

**What**: `root0 → root1 → root2` via two sequential proofs. Verify root1 matches.

**Depends on**: B5.

---

### B7. Benchmark & Threshold Calibration

**What**: Measure constraint counts for SSMC vs SMT at column sizes [10, 50, 100, 200, 500, 1000, 5000]. Calibrate `P`, `R`, `L`, `P_stream`, break-even `m*`, width-class impact.

**Depends on**: B5.

---

## M6 — Batch Proof (Phase C)

> Multi-tx batches with inter-tx RAW dependencies.
> GlobalSortedMem uses Tier 2 encoding: `val[w(T)]` + `val_is_null`, `mem[w(T)]` + `mem_is_null`.

### C1. GlobalSortedMem Construction

**What**: Build GlobalSortedMem from batch execution events.

Columns: `(t, c, r, τ, is_init, is_write, val[w(T)], val_is_null, mem[w(T)], mem_is_null, is_real)`

Init rows: `τ=0, is_init=1, is_write=0`, value from base state. Access rows: `τ=clk+1, is_init=0`. Sorted by `(t, c, r, τ)` lexicographic. Per-`(t,c)` segments. Segment-first init constraint. Init row uniqueness per `(t,c,r)`.

| File | What |
|------|------|
| `tabula-proof/src/sorted_mem.rs` (NEW) | `GlobalSortedMemBuilder` |

**Depends on**: B5.

---

### C2. LogUp Argument & Clock Binding

**What**: Link execution access log ↔ GlobalSortedMem via LogUp.

**Fingerprint**: `Φ(row) = α·t + β·c + a·r + b·τ + d·is_write + f·val_is_null + Σ_j e_j·val[j]`

**Multiplicities**: execution `m = is_access`, sorted `m = is_real · (1 - is_init)`.

**Clock binding**: `τ = clk_i + 1`, `clk_{i+1} = clk_i + is_access_i`.

| File | What |
|------|------|
| `tabula-proof/src/logup.rs` (NEW) | LogUp argument, fingerprint, clock constraints |

**Depends on**: C1.

---

### C3. Write Coalescing & State Update

**What**: Extract `WriteSet_batch_final` from GlobalSortedMem (§8.6).

Auxiliary columns: `same_key` (random-challenge combined diff + inverse helper), `is_last_for_key`, `has_written` (running boolean OR). UpdateSet: `is_real ∧ is_last_for_key ∧ has_written`.

| File | What |
|------|------|
| `tabula-proof/src/write_coalesce.rs` (NEW) | WriteSet extraction, UpdateSet |

**Depends on**: C1.

---

### C4. AppliedTxDigest & Failed Tx Exclusion

**What**: Compute `AppliedTxDigest` from applied tx list as public input. Failed txs excluded from trace.

**Depends on**: C1.

---

### C5. End-to-End Batch Proof

**What**: Multi-tx batch with inter-tx RAW dependencies.

**Test case**: Tx1 writes `balance[A] -= 10`, Tx2 reads `balance[A]` and sees updated value.

**Depends on**: C2, C3, C4.

---

## Parallel Tracks (independent of M4-M6)

> Can start after M2, run alongside proof system work.

### CB1. IR: Basic Block CFG

> See [conditional-branching-research.md](./conditional-branching-research.md), [ideal-conditional-design.md](./ideal-conditional-design.md)

**What**: Replace `TxTypeDef.body: Vec<Instruction>` with `TxBody` — CFG of `BasicBlock`s with `Terminator`s (Jump/Branch/Return/Abort). DAG-only.

**Scope**: `ir.rs`, `tx.rs`, `interpreter.rs`, `program.rs`, `lower.rs`, serialization, all tests.

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

## Dependency Graph

```
M1 — IR Housekeeping (all 5 parallel)
  S1  ──┐
  CK  ──┤
  LK  ──┼── all parallel ──► M2 — 2-Slot Migration
  S4  ──┤                      S2(a-g) + BG + ST
  S5  ──┘                           │
                                     ├──► M3 — NF Validation
                                     │      S3(a-d)
                                     │
                                     ├──► Parallel Tracks
                                     │      CB1 → CB2, CLI1, DSL1-3
                                     │
  P1 ◄── can start with M1          │
   │                                 │
   ├──► P2 (BabyBear ValueCodec)     │
   │                                 │
   ├──► P3 (Poseidon2 Hasher)        │
   │      │                          │
   │      ├──► P4 (SMT)  ────┐      │
   │      │                   │      │
   │      └──► P5 (SSMC) ────┤      │
   │                          ▼      │
   │                    P6 (Hybrid)  │
   │                          │      │
   └──────────────────────────┤      │
                              ▼      ▼
                     M5 — Single-Tx Proof (Phase B)
                       B1 → B2 → B3+B4 → B5 → B6+B7
                                            │
                                            ▼
                     M6 — Batch Proof (Phase C)
                       C1 → C2+C3+C4 → C5
```

**Critical path**: M1 → M2 → M3 ──────────────────────────────► B1 → B2 → B3+B4 → B5 → C1 → C2+C3 → C5
                                 P1 → P3 → P4+P5 → P6 ──┘

**Strategy**: M1 and P1 start in parallel. M2 is the bottleneck — prioritize it. P2-P6 proceed independently on the crypto track. Both tracks converge at B1.

**Estimated new file count**: ~15 new source files across `tabula-commitment` and `tabula-proof`.
