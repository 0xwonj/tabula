# STARK Field Selection Tradeoffs

> Comparative analysis of finite field choices for STARK proving systems and their
> implications for Tabula's architecture.

## Context

Tabula currently uses BabyBear (p = 2^31 - 2^27 + 1) via Plonky3 with FRI-based
polynomial commitment and Poseidon2 for in-circuit hashing. This document surveys the
fundamental tradeoffs between field choices and emerging proof system architectures to
inform future decisions.

---

## The Four Contending Fields

### BabyBear (p = 2^31 - 2^27 + 1)

A 31-bit Crandall prime with modulus 15 * 2^27 + 1. Used by RISC Zero, SP1 (Succinct),
and as Plonky3's primary field.

**Key properties:**
- Two-adicity of 27 (largest power-of-2 subgroup: 2^27 elements)
- Full multiplicative group of size 15 * 2^27, enabling FRI all the way down to degree 15
- Standard FFT-friendly: conventional NTT works directly
- Extension field: degree-4 (BabyBear4) provides 124-bit security

### Mersenne31 (p = 2^31 - 1)

The eighth Mersenne prime. Used by StarkWare's Stwo (now S-two, live on Starknet mainnet
since November 2025).

**Key properties:**
- Two-adicity of only 1 (multiplicative group has size 2^31 - 2 = 2 * 1073741823)
- Cannot use conventional FFT/NTT -- requires Circle STARK construction
- Mersenne form enables extremely fast modular reduction (shift + mask)
- Extension field: degree-4 (M31^4) provides ~124-bit security

### KoalaBear (p = 2^31 - 2^24 + 1)

A newer 31-bit Crandall prime with modulus 127 * 2^24 + 1. Introduced in Plonky3 as an
optimization over BabyBear.

