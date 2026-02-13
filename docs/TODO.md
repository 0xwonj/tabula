# Tabula — Execution Plan

> Concrete implementation roadmap from current state to working proof system.
> Each task includes files to change, dependencies, and acceptance criteria.
> Aligned with proof-spec v0.9, semantics-spec v0.2.1, and architecture v0.4.4.

---

## Status Overview

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | ✅ DONE | Reference interpreter, overlay, DSL compiler, CLI |
| Phase 1.5 | ✅ DONE | Trait polish (T1-T3) |
| Phase 1.6 | **NEXT** | SSA enforcement in code |
| Phase 1.7 | Planned | IR alignment (2-slot R/W, NF validation, Select, Hash encoding) |
| Phase 2a | Planned | Plonky3 foundation (Poseidon, SMT, SSMC, Hybrid VC) |
| Phase 2b | Planned | Single-tx proof (Phase B from proof-spec) |
| Phase 3 | Planned | Batch proof with memory argument (Phase C from proof-spec) |

**Current state**: 9,400 LOC across 6 crates, 203 tests, `cargo clippy` zero warnings.

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

**Spec patches applied** (proof-spec v0.6.1 → v0.7 → v0.8 → v0.9):

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
| Stage 1/2/3 naming (was Phase A/B/C); receiptsDigest out-of-protocol note; Layer B/C disambiguation | architecture v0.4.4 |
| WriteSet_final → WriteSet (per-tx) / WriteSet_batch_final (per-batch); Lookup access-row heading fix; budgets §1.7→§1.8; StaticTableRoot in §5.1 | proof-spec v0.9 |
| StaticTableRoot + budgets in ApplyBatchStatement; CellKey canonical order `(t,c,r)` note; write_set_final terminology | architecture v0.4.4 |

</details>

---

## Phase 1.6 — SSA Enforcement

> Turns the "True SSA" spec claim into a validated code invariant.

### S1. True SSA Enforcement in IR

**What**: Enforce that each destination slot index appears at most once in a transaction body.

**Why**: The spec (§6.2, §10.6) claims "IR is SSA, therefore no intra-tx memory argument is needed." Current code allows slot reuse (`set_slot` overwrites). Without enforcement, a valid program could reassign slots, breaking the SSA claim.

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/ir.rs` | Add `validate_ssa(instructions: &[Instruction]) -> Result<(), TabulaError>` — collect all dst slots, reject duplicates |
| `tabula-executor/src/program.rs` | Call `validate_ssa()` in `Program::register()` |
| `tabula-lang/src/lower.rs` | Add assertion test verifying lowering produces SSA |
| Tests | New test: duplicate `dst:0` rejected; existing programs pass unchanged |

**Acceptance**:
- `Program::register()` rejects a program with duplicate dst slots
- All 203 existing tests pass (existing programs are already SSA)
- New test: `validate_ssa` catches `[Read{dst:0,...}, Add{dst:0,...}]`

**Depends on**: Nothing (start immediately).

---

## Phase 1.7 — IR Alignment

> Aligns the Rust IR implementation with the normative semantics-spec v0.2.1.
> These are code changes that reconcile implementation divergences identified in the cross-document review.

### S2. Two-Slot Read/Write IR Migration

**What**: Migrate `Read` and `Write` instructions from single-slot form (`Value::Null` for absence) to the normative two-slot form where null is a separate boolean flag.

**Current** (implementation):
```rust
Read { dst: Slot, table: TableId, row: RowExpr, col: ColId }
Write { table: TableId, row: RowExpr, col: ColId, src: ValueExpr }
// Value::Null represents absence
```

**Target** (semantics-spec §1.5.1–§1.5.2):
```rust
Read { dst_val: Slot, dst_is_null: Slot, table: TableId, row: RowExpr, col: ColId }
Write { table: TableId, row: RowExpr, col: ColId, src_val: ValueExpr, src_is_null: ValueExpr }
// dst_val MUST be canonical zero when dst_is_null = true
```

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/ir.rs` | Update `Read` and `Write` variants |
| `tabula-core/src/types.rs` | Consider removing `Value::Null` (or keeping as sugar with lowering) |
| `tabula-executor/src/interpreter.rs` | Update execute loop for two-slot Read/Write |
| `tabula-executor/src/resolve.rs` | Update expression resolution |
| `tabula-executor/src/program.rs` | SSA validation accounts for both dst slots |
| `tabula-executor/src/overlay.rs` | Return `(Value, bool)` from read operations |
| `tabula-lang/src/lower.rs` | Emit two-slot IR |
| All tests | Update IR construction |

