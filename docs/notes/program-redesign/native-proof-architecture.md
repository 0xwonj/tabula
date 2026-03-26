# Post-Cutover Proof Architecture

> **Status**: Canonical current-state proof architecture for the core-first rewritten path
> **Date**: 2026-03-25
> **Scope**: Summarizes the final post-cutover proof boundary for the rewritten pipeline

## Current Proof Boundary

Current behavior on the rewritten path:

- source compiles and registers without legacy artifact back-conversion
- runtime setup builds directly from `tabula_compiler::RegisteredProgram`
- rewritten tx batches execute, prove, and verify through `tabula_runtime`
- V2 context and typed event semantics survive through proof generation
- V3 `if` / `match` control survives through canonical guarded lowering and
  proof generation
- AIR public values remain only old/new state roots
- semantic proof statements remain native and digest-bound in the
  transcript
- `StaticTableRoot` is transcript-bound in the native proof statement and
  verifier contract
- rewritten queries execute correctly and still remain execution-only

## Core-First Scope

Implemented and exercised end to end:

- state reads and writes
- builtin hash
- checked assertions and partial operations
- public context binding
- typed event emission
- canonical guarded control from statement-level `if` and `match`
- static canonical relation proving for `EnumSet` and `Map`
- source `Range` and `Set` through their existing canonical normalization

Still deferred:

- capability proving
- property-read proving
- query proving
- `Extern` relation proving

## Deferred Extensions

Still intentionally deferred:

- capability proving
- property-read proving
- query proving
- `Extern` relation proving
- later spec-layer features

## Deferred Relation Performance Follow-Ups

The static canonical relation path is now sound and compiler-sealed, but two
performance-oriented follow-ups were intentionally deferred from the soundness
repair pass.

These are **not correctness blockers**. They are future optimizations that
should be implemented as a separate pass so they do not blur soundness fixes
with trace-shape redesign.

### 1. Shrink the execution relation relay

Current state:

- the `RelationProof` execution row carries both:
  - execution-facing relation proof relay data
  - tuple payload material needed only by transcript proving

That means the generic execution trace pays a width cost for relation tuple
payload that is not intrinsically part of execution correctness.

Intended future shape:

- keep only execution-facing relation proof relay fields in the execution lane:
  - `relation_id`
  - assert/eval flag
  - `tx_index`
  - `effect_ordinal_in_tx`
  - `input_digest`
  - `output_digest`
  - output-write linkage needed to bind `eval relation` results back into
    execution state
- move tuple payload needed only for transcript proving into a dedicated
  relation witness carrier / transcript input lane

Why it was deferred:

- this is a trace-contract redesign, not a local cleanup
- it changes the boundary between the execution chip and the relation transcript
  chip
- it is safer to land after the sound relation proof family is stabilized

### 2. Remove tuple payload duplication across transcript rounds

Current state:

- the relation transcript path repeats tuple payload metadata across multiple
  transcript rows for the same tuple digest call
- this increases relation transcript trace width and memory use, especially for
  larger tuples or relation-heavy workloads

Intended future shape:

- store only block-local or state-local transcript data on continuation rows
- avoid repeating the full tuple payload on every round row
- keep one canonical tuple schedule, but lower it into a more compact
  per-round trace representation

Why it was deferred:

- this requires redesigning the internal row encoding of the
  `RelationTranscriptChip`
- it is more intrusive than the soundness repair because it changes how
  transcript rows are materialized, not just how they are constrained

### Recommended order

If and when these are implemented, the recommended order is:

1. shrink the execution relation relay
2. then compact the transcript trace rows

That order keeps the higher-level chip responsibility split clear first, then
optimizes the internal transcript trace shape after the relay contract is
stable.

## Architectural Conclusion

The canonical proof stack is now:

`source -> tabula-compiler -> RegisteredProgram -> tabula-runtime -> tabula-witness -> PreparedMachineInput -> tabula-machine`

Within the current scope, this path is native end to end. AIR public
values remain minimal, while richer semantic proof meaning stays in the runtime
`ProofStatement` digest bound through the transcript. Static canonical relation
proof preparation is witness-owned: `tabula-runtime` orchestrates execution and
semantic journal reduction, `tabula-witness` derives the sealed static relation
table and transcript claims, and machine/chips consume only prepared relation
witnesses.
