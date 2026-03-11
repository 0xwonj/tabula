# Binius Proving System Research

> Deep analysis of Binius / Binius64 as a potential crypto foundation for Tabula.

## Executive Summary

Binius is a SNARK proving system built on binary tower fields (GF(2^k)) rather than
prime fields like BabyBear. Developed by Irreducible (formerly Ulvetanna), it trades
larger proof sizes and slower verification for significantly faster proving and zero
embedding overhead for small data types. The original Binius repo was archived
September 2025 in favor of Binius64, which operates natively on 64-bit machine words.

**Key finding**: Binius is architecturally compelling for computation-heavy workloads
(hash verification, bitwise logic, VM execution) but is a poor fit for Tabula's
database proving model, which centers on algebraic constraints over structured data
(field elements, cell values, state transitions) rather than bitwise operations.

---

## 1. Field: Binary Tower Construction

Binary tower fields are built by iterated quadratic extension starting from GF(2):

```
T_0 = GF(2) = {0, 1}
T_{k+1} = T_k[X_k] / <X_k^2 + X_{k-1}*X_k + 1>
```

This produces fields with 2^(2^k) elements:
- T_3 = GF(2^8) = 256 elements (byte)
- T_4 = GF(2^16) = 65,536 elements
- T_7 = GF(2^128) = 128-bit security level

### Arithmetic properties

| Operation | Cost | Note |
|-----------|------|------|
| Addition | XOR | Single CPU cycle, no carry propagation |
| Multiplication | Recursive Karatsuba | Sub-quadratic, 3 sub-muls per level |
| Coordinate extraction | Bit manipulation | Free — elements ARE bitstrings |
| Reduction | XOR + reindex | Irreducible polys are simple trinomials |

### Comparison with prime fields

| Aspect | Binary tower (Binius) | BabyBear (p = 2^31 - 2^27 + 1) |
|--------|----------------------|----------------------------------|
| Element size | Power-of-2 bits | 31 bits |
| Addition | XOR (1 cycle) | Modular add (1-2 cycles) |
| Multiplication | Karatsuba chain | Montgomery / Barrett |
| Data embedding | Zero overhead — bits map directly | Small values waste field capacity |
| Hardware support | GFNI (Intel), NEON (ARM) | Standard integer ALU |
| Extension field | Natural tower inclusions | EF4 = quartic extension |

The fundamental advantage: in BabyBear, a boolean value (0 or 1) occupies a 31-bit
field element. In binary fields, it occupies 1 bit. This matters when proving
bit-heavy computations (hashes, bitwise logic) but matters less for algebraic
relations over structured data.

---

## 2. Polynomial Commitment Scheme (PCS)

Binius uses multilinear polynomials F(x_1, ..., x_k) evaluated on the Boolean
hypercube {0,1}^k, rather than univariate polynomials on roots of unity.

### Evolution of the PCS

| Version | Scheme | Proof size (2^32 coefficients over F_2) | Asymptotic |
|---------|--------|----------------------------------------|------------|
| Original | Brakedown-style (linear codes) | ~11.5 MiB | O(sqrt(n)) |
| FRI-Binius | Adapted BaseFold for binary towers | ~3.5 MiB | Polylogarithmic |
| Binius64 | Optimized FRI-Binius | ~300 KiB (benchmarked circuits) | Polylogarithmic |

### How FRI-Binius works

FRI-Binius combines three ideas:
1. Binary field FRI protocol (Ben-Sasson et al.)
2. BaseFold multilinear PCS (Zeilberger, Chen, Fisch)
3. Binius block-level encoding

It supports tiny binary fields (even F_2) with no embedding overhead during
commitment, and reduces proof sizes from O(sqrt(n)) to polylogarithmic.

### Comparison with Tabula's current PCS

Tabula uses FRI over BabyBear with Poseidon2 hashing (via Plonky3). The PCS
operates on univariate polynomials evaluated at roots of unity. Switching to
Binius would require replacing the entire polynomial commitment pipeline,
including the Fiat-Shamir transcript, Merkle tree structure, and opening proofs.