**Acceptance**:
- `Read` produces two slots: value + is_null boolean
- `Write` accepts two sources: value + is_null boolean
- `dst_val` is canonical zero when `dst_is_null = true` (enforced at interpreter level)
- `dst_val ≠ dst_is_null` (distinct slots, validated by SSA check)
- All existing tests pass with updated IR

**Depends on**: S1 (SSA enforcement must be in place first).

---

### S3. NF-1~NF-4 Validation in `Program::register()`

**What**: Add compile-time validation for all four normal-form rules from semantics-spec §2.3.

**Rules**:
- **NF-1 (Unique-Read)**: At most one `Read` per `(t, c, r)` per tx body
- **NF-2 (Unique-Write)**: At most one `Write` per `(t, c, r)` per tx body
- **NF-3 (No-Read-After-Write)**: No `Read` to a `(t, c, r)` that has a prior `Write` in the same body
- **NF-4 (Key-Alias Resolvability)**: For any two state-access instructions targeting the same `(t, c)`, their row expressions must be provably equal or provably distinct (semantics-spec §2.5)

**Changes**:

| File | What |
|------|------|
| `tabula-executor/src/program.rs` | Add `validate_normal_form()` called from `register()` |
| Tests | Rejection tests for each NF violation |

**Acceptance**:
- Programs violating any NF-1~NF-4 are rejected at registration time with descriptive error
- All existing programs pass (they already satisfy NF)
- NF-4 uses the RowExpr equality definition: `Lit(n)` vs `Lit(m)` provably distinct; `Slot(s)` vs `Param(p)` ambiguous → reject

**Depends on**: S2 (needs two-slot IR to correctly identify Read/Write targets).

---

### S4. Select Op Implementation

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
| `tabula-executor/src/program.rs` | SSA validation, type checking (cond must be Bool, if_true/if_false same type) |
| `tabula-lang/src/parser.rs` | Parse `select(cond, a, b)` expression |
| `tabula-lang/src/lower.rs` | Lower to `Select` instruction |
| Tests | Unit + integration tests |

**Acceptance**:
- `Select` with `cond=true` returns `if_true`; `cond=false` returns `if_false`
- Type mismatch between `if_true`/`if_false` rejected at registration
- Non-Bool `cond` rejected at registration
- DSL: `let x = select(flag, a, b)` compiles and executes correctly

**Depends on**: Nothing (can be done in parallel with S2/S3).

---

### S5. Hash Input Encoding in Interpreter

**What**: Align the IR `Hash` instruction with the normative input encoding from semantics-spec §1.5.5.

**Normative encoding**:
```
Hash(inputs) → Poseidon(domain_tag_hash || n || ComEnc(x_0) || ... || ComEnc(x_{n-1}))
```
where `n` = number of inputs, `ComEnc` = Tier 1 value encoding.

**Current**: The interpreter uses `Hasher::hash_many()` which takes raw bytes. This is correct for mock (Blake3), but the encoding must be specified so that the same program produces the same hash under different hasher implementations.

**Changes**:

| File | What |
|------|------|
| `tabula-core/src/traits.rs` | Add `hash_ir(inputs: &[Value]) -> Digest` with default impl using `hash_many` |
| `tabula-executor/src/interpreter.rs` | Use `hash_ir()` for the `Hash` instruction |
| Documentation | Encoding format documented inline |

**Acceptance**:
- `Hash` instruction uses length-prefixed, `ComEnc`-encoded inputs
- Mock hasher: borsh encoding as before (compatible default)
- Future Poseidon hasher will use native BabyBear ComEnc per §1.5.5

**Depends on**: Nothing (can be done in parallel).

---

## Phase 2a — Plonky3 Foundation

> Build crypto primitives independently testable before STARK integration.
> All items use Schema-Typed Digest-Native Two-Tier encoding (proof-spec §10.3).

### P1. Add Plonky3 workspace dependencies

**What**: Add `p3-*` crates to workspace Cargo.toml behind feature flags.

**Changes**:

| File | What |
|------|------|
| `Cargo.toml` (workspace) | Add `p3-field`, `p3-baby-bear`, `p3-poseidon2`, `p3-symmetric`, `p3-uni-stark`, `p3-fri`, `p3-air`, `p3-matrix`, `p3-commit`, `p3-merkle-tree` |
| `tabula-commitment/Cargo.toml` | `p3-field`, `p3-baby-bear`, `p3-poseidon2`, `p3-symmetric` under `[features] stark = [...]` |
| `tabula-proof/Cargo.toml` | All `p3-*` under `[features] stark = [...]` |

**Acceptance**: `cargo check --features stark` compiles.

**Depends on**: Nothing.

---

### P2. BabyBear ValueCodec

**What**: Implement `ValueCodec` for BabyBear using Schema-Typed Digest-Native Two-Tier encoding (§10.3).

**Encoding** — per-type variable width, no type tag:
- `Bool` → `w=1`: single boolean FE `{0,1}`
- `U64` → `w=3`: 3 BabyBear limbs `(x0, x1 ∈ [0, 2^31), x2 ∈ {0,1,2,3})`
- `I64` → `w=3`: offset encoding `x + 2^63` → same 3 limbs as U64
- `Digest` → `w=8`: 8 native BabyBear FE (Poseidon2 squeeze, NOT byte-decomposed)
- `Null` → structural (represented by `val_is_null` flag in Tier 2)

**Two tiers**:
- **Tier 1 (ComEnc)**: `w(T)` FE only — SSMC/SMT commitments (non-null)
- **Tier 2 (TraceEnc)**: `w(T)` FE + `val_is_null` boolean — memory traces

**Changes**:

| File | What |
|------|------|
| `tabula-commitment/src/baby_bear_codec.rs` (NEW) | `BabyBearCodec` implementing `ValueCodec<FieldRepr = BabyBear>` |
| `tabula-commitment/src/lib.rs` | `#[cfg(feature = "stark")] pub mod baby_bear_codec;` |

**Acceptance**: Round-trip encode/decode for each variant. `field_elements_per()` returns 1/3/3/8 for Bool/U64/I64/Digest. I64 offset encoding preserves ordering.

**Depends on**: P1.

---

### P3. Poseidon2 Hasher

**What**: Implement `Hasher` trait using Poseidon2 over BabyBear.

**Key design**: Poseidon2 output = 8 BabyBear FE (~248 bits, ~124-bit collision resistance). The `Hasher` trait's `Digest = [u8; 32]` maps to `8 × 4 LE bytes`. Internally, maintain native `[BabyBear; 8]` representation for in-circuit composability.

**Changes**:

| File | What |
|------|------|
| `tabula-commitment/src/poseidon_hasher.rs` (NEW) | `PoseidonHasher` implementing `Hasher`, plus `NativeDigest([BabyBear; 8])` type for in-circuit use |
| `tabula-commitment/src/lib.rs` | `#[cfg(feature = "stark")] pub mod poseidon_hasher;` |

**Key methods**:
- `hash(data: &[u8])` → absorb into Poseidon sponge, squeeze 8 BabyBear FE → 32 bytes
- `hash_pair(left, right)` → `Poseidon(left || right)` — Merkle building block
- `hash_domain(tag: u8, ...)` → domain-separated hash (tags: `0x00`=SSMC, `0x01`=SMT, `0x10`=leaf, `0x11`=tables, `0x12`=cols)

**Acceptance**: Deterministic, known test vectors. Cross-check `hash_pair` against reference Poseidon2.

**Depends on**: P1.

---

### P4. Sparse Merkle Tree (SMT)

**What**: 64-level SMT with Poseidon2. Two uses:
1. **Cell-level** (Strategy B): commits large columns, leaves = `ComEnc(T)` (variable width per column)
2. **Meta-level** (root structure): `SMT_cols` per table + `SMT_tables` global, leaves = `Digest` (8 FE)

**Data structures**:
- `SparseMerkleTree<V>` — generic over leaf value type (supports both `Vec<BabyBear>` for cell-level and `NativeDigest` for meta-level)
- `MerkleProof` — 64 sibling hashes
- Zero-subtree cache — precomputed empty subtree hash at each level

**Operations**:
- `new(domain_tag: u8, depth: usize)` — (tags: `0x01`=cell, `0x11`=tables, `0x12`=cols; depth: 64 for cell, 16-24 for meta)
- `insert(key: u64, value: &[BabyBear])` — set leaf
- `root() → NativeDigest`
- `open(key: u64) → (Option<Vec<BabyBear>>, MerkleProof)`
- `verify(root, key, value, proof) → bool`
- `update(key, old_value, new_value) → (new_root, MerkleProof)`

