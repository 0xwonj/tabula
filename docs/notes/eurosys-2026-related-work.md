# EuroSys 2027 — Related-Work Framing

Working note capturing the **related-work positioning frame** for the
EuroSys 2027 submission (deadline 2026-05-14; conference April 2027).
This note is:

- **Authoritative for related-work shape** until superseded by a draft.
- **Not authoritative** for architecture — trust
  [`docs/design/architecture.md`](../design/architecture.md).

Supporting notes:

- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) —
  the headline claim (C1) and mechanism list (M1-M5) that this frame
  has to instantiate.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — what
  "same workload" means in the empirical comparison below.

## Why This Note Exists

The headline claim is *compiler–proof co-design turns runtime re-proving
into compile-time sealing, for typed tabular state transitions*. Related
work must instantiate that claim directly: show the design space, place
prior systems inside it, and justify why Tabula's corner was not already
occupied.

An earlier draft used a linear "specialization level" taxonomy (general
zkVMs → custom-VM zkVMs → hand-rolled circuits). That framing is
misleading — Cairo is not *more specialized* than SP1, just specialized
at a different layer — and it does not tie cleanly to C1's
compile-time-vs-runtime axis. This note locks a **2×2 frame** that fixes
both problems and produces a figure that can double as the §1 anchor.

## The Frame

Two axes:

- **Sealing time.** Is the proof topology — program binding, table /
  memory layout, shard count and anchor multiplicities, lookup tables
  — fixed at *compile time* (baked into the artifact the verifier
  checks against) or built per proof at *runtime*?
- **Programmability.** Can users supply arbitrary programs against a
  fixed prover/verifier infrastructure (*programmable*), or is the
  prover tied to one specific computation (*non-programmable*)?

|                         | **Runtime-sealed**                                         | **Compile-time-sealed**         |
|-------------------------|------------------------------------------------------------|---------------------------------|
| **Programmable**        | SP1, RISC0, Jolt, Ceno, OpenVM · Cairo, Valida, Miden      | **Tabula**                      |
| **Non-programmable**    | (structurally empty — strictly worse)                      | Lighter, zkLedger               |

Two observations this frame makes explicit:

1. **The top-left was empty before Tabula.** Every prior programmable
   zkVM — whether RISC-V-native or custom-VM — is runtime-sealed. The
   compile-time-sealed column was only reachable by giving up
   programmability.
2. **The bottom-right does not exist in practice.** A non-programmable
   system that also deferred sealing to runtime would be strictly worse
   than a compile-time one on every axis. The design space is three
   occupied cells, and one occupied corner (Tabula).

## Why The Top-Left Was Empty

Compile-time sealing requires the compiler to reason about a program's
proof topology — which columns or tables it touches, which memory
invariants hold, which lookups it generates — *before* the program
runs. For programs that admit arbitrary memory aliasing, dynamic
dispatch, or data-dependent control flow, this is either undecidable
or prohibitively conservative.

Tabula pays for the top-left corner by accepting a **domain
restriction**: typed tabular state transitions over static
`(table, column)` coordinates with dynamic `row`. This restriction is
exactly what makes the compile-time analyses tractable — NF-1..4 and
True SSA become linear-scan checks, the `relation` construct lowers to
a finite static-table closure, shard topology becomes a type-directed
layout decision, and chip widths are determined by column type.

The C1 claim in one sentence: **the top-left corner opens up once you
accept a domain that admits compile-time sealing**. The paper does not
claim the domain restriction is universally desirable — it claims that
within workloads that fit the typed tabular shape, the corner is
reachable without loss of soundness.

## The Three Occupied Cells

### Cell A — Programmable, Runtime-Sealed, Generic ISA

**Systems:** SP1, RISC0, Jolt, Ceno, OpenVM.

