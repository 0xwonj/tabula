# Tabula vs zkVM Benchmark Spec

> Purpose: define a fair, repeatable comparison protocol between Tabula and
> general-purpose zkVMs when the proving abstraction is not the same.
> Status: research evaluation spec, not canonical architecture.

## Why A New Spec Is Needed

zkVMs can often be compared on the same low-level workload because they prove a
similar thing: execution of a general-purpose machine.

Tabula does not prove a machine trace as its primary semantic unit. It proves
typed state transitions and binds them into a semantic proof statement. That
means "same cycles", "same guest steps", or "same instruction mix" is not the
right comparison boundary.

The correct unit of comparison is:

> the same public semantic claim about the same application transition

This document defines how to make that precise.

## Goals

- compare Tabula and zkVMs on equal semantic claims rather than equal machine
  internals
- separate product-facing benchmark questions from architecture research
  questions
- make benchmark reports interpretable even when the systems use different
  internal state models and proving boundaries
- provide a small canonical workload suite that matches Tabula's intended use
  cases without hiding its non-goals

## Non-Goals

- producing one universal scalar score for "which system is faster"
- forcing Tabula into a zkVM-shaped benchmark boundary
- rewarding or penalizing one system for compile-time work without reporting it
  explicitly
- treating query-only execution as part of Tabula's current tx-proof path

## Current Tabula Proof Scope

Tabula's current comparison boundary should follow the proving surface the
repository already owns:

- proved stateful transaction batches
- semantic public context bound into the statement
- old/new state commitment roots
- event digest bound into the statement

Query execution is intentionally supported as execution-only, but query proving
is currently absent. Cross-system benchmarks must not compare "query proved in a
zkVM" against "query executed but not proved in Tabula" and call that an equal
workload.

## Core Rule

The benchmark must equalize the proof statement before it compares prover time.

The benchmark statement must answer:

> Given a fixed program identity, old committed state, public context, and tx
> batch, does the proof establish the same accepted transition to the same new
> committed state and the same public semantic outputs?

If the answer is not literally yes, the workloads are not benchmark-equivalent.

## Canonical Comparison Modes

Two benchmark modes are required. A third optional mode is useful for backend
diagnostics.

### 1. Semantic-Native Mode

Each system implements the same application semantics in its natural style.

Use this mode to answer:

> Which system would I actually want to ship for this application shape?

Rules:

- application semantics must match
- public statement fields must match
- each system may use its own native internal lowering and optimization
- compile-time or preprocessing work may differ, but it must be reported
  separately from steady-state proving

This is the primary benchmark mode for product decisions.

### 2. Constraint-Equalized Mode

The benchmark fixes more than semantics. It also fixes the proof-visible state
model and commitment rules as much as possible.

Use this mode to answer:

> Where does the performance difference come from once the semantic claim is
> equalized more aggressively?

Rules:

- application semantics must match
- public statement fields must match
- state commitment model should match when feasible
- hash family and public digest conventions should match when feasible
- if exact matching is not feasible, the mismatch must be called out explicitly

This is the primary benchmark mode for architecture research.

### 3. Backend Microbench Mode

This mode is optional and should never be the headline result.

Use it to measure isolated components such as:

- commitment updates
- path proving
- hash-heavy kernels
- witness preparation
- proof assembly after execution is already materialized

This mode is useful for diagnosis, but it does not replace a semantic benchmark.

## Canonical Proof Statement Boundary

The benchmark must define one minimal public claim that every system proves.

For Tabula-oriented transaction benchmarks, the canonical public claim is:

| Field | Meaning |
| --- | --- |
| `program_binding` | exact program or artifact identity bound by the proof |
| `old_state_root` | commitment to the pre-state |
| `batch_payload` | canonical tx batch payload |
| `public_context` | public context inputs |
| `new_state_root` | commitment to the post-state |
| `event_digest` | digest of proof-visible emitted events |
| `failure_policy` | accepted and rejected cases for the same tx batch |

Rules:

- if a system exposes extra public outputs, clamp comparison to the common
  fields above and report extras separately
- if one system proves fewer fields, the benchmark is invalid until the proof
  boundary is aligned
- if one system proves helper query logic that the other does not, either move
  that logic into the common proved path or remove it from both sides

## Required Timing Categories

Every benchmark report must include these categories.

### Primary

- `end_to_end_latency`
  - from semantic inputs to proof bytes
  - includes execution or witness generation on the critical path
- `verify_latency`
- `proof_size_bytes`

### Secondary

- `prove_only_latency`
  - only when the system exposes a real post-execution proving seam
- `compile_or_setup_latency`
  - compilation, image build, preprocessing, or warm-up required before the
    first proof
- `peak_rss_bytes`
- `total_cpu_time`

Headline comparisons should prefer `end_to_end_latency`, not `prove_only_latency`.
`prove_only_latency` is useful, but it is not sufficient for Tabula vs zkVM
comparisons because the systems can expose different execution/proving seams.

## Warm vs Cold Policy

Every report must separate:

- `cold_first_proof`
  - from a fresh process and cold setup state
- `warm_steady_state`
  - after the program artifact, runtime image, or equivalent reusable setup is
    already prepared

The benchmark must state which of the following were reused:

- compiled program or guest image
- runtime or verifier warm-up
- proving keys, preprocessed tables, or equivalent reusable setup
- cached state commitments or Merkle metadata

If one system amortizes work across many proofs and another does not, that is a
real architectural property. The benchmark should expose it, not hide it.

## Workload Descriptor

Every measured point must publish a workload descriptor alongside the timing
numbers.

The canonical workload vector is:

```text
W = (
  state_rows,
  batch_size,
  tx_count,
  touched_rows,
  touched_fields,
  read_count,
  write_count,
  relation_count,
  capability_count,
  hash_count,
  emitted_event_count
)
```