**Domain separation**: `NodeHash(level, left, right) = Poseidon(domain_tag || level || left || right)`.

**Changes**:

| File | What |
|------|------|
| `tabula-commitment/src/smt.rs` (NEW) | `SparseMerkleTree`, `MerkleProof`, all operations |
| `tabula-commitment/src/lib.rs` | Add module |

**Acceptance**:
- Empty tree root deterministic
- Insert/open round-trip
- Non-membership: non-existent key returns default leaf with valid proof
- Domain separation: different tags produce different roots for same data
- Depth parameterization works (64 for cell-level, 16-24 for meta-level)
- 1000+ key stress test

**Depends on**: P3.

---

### P5. SSMC (Small Sparse Map Commitment)

**What**: Sorted sparse list commitment for small columns. Witness-based AIR sub-table. Membership/non-membership via LogUp. Update via 3-way merge proof. All values use **Tier 1 (ComEnc)** encoding — `w(T)` FE, always non-null (§10.3).

**Data structures**:
- `SsmcTable` — sorted list of `(RowKey, Vec<BabyBear>, next_key, is_first, is_last)` entries. Value width `w(T)` determined by column schema.
- `SsmcCommitment` — domain-separated Poseidon streaming: `Poseidon(0x00 || t || c || k_0 || v_0 || ...)`
- `MergeTrace` — `(key, source, old_val, write_val, new_val, in_new)` rows

**Operations**:
- `commit(t, c, entries: &[(RowKey, &[BabyBear])]) → NativeDigest`
- `open_membership(key) → LogUpWitness`
- `open_non_membership(key) → GapWitness` — interior / before-first / after-last
- `merge_update(old_table, writes) → (new_table, MergeTrace)` — supports delete via `WRITE(k, Null)`

**Constraint requirements** (for later AIR integration):
- Sorted uniqueness: `next_key > key` via u64 borrow-chain gadget (§4.2.R)
- Boundary: `is_first`/`is_last` flags (no sentinels — 0 and 2^64-1 are valid keys)
- Empty column: ColumnMeta `is_empty_old=1`, `Com_empty = Poseidon(0x00 || t || c)`
- Hash chain: `h_0 = Poseidon(0x00 || t || c || k_0 || v_0)`, `h_i = Poseidon(h_{i-1} || k_i || v_i)`
- Merge: 2-bit source encoding (s1,s0), `in_new` flag, LogUp completeness

**Changes**:

| File | What |
|------|------|
| `tabula-commitment/src/ssmc.rs` (NEW) | `SsmcTable`, `SsmcCommitment`, `MergeTrace`, all operations |
| `tabula-commitment/src/lib.rs` | Add module |

**Acceptance**:
- Commitment deterministic (domain-separated)
- Membership: LogUp witness verifies for existing key
- Non-membership: gap witness verifies (interior, before-first, after-last, empty column)
- Strict inequality: `r - key - 1` and `next_key - r - 1` decompose to non-negative BabyBear limbs
- Merge: NewList = OldList ⊕ WriteSet, completeness holds, delete removes key
- Rejects unsorted or duplicate-key input

**Depends on**: P3.

---

### P6. Hybrid State Commitment Layer

**What**: Per-column strategy selection (SSMC ≤ threshold, SMT > threshold). Uniform digest. Two-level root structure with inclusion proofs and meta-level update proofs.

**Architecture**:
```
LeafDigest(t,c) = Poseidon(0x10 || t || c || tag_c || Com[t,c])
TableRoot[t] = SMT_cols.Root(key=c, value=LeafDigest(t,c))
oldRoot = SMT_tables.Root(key=t, value=TableRoot[t])
```

**ColumnMeta** wiring: `(t, c, tag, Com_old, Com_new, is_empty_old, is_empty_new, is_touched)` — binds per-column commitments to root inclusion/update proofs.

**Changes**:

| File | What |
|------|------|
| `tabula-commitment/src/hybrid.rs` (NEW) | `HybridVC`, `ColumnProof` (enum: SSMC/SMT), `ColumnMeta`, strategy dispatch |
| `tabula-commitment/src/table.rs` | Refactor: use `SMT_cols` for table root |
| `tabula-commitment/src/root.rs` | Refactor: use `SMT_tables` for global root |
| `tabula-commitment/src/lib.rs` | Add module |

