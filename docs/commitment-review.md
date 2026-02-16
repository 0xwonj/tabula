# tabula-commitment Crate Review

**Date**: 2026-02-15
**Scope**: All 8 source files (`lib.rs`, `field.rs`, `hasher.rs`, `poseidon.rs`, `codec.rs`, `smt.rs`, `ssmc.rs`, `hybrid.rs`)
**Crate version**: Post-M4 (Plonky3 foundation), pre-M9 (LogUp wiring)

---

## 1. Architecture

### 1.1 Module Dependency DAG

```
field.rs ─────→ hasher.rs ─────→ poseidon.rs   (hash backend)
  (NativeDigest,    (FieldHasher     (PoseidonHasher: FieldHasher + Hasher)
   domain tags,      trait,
   u64 limbs)        MockFieldHasher)
                       │
                       ├────→ codec.rs          (value encoding)
                       │       (BabyBearCodec: ValueCodec)
                       │
                       ├────→ smt.rs            (Sparse Merkle Tree)
                       │       (SparseMerkleTree<H>, MerkleProof)
                       │
                       ├────→ ssmc.rs           (Small Sparse Map Commitment)
                       │       (SsmcList, SsmcCommitment, MergeTrace)
                       │
                       └────→ hybrid.rs         (top-level facade)
                               (HybridVC<H>, ColumnState, ColumnMeta)
```

All modules are gated behind the `stark` Cargo feature. Without it, the crate compiles as an empty shell — clean separation from the rest of the workspace.

### 1.2 Two-Layer Hash Abstraction

| Layer | Trait | Domain | Used By |
|-------|-------|--------|---------|
| Byte-level | `tabula_core::traits::Hasher` | Executor, CLI | `hash(&[u8])→Digest`, `hash_pair`, `hash_ir` |
| Field-element-level | `FieldHasher` | SMT, SSMC, HybridVC | `hash(&[F])→Digest`, `compress`, `hash_domain` |

`PoseidonHasher` implements **both** traits, bridging the two layers. The byte-level `Hasher::hash` packs each byte as a single BabyBear FE, then delegates to `FieldHasher::hash`. This is intentionally simple (1 byte = 1 FE) to guarantee canonicality at the cost of throughput.

### 1.3 Strengths

- **Clean layering**: Each module has a single responsibility. No circular dependencies.
- **Trait-generic data structures**: `SparseMerkleTree<H>` and `HybridVC<H>` are parameterized over `FieldHasher`, enabling `MockFieldHasher` in tests and `PoseidonHasher` in production.
- **Feature gating**: All Plonky3 dependencies are isolated behind `stark`. Crates that depend on `tabula-commitment` without the feature pay zero compile cost.
- **Facade pattern**: `HybridVC` encapsulates the SSMC/SMT dispatch, leaf construction, and two-level state root computation behind a single coherent API.

### 1.4 Concerns

- **Flat module layout**: All 7 submodules are siblings under `src/`. At ~2300 total lines this is fine, but `ssmc.rs` (484 lines) and `hybrid.rs` (522 lines) are approaching the threshold where further growth would benefit from subdivision (e.g., `ssmc/list.rs`, `ssmc/merge.rs`).
- **No batch-level orchestration**: `HybridVC` operates per-column. The caller (witness generator in `tabula-proof`) must manually loop over columns, compute leaves, build SMTs, and assemble the state root. A batch-level API would reduce this coordination burden.

---

## 2. Correctness

### 2.1 CRITICAL — Doc/code mismatch in codec.rs

**File**: `codec.rs:17`
**Issue**: Doc comment says `"31+31+2 bit limbs"` but the actual encoding in `field.rs:74-87` is `30+30+4 bits`.

```rust
// codec.rs line 17 — WRONG
/// - U64    → 3 FE (31+31+2 bit limbs)

// field.rs line 74 — CORRECT
/// Decomposition (30+30+4 bits):
/// - x0 = bits [0..30)  in [0, 2^30)
/// - x1 = bits [30..60) in [0, 2^30)
/// - x2 = bits [60..64) in [0, 16)
```

The code is correct — 30-bit limbs (max 1073741823) fit comfortably within BabyBear (p = 2013265921). The 31+31+2 split would produce limbs up to 2^31 − 1 = 2147483647 > p, causing lossy modular reduction. The doc is simply wrong.

**Impact**: Anyone reading the codec docs without cross-referencing `field.rs` would get an incorrect understanding of the limb decomposition. This could lead to bugs in AIR constraint implementations that depend on limb range.

