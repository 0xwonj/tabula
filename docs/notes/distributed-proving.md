# Distributed Proving / Separable Shard Artifacts

This note captures the design analysis for *separable per-shard proof
artifacts* on top of Tabula's static-coordinate sharding — including
distributed proving across machines and incremental re-proving under state
updates. The paper (EuroSys 2027) commits to **Definition 1** (single-machine
per-column sharding with end-to-end cross-shard bus balance) and defers
Definition 2 to follow-up work.

This note is **not authoritative**. The current architecture contract lives
in [`docs/design/architecture.md`](../design/architecture.md). This file
exists so that the analysis does not need to be re-derived next time the
question comes up.

## Terminology

- **Definition 1** — *Single-machine per-column sharding.*
  The prover constructs one independent `ProofInstance` per tier (execution
  + per-column shards + root); all instances are proved in parallel on one
  machine; cross-shard LogUp cumulative-sum balance is checked end-to-end
  at compose time.
- **Definition 2** — *Separable shard artifacts.*
  Each shard's sub-proof can be produced, serialized, transmitted, and
  verified independently of the others, with an explicit composition step
  that closes cross-shard bus balance across the set of shard artifacts.

Def 2 is a strict superset of Def 1. Any Def 2 implementation specializes
to Def 1 by co-locating all shard producers on one machine.

## Current state of the infrastructure

Substantial groundwork toward Def 2 is already present:

- **Per-tier independent proving.**
  `crates/machine/src/proof/prover.rs:63-123` constructs one `ProofInstance`
  per tier (execution, per-column shards, root). Each runs a full STARK
  phase (commit main / build permutation traces / prove) in parallel using
  only shared LogUp challenges and the synchronized transcript.
- **External bus tracking.**
  `crates/machine/src/setup/metadata.rs:125-132` identifies buses unbalanced
  within a single tier. Within-tier balance is enforced per tier; external
  cumsums are carried forward.
- **Cross-shard cumsum balance.**
  `crates/machine/src/proof/prover.rs:105-110` and
  `crates/machine/src/proof/verifier.rs:108-118` invoke
  `check_cross_proof_bus_balance()`, enforcing `Σ external cumsums = 0`
  per bus across all tiers.

What is **missing** for Def 2, relative to the above:

- A per-shard `SubProofEnvelope` extraction API that does not require
  assembling a full `TabulaProof`.
- A partial verifier API that accepts a subset of shard proofs plus
  external cumsum constraints and returns a partial verification judgement.
- A first-class composition layer over serialized artifacts (today the
  composition is implicit in `check_cross_proof_bus_balance()` called over
  an in-memory set of envelopes).
- A serialization contract for `SubProofEnvelope` that survives transport
  between machines or time gaps.

Rough scope, from inspection: moderate rewrite of the machine/proof layer
(~2–4 weeks). Not a structural change — the underlying architecture was
designed for this.

## Use cases, evaluated honestly

### 1. Incremental proving under state updates

**Assessment: genuine, architecturally natural, tabular-specific.**

Tabula currently proves *entire transaction batches* end-to-end. This is
a current implementation choice, not an architectural requirement. The
per-column SSMC commitment means a batch that only touches columns
`C₁, C₂` leaves the SSMC roots of other columns unchanged. If shard
proofs are separable, one can re-prove only the touched columns' shards
and carry previous shard proofs for unchanged columns into the
composition. Cross-shard bus balance still closes because untouched
columns contribute zero sends on the relevant buses.

This is a structural follow-up *enabled* by the current design, not an
idea bolted on. Natural workloads: long-running state machines where
typical batches touch only a fraction of columns (common in L2
app-specific rollups).

Unlike the distribution use case below, incremental proving has no
general-systems analogue — it leverages tabular-specific structure and
is the most paper-native follow-up direction for this line of work.

### 2. Prover decentralization across operators

**Assessment: real market motivation, but the novelty lives in the
coordination layer, not in Def 2 itself.**

Independent prover operators each take a shard. Natural fit for L2
prover networks seeking censorship resistance, redundancy, or cost
competition. However, the substantive work is in trust model, artifact
transport, payment/slashing, and composition orchestration — all
protocol-layer concerns. Def 2 only provides the zkVM primitive. General
proving networks (Succinct, Gevulot, …) already distribute whole proofs
and do not need per-shard separability.

### 3. Memory or hardware limits

**Assessment: not currently binding.**
Single-machine proving capacity is not a bottleneck at Tabula's current
workload scale. Would become relevant at substantially larger state
sizes.

### 4. Parallelism beyond a single machine

**Assessment: weak.**
Network latency usually negates the benefit until per-machine compute
saturates. Def 1 (rayon across cores) is close to optimal for workloads
that fit on one box.

## Why deferred for the current paper

- The headline claim is **compiler–proof co-design for typed tabular
  state transitions**. Def 1 already substantiates the "static-coordinate
  sharding" part of that claim end-to-end.
- Def 2 adds a distributed-systems axis largely orthogonal to the
  co-design story. The hard parts (serialization, partial verifier API,
  coordination protocol) are standard distributed-systems patterns and
  do not teach anything new about typed tabular zkVMs.
- A weak Def 2 subsection invites reviewer comparison with general
  proving networks, where Tabula has no specific contribution along that
  axis.
- Incremental proving (the most paper-native follow-up) would need its
  own evaluation scaffolding — state-update-centric workloads and an
  incremental benchmark — that is not currently in scope.

## What the paper will say

The Future Work section will contain one paragraph explicitly naming
separable shard artifacts *and* incremental proving as structural
follow-ups, for example:

> The static `(table, col)` coordinates and per-column SSMC commitments
> make shard-level proof separation a natural follow-up: individual shards
> could be proved on independent machines and composed via the same
> cross-shard LogUp balance enforced here. The same structure enables
> incremental re-proving under state updates — a batch that touches only a
> subset of columns would re-prove only those columns' shards while
> carrying unchanged shards' proofs forward. Both directions are out of
> scope for this paper but are structurally enabled by the design
> presented.

## Pointers

- Architecture canon: [`docs/design/architecture.md`](../design/architecture.md)
- Per-column SSMC commitments: `crates/chips/src/shards/ssmc.rs`,
  `crates/commitment/`
- Prover / verifier composition: `crates/machine/src/proof/prover.rs`,
  `crates/machine/src/proof/verifier.rs`
- External bus identification: `crates/machine/src/setup/metadata.rs`
