# M4 — Plonky3 Foundation: Design Document

> **Status: ✅ COMPLETE** — Implemented in commit `7ebea54`.

> Complete design for building protocol-level cryptographic primitives.
> All items independently testable. No STARK machinery. No AIR. No traces.
> Pure functions: `f(state, writes) → (new_root, proofs)`.

---

## 1. Philosophy

### 1.1 Native-First, Bytes-Second

Poseidon2 operates on BabyBear field elements (`p = 2^31 - 2^27 + 1 = 2013265921`). The canonical internal representation for all protocol hashing is **`[BabyBear; 8]`** (8 field elements, ~248 bits). Byte arrays `[u8; 32]` exist only at boundaries — public inputs, serialization, mock compatibility. Every internal operation stays in the field.

### 1.2 Two-Layer Abstraction: Bytes vs Field Elements

Hashing exists at two distinct levels in Tabula:

- **`Hasher` trait** (in `tabula-core`) = byte-level abstraction (`&[u8] → [u8; 32]`). Used by the executor for `hash_ir`, batch digests, etc. Mock = Blake3.
- **`FieldHasher` trait** (in `tabula-commitment`) = field-element-level abstraction (`&[F] → Digest`). Used by SMT, SSMC, HybridVC. Default = Poseidon2.

The proof-spec mandates Poseidon2 as the canonical `hash_id`, but the ZK hash landscape evolves quickly (Monolith, Griffin, etc.). By making the commitment layer trait-based:
- Hash function can be swapped by implementing one trait
- Unit tests run with a fast mock hasher (no Poseidon2 overhead)
- Benchmarking between hash functions is trivial
- SMT/SSMC logic stays pure — no coupling to a specific hash

### 1.3 Encoding Is Schema-Driven

`ValueCodec` needs the column's `ValueType` to determine field-element width. There is no universal-width encoding. The two tiers are a hard boundary:

| Tier | Width | Use | Null handling |
|------|-------|-----|---------------|
| **Tier 1 (ComEnc)** | `w(T)` FE | SSMC/SMT commitments | No null — only non-null values committed |
| **Tier 2 (TraceEnc)** | `w(T) + 1` FE | Execution traces (M5) | `val_is_null` boolean appended |

M4 implements **Tier 1 only**. Tier 2 arrives in M5 (witness generation).

### 1.4 Correctness Before Performance

Every data structure starts with the obviously-correct naive implementation. Optimizations (sponge streaming, batch proofs, SIMD) are deferred to after the full proof pipeline works. The goal of M4 is: deterministic, well-tested, spec-compliant primitives.

### 1.5 Zero Impact on Existing Code

All Plonky3 dependencies are behind `features = ["stark"]`. The default build (mock-only) stays lightweight. No existing test, module, or API changes. The commitment crate goes from empty to populated — no migration.

---

## 2. Architecture Rules

### R1. Crate boundary

```
tabula-core          (no p3 deps, ever)
  └── Hasher trait (bytes), ValueCodec trait, Digest = [u8; 32]

tabula-commitment    (p3 deps behind "stark" feature)
  ├── FieldHasher trait (field elements) — generic hash abstraction
  ├── PoseidonHasher: impl FieldHasher + impl Hasher — default production hash
  ├── SMT<H>, SSMC<H>, HybridVC<H> — generic over FieldHasher
  └── NativeDigest, BabyBearCodec — encoding layer

tabula-proof         (p3 deps behind "stark" feature, later)
  └── AIR constraints, STARK prover/verifier (M5+)
```

### R2. Domain separation tags (constants)

All Poseidon calls carry an explicit domain tag as the **first absorbed field element**:

```rust
pub const DOMAIN_SSMC: u32  = 0x00;  // SSMC commitment
pub const DOMAIN_SMT: u32   = 0x01;  // SMT internal node
pub const DOMAIN_LEAF: u32  = 0x10;  // SMT leaf (ColumnMeta)
pub const DOMAIN_TABLE: u32 = 0x11;  // SMT_tables node
pub const DOMAIN_COL: u32   = 0x12;  // SMT_cols node
```

These live in `tabula-commitment/src/field.rs` as `BabyBear` constants.

### R3. NativeDigest conversion

```
NativeDigest([BabyBear; 8])  ←→  Digest([u8; 32])
```

Conversion: each BabyBear → canonical u32 (in `[0, p)`) → 4 LE bytes. 8 × 4 = 32 bytes. Inverse: 4 LE bytes → u32 → BabyBear (rejecting values ≥ p).