**Fix**: Change `"31+31+2 bit limbs"` → `"30+30+4 bit limbs"` in `codec.rs:17`.

### 2.2 MEDIUM — SMT `node_hash_static` ignores domain_tag and level

**File**: `smt.rs:167-179`

```rust
fn node_hash_static(
    hasher: &H,
    _domain_tag: u32,  // ← ignored
    _level: usize,     // ← ignored
    left: &H::Digest,
    right: &H::Digest,
) -> H::Digest {
    hasher.compress(left, right)  // plain compress, no domain separation
}
```

The function signature suggests per-node domain separation, but the implementation uses plain `compress()`. The accompanying comment explains the rationale:

> Domain separation is achieved by using different domain_tag values in the tree constructor, which produces different empty_hashes chains. The compress function itself is a fixed-width permutation-based construction.

**Analysis**: This works for Tabula's use case because:
1. Different domain tags → different `empty_hashes[0]` → different intermediate empty hashes at every level.
2. Leaf values fed to the tree include domain-specific data (e.g., `compute_leaf` in hybrid.rs prepends `DOMAIN_LEAF`).
3. Two trees with different domain tags will produce different roots even for identical leaf sets, because the empty sibling hashes differ.

**However**, the proof-spec (§4) specifies domain-separated node hashing with tags `0x11`/`0x12` for tables/cols. The current implementation doesn't match this — internal nodes use untagged compression.

**Concerns**:
- The unused parameters are misleading. A reader might assume they're used.
- If a future refactor removes the domain tag from the constructor, the unused parameters would give false confidence.

**Recommendation**: Either (a) remove the unused parameters and document the domain-separation strategy in the struct-level doc, or (b) actually use them per proof-spec. Option (a) is simpler and matches current behavior.

### 2.3 MEDIUM — hash_ir domain tag 0x02 not in field.rs

**File**: `poseidon.rs:94`

```rust
fes.push(BabyBear::new(0x02)); // DOMAIN_TAG_HASH_IR
```

The `0x02` tag is a magic number. `field.rs` defines 5 named domain tag constants:

| Constant | Value | Purpose |
|----------|-------|---------|
| `DOMAIN_SSMC` | `0x00` | SSMC commitment |
| `DOMAIN_SMT` | `0x01` | SMT internal node |
| `DOMAIN_LEAF` | `0x10` | SMT leaf (ColumnMeta) |
| `DOMAIN_TABLE` | `0x11` | SMT_tables node |
| `DOMAIN_COL` | `0x12` | SMT_cols node |

`0x02` is not among them. While it doesn't collide with any existing tag, the lack of a named constant risks future collision if someone adds a new tag without checking `poseidon.rs`.

**Fix**: Add `pub const DOMAIN_HASH_IR: u32 = 0x02;` to `field.rs` and use it in `poseidon.rs`.

### 2.4 LOW — Bytes32 not fully representable

**File**: `codec.rs:92-103`

```rust
Value::Bytes32(b) => {
    for (i, chunk) in b.chunks_exact(4).enumerate() {
        let val = u32::from_le_bytes(chunk.try_into().unwrap());
        if val >= BabyBear::ORDER_U32 {
            return Err(TabulaError::EncodingError(...));
        }
        fes.push(BabyBear::new(val));
    }
}
```

Each 4-byte chunk must be `< p = 2013265921`. This means:
- Of the 2^32 possible 4-byte values, 2^32 − p = 2,281,701,375 values are rejected per chunk.
- Rejection rate per chunk: ~53.1%.
- Only ~(p/2^32)^8 ≈ 0.15% of all 256-bit values are representable as `Bytes32`.

This is by design — `NativeDigest` has the same constraint (8 canonical BabyBear FE), and `Bytes32` is primarily used as a container for Poseidon2 digest output. But the implication is that `Bytes32` is **not** an arbitrary 256-bit value.

**Recommendation**: Add a doc note to `Value::Bytes32` (in `tabula-core`) or to the codec explaining this constraint. Callers creating `Value::Bytes32` from external data must validate canonicality.

### 2.5 LOW — SsmcList entry count truncation

**File**: `ssmc.rs:150`

```rust
input.push(BabyBear::new(self.entries.len() as u32));
```

Silent truncation if `entries.len() > u32::MAX`. Not a realistic concern for SSMC (small columns by definition, typically ≤ threshold of ~100-300 rows), but a `debug_assert!` would be defensive:

```rust
debug_assert!(self.entries.len() <= u32::MAX as usize, "SSMC entry count overflow");
```