Not every system will expose every field directly. When exact counts are
unavailable, the report should provide the closest trustworthy approximation and
mark the field as estimated.

This vector exists so that benchmark readers can distinguish:

- fixed prover cost vs marginal tx cost
- state-heavy vs compute-heavy behavior
- lookup or relation-heavy behavior vs plain updates

## Canonical Workload Families

The suite should contain at least four families. Three are primary. One is a
negative control.

### Family A: Transfer

Intent:

- state-heavy, low-compute transition
- exposes read/write and event behavior without heavy cryptographic logic

Semantics:

- debit one account
- credit another account
- enforce ownership and sufficient balance
- emit one transfer event

Scale axes:

- number of accounts in committed state
- batch size
- number of transfers touching distinct rows vs repeated rows

Why it matters:

- this is the baseline state transition workload most product users understand
- it exercises the proving boundary Tabula is designed for

### Family B: Membership Approval

Intent:

- relation-heavy, policy-heavy transition
- exposes proof-visible checks that are not just raw value movement

Semantics:

- check approval policy against member row and public context
- update member tier or approval state
- emit one semantic event

Scale axes:

- number of members
- number of relation checks
- branching or rule diversity

Why it matters:

- it captures the kind of typed policy workflow where Tabula should look
  structurally different from a zkVM

### Family C: AMM Swap Settlement

Intent:

- mixed workload with state updates plus hash or capability activity
- exposes a more application-like proving boundary than a pure transfer

Semantics:

- validate one swap against pool state and public context
- update committed settlement state
- emit one swap event
- bind any quote or digest logic that is intended to be compared as part of the
  proved transition

Scale axes:

- number of pools
- batch size
- number of hash or capability invocations
- touched-column ratio

Why it matters:

- it shows whether Tabula's structured state model still helps once nontrivial
  application logic is mixed in

### Family D: Compute-Heavy Control

Intent:

- negative control
- identifies the boundary where a general-purpose zkVM may be the more natural
  fit

Semantics:

- minimal committed-state interaction
- substantial arithmetic, hashing, or loop-heavy guest logic

Examples:

- long hash chain
- arithmetic accumulation
- branch-heavy pure computation with no meaningful committed state change

Why it matters:

- this keeps the benchmark honest
- Tabula should not claim universality by only measuring its natural sweet spot

This family should be reported, but it should not become the single headline
benchmark for the whole comparison.

## Parameter Sweeps

Each primary workload family must be measured over at least these sweeps:

- `batch_size`
  - recommended minimum: `1, 8, 64, 256`
- `state_rows`
  - recommended minimum: one small, one medium, one large committed state
- `touch_pattern`
  - repeated hot rows vs mostly distinct rows

If the system supports meaningful parallelism, also report:

- fixed thread count
- optional thread sweep as a separate appendix, not mixed into the primary
  headline table

The benchmark should fit a simple model where useful:

```text
T(batch_size) ~= a + b * batch_size
```

Where:

- `a` is fixed prover overhead
- `b` is marginal cost per tx for the chosen workload family

This matters because Tabula may trade fixed overhead and marginal cost
differently from a zkVM.

## Required Reporting Rules

Every result table must include:

- system name and version
- benchmark mode: `semantic-native` or `constraint-equalized`
- hardware summary
- thread count
- build profile
- security setting summary
- workload family name
- workload descriptor vector
- cold and warm timing categories
- proof size
- verifier time
- any mismatch or caveat in the proof boundary

Every report must also include a short plain-language caveat block answering:

1. What exactly is being proved?
2. What is intentionally excluded?
3. What work is amortized across proofs?
4. Which result should a product team treat as the headline number?

## Invalid Comparison Patterns

The following comparisons are invalid under this spec:

- comparing Tabula `prove` on a prebuilt execution receipt to zkVM
  end-to-end proving and calling it apples-to-apples
- comparing a Tabula execution-only query against a zkVM proof of that same
  query
- comparing different public statements and calling them the same workload
- hiding compile-time or setup-time preprocessing that materially affects the
  first proof
- comparing different batch payload semantics under the same benchmark name

## Initial Repository-Aligned Seeds

The current repository already contains useful starting points, but they are not
all benchmark-ready headline workloads.

### Good Seeds

- `membership`
  - good seed for Family B
- `dex`
  - good seed for Family C after the proved boundary is tightened so that all
    compared swap logic is actually in the proved path

### Smoke Or Diagnostic Seeds

- `basic`
  - useful prover smoke test
  - too small and too synthetic to be a headline comparison workload

### Intended But Not Yet Stable Seed

- `bank`
  - intended to represent Family A
  - should become the canonical transfer benchmark once the current proof-path
    instability is fixed

## Practical Reading Of Results

The benchmark should support three distinct conclusions without conflating them.

1. Product conclusion
   - Which system gives lower end-to-end proof latency and acceptable proof
     size for the same application transition?

2. Architecture conclusion
   - Does Tabula gain from semantic structure once the proof boundary is
     aligned fairly?

3. Boundary conclusion
   - On which workload families does the mismatch between native state-machine
     proving and general-purpose machine proving matter most?

If the benchmark cannot answer all three cleanly, the benchmark is underspecified.

## Recommended Next Step

A minimal first comparison suite under this spec should be:

1. transfer
2. membership approval
3. AMM swap settlement
4. compute-heavy control

For each workload:

- implement one Tabula-native version
- implement one zkVM-native version
- align the proof statement
- publish cold and warm results
- publish the workload descriptor vector
- fit fixed vs marginal cost over a batch-size sweep

That is the smallest suite likely to produce an honest Tabula vs zkVM
comparison rather than a benchmark that accidentally rewards one abstraction
boundary over another.