### R4. Key encoding in Poseidon

- `TableId(u32)` → 1 BabyBear FE (fits in `[0, 2^31)`)
- `ColId(u16)` → 1 BabyBear FE
- `RowKey(u64)` → **3 BabyBear limbs** (same decomposition as U64 ComEnc):

```
x0 = val & 0x3FFF_FFFF          // bits [0..30)   → [0, 2^30)
x1 = (val >> 30) & 0x3FFF_FFFF  // bits [30..60)  → [0, 2^30)
x2 = val >> 60                   // bits [60..64)  → [0, 16)
```

30+30+4 split (not 31+31+2) because BabyBear p = 2013265921 < 2^31 - 1; 31-bit limbs can exceed p, causing lossy mod-reduction. 30-bit limbs (max 2^30-1 = 1073741823) are always < p.

One encoding scheme for all u64 values — keys and data share `encode_u64_limbs()`.

### R5. Poseidon2 configuration

| Param | Value |
|-------|-------|
| Field | BabyBear (`p = 2^31 - 2^27 + 1 = 2013265921`) |
| S-box | x^7 |
| Full rounds | 8 (4 + 4) |
| Partial rounds | 13 |
| **Width** | **16** (single permutation for both sponge and compression) |
| Rate | 8 (= width - capacity) |
| Capacity | 8 |
| Digest size | 8 FE (~248 bits, ~124-bit collision resistance) |

Single permutation: `default_babybear_poseidon2_16()`. Both sponge hashing and Merkle compression use the same width-16 permutation — sponge absorbs 8 FE/round, compression takes 2×8 FE input.

### R6. No `unsafe`, no custom crypto

All field arithmetic via Plonky3's `MontyField31` (Montgomery form, constant-time). No hand-rolled modular arithmetic. No `unsafe` blocks. Trust the library.

### R7. FieldHasher trait design

```rust
/// Field-element-level hash abstraction for the commitment layer.
/// Distinct from core::Hasher (which is byte-level).
pub trait FieldHasher: Clone + Send + Sync {
    /// The field element type.
    type F: Clone + Copy + Default + Eq + Send + Sync;
    /// The digest type (fixed-size output).
    type Digest: Clone + Copy + Default + Eq + Send + Sync + Debug;

    /// Hash a variable-length sequence of field elements.
    fn hash(&self, input: &[Self::F]) -> Self::Digest;
    /// 2-to-1 compression (for Merkle tree internal nodes).
    fn compress(&self, left: &Self::Digest, right: &Self::Digest) -> Self::Digest;
    /// Domain-separated hash (tag prepended before input).
    fn hash_domain(&self, tag: u32, input: &[Self::F]) -> Self::Digest;

    /// The zero/empty digest (identity for empty trees).
    fn zero_digest(&self) -> Self::Digest { Self::Digest::default() }
}
```

All commitment data structures (`SparseMerkleTree`, `SsmcList`, `HybridVC`) are generic over `H: FieldHasher`. This enables:
- **Testing**: Mock hasher (e.g., xor-fold) for fast unit tests
- **Benchmarking**: Compare Poseidon2 vs future alternatives
- **Swappability**: Change hash function without touching tree/commitment logic

---

## 3. File Structure

```
tabula-commitment/src/
├── lib.rs          # Feature gates, module declarations, re-exports
├── hasher.rs       # FieldHasher trait + MockFieldHasher (for tests)
├── field.rs        # NativeDigest, domain tags, BabyBear helpers, RowKey encoding
├── codec.rs        # BabyBearCodec: ValueCodec<FieldRepr = BabyBear>
├── poseidon.rs     # PoseidonHasher: impl FieldHasher + impl Hasher
├── smt.rs          # SparseMerkleTree<H>, MerkleProof, SmtError
├── ssmc.rs         # SsmcList<H>, SsmcCommitment, MergeTrace, MergeStep
└── hybrid.rs       # HybridVC<H>, ColumnMeta, ColumnProof, strategy dispatch
```

8 files (including lib.rs), estimated ~1500-2000 LOC total.

---

## 4. Type Design

### 4.0 FieldHasher Trait and Implementations

The trait (in `hasher.rs`) defines the interface. Two implementations:

**PoseidonHasher** (production, in `poseidon.rs`):
```rust
impl FieldHasher for PoseidonHasher {
    type F = BabyBear;
    type Digest = NativeDigest;  // [BabyBear; 8]
    ...
}
```

