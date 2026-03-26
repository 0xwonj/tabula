# SDK + EXT Module Layout

> Status: proposed target
> Audience: maintainers
> Scope: physical module, file, and directory structure for `tabula-sdk` and `tabula-ext`

This note defines the ideal internal layout for `tabula-sdk` and `tabula-ext`
after the public API cutover. It is not a request to change product-facing
paths. The goal is to make the codebase match the mental model we already want
developers to learn.

The public architecture note answers "what are the nouns and flows?".
This note answers "where should the code live?".

## 1. Design Goals

The physical layout should satisfy all of these at once:

- keep the crate roots small and noun-driven
- keep default application DX centered on root re-exports, not deep module paths
- isolate expert-only escape hatches physically, not just conceptually
- make feature-gated surfaces obvious from the directory tree
- split files by responsibility, not by incidental implementation history
- make built-in implementations live next to their contract family, not inside
  the public noun file

## 2. Current Layout Review

The current API surface is much better than before, but the file layout still
shows the history of the migration.

### 2.1 `tabula-sdk`

Current pressure points:

- [`crates/sdk/src/sdk.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/sdk.rs)
  mixes `Sdk`, `SdkBuilder`, cache preparation, environment construction,
  extension installation, and low-level builder helpers.
- [`crates/sdk/src/program.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/program.rs)
  mixes the `Program` handle with three unrelated builders.
- [`crates/sdk/src/runner.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/runner.rs)
  mixes `Runner`, `ExecutionReceipt`, `QueryResult`, and `TxOutcomeSummary`.
- [`crates/sdk/src/schema.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/schema.rs)
  mixes handle definitions with schema indexing and lookup logic.