**Key properties:**
- Two-adicity of 24 (lower than BabyBear's 27)
- Optimal power map: x -> x^3 (vs x -> x^7 for BabyBear, x -> x^5 for M31)
- The lower S-box degree means ~50% smaller traces for arithmetic hash proofs
- Same Montgomery arithmetic pipeline as BabyBear

### Goldilocks (p = 2^64 - 2^32 + 1)

A 64-bit prime used in Plonky2 (legacy). Two-adicity of 32.

**Key properties:**
- 64-bit field elements (2x the width of the 31-bit fields)
- Degree-2 extension provides 128-bit security (cheaper extension than 31-bit fields)
- Largely superseded by 31-bit fields for new designs
- Still relevant for systems needing large native arithmetic range

### Binary Tower Fields (GF(2^k))

Used by Binius (Irreducible) and Polyhedra's Binary GKR. Operate over GF(2) with tower
extensions.

**Key properties:**
- Addition is XOR (carryless, no modular reduction)
- 5x more hardware-efficient than M31 multipliers in custom silicon
- Require Brakedown-style commitment (larger proofs than FRI)
- Boolean computations (Keccak, bitwise ops) are essentially free

---

## Comparison Matrix

| Criterion | BabyBear (Plonky3) | M31 (Stwo) | KoalaBear (Plonky3) | Binary Tower (Binius) | Goldilocks |
|---|---|---|---|---|---|
| **Native mul speed (SIMD)** | 1.71 ele/cyc (AVX2), 21-cycle latency | 2 ele/cyc (AVX2), 13-cycle latency | Same as BabyBear (MontyField31) | N/A on commodity CPUs; 5x in custom HW | ~0.5 ele/cyc (64-bit ops) |
| **Mul speed ratio** | 1.0x (baseline) | ~1.3x faster | ~1.0x (same pipeline) | 5x (ASIC only) | ~0.4x (slower) |
| **Extension field cost** | 4x degree; BabyBear4 mul = ~16.5K gates | 4x degree; similar cost | 4x degree; similar cost | Variable tower height | 2x degree; cheaper extension |
| **FFT/NTT** | Standard NTT, max 2^27 | Circle FFT (non-standard) | Standard NTT, max 2^24 | Not applicable (no NTT) | Standard NTT, max 2^32 |
| **FRI compatibility** | Native FRI | Circle FRI | Native FRI | Brakedown PCS (not FRI) | Native FRI |
| **GKR compatibility** | Yes (via sum-check) | Yes (via sum-check) | Yes (via sum-check) | Native (Binary GKR) | Yes (via sum-check) |
| **Recursion cost** | Low: Poseidon2 x^7, ~570ms for 1365 Keccak | Medium: Poseidon2 x^5, ~2.85s per verification (M3 Max) | Lowest: Poseidon2 x^3, ~480ms for 2^19 perms | Unknown/immature | Medium: wider field helps but slower base ops |
| **Hash options** | Poseidon2 (x^7), Blake3, Keccak | Poseidon2 (x^5), Blake3 | Poseidon2 (x^3), Blake3, Keccak | Groestl, Vision/Rescue, Blake3 | Poseidon2, Blake3, Keccak |
| **Poseidon2 speed** | 1.0 us (AVX2, w=16) | 0.71 us (AVX2, w=16) | 0.78 us (AVX2, w=16) | N/A | Slower (64-bit field) |
| **Ecosystem maturity** | High: RISC Zero, SP1, Plonky3 | High: Stwo/S-two on Starknet mainnet | Medium: Plonky3, OpenVM 2.0 | Low-Medium: Binius64, Expander | Legacy: Plonky2 |
| **Lookup arguments** | LogUp (permutation-based) | LogUp + sum-check | LogUp (permutation-based) | LogUp-GKR (native) | LogUp (permutation-based) |
| **Max trace size** | 2^27 rows | Unlimited (Circle) | 2^24 rows | Unlimited | 2^32 rows |
| **Proof size** | ~1.86 MB (1365 Keccak) | ~92.5 KB (2048 Blake2s) | Similar to BabyBear | ~304 KB (1365 Keccak) | Larger (wider field) |
| **Verifier time** | ~15.8 ms (1365 Keccak) | ~2.5 ms (2048 Blake2s) | Similar to BabyBear | ~29.2 ms (1365 Keccak) | Slower |

### Benchmark Context

The benchmark numbers above come from different workloads (Keccak vs Blake2s), so
direct comparison requires caution. The relative rankings are more meaningful than
absolute numbers. Source: Binius benchmark page (binius.xyz/benchmarks) and Plonky3
small-field analysis (hackmd.io/@Syxton/small_fields_in_plonky3).

### End-to-End Proof Benchmarks (Plonky3, 2^19 Poseidon2 permutations, Keccak MMCS)

| Field | Proving time | Relative |
|---|---|---|
| KoalaBear | 480 ms | 1.0x (baseline) |
| BabyBear | 1.1 s | 2.3x slower |
| Mersenne31 | 1.8 s | 3.75x slower |

**Why M31 is slower end-to-end despite faster multiplication:** M31 requires Circle FFT
(less mature optimization), and Poseidon2 with x^5 S-box requires more rounds than x^3
(KoalaBear). The raw multiplication advantage does not compensate for the higher-level
protocol overhead in current implementations.

---

## Deep Dives

### Sum-Check over Small Fields (Bagad, Domb, Thaler 2024)

The paper "The Sum-Check Protocol over Fields of Small Characteristic" (ePrint 2024/1046)
presents a critical optimization: when sum-check operates over an extension field of a
much smaller base field, most prover multiplications can remain in the base field. For
polynomials outputting base-field values, this reduces extension field operations by
**multiple orders of magnitude**.

Tested with BabyBear4 (degree-4 extension) on Intel i7, single-core and six-core.

**Relevance to Tabula:** This directly applies to LogUp-GKR over BabyBear. The permutation
argument sum-check currently operates in BabyBear4; this optimization would keep most
work in the 31-bit base field.

### LogUp-GKR (Haboeck 2023)

The paper "Improving logarithmic derivative lookups using GKR" (ePrint 2023/1284)
introduces two key innovations:

1. **Reduced commitments:** When performing lookups across M columns, the prover commits
   to only one extra column (the multiplicities), vs. M helper columns in standard LogUp.

2. **Univariate-to-multilinear bridge:** A novel transformation converts a univariate
   polynomial commitment scheme into a multilinear one, enabling GKR's multilinear
   framework to work with standard univariate FRI commitments.

**This directly answers the question of combining univariate FRI with GKR.** The bridge
works by evaluating the univariate polynomial at appropriate points to recover multilinear
evaluations.

### SWIRL (OpenVM 2.0, 2025)

SWIRL (Stacked WHIR with Interaction Reductions via LogUp) is the most complete
production system combining sum-check, GKR, and FRI-like commitments:

- Built on WHIR (a direct FRI replacement with super-fast verification)
- Uses modular sum-check to interoperate between different polynomial domains
- Incorporates customized Zerocheck and LogUp-GKR
- "Stacked Reduction" links everything to WHIR
- Includes a "reformulated Univariate Skip" optimized for small fields
- Operates over BabyBear/KoalaBear

**Performance:** OpenVM 2.0 proves mainnet Ethereum blocks in real time at p99 level,
proves RISC-V at 139 MHz on 16x RTX 5090 GPUs, with proof sizes under 300 KB and
100 bits provable security.

### Binary GKR (Polyhedra 2025)

Polyhedra's Binary GKR operates directly over GF(2) and achieved a speed record for
Keccak proving:

| System | 8192 Keccak invocations | Proof size | Verifier time |
|---|---|---|---|
| Binary GKR | 2.18 s | 1.052 MB | 0.035 s |
| Binius | 12.35 s | 0.548 MB | 0.213 s |

Key techniques: bit packing (reduces prover work from O(N) to O(N/log N)), precomputed
lookup tables fitting in L3 cache, and exploiting Keccak's repetitive round structure.

**Limitation:** Binary field only. Not compatible with BabyBear/M31 without a field bridge.

### Stwo Recursion (Mersenne31)

Stwo's recursive verification over M31 uses specialized Poseidon2 components:
- Generic Plonk in-circuit: ~2,186 rows per Poseidon2 evaluation (65,580 cells)
- Specialized component: 6 rows, 576 cells (114x improvement)
- Recursive verification: ~2.85 seconds per proof on M3 Max

The dominant cost in recursion is Merkle tree opening verification (many hash invocations).
Poseidon2's efficiency over the proving field is thus critical for recursion performance.

---

## Verifiable Database Systems

### Existing Work

| System | Approach | Performance |
|---|---|---|
| Proof of SQL (Space and Time) | zk-SNARK over SQL | Sub-second for 1M+ rows, GPU-accelerated |
| ZKSQL | ZK proofs for ad-hoc SQL | DAG of database operators |
| IntegriDB | Verifiable outsourced SQL | Range queries, JOIN, SUM, MAX/MIN, COUNT, AVG |
| vSQL | Verifiable arbitrary SQL | Dynamic outsourced databases |
| Spitz | Verifiable database system | Academic prototype |

**Relevance to Tabula:** Tabula's structured-storage model (tables, columns, cells) is
closer to a database than a general-purpose VM. Proof of SQL demonstrates that STARK-like
techniques can prove database operations at practical speeds. Tabula's column-oriented
sharding aligns well with this paradigm.

---

## Key Findings for Tabula

### 1. BabyBear remains a sound choice

BabyBear offers the best balance of ecosystem maturity, FFT compatibility, and extension
field support. KoalaBear is strictly better for recursive proofs (lower S-box degree) but
has a smaller max trace (2^24 vs 2^27). Tabula's sharded architecture with smaller
per-shard traces makes KoalaBear's lower two-adicity less problematic.

### 2. KoalaBear is worth evaluating

KoalaBear's 2.3x end-to-end advantage over BabyBear in Poseidon2-heavy workloads is
substantial. Since Tabula uses Poseidon2 for both in-circuit hashing and recursive
verification, this advantage compounds. The migration cost is low (same Plonky3 crate,
same MontyField31 pipeline).

### 3. Mersenne31 is not advantageous for Tabula today

Despite faster raw multiplication (1.3x), M31's end-to-end performance is currently
3.75x slower than KoalaBear in Plonky3 benchmarks due to immature Circle FFT
optimization and higher S-box degree. M31's unlimited trace size is unnecessary given
Tabula's sharded architecture.

### 4. The LogUp-GKR path is viable with univariate FRI

The Haboeck 2023 paper provides a concrete univariate-to-multilinear bridge, and SWIRL
demonstrates this in production. This means Tabula can adopt GKR-based lookups without
abandoning FRI. The sum-check-over-small-fields optimization (Bagad et al. 2024) would
further reduce the cost of GKR over BabyBear4.

### 5. Binary fields are a long-term consideration

Binary towers offer 5x hardware efficiency in custom silicon and natural GKR
compatibility, but require Brakedown PCS (larger proofs) and have immature tooling.
Not practical for Tabula in the near term.

### 6. SWIRL/WHIR represents the state of the art

OpenVM 2.0's SWIRL architecture -- combining WHIR (FRI successor), LogUp-GKR, and
optimized sum-check over small fields -- achieves the best known performance for
general-purpose proving. Tabula should monitor WHIR as a potential FRI replacement.

---

## Open Questions

1. **KoalaBear migration cost:** What is the effort to switch Tabula from BabyBear to
   KoalaBear? Both use MontyField31 in Plonky3, suggesting low friction.

2. **LogUp-GKR implementation timeline:** Given that SWIRL demonstrates the full pipeline,
   what subset is needed for Tabula's permutation argument?

3. **WHIR maturity:** WHIR provides super-fast verification and is post-quantum. When will
   Plonky3 offer a production-ready WHIR backend?

4. **Recursion strategy:** With sharded proofs, Tabula needs recursive aggregation.
   KoalaBear's Poseidon2 x^3 provides the fastest known recursion. Is this sufficient,
   or does Tabula need a SNARK wrapper (Groth16) for on-chain verification?

---

## Sources

- [Small Fields for Zero-Knowledge (ICME blog)](https://blog.icme.io/small-fields-for-zero-knowledge/)
- [Circle STARKs (Vitalik Buterin)](https://vitalik.eth.limo/general/2024/07/23/circlestarks.html)
- [Binary Tower Fields are the Future (Irreducible)](https://www.irreducible.com/posts/binary-tower-fields-are-the-future-of-verifiable-computing)
- [Small Fields in Plonky3 (HackMD)](https://hackmd.io/@Syxton/small_fields_in_plonky3)
- [Efficient Prime Fields for ZK (HackMD)](https://hackmd.io/@Voidkai/BkNX3xUZA)
- [Why I'm Excited by Circle STARK and Stwo (StarkWare)](https://starkware.co/integrity-matters-blog/why-im-excited-by-circle-stark-and-stwo/)
- [Binius Benchmarks](https://www.binius.xyz/benchmarks/)
- [Binius STARKs Analysis (Vitalik Buterin)](https://vitalik.eth.limo/general/2024/04/29/binius.html)
- [Sum-Check over Small Characteristic (Bagad, Domb, Thaler 2024)](https://eprint.iacr.org/2024/1046)
- [LogUp-GKR (Haboeck 2023)](https://eprint.iacr.org/2023/1284)
- [SWIRL Whitepaper (OpenVM)](https://openvm.dev/swirl.pdf)
- [OpenVM 2.0 Announcement](https://blog.openvm.dev/2.0)
- [Binary GKR Speed Record (Polyhedra)](https://blog.polyhedra.network/binary-gkr/)
- [Recursive Proofs in Stwo (L2IV Research)](https://l2ivresearch.substack.com/p/recursive-proofs-in-stwo-part-ii)
- [StarkWare Proving Record (Stwo)](https://starkware.co/blog/starkware-new-proving-record/)
- [Stwo Prover Announcement (StarkWare)](https://medium.com/starkware/stwo-prover-the-next-gen-of-stark-scaling-is-here-f7429e764127)
- [S-two Live on Starknet Mainnet](https://www.starknet.io/blog/s-two-is-live-on-starknet-mainnet-the-fastest-prover-for-a-more-private-future/)
- [Proof of SQL (Space and Time)](https://www.spaceandtime.io/blog/proof-of-sql-101)
- [ZKSQL (VLDB)](https://www.vldb.org/pvldb/vol16/p1804-li.pdf)
- [Expander GKR Prover (Polyhedra)](https://blog.polyhedra.network/introducing-expander-the-fastest-gkr-proof-system-to-date/)
- [GKR Protocol Tutorial (LambdaClass)](https://blog.lambdaclass.com/gkr-protocol-a-step-by-step-example/)
- [WHIR: Reed-Solomon Proximity Testing](https://eprint.iacr.org/2024/1586.pdf)
- [Poseidon2 Hash Function](https://eprint.iacr.org/2023/323.pdf)
- [BabyBear Benchmark Repository](https://github.com/0xkanekiken/baby-bear-benchmark)
- [Plonky3 Repository](https://github.com/Plonky3/Plonky3)