**MockFieldHasher** (testing, in `hasher.rs`):
```rust
/// Fast mock: xor-fold input into 8 elements. NOT cryptographic.
/// Useful for testing tree/commitment logic without Poseidon overhead.
pub struct MockFieldHasher;

impl FieldHasher for MockFieldHasher {
    type F = BabyBear;
    type Digest = NativeDigest;
    ...
}
```

Both share `F = BabyBear` and `Digest = NativeDigest`, so all data structures work with either. The mock lets SMT/SSMC unit tests run without Poseidon2.

### 4.1 NativeDigest

```rust
/// 8 BabyBear field elements — canonical Poseidon2 output.
/// This is the primary hash representation inside the commitment layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeDigest(pub [BabyBear; 8]);

impl NativeDigest {
    pub const ZERO: Self = Self([BabyBear::ZERO; 8]);

    /// Convert to byte-level Digest (for public inputs / serialization).
    pub fn to_bytes(&self) -> Digest { ... }

    /// Convert from byte-level Digest. Rejects non-canonical values.
    pub fn from_bytes(bytes: &Digest) -> Result<Self, CommitmentError> { ... }
}
```

### 4.2 BabyBear Value Encoding (Tier 1 — ComEnc)

```rust
impl BabyBearCodec {
    /// Encode a non-null Value into w(T) field elements.
    pub fn com_enc(&self, value: &Value) -> Vec<BabyBear> { ... }

    /// Decode w(T) field elements back to Value.
    pub fn com_dec(&self, fes: &[BabyBear], ty: ValueType) -> Result<Value, ...> { ... }

    /// Number of field elements for a given type (Tier 1).
    pub fn width(&self, ty: ValueType) -> usize {
        match ty {
            ValueType::Bool => 1,
            ValueType::U64 | ValueType::I64 => 3,
            ValueType::Bytes32 => 8,
        }
    }
}
```

Width table:

| Type | Width | Encoding |
|------|-------|----------|
| Bool | 1 | `{0, 1}` |
| U64 | 3 | `(x0, x1, x2)` where `x0, x1 ∈ [0, 2^30)`, `x2 ∈ [0, 16)` |
| I64 | 3 | offset: `(val + 2^63)` → same 3-limb as U64 (order-preserving) |
| Bytes32 | 8 | 8 native BabyBear FE (Poseidon2 squeeze output) |

**U64 decomposition** (3 limbs, total 64 bits):
```
x0 = val & 0x3FFF_FFFF          // bits [0..30)   → [0, 2^30)
x1 = (val >> 30) & 0x3FFF_FFFF  // bits [30..60)  → [0, 2^30)
x2 = val >> 60                   // bits [60..64)  → [0, 16)
```

All three fit in BabyBear (max limb = 2^30-1 = 1073741823 < p = 2013265921).

**I64 offset encoding**: `encoded = (val as i128 + 2^63) as u64` → then 3-limb U64 encoding. This maps `i64::MIN → 0`, `0 → 2^63`, `i64::MAX → 2^64 - 1`. Preserves ordering.

**Bytes32**: Already 8 BabyBear FE from Poseidon2 squeeze — store as-is. For non-Poseidon Bytes32 (e.g., externally-provided keys), decompose: 32 bytes → 8 × 4-byte LE → 8 BabyBear (rejecting if any chunk ≥ p).

### 4.3 Poseidon2 Sponge Wrapper

```rust
pub struct PoseidonHasher {
    sponge: PaddingFreeSponge<Poseidon2BabyBear<16>, 16, 8, 8>,
    compress_fn: TruncatedPermutation<Poseidon2BabyBear<16>, 2, 8, 16>,
}
```

Implements **two** traits:

```rust
/// Field-element-level interface — used by SMT, SSMC, HybridVC.
impl FieldHasher for PoseidonHasher {
    type F = BabyBear;
    type Digest = NativeDigest;

    fn hash(&self, input: &[BabyBear]) -> NativeDigest { ... }
    fn compress(&self, left: &NativeDigest, right: &NativeDigest) -> NativeDigest { ... }
    fn hash_domain(&self, tag: u32, input: &[BabyBear]) -> NativeDigest { ... }
}

/// Byte-level interface — used by executor (hash_ir, batch digest, etc.).
impl Hasher for PoseidonHasher {
    fn hash(&self, data: &[u8]) -> Digest { ... }
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest { ... }
    /// Override: uses native FE encoding per semantics-spec §1.5.5.
    fn hash_ir(&self, inputs: &[Value]) -> Digest { ... }
}
```