---

## 3. GKR-LogUp Integration

GKR (Goldwasser-Kalai-Rothblum) is a native component of Binius, not a bolt-on.

### In Binius64

- **MUL constraints** use GKR to verify 64-bit unsigned integer multiplications
- **AND constraints** use "a variant of Gruen's univariate skip"
- Both reduce to a common "shift reduction" internal component
- The sumcheck protocol ties everything together

### Comparison with Tabula's LogUp

Tabula currently uses a custom LogUp implementation with EF4 cumulative sums
and Fiat-Shamir challenges (see `stark/src/permutation/`). The GKR approach
in Binius is more efficient for binary-field operations but is tightly coupled
to the binary tower field structure — it cannot be extracted and used with
BabyBear fields.

---

## 4. Constraint System

### Binius V0: M3 Arithmetization

M3 (Multi-Multiset Matching) is Binius's original arithmetization framework:

- **Tables** replace sequential execution traces. Each table has fixed-width rows
  with columns typed by tower field height
- **Channels** connect tables via push/pull semantics. Channel balance (pushes = pulls)
  proves correctness
- **No global trace** — computation decomposes into modular, independent tables
- **No temporality** — tables have no inherent time ordering
- **Flexible lengths** — prover populates tables of arbitrary size

This is conceptually similar to Tabula's sharded architecture (independent
MemoryShard, StateShard, MetaShard tables connected by buses).

### Binius64: R1CS-like over 64-bit words

Binius64 replaces M3 with a simpler constraint language:

- Operates natively on 64-bit unsigned integers (not field elements)
- XOR-of-shifts instead of linear combinations
- AND constraints for bitwise operations
- MUL constraints for 64-bit unsigned multiplication
- Shift operations (logical left/right, arithmetic right) built in
- 64-fold reduction in constraint complexity vs bit-level approaches

### API for circuit definition (M3 example)

```rust
// Define a table with committed columns
let mut table = cs.add_table("merkle_tree_roots");
let root_id = table.add_committed("root_id");
let digest = table.add_committed_multiple("digest");

// Push/pull on channels
nodes_channel.push(root_id, digest, depth, index);
nodes_channel.pull(child_digest);

// Fill tables via TableFiller trait
witness.fill_table_parallel(&self.table, &events)?;
```

### Comparison with Tabula's AIR constraint system

| Aspect | Tabula (Plonky3 AIR) | Binius M3 | Binius64 |
|--------|---------------------|-----------|----------|
| Primitive | Field element (BabyBear) | Tower field element | 64-bit word |
| Constraint style | AIR polynomial identities | Table+Channel balance | R1CS-like AND/MUL |
| Inter-chip comm | LogUp buses | M3 channels | N/A (single circuit) |
| Trace structure | Row-major matrices | Independent tables | Flat witness |
| Extension | ChipId/BusId registration | Table/channel definition | Fixed constraint types |

---

## 5. Applications and Suitability

### Where Binius excels