---

## 3. Performance

### 3.1 SSMC commitment is not cached

**File**: `hybrid.rs:159-163`

```rust
pub fn column_commitment(&self, state: &ColumnState<H>) -> NativeDigest {
    match state {
        ColumnState::Ssmc(list) => list.commit(&self.hasher).0,  // O(n) every time
        ColumnState::Smt(tree) => tree.root(),                    // O(1)
    }
}
```

Every call to `column_commitment()` for an SSMC column re-hashes the entire entry list. The SMT path returns a cached root in O(1). This asymmetry means:

- Calling `column_commitment()` twice on the same SSMC column does redundant work.
- The `commit()` method allocates a `Vec` for the hash input on every call.

**Impact**: For the out-of-circuit commitment layer, this is unlikely to be a bottleneck (SSMC columns are small by definition). But it's wasteful and could matter if `column_commitment()` is called in a loop.

**Fix options**:
1. Cache the digest in `SsmcList` (with dirty flag on mutation).
2. Cache at the `ColumnState::Ssmc` level (wrapping `SsmcList` + `Option<NativeDigest>`).
3. Accept the cost and document it.

### 3.2 SMT clone-on-write

**File**: `hybrid.rs:183-184`

```rust
ColumnState::Smt(old_tree) => {
    let mut tree = old_tree.clone();  // clones entire BTreeMap
```

`apply_column_writes` for SMT columns clones the entire tree before applying mutations. This preserves immutability of the old state (important for ColumnMeta which needs both `com_old` and `com_new`), but the clone cost is O(nodes + leaves).

For a tree with depth 32 and N entries, this means cloning up to ~N leaf entries + ~N×32 internal nodes in the worst case (though sparse trees typically have far fewer nodes).

**Impact**: Acceptable for now. The out-of-circuit commitment layer is not in the prover hot path. If SMT columns become common (many columns exceeding threshold), this could be optimized with:
- Copy-on-write via `Rc`/`Arc` (structural sharing)
- In-place mutation with explicit snapshot API

### 3.3 Ephemeral state SMT construction

**File**: `hybrid.rs:234-255`

```rust
pub fn compute_table_root(&self, col_leaves: &BTreeMap<ColId, NativeDigest>) -> NativeDigest {
    let mut tree = SparseMerkleTree::new(self.hasher.clone(), COL_STATE_SMT_DEPTH, DOMAIN_COL);
    for (&col, &leaf) in col_leaves {
        tree.insert(col.0 as u64, leaf);
    }
    tree.root()
}
```

Both `compute_table_root` and `compute_state_root` construct fresh SMT instances from scratch, insert all leaves, extract the root, and drop the tree. The empty-hash precomputation (16 or 32 levels of compression) is repeated on every call.

**Impact**: Negligible for single invocations. Wasteful if called repeatedly across batches. A persistent tree with incremental updates would amortize the setup cost.

### 3.4 Vec allocations in hash_domain

**File**: `hasher.rs:61-65`, `poseidon.rs:62-66`

```rust
fn hash_domain(&self, tag: u32, input: &[BabyBear]) -> NativeDigest {
    let mut prefixed = Vec::with_capacity(1 + input.len());
    prefixed.push(BabyBear::new(tag));
    prefixed.extend_from_slice(input);
    // ...
}
```

Every `hash_domain` call allocates a `Vec`. For small inputs (e.g., empty-leaf hash with 0 input elements), this is a 1-element heap allocation. Could use `SmallVec<[BabyBear; 16]>` or a stack buffer for common small sizes.

**Impact**: Micro-optimization. Not worth addressing unless profiling shows this is hot.

---

## 4. Maintainability

### 4.1 Test Coverage

| Module | Tests | Coverage Notes |
|--------|-------|----------------|
| `field.rs` | 8 | NativeDigest round-trip, non-canonical rejection, u64 limb encoding, domain tag distinctness |
| `hasher.rs` | 5 | MockFieldHasher determinism, distinctness, domain separation |
| `poseidon.rs` | 8 | PoseidonHasher determinism, distinctness, hash_ir all types |
| `codec.rs` | 14 | BabyBearCodec round-trip all types, I64 ordering, Tier 2 TraceEnc, null canonicality |
| `smt.rs` | 11 | Insert/remove/prove/verify, bulk operations, domain separation, depth variation |
| `ssmc.rs` | 13 | Commitment determinism, merge (all source types), delete, complex scenario |
| `hybrid.rs` | 14 | Strategy dispatch, leaf digest, table/state root, apply_writes, full pipeline |
| **Total** | **73** | |