### 4.4 Sparse Merkle Tree

```rust
pub struct SparseMerkleTree<H: FieldHasher> {
    hasher: H,
    depth: usize,                                    // 64 for cell-level, 16-32 for meta-level
    nodes: BTreeMap<(usize, u64), H::Digest>,        // (level, index) → hash
    leaves: BTreeMap<u64, H::Digest>,                 // key → leaf value
    empty_hashes: Vec<H::Digest>,                     // empty[i] for each level
    domain_tag: u32,                                  // DOMAIN_SMT, DOMAIN_COL, or DOMAIN_TABLE
}

pub struct MerkleProof<D> {
    pub key: u64,
    pub value: Option<D>,      // None = non-membership
    pub siblings: Vec<D>,      // length = depth
    pub path_bits: Vec<bool>,  // key decomposed into bits
}
```

Node hash formula:
```
node_hash(level, left, right) = Poseidon(domain_tag || level || left[0..8] || right[0..8])
```

Input: 1 (tag) + 1 (level) + 8 (left) + 8 (right) = 18 FE. With rate=8 sponge: 3 absorptions (8+8+2). For compression (2 digest inputs only), use `TruncatedPermutation` directly (16 FE = full width).

Empty tree: `empty[0] = hasher.zero_digest()`. `empty[i+1] = node_hash(i, empty[i], empty[i])`. Precomputed at construction.

### 4.5 SSMC

```rust
/// A sorted list of (key, value) entries for a single (table, col).
pub struct SsmcList<H: FieldHasher> {
    table: TableId,
    col: ColId,
    /// Sorted by key. Invariant: no duplicate keys, strictly ascending.
    entries: Vec<SsmcEntry<H>>,
}

pub struct SsmcEntry<H: FieldHasher> {
    pub key: RowKey,
    pub value: Vec<H::F>,  // ComEnc(T), width = w(T)
}

/// The commitment digest to an SsmcList.
pub struct SsmcCommitment<D>(pub D);
```

Commitment formula (hash chain):
```
absorb: [DOMAIN_SSMC, t, c, n, k_0, ...ComEnc(v_0)..., k_1, ...ComEnc(v_1)..., ...]
result: hasher.hash(&input) → H::Digest
```

where `n` = number of entries, `t` = table as FE, `c` = col as FE, `k_i` = key as 3-limb FE.

Empty column: `Com_empty = Poseidon(DOMAIN_SSMC || t || c || 0)` (n=0, no entries).

**Membership proof**: witness = entry at index `i`, plus `prev_key` and `next_key` for gap proof.

**Non-membership proof**: witness = the two adjacent entries `(k_i, k_{i+1})` where `k_i < target < k_{i+1}`, with strict inequality decomposed into BabyBear limbs. Boundary cases: before-first (`is_first=true`), after-last (`is_last=true`).

**Merge trace**:

```rust
pub struct MergeStep<F> {
    pub key: RowKey,
    pub source: MergeSource,
    pub old_val: Option<Vec<F>>,   // from OldList (None if write_only)
    pub write_val: Option<Vec<F>>, // from WriteSet (None if old_only)
    pub new_val: Option<Vec<F>>,   // in NewList (None if deleted)
    pub in_new: bool,              // true if key appears in NewList
}

pub enum MergeSource {
    OldOnly,    // (0, 1) — key only in old
    WriteOnly,  // (1, 0) — key only in writes
    Both,       // (1, 1) — key in both → write overwrites
}
```

Delete = `MergeSource::Both` with `write_val = None` → `in_new = false`.

### 4.6 Hybrid VC

```rust
pub enum CommitmentStrategy {
    Ssmc,  // ≤ threshold rows
    Smt,   // > threshold rows
}

pub struct ColumnMeta<D> {
    pub table: TableId,
    pub col: ColId,
    pub tag: CommitmentStrategy,
    pub com_old: D,
    pub com_new: D,
    pub is_empty_old: bool,
    pub is_empty_new: bool,
    pub is_touched: bool,
}

pub struct HybridVC<H: FieldHasher> {
    threshold: usize,  // TBD, estimated 100-300
    hasher: H,
}
```

Two-level root:
```
LeafDigest(t, c) = Poseidon(DOMAIN_LEAF || t || c || tag || Com[t,c])
TableRoot[t]     = SMT_cols(key=c, value=LeafDigest(t,c))
StateRoot        = SMT_tables(key=t, value=TableRoot[t])
```