General-purpose RISC-V zkVMs. They accept arbitrary RISC-V ELF and
produce proofs of execution. The prover/verifier pair is fixed; every
program-specific quantity — memory consistency arguments, lookup shape,
paging, syscall handling — is re-derived per proof. Program binding is
typically a hash of the ELF (analogous to Tabula's `semantic_hash`),
but encoding and layout decisions are reconstructed dynamically.

**OpenVM is the closest philosophical neighbor.** Its precompile
extensibility — users register domain-specific chips dispatched via
custom opcodes — is a *half-step* toward compile-time co-design.
However, the base VM stays runtime-dynamic: precompiles are additive,
not a replacement for RISC-V's generality, and the topology of the
proof around the precompiles is still built per proof. OpenVM's story
is "you can add domain chips," not "the prover is sealed around your
program's shape."

**What Tabula trades away versus Cell A:** generality. Tabula cannot
run arbitrary Rust or C — only programs expressible in `tabula-lang`
over the typed tabular model.

**What Tabula gets in exchange:**

- *M1* eliminates intra-tx RAM consistency entirely (Cell A all pay
  this as a permutation argument over their memory trace).
- *M2* allocates per-type widths (Cell A uses uniform 64-bit words).
- *M3* shards by static `(table, col)` coordinate (Cell A has no
  column concept; execution is monolithic).
- *M5* lowers domain relations to static tables at compile time (Cell
  A's analogues are runtime range / membership predicates).

### Cell B — Programmable, Runtime-Sealed, Custom ISA

**Systems:** Cairo/StarkNet, Valida, Miden.

zkVMs where the ISA itself is co-designed with the prover. Cairo uses
a CAIRO-native ISA with algebraic memory (read-once addressable
permutation); Valida is RISC-V-shaped but with prover-aware choices;
Miden is stack-based and tuned for STARKs. All three are
Turing-complete and accept arbitrary programs.

**Why this is not "more specialized than Cell A."** Cell B systems
are co-designed at the ISA layer, not at the compiler-proof seam. The
prover still constructs program-specific trace layout, memory usage
patterns, and lookup topology per proof — just against AIR-friendlier
primitives. Cairo's algebraic RAM is a cheap RAM-consistency
construction, but the *program's* memory usage is still reconstructed
per proof; Tabula's M1 does not make RAM cheaper — it *eliminates the
argument* for intra-tx flows.

**The co-design layer contrast, sharpened:**

- Cell B co-designs the **ISA**: which opcodes exist, how they map to
  AIR constraints, what primitive types the VM natively carries. This
  is VM-level co-design.
- Tabula co-designs the **compiler↔proof seam**: NF enforcement in
  IR (M1), static-coordinate sharding from the type system (M3),
  sealed artifact binding (M4), relation lowering (M5). None of these
  require a specific ISA.

This distinction is load-bearing. Without it, a reviewer can
legitimately ask "isn't Cairo already co-designed?" The answer is
"yes — at the ISA layer. Our contribution is co-design at a higher
layer, above any specific ISA." M1-M5 are all above the ISA; they
would compose with a Cell B system's ISA as readily as with a
RISC-V-like one.

### Cell C — Non-Programmable, Compile-Time-Sealed

**Systems:** Lighter (orderbook DEX), zkLedger (NSDI'18, confidential
financial ledger).

Hand-built cryptographic constructions — circuits or sigma-protocol
compositions — for one application or one application class. They seal
everything at compile time because "compile time" and "runtime" nearly
coincide: the circuit is the program, and the program is the circuit.

**Lighter and zkLedger — same cell, different roles.**

- **Lighter** is one-off: a hand-rolled circuit for spot-orderbook DEX
  matching. Changing the app — a new order type, a new risk check —
  means rewriting the circuit.
- **zkLedger** is class-scoped: a construction for confidential ledger
  auditing that instantiates across deployments but only across that
  class. The matrix structure (assets × parties) and the sigma-protocol
  auditing queries are baked into the construction.

**zkLedger as the closest tabular-semantics precedent.** It recognizes
the same "columns as assets, rows as parties" structure that motivates
Tabula's typed tabular model. What separates Tabula from zkLedger is
*programmability*: zkLedger's matrix shape is hard-coded into a
cryptographic construction; Tabula's is a type in a DSL compiled to a
general tabular zkVM.

**What Cell C already achieves that Cell A/B do not:** compile-time
sealing. The whole circuit *is* the sealed artifact. The cost is that
adding capability means rebuilding the circuit, and doing so safely
demands circuit-engineering skill, not software-engineering skill.

**Tabula's contribution relative to Cell C:** lift the compile-time
sealing behavior into a programmable system. The same sealing wins,
but the "program" is a DSL source file typed against a schema, not a
hand-built circuit.

## Tabula's Corner

Programmable, compile-time-sealed, domain-restricted to typed tabular
state transitions.

The five mechanisms (M1-M5, contributions note §C2) are the concrete
shape that occupying the top-left corner takes:

| M   | What it seals at compile time                                          | What it replaces at runtime                         |
|-----|------------------------------------------------------------------------|-----------------------------------------------------|
| M1  | Intra-tx RAM consistency (NFs + True SSA)                              | Runtime RAM permutation argument (Cell A/B)         |
| M2  | Per-column chip widths (Bool=1, U64=3, I64=3, Digest=8)                | Uniform-width VM words (Cell A/B)                   |
| M3  | Per-column shard topology, anchor multiplicities, scheme selection     | Monolithic trace layout (Cell A/B)                  |
| M4  | Artifact-bound `PublicStatement` / `BoundStatement` verification object | "Statement inside the proof" idiom (Cell A/B common)|
| M5  | `relation` construct → RangeCheck / StaticTableLookup                 | Runtime range / membership predicates (Cell A/B)    |

M1, M2, M3, and M5 save runtime work. M4 is a **soundness-layer** win:
statement-first verification makes `(sealed artifact, expected
statement, proof)` a single checked unit. Cell A/B systems typically
check `(program hash, proof)` and extract the statement from inside
the proof — not unsound, but it shifts the trust boundary in a way
M4 explicitly tightens.

## Empirical Vs. Structural Comparison

The §Related Work and §Evaluation sections distinguish two comparison
modes and must be explicit about which is used for which cell.

### Empirical (end-to-end numbers)

- **Cell A (required).** SP1 is the primary empirical baseline; RISC0
  is best-effort. Both accept the Tabula workload (StarkEx-class
  multi-asset rollup, per [`eurosys-2026-workload.md`](eurosys-2026-workload.md))
  under a Poseidon2-SMT hashing-parity convention. Ablation A5 owns
  this axis.

- **Cell B (not benchmarked).** Rationale:
  - Porting the workload to Cairo / Valida / Miden is nontrivial and
    confounds the measurement with programmer effort and ecosystem
    tooling differences.
  - Cairo's proving stack (STARK-curve ECDSA, Pedersen hashing) is
    Starknet-ecosystem-coupled; swapping for Poseidon2 violates the
    "same hashing primitives" rule from the workload spec.
  - Valida and Miden have thinner benchmarking tooling; including
    them empirically is a high-variance use of the remaining window.

- **Cell C (not applicable).** Lighter proves a DEX orderbook;
  zkLedger proves sigma-protocol ledger queries. Neither accepts our
  workload; emulating them inside Tabula is not a comparison.

### Structural (discussion only)

- Cell B receives a paragraph contrasting **co-design layer** (ISA vs.
  compiler-proof seam).
- Cell C receives a paragraph contrasting **programmability**
  (hand-rolled circuit vs. DSL-compiled zkVM).

The paper must state this split explicitly, e.g.:

> We compare empirically against SP1 (required) and RISC0 (best-effort)
> because these are the only systems that accept the same workload
> under fair conditions. Cell B and Cell C systems are contrasted
> structurally, not benchmarked — see §N for our rationale.

Without that note, reviewers may read the missing Cell B numbers as an
omission rather than a methodological choice.

## Per-System Notes

### Cell A

- **SP1** — Succinct Labs. RISC-V zkVM with Plonky3 backend. Primary
  empirical baseline. Poseidon2 precompile availability must be
  verified in the current release.
- **RISC0** — Risc Zero. RISC-V zkVM with STARK backend. Best-effort
  empirical baseline; drop if setup cost exceeds one week.
- **Jolt** — a16z. Lasso-based lookup-heavy RISC-V zkVM. Mention but
  do not benchmark — a comparison would be confounded by proof-system
  choice, not co-design.
- **Ceno** — RAM-focused zkVM. Worth structural treatment: its
  contribution is non-uniform RAM handling, which overlaps Tabula's
  M1 from the opposite direction (they optimize the argument; we
  eliminate it for intra-tx).
- **OpenVM** — Axiom. RISC-V + precompile extensibility. Treat as
  Cell A with an explicit callout: precompiles are a half-step toward
  compile-time co-design but remain runtime-dynamic in the base VM.

### Cell B

- **Cairo/StarkNet** — StarkWare. Custom ISA with algebraic memory.
  The Cell B system closest in spirit to Tabula (typed, structured),
  differing in co-design layer and in programmability model (Turing-
  complete vs. domain-restricted).
- **Valida** — Lita Foundation. RISC-V-shaped custom ISA with
  prover-aware design choices.
- **Miden** — Polygon. Stack-based custom VM tuned for STARKs.

### Cell C

- **Lighter** — app-specific orderbook circuit. Cited as the
  co-design-by-hand precedent: every sealing win Tabula gets
  automatically, Lighter gets by hand-building one circuit per app.
- **zkLedger** (NSDI'18) — tabular-semantics precedent. Cited as the
  closest prior art on the typed tabular axis, despite operating at a
  different cryptographic layer (sigma protocols + Pedersen, not
  STARK). The right place in the paper to make the "programmable
  tabular" move explicit.

## Explicitly Out Of Scope

- **zkEVMs** (Polygon zkEVM, Scroll, zkSync Era). Specialized for EVM
  semantics. Different domain; neither our workload nor our co-design
  axis overlaps. One-sentence out-of-scope note suffices.
- **Powdr.** PIL-based compiler to PLONKish constraint systems. A
  compiler-layer project adjacent to Tabula, but it does not co-design
  the proof system — it emits constraints for an external backend.
  Mention only if space allows.
- **General proving networks** (Succinct, Gevulot, etc.). Proof
  distribution layer, orthogonal to co-design. Relevant to
  [`distributed-proving.md`](distributed-proving.md) future work, not
  to the current paper.
- **Proof-system primitives** (Spartan, Brakedown, HyperPlonk, Nova /
  HyperNova folding). Wrong layer. Tabula uses Plonky3 (STARK +
  Poseidon2) as given; we do not compete on proof-system design.

## Pre-Empted Reviewer Objections

Likely objections and the frame's answers. All belong in §Related Work
or §Discussion, not in §1.

- *"Compile-time sealing is trivial — just specialize the prover for a
  fixed program."* That is Cell C. The claim is **programmable**
  compile-time sealing, which requires domain structure.
- *"Cairo's algebraic RAM is compile-time sealing of RAM consistency."*
  It is a cheap RAM-consistency construction, but the program's memory
  *usage pattern* is still reconstructed per proof. M1 eliminates the
  argument for intra-tx flows entirely; Cairo makes it cheaper.
- *"OpenVM precompiles are compile-time sealing."* They seal specific
  chips, not proof topology (memory shape, shard count, scheme
  selection). Partial, not full — called out explicitly in Cell A.
- *"zkLedger already covers tabular ledgers."* At the cost of
  programmability — changing queries means changing the
  sigma-protocol construction. Tabula's "shape" is a type in a DSL,
  not a cryptographic construction.
- *"You're just formalizing what circuit engineers already do by
  hand."* Exactly — M1-M5 lift ad-hoc per-app decisions into
  compiler-enforceable, DSL-accessible discipline, making co-design
  reusable instead of re-done per circuit.

## Open Decisions

- **§1 Introduction placement.** The 2×2 is strong enough to anchor
  §1 as a motivating figure rather than being buried in §Related Work.
  Decision deferred to the section-outline task; recommend promoting.
- **Figure style.** Text table (as in this note) vs. a quadrant figure
  with systems plotted as points vs. a Venn-style diagram over axes.
  Decide during figure layout for the draft.
- **OpenVM empirical inclusion.** If OpenVM setup is no harder than
  SP1's and Poseidon2 is available, include it in A5. Otherwise
  relegate to Cell A structural discussion.
- **Ceno treatment length.** The M1-vs-Ceno structural paragraph is
  the single most interesting Cell A contrast. Worth a dedicated
  subsection if space allows; otherwise one paragraph inside the
  Cell A discussion.

## Pointers

- Contributions and M1-M5 surface: [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md)
- Workload spec (what "same workload" means): [`eurosys-2026-workload.md`](eurosys-2026-workload.md)
- Distributed proving (orthogonal axis, Future Work): [`distributed-proving.md`](distributed-proving.md)
- Architecture canon: [`../design/architecture.md`](../design/architecture.md)