**Missing test scenarios**:
- No test using `PoseidonHasher` for SMT/SSMC/HybridVC operations (all use `MockFieldHasher`). While the trait abstraction should guarantee correctness, at least one integration test with the real hasher would catch any Poseidon2-specific edge cases.
- No test that explicitly verifies strategy migration does NOT happen (i.e., SSMC column stays SSMC after growing past threshold via `apply_column_writes`). This behavior should be documented-by-test.
- No test for `SsmcList::from_sorted` with a single entry (trivial but good for completeness).
- No test for SMT with max-depth paths (depth=32, key near u64::MAX).

### 4.2 Error Handling

| Pattern | Usage | Assessment |
|---------|-------|------------|
| `Result<_, TabulaError>` | codec encode/decode, NativeDigest from_bytes, SsmcList from_sorted, u64 limb decode | Good — propagates errors cleanly |
| `expect()` | `hybrid.rs:142` (`commit_column`), `poseidon.rs:82-83` (`hash_pair`) | Panics on malformed input — caller must guarantee preconditions |
| No error path | `SsmcList::insert`, `SsmcList::remove`, `SparseMerkleTree::insert` | Infallible by design (sorted insertion, BTreeMap ops) |

The `expect()` calls in `commit_column` and `hash_pair` are a maintainability risk. If a caller passes unsorted entries or non-canonical digests, the program panics rather than returning an error. Consider:
- `commit_column`: Change the internal `SsmcList::from_sorted` call to propagate `Result`.
- `hash_pair`: Change to `Result<Digest, TabulaError>` or at minimum document the precondition at the trait level.

### 4.3 Code Clarity

- **Naming**: Excellent. `NativeDigest`, `SsmcList`, `MergeSource`, `ColumnState`, `CommitmentStrategy` — all self-explanatory.
- **Comments**: Adequate. Key design decisions are documented (limb split rationale, domain separation strategy). Could improve with more cross-references to proof-spec sections.
- **No unsafe**: Zero `unsafe` blocks across the entire crate.
- **No panics in core paths**: Only test helpers and the two `expect()` calls mentioned above.

---

## 5. Extensibility

### 5.1 No Strategy Migration

When entries grow past the threshold (SSMC→SMT) or shrink below it (SMT→SSMC), `apply_column_writes` preserves the original strategy:

```rust
// hybrid.rs:177-182
ColumnState::Ssmc(old_list) => {
    let (new_list, com, trace) = SsmcList::merge(...);
    (ColumnState::Ssmc(new_list), ...)  // stays SSMC regardless of new size
}
```

A column that starts as SSMC with 3 entries and accumulates 10,000 writes remains SSMC. The commitment hash chain grows O(n) while an SMT would provide O(log n) updates.

**Current status**: Known deferred item. The proof-optimization-architecture.md mentions threshold calibration as a future task (B7). Strategy migration adds complexity (the AIR must handle both SSMC and SMT proof for the same column across batches).

**Recommendation**: Add a `// TODO: strategy migration` comment in `apply_column_writes` and document the current behavior in the module-level doc.

### 5.2 Hardcoded SMT Depths

```rust
const COL_DATA_SMT_DEPTH: usize = 32;     // row-level key space
const COL_STATE_SMT_DEPTH: usize = 16;    // column-level (max 65536 cols/table)
const TABLE_STATE_SMT_DEPTH: usize = 32;  // table-level (max ~4B tables)
```

These are reasonable defaults but not configurable via `HybridVC::new()`. If the protocol ever needs deeper/shallower trees (e.g., for different key spaces), this requires a code change.

**Recommendation**: Either make depths configurable (add to `HybridVC::new`) or document the assumptions about key-space sizes alongside the constants.

### 5.3 FieldHasher Trait Is Well-Designed for Extension

The trait is minimal: `hash`, `compress`, `hash_domain`, plus a defaulted `zero_digest`. Adding a new hash backend (e.g., Rescue, Griffin, BLAKE3-over-FE) requires only implementing these 3 methods. The associated types `F` and `Digest` provide full flexibility.

### 5.4 Missing Batch-Level API

The current API requires callers to orchestrate per-column operations manually:

```
for each (table, col):
    1. apply_column_writes(old_state, writes) → (new_state, com_new, trace)
    2. compute_leaf(table, col, tag, com_new) → leaf
    3. collect leaves per table
for each table:
    4. compute_table_root(col_leaves) → table_root
5. compute_state_root(table_roots) → state_root
```