---

## 5. Implementation Plan

### P1. Plonky3 Workspace Dependencies

**Files changed**: 2 (`Cargo.toml` workspace, `tabula-commitment/Cargo.toml`)

Workspace additions:
```toml
# Plonky3 (BabyBear + Poseidon2)
p3-field       = "0.4"
p3-baby-bear   = "0.4"
p3-poseidon2   = "0.4"
p3-symmetric   = "0.4"
p3-merkle-tree = "0.4"
```

Commitment crate:
```toml
[features]
default = []
stark = ["p3-field", "p3-baby-bear", "p3-poseidon2", "p3-symmetric"]

[dependencies]
tabula-core = { workspace = true }
p3-field      = { workspace = true, optional = true }
p3-baby-bear  = { workspace = true, optional = true }
p3-poseidon2  = { workspace = true, optional = true }
p3-symmetric  = { workspace = true, optional = true }
```

**Acceptance**: `cargo check -p tabula-commitment --features stark` compiles. `cargo check` (no features) still compiles with empty crate.

---

### P2. BabyBear ValueCodec

**New file**: `tabula-commitment/src/codec.rs`
**Also touches**: `tabula-commitment/src/field.rs` (new), `lib.rs`

`field.rs` contains:
- `NativeDigest` type + `to_bytes`/`from_bytes`
- Domain tag constants
- `encode_u64_limbs(val: u64) -> [BabyBear; 3]` helper
- `decode_u64_limbs(limbs: &[BabyBear; 3]) -> Result<u64>` helper

`codec.rs` contains:
- `BabyBearCodec` struct implementing `ValueCodec<FieldRepr = BabyBear>`
- `com_enc(&self, value: &Value) -> Vec<BabyBear>` (Tier 1)
- `com_dec(&self, fes: &[BabyBear], ty: ValueType) -> Result<Value>` (Tier 1 decode)
- `width(ty: ValueType) -> usize`

**Tests** (~15):
- Round-trip encode/decode for each type (U64, I64, Bool, Bytes32)
- U64 boundary values: 0, 1, `u64::MAX`, `2^31 - 1`, `2^31`, `2^62`
- I64 boundary: `i64::MIN`, -1, 0, 1, `i64::MAX`
- I64 offset ordering preservation: `encode(a) < encode(b) iff a < b` (lexicographic on limbs)
- `field_elements_per()` returns correct widths: 1, 3, 3, 8
- NativeDigest ↔ Digest round-trip
- NativeDigest::from_bytes rejects non-canonical (value ≥ p)

**Depends on**: P1

---

### P3. FieldHasher Trait + Poseidon2 Implementation

**New files**: `tabula-commitment/src/hasher.rs`, `tabula-commitment/src/poseidon.rs`
**Touches**: `lib.rs`

`hasher.rs` contains:
- `FieldHasher` trait definition (see R7)
- `MockFieldHasher` — fast non-cryptographic implementation for unit tests

`poseidon.rs` contains:
- `PoseidonHasher` struct
- `impl FieldHasher for PoseidonHasher` (native FE interface)
- `impl Hasher for PoseidonHasher` (byte interface + `hash_ir` override)
- Constructor: `PoseidonHasher::new()` — from `default_babybear_poseidon2_16()`

**`hash_ir` override** (normative encoding per §1.5.5):
```
Poseidon(DOMAIN_TAG_HASH_IR_FE || n || ComEnc(x_0) || ... || ComEnc(x_{n-1}))
```
Uses BabyBearCodec internally for `ComEnc`. Domain tag = `0x02` as BabyBear FE.

**Tests** (~12):
- FieldHasher: `hash` deterministic (same input → same output)
- FieldHasher: `hash` distinct (different input → different output)
- FieldHasher: `compress` deterministic
- FieldHasher: `hash_domain` with different tags produces different outputs
- MockFieldHasher: satisfies same determinism/distinctness properties
- PoseidonHasher: `Hasher::hash` byte interface works
- PoseidonHasher: `Hasher::hash_pair` works
- PoseidonHasher: `Hasher::hash_ir` produces same result as manual native encoding
- PoseidonHasher: `hash_ir` with empty inputs
- PoseidonHasher: `hash_ir` with each value type

**Depends on**: P1, P2 (uses BabyBearCodec for `hash_ir`)

---

