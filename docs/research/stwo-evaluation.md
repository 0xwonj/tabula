# Stwo (StarkWare) Evaluation as Crypto Foundation

> Research date: 2026-03-12
> Status: Research only. No implementation decision.

## Executive Summary

Stwo is StarkWare's next-generation Circle STARK prover/verifier framework. It operates
over the Mersenne-31 (M31) field with Circle FRI and native GKR-LogUp. It is open-source
(Apache 2.0), published on crates.io, and live on Starknet mainnet since November 2025.

**Key finding**: Stwo is a viable alternative crypto foundation to Plonky3, but a migration
would be a 3-6 month effort touching every STARK-coupled crate (stark, gadgets, chips,
machine, witness, commitment). The 1.3x base-field speedup over BabyBear is real but
modest. The primary value proposition is native GKR-LogUp (eliminating committed permutation
traces) and Circle FRI efficiency. A migration is not justified unless Tabula needs to
adopt GKR-LogUp anyway and Plonky3 does not provide it.

---

## 1. Field: M31 (Mersenne-31)

| Property | M31 | BabyBear (current) |
|----------|-----|--------------------|
| Prime | p = 2^31 - 1 | p = 2^31 - 2^27 + 1 |
| Size | 31 bits | 31 bits |
| Multiplication (vectorized) | ~0.3 cycles | ~0.39 cycles |
| Relative speed | 1.3x faster | baseline |
| 2-adicity | Poor (p-1 = 2 * (2^30-1)) | Excellent (2^27) |
| FFT domain | Circle group (2^31 points) | Multiplicative subgroup (2^27) |
| Extension field | QM31 = degree-4, ~124 bits | EF4 = degree-4, ~124 bits |
| SIMD width | 16x u32 (AVX-512) | 16x u32 (AVX-512) |

**Analysis**: M31's 1.3x multiplication speedup is real and consistent across benchmarks.
However, the poor 2-adicity forces the use of Circle STARKs (a fundamentally different
polynomial commitment scheme), which is the main architectural difference -- not the field
itself. Extension field work (quotients, FRI) is comparable in both.

---

## 2. PCS: Circle FRI

Circle FRI replaces standard multiplicative-group FRI. Key differences:

- **Domain**: Points on the unit circle x^2 + y^2 = 1 over M31, not powers of a generator.
  The circle contains 2^31 points -- larger than BabyBear's 2^27 multiplicative subgroup.
- **Squaring map**: pi(x,y) = (2x^2 - 1, 2xy) halves the domain each round (analogous
  to standard FRI folding).
- **First round special**: Uses both squaring and inversion maps to reduce from bivariate
  (circle polynomial) to univariate.
- **Vanishing polynomials**: Cannot use odd-degree vanishing polynomials. DEEP quotients
  decompose into real and imaginary components over F(i), requiring separate random weights.
- **Twiddle factors**: Precomputed from circle geometry, used in even-odd decomposition.
- **Security**: Equivalent to standard FRI -- same soundness model, same query complexity.

**Implication for Tabula**: Circle FRI is mathematically well-founded but architecturally
incompatible with p3-fri. Every component that touches FRI, polynomial commitment, or
domain arithmetic would need replacement. This is the core of the migration cost.

---

## 3. GKR-LogUp

Stwo has **native GKR-LogUp** -- this is its most significant architectural advantage
over Plonky3-based systems.

### How it works

1. LogUp lookup arguments are expressed as rational polynomial sums (sum of 1/(f(x) - t(x)))
2. The sum is verified via the GKR (Goldwasser-Kalai-Rothblum) protocol
3. GKR constructs a layered arithmetic circuit and verifies it via sum-check, recursively
4. The prover avoids committing to intermediate layers -- only the final evaluation is checked
5. Result: **no permutation trace columns, no permutation NTT, no permutation Merkle commit**

### Performance impact