**Acceptance**:
- Strategy selection by column size
- Both membership and non-membership for both strategies
- Two-level root: `SMT_cols` + `SMT_tables`
- Inclusion proofs: `Com[t,c]` ∈ `oldRoot` for touched columns
- Meta-level update proofs: `oldRoot → newRoot` via ColumnMeta transitions
- Round-trip: commit → open → verify for both strategies

**Depends on**: P4, P5.

---

## Phase 2b — Single-Tx Proof (Phase B)

> Execute tx → generate witness → build AIR trace → prove → verify.
> Value encoding: Tier 1 (ComEnc) in commitments, Tier 2 (TraceEnc) in execution trace.

### B1. Witness Generator

**What**: Run executor, collect trace, format as AIR witness columns.

**Input**: `Transaction`, `Program`, `StateSnapshot` (with SSMC/SMT column data)
**Output**: `WitnessData` — structured columns:
- Instruction trace (opcode, operands)
- Slot values — **Tier 2**: `w(T)` FE + `val_is_null` per slot
- Read/Write access events (t, c, r, val, val_is_null, is_write, τ)
- SSMC witnesses OR SMT Merkle paths — **Tier 1**: `w(T)` FE, non-null
- ColumnMeta rows
- State root transition (old → new)

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/witness.rs` (NEW) | `WitnessGenerator`, `WitnessData` |
| `tabula-proof/src/lib.rs` | Add module |

**Depends on**: P2, P6, existing executor.

---

### B2. AIR Trace Layout

**What**: Define Plonky3 AIR column layout.

**Width-class AIR chips** (from §10.3): value columns have type-dependent width.
- **Narrow** (w=1): Bool columns
- **Standard** (w=3): U64/I64 columns
- **Wide** (w=8): Digest columns

**Instruction trace columns**:
- `is_real`, `opcode`, `operand_0..3`
- `slot_0..N` — Tier 2 encoding: w(T) FE + `val_is_null` per slot
- `is_access`, `clk` — access counter (§8.7)
- Access columns: `(t, c, r, val, val_is_null, is_write, τ)` — Tier 2 encoding

**Global auxiliary tables**:
- **GlobalSSMC** — `(t, c, key, value[w(T)], next_key, is_first, is_last, hash_acc, is_real)` — Tier 1
- **GlobalMerge** — `(t, c, key, s1, s0, old_val[w(T)], write_val[w(T)], new_val[w(T)], in_new, is_real)` — Tier 1
- **GlobalSortedMem** — `(t, c, r, τ, is_init, is_write, val[w(T)], val_is_null, mem[w(T)], mem_is_null, is_real)` — Tier 2
- **ColumnMeta** — `(t, c, tag, Com_old[8], Com_new[8], is_empty_old, is_empty_new, is_touched, is_real)`

All global tables: `is_real` prefix constraint, `same_group` segment boundaries, strict lexicographic `(t,c)` ordering at boundaries.

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/air.rs` (NEW) | `TabulaAir` implementing `p3_air::Air`, column definitions |
| `tabula-proof/src/lib.rs` | Add module |

**Depends on**: B1.

---

### B3. Instruction Constraints

**What**: Per-opcode AIR constraints.