### P4. Sparse Merkle Tree

**New file**: `tabula-commitment/src/smt.rs`

Types:
- `SparseMerkleTree<H: FieldHasher>` — parameterized by hasher, `depth`, and `domain_tag`
- `MerkleProof<D>` — siblings + path bits (generic over digest type)
- `SmtUpdate<D>` — old value, new value, proof

Operations:
- `new(hasher, depth, domain_tag)` → empty tree with precomputed empty hashes
- `root() → H::Digest`
- `get(key) → Option<H::Digest>`
- `insert(key, value) → MerkleProof<H::Digest>`
- `remove(key) → MerkleProof<H::Digest>`
- `prove(key) → MerkleProof<H::Digest>` (membership or non-membership)
- `verify_proof(root, proof, hasher) → bool` (static, no &self)
- `update(key, old_val, new_val) → (old_root, new_root, SmtUpdate<H::Digest>)`

Note: Unit tests use `MockFieldHasher` for fast execution. Integration tests use `PoseidonHasher`.

Internal:
- `empty_hashes: Vec<H::Digest>` — `empty[0] = zero_digest()`, `empty[i+1] = node_hash(i, empty[i], empty[i])`
- `nodes: BTreeMap<(usize, u64), H::Digest>` — sparse storage (level, path_prefix)
- Key bits: `bit(i) = (key >> i) & 1` for level `i` (LSB first)

Node hash: `hasher.hash_domain(domain_tag, &[level, ...left, ...right])`.

**Tests** (~15):
- Empty tree has deterministic root
- Insert single key, verify root changes
- Insert + prove → membership valid
- Prove absent key → non-membership valid
- Insert + remove → root returns to empty
- 100-key insert, all membership proofs valid
- Non-membership on populated tree
- Different domain tags → different roots (same data)
- Depth parameter works (depth=16 for ColId, depth=32 for TableId, depth=64 for RowKey)
- Update proof: `oldRoot → newRoot` verifiable
- Duplicate insert (update value) produces correct new root
- 1000-key stress test: all proofs valid after bulk insert

**Depends on**: P3

---

### P5. SSMC (Sorted Sparse Map Commitment)

**New file**: `tabula-commitment/src/ssmc.rs`

Types:
- `SsmcList<H: FieldHasher>` — sorted entries for one (table, col)
- `SsmcEntry<H: FieldHasher>` — `(RowKey, Vec<H::F>)`
- `SsmcCommitment<D>` — the hash chain result
- `SsmcMembershipProof` — entry + position witness
- `SsmcNonMembershipProof` — adjacent entries + gap decomposition
- `MergeTrace` — Vec of `MergeStep`
- `MergeStep` — `(key, source, old_val, write_val, new_val, in_new)`
- `MergeSource` — `OldOnly | WriteOnly | Both`

Operations:
- `SsmcList::new(table, col)` → empty list
- `SsmcList::from_sorted(table, col, entries)` → validates sorted + unique
- `SsmcList::insert(key, value)` → inserts maintaining sort
- `SsmcList::commit(hasher, codec) → SsmcCommitment`
- `SsmcList::prove_membership(key) → SsmcMembershipProof`
- `SsmcList::prove_non_membership(key) → SsmcNonMembershipProof`
- `SsmcList::merge(old, writes, hasher, codec) → (SsmcList, SsmcCommitment, MergeTrace)`

Commitment formula:
```
input = [DOMAIN_SSMC, t_fe, c_fe, n_fe, k_0[0..3], v_0[0..w], k_1[0..3], v_1[0..w], ...]
Com = hasher.hash(&input)
```

Empty: `input = [DOMAIN_SSMC, t_fe, c_fe, 0]` → `Com_empty`.

Non-membership gap proof:
- Interior: `k_i < target < k_{i+1}` → strict inequality witnesses `(target - k_i - 1)` and `(k_{i+1} - target - 1)` decomposed into BabyBear limbs (3 limbs each, all in `[0, p)`)
- Before-first: `target < k_0` → witness `(k_0 - target - 1)` with `is_first=true`
- After-last: `target > k_{n-1}` → witness `(target - k_{n-1} - 1)` with `is_last=true`
- Empty list: trivially non-member (proof = `is_empty=true`)