- [`crates/sdk/src/value.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/value.rs)
  mixes encode traits, decode traits, built-in codecs, and tuple arg packing.
- [`crates/sdk/src/advanced.rs`](/Users/wonj/Projects/tabula/crates/sdk/src/advanced.rs)
  mixes raw re-exports, builder extension hooks, accessors, and helper
  functions.

### 2.2 `tabula-ext`

Current pressure points:

- [`crates/ext/src/extension.rs`](/Users/wonj/Projects/tabula/crates/ext/src/extension.rs)
  mixes contribution nouns, the bundle noun, the builder noun, and validation.
- [`crates/ext/src/root.rs`](/Users/wonj/Projects/tabula/crates/ext/src/root.rs)
  mixes root contracts with the built-in SMT implementation.
- [`crates/ext/src/scheme.rs`](/Users/wonj/Projects/tabula/crates/ext/src/scheme.rs)
  mixes runtime traits, proof contracts, setup inputs, materialized backends,
  and the bundle wrapper.
- [`crates/ext/src/backend/mod.rs`](/Users/wonj/Projects/tabula/crates/ext/src/backend/mod.rs)
  is still compact, but it is the right place to isolate explicit
  backend-authoring submodules rather than letting backend nouns leak back into
  the root.

The code works. The layout is the part that is still flatter than the domain.

## 3. Layout Rules

These rules should drive the structure.

### 3.1 Root Rules

- `lib.rs` should contain almost no logic.
- `lib.rs` should mostly be `mod`, `pub mod`, and `pub use`.
- the default happy path should come from root re-exports
- only explicit expert namespaces should remain public submodules

For `tabula-sdk`, the only public submodules should be:

- `advanced`
- `prelude`

For `tabula-ext`, the only public submodules should be:

- `backend`
- `root`
- `scheme`
- `prelude`

### 3.2 Responsibility Rules

- a file should define one concept family
- builders should not live in the same file as the handle they use unless the
  builder is trivial
- validation should not live in the same file as stable noun definitions
- built-in implementations should not live in the same file as the trait that
  third parties implement

### 3.3 Feature Rules

- feature-heavy code should be grouped physically so `#[cfg]` fences happen at
  module boundaries first
- verify/prove-specific SDK wrappers should live together
- EXT proof/backend contracts should live together

## 4. Target `tabula-sdk` Layout

The ideal internal tree is:

```text
crates/sdk/src/
  lib.rs
  prelude.rs
  error.rs

  app/
    mod.rs
    sdk.rs
    builder.rs
    environment.rs
    cache.rs
    install.rs

  model/
    mod.rs
    artifact.rs
    state.rs
    context.rs
    batch.rs

  program/
    mod.rs
    handle.rs
    builders/
      mod.rs
      state.rs
      context.rs
      batch.rs

  schema/
    mod.rs
    schema.rs
    handles.rs

  prepared/
    mod.rs
    runner.rs
    verifier.rs
    receipt.rs
    query_result.rs
    proof.rs
    tx_outcome.rs

  value/
    mod.rs
    encode.rs
    decode.rs
    args.rs
    builtins.rs

  advanced/
    mod.rs
    reexports.rs
    builder_ext.rs
    access.rs
```

### 4.1 Why This Tree Is Better

- `app/` holds the application embedding/session layer:
  `Sdk`, `SdkBuilder`, `Environment`, cache keys, and extension installation.
- `model/` holds the portable boundary carriers:
  `Artifact`, `State`, `Context`, and `TransactionBatch`.
- `program/` holds the opened semantic handle and symbol-first builders.
- `schema/` holds stable lookup/indexing and lightweight handle types.
- `prepared/` holds runtime-prepared wrappers and their results.
- `value/` holds the DX codec system in one place.
- `advanced/` keeps all raw escape hatches physically separate.

That mirrors the actual lifecycle:

1. install app environment
2. load or compile model objects
3. open a program
4. build inputs by symbol
5. prepare runtime handles
6. execute, prove, or verify

### 4.2 Current-To-Target Mapping

- `sdk.rs`
  - split into `app/sdk.rs`, `app/builder.rs`, `app/cache.rs`, `app/install.rs`
- `environment.rs`
  - move to `app/environment.rs`
- `artifact.rs`, `state.rs`, `context.rs`, `batch.rs`
  - move under `model/`
- `program.rs`
  - split into `program/handle.rs` and `program/builders/{state,context,batch}.rs`
- `runner.rs`
  - split into `prepared/runner.rs`, `prepared/receipt.rs`,
    `prepared/query_result.rs`, `prepared/tx_outcome.rs`
- `verifier.rs`
  - move to `prepared/verifier.rs`
- `proof.rs`
  - move to `prepared/proof.rs`
- `schema.rs`
  - split into `schema/handles.rs` and `schema/schema.rs`
- `value.rs`
  - split into `value/encode.rs`, `value/decode.rs`, `value/args.rs`,
    `value/builtins.rs`
- `advanced.rs`
  - split into `advanced/reexports.rs`, `advanced/builder_ext.rs`,
    `advanced/access.rs`

### 4.3 Important Boundary Rules For SDK

- `Program` should stay in its own file and not grow builder logic again.
- `Runner` should only own prepared interaction methods.
- `ExecutionReceipt`, `QueryResult`, and `TxOutcomeSummary` should stay out of
  `runner.rs`.
- `SdkBuilder::apply_extension` should not stay inline inside the public
  builder file. It belongs in `app/install.rs`.
- cache key computation and cache containers belong in `app/cache.rs`, not in
  the root `Sdk` definition file.

## 5. Target `tabula-ext` Layout

The ideal internal tree is:

```text
crates/ext/src/
  lib.rs
  prelude.rs
  error.rs

  bundle/
    mod.rs
    extension.rs
    builder.rs
    validate.rs
    capability.rs
    type_contribution.rs
    encoding_contribution.rs
    scheme_contribution.rs

  scheme/
    mod.rs
    runtime.rs
    contracts.rs
    factory.rs

  root/
    mod.rs
    backend.rs
    witness.rs
    builtins/
      mod.rs
      smt.rs

  backend/
    mod.rs
    execution.rs
    scheme.rs
    prelude.rs
```

### 5.1 Why This Tree Is Better

- `bundle/` isolates the extension install bundle story:
  noun definitions, builder flow, and validation.
- `scheme/` isolates column-scheme authoring contracts.
- `root/` isolates root-backend authoring contracts and built-ins.
- `backend/` remains the explicit expert namespace for low-level chip/AIR work.

This keeps authoring-time concepts separate from proof-backend concepts.

### 5.2 Current-To-Target Mapping

- `extension.rs`
  - split into `bundle/extension.rs`, `bundle/builder.rs`,
    `bundle/validate.rs`, `bundle/capability.rs`,
    `bundle/type_contribution.rs`, `bundle/encoding_contribution.rs`,
    `bundle/scheme_contribution.rs`
- `root.rs`
  - split into `root/backend.rs`, `root/witness.rs`, `root/builtins/smt.rs`
- `scheme.rs`
  - split into `scheme/runtime.rs`, `scheme/contracts.rs`, `scheme/factory.rs`
- `backend/mod.rs`
  - keep as namespace root but move the inline prelude into `backend/prelude.rs`

### 5.3 Important Boundary Rules For EXT

- `Capability`, `TypeContribution`, `EncodingContribution`, and
  `SchemeContribution` should remain plain noun files. They should not carry
  bundle validation logic.
- `ExtensionBuilder` should orchestrate, not validate inline. Validation should
  move to `bundle/validate.rs`.
- built-in SMT root code should stay under `root/builtins/`, not beside the
  public traits that extension authors implement.
- top-level `scheme/` is about extension contracts. Top-level `backend/` is
  about explicit low-level implementation machinery. Those should not blur.

## 6. Public Path Policy

This layout should not make common paths deeper.

The public happy path should stay:

```rust
use tabula_sdk::{
    Artifact, Context, Environment, ExecutionReceipt, Program, Proof, QueryResult,
    Runner, Sdk, SdkBuilder, State, Statement, TransactionBatch, Verifier,
};

use tabula_ext::{
    Capability, EncodingContribution, Extension, ExtensionBuilder, RootBackend,
    SchemeContribution, TypeContribution,
};
```

Internal directories should get deeper. Public imports should not.

The only wildcard-friendly helper should be `prelude.rs`.

## 7. Test Layout

The test trees should mirror the product surface instead of old migration
history.

Recommended `tabula-sdk` integration tests:

```text
crates/sdk/tests/
  architecture.rs
  compile.rs
  open.rs
  execute.rs
  verify.rs
  prove.rs
  extensions.rs
  advanced.rs
```

Recommended `tabula-ext` tests:

```text
crates/ext/tests/
  bundle_validation.rs
  scheme_contracts.rs
  root_contracts.rs
  backend_surface.rs
```

The current cfg-specific tests are useful, but the filenames should converge on
surface areas rather than migration phases once the layout stabilizes.

## 8. Refactor Order

The safest structural refactor order is:

1. split `tabula-sdk` without changing root exports
2. split `tabula-ext` without changing root exports
3. move inline `prelude` modules to `prelude.rs`
4. update architecture tests to assert the new internal paths
5. only after the tree is stable, consider deeper behavioral refinements

That order keeps the change mostly mechanical and avoids mixing file moves with
API changes.

## 9. Non-Goals

This note does not propose:

- changing crate names
- adding more product-facing crates
- deepening public module paths
- changing runtime semantics
- changing artifact wire format
- changing the current feature matrix

It is only about making the code layout match the architecture we already want.