- **Hash function proving**: Keccak, Blake2s, SHA-256 — native bitwise operations
- **Ethereum state proofs**: MPT inclusion proofs (Irreducible's first production app)
- **VM execution verification**: Bitwise ALU operations map naturally
- **Signature aggregation**: ECDSA, hash-based signatures (XMSS+WOTS)

### Where Binius is less suitable

- **Algebraic constraints**: Operations like "a * b = c mod p" over prime fields
  are unnatural in binary fields
- **Structured data proving**: Database cell values, state transitions over
  algebraic types need field arithmetic, not bitwise operations
- **Small proof sizes**: Binius proofs are 200-400 KiB (Binius64 benchmarks),
  compared to ~90 KiB for Stwo on similar workloads
- **Fast verification**: Binius verification is slower than FRI-based systems

---

## 6. Performance Benchmarks

### Binius64 benchmarks (AWS instances, multi-threaded)

| Circuit | Prove (ms) | Verify (ms) | Proof size |
|---------|-----------|-------------|------------|
| Keccak-256 (1365 perms, 1 MiB) | 111.82 | 21.74 | 304.45 KiB |
| Blake2s (2048 compressions) | 166.15 | 45.45 | 360.14 KiB |
| ECDSA aggregation | 294.30 | 15.19 | 187.75 KiB |
| Hash-based sig aggregation | 627.53 | 135.34 | 322.11 KiB |

### Comparison on Keccak-256 (same workload)

| System | Prove (ms) | Proof size |
|--------|-----------|------------|
| Binius64 | 111.82 | 304.45 KiB |
| Plonky3 | 261 | 1,861.32 KiB |
| Stwo | ~comparable | 92.5 KiB |

Binius64 proves ~2.3x faster than Plonky3 with ~6x smaller proofs on Keccak.
However, Stwo achieves ~3.3x smaller proofs and much faster verification (2.5 ms
vs 45 ms on Blake2s).

### Interpretation for Tabula

Tabula's workload is not hash-heavy. The proving bottleneck is in:
- Memory access verification (sorted permutation checks)
- State transition constraints (algebraic, not bitwise)
- SMT path verification (field arithmetic for Poseidon2 hashing)

Binius's advantage (fast bitwise proving) does not align with these bottlenecks.

---

## 7. Library Usability

### Repository status

| Property | Value |
|----------|-------|
| Original repo | `IrreducibleOSS/binius` — **archived Sept 2025** |
| Successor | `binius-zk/binius64` — active |
| License | Apache-2.0 OR MIT (dual) |
| crates.io | **Not published** — git dependency only |
| Documentation | https://docs.binius.xyz, https://www.binius.xyz |
| Rust toolchain | Nightly (GFNI/SIMD intrinsics) |
| Parallelism | Rayon (optional, default-enabled) |
| Maturity warning | "This codebase certainly contains bugs" |

### Crate structure (original Binius, indicative)

The workspace contains multiple internal crates (`field`, `m3`, `core`, etc.)
but none are published to crates.io. Using Binius as a dependency requires
git references with pinned commits, which makes version management fragile.

### Build requirements

```toml
# Recommended Cargo.toml settings
[profile.release]
lto = "thin"

# Required for optimal performance
# RUSTFLAGS="-C target-cpu=native"
```

### API stability

The API is explicitly unstable: "We will make breaking changes at will."
There is no semantic versioning, no changelog, and no deprecation policy.
This makes Binius unsuitable as a foundational dependency for a production system.

---

## 8. Key Tradeoffs: Binary vs Prime Field Systems

### When binary fields (Binius) win

1. **Bitwise computation**: Hash functions, boolean circuits, bit manipulation
2. **Prover speed**: Zero embedding overhead means less wasted computation
3. **Memory efficiency**: Data occupies exactly as many bits as needed
4. **Hardware acceleration**: GFNI (Intel), NEON (ARM) accelerate binary field ops

### When prime fields (BabyBear/Plonky3) win

1. **Algebraic constraints**: Natural multiplication, addition, comparison
2. **Proof size**: FRI over prime fields produces smaller proofs
3. **Verification speed**: FRI verification is faster than Binius verification
4. **Ecosystem maturity**: Plonky3 is published on crates.io, widely adopted
5. **API stability**: Plonky3 has a more stable, documented API
6. **Structured data**: Database values, state transitions, counters — all map
   naturally to field elements

### The fundamental question for Tabula

Tabula's constraints are algebraic in nature:
- "This cell value equals the Poseidon2 hash of these inputs"
- "The memory read at (table, column, row) returned value V"
- "The state transition from S_old to S_new is valid"
- "The SMT path from leaf to root is consistent"

None of these are bitwise operations. They are polynomial identity checks over
algebraic values. BabyBear field elements are the natural representation.

Converting these to binary field operations would:
- Add complexity (emulating field arithmetic in binary)
- Lose the algebraic structure that makes AIR constraints natural
- Gain nothing — the bottleneck is not bit-level computation

---

## 9. Assessment: Binius for Tabula

### Verdict: Not recommended

| Factor | Rating | Rationale |
|--------|--------|-----------|
| Field fit | Poor | Tabula's constraints are algebraic, not bitwise |
| PCS compatibility | None | Completely different polynomial commitment pipeline |
| Constraint system | Moderate | M3 tables/channels resemble Tabula's shards/buses conceptually |
| Performance benefit | Negligible | Tabula's bottlenecks are not bitwise computation |
| API maturity | Poor | Unstable, not on crates.io, nightly-only |
| Migration cost | Extreme | Would require rewriting every chip, gadget, and the entire proving pipeline |
| Proof size | Worse | 300 KiB vs potentially smaller FRI proofs |
| Verification speed | Worse | Slower than FRI-based verification |

### What IS worth watching

1. **M3 arithmetization concepts**: The table+channel model is elegant and could
   inform future Tabula architecture improvements. The idea of independent tables
   with channel-balanced communication is conceptually similar to Tabula's
   shard+bus architecture.

2. **GKR-LogUp as a protocol**: The GKR approach to LogUp is field-agnostic in
   principle. Implementing GKR-LogUp over BabyBear (as tracked in Tabula's
   research/optimization tasks) would capture the efficiency gains without
   switching fields.

3. **FRI-Binius techniques**: Some of the BaseFold adaptations may inform future
   PCS improvements, even in prime-field systems.

### Recommendation

Continue with BabyBear + Plonky3 as the field and framework foundation. Monitor
Binius64 for conceptual insights (M3 patterns, GKR integration patterns) but do
not invest in a migration. The GKR-LogUp research item (already tracked) is the
right way to capture the most valuable protocol innovation from the Binius
ecosystem without the architectural mismatch.

---

## Sources

- [Binius GitHub (archived)](https://github.com/IrreducibleOSS/binius)
- [Binius64 GitHub](https://github.com/binius-zk/binius64)
- [Announcing Binius64 — Irreducible](https://www.irreducible.com/posts/announcing-binius64)
- [Binius64 Benchmarks](https://www.binius.xyz/benchmarks/)
- [Binary Tower Fields are the Future — Irreducible](https://www.irreducible.com/posts/binary-tower-fields-are-the-future-of-verifiable-computing)
- [Binius: Hardware-Optimized SNARK — Irreducible](https://www.irreducible.com/posts/binius-hardware-optimized-snark)
- [FRI-Binius: Improved Polynomial Commitments — Irreducible](https://www.irreducible.com/posts/fri-binius)
- [Better, Faster, Smaller Binius — Irreducible](https://www.irreducible.com/posts/better-faster-smaller-binius)
- [Binius V0 — binius.xyz](https://www.binius.xyz/basics/binius-v0/)
- [Constraint Systems — binius.xyz](https://www.binius.xyz/blueprint/constraints/)
- [Building with Binius64 — binius.xyz](https://www.binius.xyz/building)
- [Binius: highly efficient proofs over binary fields — Vitalik Buterin](https://vitalik.eth.limo/general/2024/04/29/binius.html)
- [The fields powering Binius — LambdaClass](https://blog.lambdaclass.com/the-fields-powering-binius/)
- [How Binius is moving ZK forward — LambdaClass](https://blog.lambdaclass.com/binius-moving-zk-forward/)
- [M3 Arithmetization deep dive — LambdaClass](https://blog.lambdaclass.com/diving-deep-into-binius-m3-arithmetization-using-merkle-tree/)
- [M3 Definition — binius.xyz](https://www.binius.xyz/basics/arithmetization/m3/definition/)
- [Binius Ethereum State Proving Service — Irreducible](https://www.irreducible.com/posts/ethereum-state-proving-service)
- [Plonky3 — Polygon](https://polygon.technology/blog/open-source-polygon-plonky3-is-once-again-the-fastest-zk-proving-system)
- [Binius STARK over Binary Field — Eigen Network](https://eigenlab.medium.com/binius-stark-proof-systems-over-binary-field-226dce65bdac)
- [FRI-Binius paper (ePrint 2024/504)](https://eprint.iacr.org/2024/504)
- [Original Binius paper (ePrint 2023/1784)](https://eprint.iacr.org/2023/1784.pdf)