**Tests** (~20):
- Empty list commitment is deterministic
- Single-entry commit + verify
- Multi-entry commit is deterministic
- Membership proof: valid for existing key
- Membership proof: invalid for wrong key
- Non-membership: interior gap
- Non-membership: before first
- Non-membership: after last
- Non-membership: empty list
- Merge: old_only (key only in old → carried to new)
- Merge: write_only (new key → added to new)
- Merge: both (overwrite → new value in new)
- Merge: delete (write null → key removed from new)
- Merge: complex scenario (mix of all sources)
- Merge: resulting list is sorted and unique
- Merge: trace is complete (every key accounted for)
- Merge: empty old + writes → new equals writes
- Merge: old + empty writes → new equals old
- Unsorted input → rejected
- Duplicate keys → rejected

**Depends on**: P2 (BabyBearCodec for ComEnc), P3 (PoseidonHasher)

---

### P6. Hybrid State Commitment

**New file**: `tabula-commitment/src/hybrid.rs`
**Touches**: `lib.rs`

Types:
- `HybridVC<H: FieldHasher>` — main orchestrator
- `ColumnState<H>` — enum { Ssmc(SsmcList\<H\>) | Smt(SparseMerkleTree\<H\>) }
- `ColumnMeta<H::Digest>` — `(table, col, tag, com_old, com_new, is_empty_old, is_empty_new, is_touched)`
- `ColumnProof<H>` — enum { SsmcProof(...) | SmtProof(MerkleProof\<H::Digest\>) }
- `StateCommitment<H>` — full state: all columns, table roots, global root

Operations:
- `HybridVC::new(hasher, codec, threshold)` → empty state
- `HybridVC::commit_column(table, col, entries, value_type) → (ColumnState<H>, H::Digest)`
  - Dispatches to SSMC or SMT based on `entries.len() vs threshold`
- `HybridVC::compute_leaf(table, col, tag, commitment) → H::Digest`
  - `hash_domain(DOMAIN_LEAF, &[t, c, tag, ...Com])`
- `HybridVC::compute_table_root(table, col_leaves) → H::Digest`
  - Builds `SMT_cols` from column leaves (depth=16, domain=DOMAIN_COL)
- `HybridVC::compute_state_root(table_roots) → H::Digest`
  - Builds `SMT_tables` from table roots (depth=32, domain=DOMAIN_TABLE)
- `HybridVC::apply_writes(old_state, write_set, schemas) → (new_state, Vec<ColumnMeta>, new_root)`
  - For each touched (t,c): compute new commitment, build ColumnMeta, update root

Two-level root construction:
```
for each table t:
    for each col c in t:
        leaf[t,c] = Poseidon(DOMAIN_LEAF || t || c || tag_c || Com[t,c])
    TableRoot[t] = SMT_cols(depth=16, domain=DOMAIN_COL, leaves=leaf[t,*])
StateRoot = SMT_tables(depth=32, domain=DOMAIN_TABLE, leaves=TableRoot[*])
```

**Tests** (~12):
- Strategy dispatch: small column → SSMC, large → SMT
- Leaf digest deterministic
- Table root from single column
- Table root from multiple columns
- State root from single table
- State root from multiple tables
- apply_writes: single column update, root changes correctly
- apply_writes: multi-column update
- apply_writes: delete (write null) updates commitment
- ColumnMeta fields correct (is_touched, is_empty transitions)
- Empty column: `is_empty_old=true`, `com_old = Com_empty`
- Round-trip: commit → open → verify for both strategies

**Depends on**: P4 (SMT), P5 (SSMC)

---

## 6. Execution Order

```
P1 (deps)                    ← START HERE
  │
  ├── P2 (codec + field)     ← parallel with P3
  │
  └── P3 (FieldHasher trait  ← parallel with P2
  │    + MockFieldHasher
  │    + PoseidonHasher)
  │     │
  │     ├── P4 (SMT<H>)      ← parallel with P5 (unit tests use MockFieldHasher)
  │     │
  │     └── P5 (SSMC<H>)     ← parallel with P4 (unit tests use MockFieldHasher)
  │           │
  │           └── P6 (HybridVC<H>) ← needs both P4 and P5
  │
  └── Integration tests      ← after P6, use PoseidonHasher for real crypto
```

**Critical path**: P1 → P3 → P4 → P6 (or P1 → P3 → P5 → P6, same length)

**Parallelism**:
- P2 ∥ P3 after P1
- P4 ∥ P5 after P3

---

## 7. Testing Strategy

### Unit tests per module

