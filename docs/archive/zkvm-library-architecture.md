# Tabula zkVM Library Architecture

> Status: In Progress
> Date: 2026-03-16
> Audience: maintainers of `compiler`, `runtime`, `artifact`, `contract`, `machine`
> Related:
> - [final-target-architecture.md](./final-target-architecture.md)
> - [architecture.md](./architecture.md)
> - [codebase-architecture-review.md](./codebase-architecture-review.md)
> - [proof-spec.md](../spec/proof-spec.md)
> - [semantics-spec.md](../spec/semantics-spec.md)

---

## 1. Purpose

This document consolidates the current Tabula architecture discussions and fixes the design target
for Tabula as an **open-source zkVM library**, not as a daemon or web product.

The central question is:

> What are the mandatory library surfaces a zkVM must provide, what must remain internal, and how
> should Tabula structure its core crates around that boundary?

This document also compares Tabula's intended direction with SP1 and OpenVM and uses those
codebases as design references.

The external reference points used for this document are:

- SP1 workspace `v6.0.2`
- OpenVM stable mainline `v1.5.0`
- OpenVM public v2 track on `develop-v2.0.0-rc.1` and `v2.0.0-alpha`

The reason to include both OpenVM stable and public v2 is that the v2 codebase makes the final
developer surface even more explicit through `sdk-v2` and `verify-stark`, which is highly relevant
to Tabula's own boundary design.

Implemented in the current workspace as of this document:

- `ExecutionStatement` is now part of the proof core and its digest is bound into machine proofs.
- `CompiledProgram` owns required precompile/property capabilities and column proof planning.
- `ProgramArtifact` serializes that capability/proof-plan metadata as part of the sealed program.
- `ProgramArtifact`, `StateSnapshot`, and `TransactionBatch` now own their canonical digest APIs.
- `runtime` consumes compiler-owned proof shape instead of deriving per-column setup from schemas.
- free execute now fails fast when a program requires prepared-runtime capabilities.
- prepared-runtime build validates property-query support on both executor and prover sides.
- `CompiledProgram` is now accessor-based instead of a fully mutable public struct.
- `tabula-machine` now proves from a single `MachineProofInput` and validates column inputs instead
  of relying on parallel arrays and panicking on malformed input.
- `tabula-machine` now exposes borrowed `Prover` and `Verifier` facades over shared machine setup,
  so proof generation and verification can evolve as distinct public surfaces without duplicating
  backend state.

---

## 2. Design Philosophy

Tabula should be designed as a **library-first proving system**.

That means:

1. The core product is usable without `daemon`, `web`, remote services, or a database.
2. Proof verification is possible from stable artifacts and explicit expected context.
3. Semantic ownership is centralized and never split across adapters.
4. Runtime execution and proof generation are exposed as reusable library APIs.
5. Control-plane concerns remain optional reference layers.

The right mental model is:

```text
compiler -> runtime -> verifier
```

not:

```text
cli -> daemon -> runtime
```

Adapters are replaceable.
The zkVM core is not.

---

## 3. What an Open-Source zkVM Library Must Provide

An open-source zkVM library should provide these mandatory surfaces.

### 3.1 Compile / Build Surface

Users need a stable way to turn a program into a canonical executable semantic object.

In Tabula this is:

- source or sealed program artifact input
- static validation
- canonical semantic output

This is the role of `tabula-compiler`.

### 3.2 Host Runtime Surface

Users need a host-side API for:

- execute only
- prepare once per program
- prove
- verify

This is the role of `tabula-runtime`.

### 3.3 Canonical Artifact Surface

Users need portable objects for storage, transport, caching, and offline verification.

These must be stable, digestible, and documented.

This is the role of `tabula-artifact`.

### 3.4 Contract / Compatibility Surface

Users need explicit rules for what verifier/runtime/compiler combinations are accepted.

This is the role of `tabula-contract`.

### 3.5 Verifier Surface

Users must be able to verify a proof or receipt without depending on the entire proving stack.

This verifier surface may live in `runtime` initially, but conceptually it must be a thin,
stable boundary.

---

## 4. What a zkVM Library Does Not Need To Own