| Opcode | Constraint |
|--------|-----------|
| `Read` | `slot[dst] == access_val`, `val_is_null` propagated, `is_write == 0` |
| `Write` | `access_val == slot[src]`, `val_is_null` propagated, `is_write == 1` |
| `Add/Sub/Mul` | Type-specific arithmetic over w(T) FE (u64: 3-limb with carry; i64: offset-encoded) |
| `DivMod` | `dst_q * den + dst_r == num` with range checks |
| `Assert` | `predicate_value == 1` |
| `Hash` | Poseidon2 constraint chain: inputs = ComEnc (Tier 1), output = 8 BabyBear FE (Digest) |
| `Lookup` | Static table LogUp lookup |
| `Emit` | No constraint (output only) |

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/constraints/` (NEW dir) | Per-opcode constraint modules |

**Depends on**: B2.

---

### B4. State Commitment Verification Constraints

**What**: In-circuit verification of state openings and updates. All committed values use **Tier 1 (ComEnc)** encoding — `w(T)` FE, no null flag.

**SSMC path**:
- GlobalSSMC: sorted uniqueness (u64 borrow-chain), boundary flags, Poseidon hash chain `h_i = Poseidon(h_{i-1} || key || value[w(T)])`
- Membership: LogUp lookup `(t, c, key, value[w(T)])`
- Non-membership: gap witness + strict inequality range checks
- Merge: GlobalMerge constraints, 2-bit source encoding, LogUp completeness, NewList hash chain

**SMT path**:
- 64-level Merkle path: domain-separated Poseidon hash_pair per level
- Root binding: path root matches `Com[t,c]`

**Meta-level**:
- ColumnMeta constraints: empty/touched bindings, Com_old/Com_new wiring
- Root inclusion: `LeafDigest = Poseidon(0x10 || t || c || tag || Com)` in `SMT_cols` path
- Root update: `SMT_tables` batched update proof `oldRoot → newRoot`

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/state_constraints.rs` (NEW) | SSMC, SMT, and meta-level SMT constraints |

**Depends on**: B2, P4, P5.

---

### B5. End-to-End Single-Tx Proof

**What**: Wire everything: witness → AIR → Plonky3 STARK → verify.

**Test case**: `Read(balance) → Sub(balance, amount) → Write(balance)` — simple debit tx.

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/prover.rs` (NEW) | `StarkProver` implementing `Prover` trait |
| `tabula-proof/src/verifier.rs` (NEW) | `StarkVerifier` implementing `Verifier` trait |
| `tabula-proof/src/statement.rs` | Rename `batch_digest` → `applied_tx_digest` |
| `tabula-proof/tests/single_tx.rs` (NEW) | End-to-end proof test |

**Depends on**: B3, B4.

---

### B6. Proof Chaining Test

**What**: `root0 → root1 → root2` via two sequential B proofs. Verify root1 matches across proofs.

**Depends on**: B5.

---

### B7. Benchmark & Threshold Calibration

**What**: Measure constraint counts for SSMC vs SMT at column sizes [10, 50, 100, 200, 500, 1000, 5000]. Calibrate:
- Poseidon permutation cost `P`
- u64 range-check cost `R`
- LogUp per-access cost `L`
- Streaming hash cost `P_stream`
- SSMC/SMT break-even size `m*` (estimated 100-300 rows)
- Impact of variable-width encoding on trace width per width-class

**Depends on**: B5.

---

## Phase 3 — Batch Proof with Memory Argument (Phase C)

> Multi-tx batches with inter-tx RAW dependencies.
> GlobalSortedMem uses Tier 2 encoding: `val[w(T)]` + `val_is_null`, `mem[w(T)]` + `mem_is_null`.

### C1. GlobalSortedMem Construction

**What**: Build GlobalSortedMem from batch execution events.

**Columns per row** (§8.8):
`(t, c, r, τ, is_init, is_write, val[w(T)], val_is_null, mem[w(T)], mem_is_null, is_real)`

**Construction**:
- Init rows (`τ=0, is_init=1, is_write=0`): value from base state via hybrid opening; `val_is_null=1` if key absent
- Access rows (`τ=clk+1, is_init=0`): from execution trace
- Sort by `(t, c, r, τ)` lexicographic
- Per-`(t,c)` segments with `is_real` prefix, `same_group` boundaries
- Segment-first init constraint: first row of each segment must be init
- Init row uniqueness per `(t,c,r)`

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/sorted_mem.rs` (NEW) | `GlobalSortedMemBuilder`, construction logic |

**Depends on**: B5.

---

### C2. LogUp Argument & Clock Binding

**What**: Link execution access log ↔ GlobalSortedMem via LogUp.

**Fingerprint** (§4.5, §8.4):
```
Φ(row) = α·t + β·c + a·r + b·τ + d·is_write + f·val_is_null + Σ_j e_j·val[j]
```
- `val[j]` has `w(T)` components, each with own challenge coefficient
- `val_is_null` included to distinguish null from zero

**Multiplicities**:
- Execution side: `m = is_access`
- Sorted side: `m = is_real · (1 - is_init)` (exclude init rows)

**Clock binding** (§8.7): `τ = clk_i + 1` via AIR column equality. `clk` recurrence: `clk_{i+1} = clk_i + is_access_i`.