A batch-level method like `apply_batch(columns, all_writes) → (new_columns, column_metas, state_root)` would:
- Reduce coordination burden on callers
- Enable internal optimizations (e.g., reuse persistent SMTs, batch hash computations)
- Centralize ColumnMeta construction

---

## 6. Code Quality Checklist

| Criterion | Status | Notes |
|-----------|--------|-------|
| Readable, well-named | Pass | Excellent naming throughout |
| Functions < 50 lines | Pass | Largest function: `SsmcList::merge` at ~115 lines — could be split |
| Files < 800 lines | Pass | Largest: `hybrid.rs` at 522 lines |
| No deep nesting (> 4 levels) | Pass | Max 3 levels in merge match arms |
| Proper error handling | Partial | Two `expect()` calls risk panics (§4.2) |
| No debug statements | Pass | Clean |
| No hardcoded values | Partial | SMT depths hardcoded (§5.2), 0x02 magic number (§2.3) |
| Immutable patterns | Good | Clone-on-write for SMT, immutable `SsmcList::entries()` |
| Consistent style | Pass | Uniform formatting, idiomatic Rust |

### 6.1 `SsmcList::merge` complexity

At ~115 lines, the 3-way merge function is the longest in the crate. The match arms are repetitive (each arm constructs a `MergeStep` with slightly different fields). This could be refactored into helper closures or a `MergeStepBuilder`, but the current form is explicit and easy to audit. Not a priority.

---

## 7. Spec Compliance Summary

| Spec Reference | Implementation | Status |
|----------------|----------------|--------|
| proof-spec §4.2.R: 30+30+4 limb split | `field.rs:83-87` | Correct |
| proof-spec §10.3: ComEnc widths (Bool=1, U64/I64=3, Bytes32=8) | `codec.rs:158-164` | Correct |
| proof-spec §10.3: I64 offset encoding (val + 2^63) | `codec.rs:86-90` | Correct |
| proof-spec §10.3: TraceEnc = ComEnc + val_is_null | `codec.rs:31-47` | Correct |
| proof-spec §10.3: Null canonical zero | `codec.rs:39-41` | Correct |
| proof-spec: Domain tags (SSMC=0x00, SMT=0x01, LEAF=0x10, TABLE=0x11, COL=0x12) | `field.rs:12-20` | Correct |
| proof-spec: Poseidon2 width=16, rate=8, capacity=8 | `poseidon.rs:16-18` | Correct |
| proof-spec: NativeDigest = 8 BabyBear FE | `field.rs:29` | Correct |
| proof-spec: SSMC commitment = Poseidon(0x00 \|\| t \|\| c \|\| entries) | `ssmc.rs:145-157` | Correct (includes count) |
| proof-spec: Two-level state root (SMT_cols → SMT_tables) | `hybrid.rs:234-255` | Correct |
| proof-spec: ColumnMeta leaf = Poseidon(DOMAIN_LEAF \|\| t \|\| c \|\| tag \|\| Com) | `hybrid.rs:212-229` | Correct |
| proof-spec §4: Domain-separated node hashing | `smt.rs:167-179` | **Divergent** — nodes use plain compress (§2.2) |
| semantics-spec §1.5.5: hash_ir encoding | `poseidon.rs:91-101` | Correct (uses 0x02 tag, undocumented) |

---

## 8. Recommended Actions

### Priority 1 — Fix Now (trivial, prevents bugs)

1. **Fix codec.rs doc comment**: Line 17, `"31+31+2 bit limbs"` → `"30+30+4 bit limbs"`.
2. **Add `DOMAIN_HASH_IR` constant** to `field.rs` and reference it in `poseidon.rs:94`.

### Priority 2 — Address Soon (correctness/clarity)

3. **Resolve SMT `node_hash_static` signature**: Remove the unused `_domain_tag` and `_level` parameters, or document the intentional divergence from proof-spec §4 in both code and spec.
4. **Add doc note on Bytes32 canonicality**: Clarify that only ~0.15% of 256-bit values are representable.
5. **Replace `expect()` with `Result`** in `commit_column` and `hash_pair`.

### Priority 3 — Future Work (performance/extensibility)

6. **Cache SSMC commitment digest** to avoid O(n) recomputation.
7. **Add strategy migration** (SSMC↔SMT) in `apply_column_writes`.
8. **Persistent state SMTs** across batches for incremental root updates.
9. **Batch-level API** in `HybridVC` to reduce caller coordination.
10. **Integration test with PoseidonHasher** (not just MockFieldHasher).