These are not core zkVM responsibilities:

- daemon
- run catalog
- multi-tenant proving service
- remote job queues
- web UI
- orchestration-specific DTOs
- storage-level records and lifecycle status

Those may exist as examples or reference services, but the core must not depend on them.

This is the main reason `artifact` must remain canonical and passive, while daemon/web types must
remain local to those adapters.

---

## 5. Reference Patterns from SP1 and OpenVM

### 5.1 SP1

SP1 exposes a clear top-level user boundary through `sp1-sdk`.

The user-facing trait and client are:

- `Prover::setup`
- `Prover::execute`
- `Prover::prove`
- `Prover::verify`

See:

- [`sp1-sdk` `Prover` trait](https://github.com/succinctlabs/sp1/blob/main/crates/sdk/src/prover.rs)
- [`sp1-sdk` `ProverClient`](https://github.com/succinctlabs/sp1/blob/main/crates/sdk/src/client.rs)

SP1 also provides a guest-side library and runtime boundary:

- `sp1-lib`
- `sp1-zkvm`

See:

- [`sp1-lib`](https://github.com/succinctlabs/sp1/blob/main/crates/zkvm/lib/src/lib.rs)
- [`sp1-zkvm`](https://github.com/succinctlabs/sp1/blob/main/crates/zkvm/entrypoint/Cargo.toml)

Architecturally important points:

1. There is a single obvious host SDK entrypoint.
2. Guest runtime and host runtime are separated.
3. Proof verification is bound to verifying key and public values.
4. Internal executor, machine, recursion, prover, and verifier are more fragmented than the public
   API.

This is the right pattern: internal complexity, simple external boundary.

### 5.2 OpenVM

OpenVM is even more explicit that its `SDK` is the final interface.

Its contributor docs describe:

- `openvm-sdk` as the final proving interface
- `openvm` and `openvm-platform` as guest-side standard/runtime layers
- `openvm-circuit`, `continuations`, `toolchain`, and extension crates as internal framework

See:

- [OpenVM project layout](https://github.com/openvm-org/openvm/blob/main/docs/repo/layout.md)
- [`openvm-sdk`](https://github.com/openvm-org/openvm/blob/main/crates/sdk/src/lib.rs)
- [`openvm` guest stdlib](https://github.com/openvm-org/openvm/blob/main/crates/toolchain/openvm/src/lib.rs)

OpenVM's most relevant design choice is that users are expected to verify proofs against an
explicit `AppExecutionCommit`, not just public values.

See:

- [`AppExecutionCommit`](https://github.com/openvm-org/openvm/blob/main/crates/sdk/src/commit.rs)
- root verifier checks against expected app commits:
  [root verifier](https://github.com/openvm-org/openvm/blob/main/crates/continuations/src/verifier/root/mod.rs)

Architecturally important points:

1. User-facing API is an SDK, not raw circuit machinery.
2. Guest stdlib and host SDK are both first-class.
3. Proofs are bound to executable and VM configuration commitments.
4. Extensions span toolchain, guest, and circuit layers together.

This is especially important for Tabula: proofs should be bound to more than state roots.

### 5.3 OpenVM v2

OpenVM's public v2 track makes the same boundary even clearer.

The workspace splits developer-facing interfaces from backend internals:

- `sdk-v2` is the top-level host integration surface
- `verify-stark` is the standalone verification surface
- `toolchain/openvm` and `toolchain/platform` remain the guest/runtime side
- recursion, continuations, circuits, and extensions remain framework internals

Architecturally important points:

1. The proving entrypoint is still an SDK, not raw VM or circuit crates.
2. Verification is given its own dedicated boundary through `verify-stark`.
3. Proof verification is parameterized by explicit baseline commitments, not only proof bytes.
4. The public proof boundary is versioned independently from internal executor details.

This is a strong signal for Tabula:

- `runtime` should be the default proving API
- verifier-facing types should be explicit and stable
- `machine` should remain a backend crate, not the main integration surface
- the canonical crate-boundary contract lives in [proof-backend-contract.md](proof-backend-contract.md)

### 5.4 What Other zkVMs Actually Make Users Touch

SP1 and OpenVM both have many internal crates, but the user-facing surface is narrow.

The common pattern is:

- one obvious compile/build path
- one obvious host runtime / prove / verify path
- one verifier-friendly artifact or commitment path
- optional guest runtime only when the system executes general guest programs

They do **not** make most users assemble the proof system by composing low-level machine crates.

That is the important lesson for Tabula:

> internal modularity is good, but public integration surfaces must stay small and obvious.

---

## 6. Lessons for Tabula

The comparison to SP1 and OpenVM yields a few strong conclusions.

### 6.1 Tabula Must Expose a Small Public Core

Most users should only need:

- `tabula-compiler`
- `tabula-runtime`
- `tabula-artifact`
- `tabula-contract`

`tabula-machine` should be advanced or backend-level.
It should not be the default integration surface.

### 6.2 Tabula Does Not Need a Guest Runtime Today

SP1 and OpenVM both need guest runtime crates because they run general guest programs.

Tabula's programming model is different:

- it compiles a DSL into a canonical program model
- it does not embed a general-purpose guest standard library today

So Tabula does **not** need a guest runtime surface equivalent to `sp1-zkvm` or `openvm` yet.

Its mandatory runtime surface is the **host runtime**.

If Tabula later adds embedded or guest-executed user code, then a guest runtime layer becomes
necessary.

### 6.3 Statement Binding Must Move into the Core

Tabula currently has two different notions of "statement":

1. machine/runtime proving statement: currently state roots
2. daemon receipt statement: program hash, batch hash, metadata hash, and state hashes

That split is not a good long-term architecture.

The stronger pattern is OpenVM's:

> the proof is verified against an explicit expected execution commitment

Tabula should therefore promote a canonical `ExecutionStatement` into the core proof protocol.

### 6.4 Capability Registration Must Be Described Once

OpenVM extensions span guest, toolchain, and circuit layers together.
SP1 precompiles are integrated as a coherent platform capability.

Tabula should follow the same principle:

- executor handler
- runtime binding
- proving extension
- verifier compatibility

must be one conceptual capability, not several unrelated registrations.

### 6.5 Tabula Should Not Copy the Whole Workspace Shape

SP1 and OpenVM both have large workspaces because they serve multiple roles at once:

- guest runtime
- toolchain
- circuit backend
- recursion
- verifier targets
- CLI and service integrations

Tabula should copy the **boundary discipline**, not the raw number of crates.

Since Tabula is currently a DSL-driven proving system rather than a general guest-code VM, it does
not need to mimic guest-runtime-heavy layouts such as `sp1-zkvm` or `openvm`.

The correct takeaway is:

- small public surface
- strict semantic ownership
- explicit proof commitments
- backend internals hidden behind runtime

---

## 7. Final Architectural Decision

The final architecture for Tabula should be:

```text
tabula-compiler
  source / ProgramArtifact
    -> CompiledProgram

tabula-runtime
  CompiledProgram
    -> PreparedRuntime
    -> execute
    -> build statement
    -> prove
    -> verify

tabula-artifact
  ProgramArtifact
  StateSnapshot
  TransactionBatch
  ExecutionStatement
  ProofReceipt

tabula-contract
  ContractRef
  statement schema version
  binding version
  verifier profile version
  fail-closed compatibility policy

tabula-machine
  pure proof backend
```

And adapters remain outside that core:

```text
tabula-cli
tabula-daemon
tabula-web
```

---

## 8. Final Responsibility Split

### 8.1 `tabula-compiler`

Owns static semantics.

It must decide:

- schema legality
- tx legality
- canonical IR
- semantic digest
- required capabilities
- proof shaping plan
- contract reference

Its output is `CompiledProgram`.

`CompiledProgram` is the in-memory semantic handoff object.

### 8.2 `tabula-runtime`

Owns execution and proof orchestration.

It must provide:

- `run_compiled_batch`
- `PreparedRuntime`
- `execute`
- `build_execution_statement`
- `prove`
- `verify`

It must not:

- parse source syntax
- decide semantic compatibility policy
- invent statement hashing rules in an adapter

### 8.3 `tabula-artifact`

Owns only portable canonical objects.

It must contain:

- `ProgramArtifact`
- `StateSnapshot`
- `TransactionBatch`
- `ExecutionStatement`
- `ProofReceipt`

It must not contain:

- daemon records
- submit/list/get commands
- run statuses
- orchestration timestamps

### 8.4 `tabula-contract`

Owns fail-closed compatibility and statement schema rules.

It must define:

- what contract version is being used
- what statement schema version is being used
- what verifier profile/version is expected
- how compatibility is checked

### 8.5 `tabula-machine`

Owns proving backend internals.

It should know about:

- proof setup
- trace proving
- trace verification
- backend-specific proof objects

It should not know about:

- source files
- daemon receipts
- runtime handler registries
- control-plane records

---

## 9. Final Statement Model

The proof statement must be split conceptually into two layers.

### 9.1 AIR Public Values

This is the minimum data the AIR/backend actually consumes.

For example:

- old state root
- new state root

### 9.2 Canonical Execution Statement

This is what the product-level verifier actually trusts.

It should include:

- `program_digest`
- `contract_digest` or `contract_ref`
- `batch_digest`
- `pre_state_digest`
- `post_state_digest`
- `air_public_values`
- `verifier_profile_digest`

The proof must be bound to the digest of this statement.

This is the main architectural change still missing from the current Tabula core.

---

## 10. Public API Target

The target user experience should look like this:

```rust
let compiled = tabula_compiler::compile_source(src)?;

let runtime = tabula_runtime::PreparedRuntime::builder(compiled)
    .with_capability(...)
    .build()?;

let executed = runtime.execute(&state, &batch)?;
let statement = runtime.build_execution_statement(&state, &batch, &executed)?;
let receipt = runtime.prove(&statement, &executed)?;
runtime.verify(&statement, &receipt)?;
```

This should be enough for most users.

Direct `machine` usage should be optional advanced usage.

And the expected dependency story should be:

- most users depend on `tabula-compiler`, `tabula-runtime`, `tabula-artifact`, and
  `tabula-contract`
- verifier-only users should eventually need only a lightweight verification surface plus
  `artifact` and `contract`
- `tabula-machine` should be needed only by backend authors or advanced proof integrations

---

## 11. What Tabula Must Build Next

The highest-priority architectural tasks are:

1. Keep `ExecutionStatement` as the canonical proof claim and continue tightening verifier-facing
   APIs around it. Completed.
2. Make `CompiledProgram` own proof shape and required capability metadata. Completed.
3. Remove semantic/proof-shaping decisions from runtime builder options. Completed for column proof
   shape; runtime builder now only binds implementations and environment handlers.
4. Make `tabula-machine` a purer backend boundary. In progress.
5. Keep daemon/web as reference adapters only. In progress.

Tabula now has the core compiler/runtime/artifact/contract boundary in place, but verifier-only
surfaces and the remaining machine/backend cleanup are still pending.

---

## 12. Migration Strategy

The cleanest migration is:

1. define the final `ExecutionStatement` and `ProofReceipt`
2. make `contract` validate those versions and bindings
3. move proof-shaping metadata into `CompiledProgram`
4. collapse runtime execute/prove logic behind one prepared runtime path
5. simplify `machine` into a stricter backend API
6. remove adapter-owned receipt and statement logic

Current progress in the workspace:

- steps 1-3 are implemented
- step 4 is implemented enough for the prepared runtime to own proof shape and statement building
- step 5 is partially implemented through `MachineProofInput` and panic-free input validation
- step 6 is partially implemented; daemon now consumes runtime-produced statements, but a fully
  separate verifier-oriented receipt surface is still a future step

This order matters.

If statement and contract boundaries are not fixed first, later runtime and machine refactors will
still drift.

---

## 13. Final Position

Tabula should not aim to be a daemon with a prover inside.

It should aim to be:

1. a semantic compiler,
2. a reusable host runtime,
3. a canonical artifact and contract system,
4. and a pluggable proof backend.

That is the smallest architecture that is:

- reusable as a library,
- auditable as a proving system,
- extensible without adapter leakage,
- and stable enough to become a real open-source zkVM platform.