**Domain separation**: Each LogUp instance uses independent Fiat-Shamir challenge sets.

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/logup.rs` (NEW) | LogUp argument, fingerprint computation, clock constraints |

**Depends on**: C1.

---

### C3. Write Coalescing & State Update

**What**: Extract `WriteSet_batch_final` from GlobalSortedMem (§8.6).

**Auxiliary columns**:
- `same_key`: random-challenge combined diff + inverse helper (§8.6)
- `is_last_for_key`: `1 - (next_same_group · same_key)`
- `has_written`: running boolean OR of `is_write` per key-run

**UpdateSet**: rows where `is_real ∧ is_last_for_key ∧ has_written` → `(t, c, r, mem)`.

Link to SSMC merge / SMT update via LogUp. Apply via hybrid VC → `newRoot` via ColumnMeta + `SMT_tables` update proof.

**Changes**:

| File | What |
|------|------|
| `tabula-proof/src/write_coalesce.rs` (NEW) | WriteSet extraction, UpdateSet construction |

**Depends on**: C1.

---

### C4. AppliedTxDigest & Failed Tx Exclusion

**What**: Compute `AppliedTxDigest` from applied tx list as public input. Failed txs excluded from trace — censorship resistance is protocol-layer.

**Depends on**: C1.

---

### C5. End-to-End Batch Proof

**What**: Multi-tx batch with inter-tx RAW dependencies. Full proof pipeline.

**Test case**: Tx1 writes `balance[A] -= 10`, Tx2 reads `balance[A]` and sees the updated value.

**Depends on**: C2, C3, C4.

---

## Parallel Tracks (independent of Phase 2/3)

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

### DSL1-3. Compiler improvements

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
S1 (SSA enforcement) ─── independent, do first ───────────────────────┐
  │                                                                    │
  └──► S2 (2-slot Read/Write)                                         │
         │                                                             │
         └──► S3 (NF-1~NF-4 validation)                               │
                                                                       │
S4 (Select op) ─── independent, parallel with S1-S3 ──────────────────┤
                                                                       │
S5 (Hash encoding) ─── independent, parallel with S1-S4 ──────────────┤
                                                                       │
P1 (Plonky3 deps) ── can start in parallel with S1                    │
  │                                                                    │
  ├──► P2 (BabyBear ValueCodec: w(Bool)=1, w(U64/I64)=3, w(Digest)=8)│
  │                                                                    │
  ├──► P3 (Poseidon2 Hasher + NativeDigest)                           │
  │      │                                                             │
  │      ├──► P4 (SMT: 64-level cell, 16-24 meta)  ───┐              │
  │      │                                              │              │
  │      └──► P5 (SSMC: Tier 1 ComEnc, variable w(T)) ┤              │
  │                                                     │              │
  │                                                     ▼              │
  │                                              P6 (Hybrid VC)       │
  │                                                     │              │
  └─────────────────────────────────────────────────────┤              │
                                                        ▼              │
                                          B1 (Witness Gen: Tier 1+2) ◄┘
                                                        │
                                                        ▼
                                          B2 (AIR Layout: width-class chips,
                                              val_is_null, mem_is_null)
                                                        │
                                             ┌──────────┼──────────┐
                                             ▼          ▼          │
                                      B3 (Instr)  B4 (State)      │
                                             │          │          │
                                             └────┬─────┘          │
                                                  ▼                │
                                           B5 (E2E Proof)  ◄──────┘
                                             │         │
                                      B6 (Chain)   B7 (Bench)
                                             │
                                             ▼
                                      C1 (GlobalSortedMem:
                                          Tier 2 + val_is_null)
                                             │
                                  ┌──────────┼──────────┐
                                  ▼          ▼          ▼
                           C2 (LogUp)  C3 (Coalesce) C4 (TxDigest)
                                  │          │          │
                                  └──────────┼──────────┘
                                             ▼
                                      C5 (E2E Batch)
```

**Parallel tracks** (CB1, CB2, CLI1, DSL1-3) have no dependency on Phase 2/3.

**Critical path**: S1 → S2 → S3 → P1 → P3 → P4+P5 → P6 → B1 → B2 → B3+B4 → B5 → C1 → C2+C3 → C5

**Note**: S4 (Select) and S5 (Hash encoding) are independent and can be done in parallel with any phase.

**Estimated new file count**: ~15 new source files across `tabula-commitment` and `tabula-proof`.