- Reduces BLAKE/Poseidon hash proof cost by 2-3x vs pure STARK approaches
- Particularly beneficial for shallow, wide circuits (exactly Tabula's pattern)
- Tradeoff: increases verifier computation per round (more interaction rounds)
- Memory overhead is manageable via time-space tradeoffs

### Comparison to Tabula's current approach

| Aspect | Tabula (committed perm) | Stwo (GKR-LogUp) |
|--------|------------------------|-------------------|
| Perm trace columns | 4 * (interactions + 1) per chip | 0 |
| PCS cost | NTT + Merkle on perm trace | None |
| Code | ~700 LOC (perm pipeline) | ~400-500 LOC (sum-check) |
| Soundness | Requires PCS commitment of cumsums | Interactive sum-check |
| Estimated cost fraction | 15-30% of proving time | ~0% (folded into verification) |

**This is the strongest reason to consider Stwo.** However, GKR-LogUp can also be
implemented on top of Plonky3/BabyBear (it is a protocol-level change, not a field-level
one). The question is whether to adopt Stwo wholesale or implement GKR-LogUp independently.

---

## 4. AIR Framework & Constraint System

### Architecture

Stwo uses a component-based AIR model:

- **Components**: Independent AIR segments, each with its own trace columns and constraints
- **Trace**: Collection of M31 columns, grouped by component
- **Constraints**: Expressed directly on trace values via the `constraint-framework` crate
- **Composition polynomials**: M31 columns, four per trace component
- **Separation**: Trace construction and constraint evaluation are cleanly separated

### Developer experience (from Nexus integration)

> "Writing AIRs for S-two felt natural. We appreciated the clear API boundaries and the
> flexibility to express constraints directly on trace values."

Nexus built a full zkVM with modular AIRs (instruction decode, register read/write, memory
access, arithmetic) on Stwo, demonstrating composability.

### Comparison to Plonky3/Tabula

| Aspect | Plonky3 (Tabula) | Stwo |
|--------|-----------------|------|
| Constraint trait | `Air<AB>` / `AirBuilder` | Component-based framework |
| Trace type | `RowMajorMatrix<F>` | `Col<T>` (column-major) |
| Interaction model | Custom LogUp (InteractionAirBuilder) | Native GKR-LogUp |
| Bus abstraction | `define_bus!` macro (typed) | Built-in lookup argument |
| Preprocessing | `AirBuilder` with preprocessed | Component preprocessing |
| Extension field | `AB::EF` generic | QM31 concrete |
| Backend dispatch | Compile-time generics | Trait-based (CPU/SIMD/GPU) |

**Key difference**: Plonky3 is a toolkit (you wire components yourself); Stwo is a
framework (it provides the proving pipeline, you plug in components). Tabula's custom
machine layer partially fills the gap that Stwo provides natively.

---

## 5. Crate Structure & Library Viability

### Workspace layout (default branch: `dev`)

| Crate | Purpose |
|-------|---------|
| `crates/stwo` | Core Circle STARK prover/verifier |
| `crates/constraint-framework` | Constraint expression and evaluation |
| `crates/air-utils` | AIR utilities |
| `crates/air-utils-derive` | Derive macros for AIR |
| `crates/examples` | Demo implementations |
| `crates/std-shims` | no_std compatibility |

### crates.io

- **Published**: `stwo = "1.0.0"` (released 2025-07-18)
- **License**: Apache 2.0
- **Rust**: Nightly required (per `rust-toolchain.toml`)
- **API stability**: v1.0.0 suggests semver commitment, but README states "work-in-progress,
  not recommended for production use"

### External adoption

Multiple projects use Stwo as a library dependency:

| Project | Use case |
|---------|----------|
| **stwo-cairo** (StarkWare) | Production Cairo program proving (Starknet mainnet) |
| **Nexus** | zkVM with modular AIRs |
| **LuminAIR** (Giza) | Verifiable ML computational graphs |
| **Raito** | Bitcoin block validation proofs |
| **Keth** | Ethereum Execution Layer proving |
| **NumerAir** | Fixed-point arithmetic library for Stwo circuits |
| **ICICLE-Stwo** (Ingonyama) | GPU-accelerated backend |
| **VEX** | Provable orderbook |

**Assessment**: Stwo is demonstrably usable as a library. The ecosystem is smaller than
Plonky3's but growing, with production deployment on Starknet mainnet.

---

## 6. Performance Benchmarks

### Headline numbers

| Metric | Stwo | Plonky3 | Notes |
|--------|------|---------|-------|
| Poseidon2 hashes/sec (M3 Pro 12-core) | 620,000 | 2,000,000+ (M3 Max) | Different hardware |
| Poseidon2 hashes/sec (Intel i7 4-core) | 500,000 | N/A | |
| vs Stone (same hardware) | 940x faster | N/A | |
| Base field mul speedup | 1.3x over BabyBear | baseline | |

### Interpreting the numbers

- Plonky3 claims >2M hashes/sec on M3 Max, vs Stwo's 620K on M3 Pro. Different hardware
  makes direct comparison unreliable.
- On equivalent hardware, the gap is likely smaller. The 1.3x base-field advantage is
  partially offset by Circle FRI overhead and extension field costs.
- Both are in the same performance class. Neither has a decisive advantage.
- GPU acceleration (ICICLE-Stwo) is available but not reflected in CPU benchmarks.

### Starknet mainnet (November 2025)

Stwo is live on Starknet mainnet, generating proofs "up to an order of magnitude faster
than Stone." The Kakarot Fibrace experiment demonstrated millions of ZK proofs generated
locally on smartphones -- proving client-side viability.

---

## 7. Risks & Concerns

### Nightly Rust requirement

Stwo requires Rust nightly. Tabula currently uses Rust 2024 edition (stable). Adopting
Stwo would either force nightly or require waiting for stabilization of whatever features
Stwo depends on.

### API stability

v1.0.0 is published but the README contradicts this with "work-in-progress, not recommended
for production use." This suggests the semver promise may not be firm -- internal churn
is expected.

### Circle STARK specificity

Circle STARKs are a newer protocol with less ecosystem tooling than standard FRI-based
STARKs. Debugging tools, formal verification, and community knowledge are thinner.

### Column-major vs row-major traces

Stwo uses `Col<T>` (column-major) while Plonky3 uses `RowMajorMatrix<F>`. Tabula's
chip pattern (columns.rs / air.rs / trace.rs) would need rework for column-major layout.

### Smaller community

Plonky3 ecosystem: SP1, OpenVM, Valida, Lita, and many others.
Stwo ecosystem: stwo-cairo, Nexus, Keth, LuminAIR, Raito.
Plonky3 has broader adoption and more community knowledge.

---

## 8. Migration Cost Assessment

### Plonky3 coupling in Tabula

Crates directly importing p3-* dependencies:

| Crate | p3 crates used | Coupling depth |
|-------|---------------|----------------|
| `commitment` | field, baby-bear, poseidon2, symmetric | Medium (feature-gated) |
| `stark` | field, baby-bear, air, matrix, uni-stark | **Deep** (core abstractions) |
| `gadgets` | air, baby-bear, field | Medium (constraint helpers) |
| `chips` | (via stark, gadgets) | **Deep** (all 14 chips) |
| `machine` | field, baby-bear, air, matrix, uni-stark, commit, fri, challenger, dft, merkle-tree, symmetric, poseidon2 | **Deep** (full PCS pipeline) |
| `witness` | (via stark, chips) | Medium (trace generation) |

Total: 12 distinct p3-* crates used across 6 Tabula crates (+ transitive deps in chips, witness).

### Estimated migration scope

| Component | Files affected | Effort |
|-----------|---------------|--------|
| Field type (BabyBear -> M31) | All trace/constraint code | 2-3 weeks |
| AIR framework (AirBuilder -> Stwo component) | All 14 chips + gadgets | 3-4 weeks |
| PCS pipeline (FRI -> Circle FRI) | machine, stark | 2-3 weeks |
| Commitment (Merkle/MMCS) | machine, commitment | 1-2 weeks |
| Trace layout (row-major -> column-major) | witness, chips, stark | 2-3 weeks |
| LogUp (custom -> native GKR) | stark, machine | 1-2 weeks (net savings) |
| Testing / debugging / stabilization | All | 3-4 weeks |
| **Total** | | **14-21 weeks (3-5 months)** |

### What would NOT change

- `core` crate (zero crypto deps -- by design)
- `ir` crate (instruction set, no field deps)
- `executor` crate (zero crypto deps -- by design)
- `lang` crate (DSL compiler)
- `artifact`, `contract`, `driver`, `cli`, `daemon`, `web` (orchestration/UI)
- Bus topology and interaction semantics (LogUp fingerprints are protocol-agnostic)
- Chip definitions at the semantic level (what each chip proves)

---

## 9. Key Question: Stwo vs Plonky3 for Tabula

### Arguments for Stwo

1. **Native GKR-LogUp**: Eliminates permutation trace overhead (15-30% of proving time).
   This is Tabula's planned optimization (tasks/research.md §GKR) and Stwo provides it
   out of the box.
2. **1.3x base field speedup**: Consistent, well-benchmarked.
3. **Framework vs toolkit**: Stwo's proving pipeline is more complete, reducing custom code
   in `tabula-machine`.
4. **Production proven**: Live on Starknet mainnet with millions of proofs generated.
5. **Larger trace domain**: 2^31 circle points vs 2^27 multiplicative subgroup. Fewer
   blowup constraints for large traces.

### Arguments for staying with Plonky3

1. **Sunk cost**: ~2,600 LOC of custom STARK infrastructure already built and working.
2. **Migration cost**: 3-5 months of full-time work, touching every STARK-coupled crate.
3. **GKR is implementable on Plonky3**: The protocol is field-agnostic. Tabula can
   implement GKR-LogUp on BabyBear without switching frameworks.
4. **Broader ecosystem**: More projects, more community knowledge, more tooling.
5. **Stable Rust**: Plonky3 works on stable Rust; Stwo requires nightly.
6. **API stability**: Plonky3 v0.4 is well-understood; Stwo v1.0.0 contradicts its own
   README about production readiness.
7. **Typed buses**: Tabula's `define_bus!` macro provides compile-time type safety that
   Stwo's native lookups do not.
8. **No decisive performance gap**: Both are in the same performance class for proving.

### Recommendation

**Do not migrate to Stwo.** The migration cost (3-5 months) far exceeds the incremental
benefit. The strongest argument for Stwo (GKR-LogUp) can be achieved by implementing the
sum-check protocol directly on the current Plonky3/BabyBear stack (estimated 4-5 weeks,
per tasks/research.md).

**However, monitor Stwo for**:
- GKR-LogUp patterns to reference when implementing Tabula's own sum-check
- GPU backend patterns (ICICLE-Stwo) for future GPU offloading
- Circle STARK research for potential long-term field migration
- API stabilization -- if Stwo reaches true v2.0 stability and Plonky3 stagnates,
  reassess

---

## Sources

- [Stwo GitHub](https://github.com/starkware-libs/stwo) -- main repo, Apache 2.0
- [Why I'm Excited By Circle STARK and Stwo](https://starkware.co/integrity-matters-blog/why-im-excited-by-circle-stark-and-stwo/) -- M31 vs BabyBear analysis
- [Stwo Prover: The next-gen of STARK scaling](https://starkware.co/blog/stwo-prover-the-next-gen-of-stark-scaling-is-here/) -- architecture overview
- [StarkWare sets new proving record](https://starkware.co/blog/starkware-new-proving-record/) -- benchmark numbers
- [Stark @ Home: Math Behind Stwo](https://starkware.co/blog/starkwares-lightning-fast-next-gen-prover/) -- GKR, SIMD, field extensions
- [S-two Is Live on Starknet Mainnet](https://www.starknet.io/blog/s-two-is-live-on-starknet-mainnet-the-fastest-prover-for-a-more-private-future/) -- production deployment
- [Yet another Circle STARK tutorial (ChainSafe)](https://research.chainsafe.io/blog/circle-starks/) -- Circle FRI technical details
- [Introducing ICICLE-Stwo (Ingonyama)](https://www.ingonyama.com/post/introducing-icicle-stwo-a-gpu-accelerated-stwo-prover) -- GPU backend, API traits
- [Nexus x S-two](https://starkware.co/blog/nexus-stwo-zkvm-scalable-verifiable-computation/) -- library integration experience
- [Plonky3 migration (Miden VM)](https://hackmd.io/ScR33Ym1TmC_FBHuCCnV0w) -- prover framework migration lessons
- [Plonky3 vs Stwo benchmark](https://polygon.technology/blog/open-source-polygon-plonky3-is-once-again-the-fastest-zk-proving-system) -- head-to-head comparison
- [Awesome Stwo](https://github.com/keep-starknet-strange/awesome-stwo) -- ecosystem projects
- [Circle STARKs paper](https://eprint.iacr.org/2024/278) -- foundational research
- [Improving LogUp using GKR](https://eprint.iacr.org/2023/1284) -- GKR-LogUp protocol
- [stwo crate on crates.io](https://crates.io/crates/stwo) -- v1.0.0, Apache 2.0