| Module | Test count (est.) | Key coverage |
|--------|-------------------|--------------|
| hasher.rs | 5 | FieldHasher trait contract, MockFieldHasher properties |
| field.rs | 8 | NativeDigest conversion, limb encoding, domain tags |
| codec.rs | 15 | Round-trip all types, boundary values, width correctness |
| poseidon.rs | 12 | FieldHasher + Hasher impls, determinism, domain separation |
| smt.rs | 15 | CRUD, proofs, depth param, stress test (uses MockFieldHasher) |
| ssmc.rs | 20 | Commitment, membership, non-membership, merge (uses MockFieldHasher) |
| hybrid.rs | 12 | Strategy dispatch, two-level root, apply_writes (uses MockFieldHasher) |
| **Total** | **~87** | |

**Test speed strategy**: smt.rs, ssmc.rs, hybrid.rs unit tests use `MockFieldHasher` for fast execution (~ms). Integration tests in `tests/` use `PoseidonHasher` to verify real crypto correctness.

### Integration tests

Place in `tabula-commitment/tests/`:

1. **`state_transition.rs`**: Build initial state → apply batch of writes → verify root transition. Full pipeline: `BabyBearCodec` + `PoseidonHasher` + `HybridVC`.

2. **`cross_crate.rs`**: Verify `PoseidonHasher` works as `dyn Hasher` in executor context. Run a simple batch through executor with PoseidonHasher, verify `hash_ir` produces correct results.

### Property-based tests (proptest)

For codec: `∀ v: Value, decode(encode(v)) == v`
For SMT: `∀ keys, all membership proofs valid after bulk insert`
For SSMC: `∀ sorted entries, commit is deterministic`
For merge: `∀ (old, writes), merge result is sorted ∧ complete`

---

## 8. Acceptance Criteria (M4 Complete)

- [ ] `cargo check -p tabula-commitment --features stark` compiles
- [ ] `cargo test -p tabula-commitment --features stark` — all ~80 unit tests pass
- [ ] `cargo test -p tabula-commitment --features stark` — 2 integration tests pass
- [ ] `cargo clippy -p tabula-commitment --features stark` — zero warnings
- [ ] `cargo check` (no features) — still compiles, empty crate
- [ ] `FieldHasher` trait defined, `MockFieldHasher` + `PoseidonHasher` both implement it
- [ ] SMT, SSMC, HybridVC are generic over `H: FieldHasher`
- [ ] No changes to any other crate (tabula-core, executor, lang, cli, proof)
- [ ] NativeDigest ↔ Digest round-trip correct
- [ ] BabyBearCodec width: Bool=1, U64=3, I64=3, Bytes32=8
- [ ] I64 offset encoding preserves ordering
- [ ] All domain tags distinct and match spec
- [ ] SMT: empty root deterministic, 1000-key stress test passes
- [ ] SSMC: empty commitment deterministic, merge trace complete
- [ ] Hybrid: strategy dispatch correct, two-level root correct
- [ ] State root transition: `old_root → new_root` verified end-to-end

---

## 9. Non-Goals (Deferred)

| Item | Why deferred | When |
|------|-------------|------|
| Tier 2 (TraceEnc) | Only needed for execution traces | M5 (witness gen) |
| In-circuit constraints | Commitment computes; proof constrains | M5 (AIR) |
| Sponge streaming optimization | Correctness first, optimize later | Post-B7 |
| Threshold calibration | Needs real constraint cost data | B7 |
| STARK prover/verifier | Separate concern | M5 |
| `p3-merkle-tree` MerkleTreeMmcs | Plonky3's tree is for FRI, not state SMT | N/A |
| LogUp argument | In-circuit only, not out-of-circuit | M5/M6 |

---

## 10. Risk Log

| Risk | Impact | Mitigation |
|------|--------|------------|
| Plonky3 0.4 API instability | Build breaks | Pin exact version in Cargo.lock, test in CI |
| BabyBear p ≠ 2^31 - 1 (it's 2^31 - 2^27 + 1) | Wrong range checks | Use `BabyBear::ORDER_U32` constant, never hardcode modulus |
| Bytes32 → 8 BabyBear lossy (4 bytes > p) | Encoding failure | Reject non-canonical Bytes32 at encoding boundary |
| SMT depth=64 memory for sparse trees | OOM on large state | Sparse BTreeMap storage, only store non-empty nodes |
| SSMC O(n) commitment recomputation | Slow for large columns | Acceptable for M4 (SSMC only for small columns ≤ threshold) |
| Merge trace correctness | Wrong state transitions | Exhaustive property-based testing |
