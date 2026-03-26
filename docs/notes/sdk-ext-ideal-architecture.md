# SDK + EXT Ideal Architecture

> Status: proposed target
> Audience: maintainers
> Scope: ideal product-facing structure for `tabula-sdk` and `tabula-ext`

This note defines the target public architecture for Tabula after the
capability vocabulary cleanup. It is a target design, not a description of the
current implementation.

The goal is not only to clean up names. The goal is to give Tabula one
application-facing SDK, one extension authoring surface, and one obvious
mental model for how programs are compiled, opened, executed, proved, and
extended.

## 1. Design Thesis

The ideal structure is:

- `tabula-sdk` for application embedding
- `tabula-ext` for extension authoring
- `Sdk` owning one immutable `Environment` plus prepared caches
- `Artifact` as the sealed portable program form
- `Program` as an opened artifact handle inside one SDK environment
- `Runner` as the prepared execute/prove engine for one `(artifact, environment)`
- `Verifier` as the prepared verification engine for one `(artifact, environment)`
- `ExecutionReceipt` as the canonical runtime-owned execution result
- `Extension` as one atomic install bundle of semantic + runtime + optional
  proof contributions

That shape is ideal for Tabula because Tabula is not just a compiler and not
just a runtime. It is a system where:

- compilation seals semantics
- runtime owns execution and proof orchestration
- extensions can affect both authoring-time semantics and runtime/proof setup
- proving is heavy and must stay opt-in

The public architecture should reflect those facts directly.

## 2. Hard Requirements

The target design should satisfy all of these at once:

1. Application developers should be able to stay almost entirely inside
   `tabula-sdk`.
2. Extension authors should install one bundle, not manually update compiler
   catalogs, runtime registries, and backend hooks one by one.
3. The default SDK path should be symbol-first and schema-aware.
4. Runtime should remain the canonical owner of execute/prove/verify policy.
5. The happy path should not expose raw compiler/runtime internals.
6. Default dependency load should be `compile + execute`; proof remains opt-in.
7. Prepared runtimes and verifiers should be cached and reused automatically.
8. Capability should remain the public semantic noun; capability transcript is
   only the proof-facing materialization of proof-visible capability calls.

## 3. What The SDK Should Not Be

The ideal SDK should not be:

- a flat crate root that re-exports compiler, runtime, and IR internals
- a second policy layer that duplicates runtime behavior
- a low-level typed-value transport API disguised as a product surface
- a place where users manually construct `PortableValue` for standard flows
- a place where users manually look up `EntryId`, `FieldId`, or `TableId`

Those escape hatches can exist, but only under an explicit `advanced`
namespace.

## 4. Public Nouns

The product-facing vocabulary should be fixed to the following nouns.

### 4.1 SDK Nouns

- `Sdk`
  - top-level embedding surface
- `Environment`
  - immutable installed extension set and runtime behavior used by one SDK
- `Artifact`
  - sealed portable program form
- `Program`
  - opened artifact handle bound to one SDK environment
- `Runner`
  - prepared execute/prove handle
- `Verifier`
  - prepared verification handle
- `Schema`
  - stable introspection view of entries, tables, fields, and context
- `State`
  - committed state boundary object
- `Context`
  - public context boundary object
- `TransactionBatch`
  - batch request object
- `ExecutionReceipt`
  - canonical runtime-owned execution result
- `QueryResult`
  - canonical query result
- `Statement`
  - public execution claim
- `Proof`
  - proof object plus statement

### 4.2 EXT Nouns

- `Extension`
  - immutable install bundle
- `ExtensionBuilder`
  - builder for one install bundle
- `TypeContribution`
  - descriptor + runtime + optional Rust codec for one type family
- `EncodingContribution`
  - profile + runtime for one encoding family
- `SchemeContribution`
  - scheme profile + runtime/proof materializer for one scheme family
- `Capability`
  - semantic capability contract plus runtime behavior and proof visibility
- `RootBackend`
  - optional root proving family contribution

### 4.3 Internal Nouns That Should Not Dominate The Happy Path

- `RegisteredProgram`
- `CompiledProgram`
- `CompilerCatalogs`
- `HostEnvironment`
- `RuntimeRegistries`
- `InstalledSchemes`
- `TabulaRuntime`
- `TabulaStarkConfig`
- `ColumnBackendFactoryBundle`
- raw `EntryId`, `TableId`, `FieldId`

These may still exist internally or under `advanced`, but they are not the
default SDK story.

## 5. Object Model

The target object graph should be:

```text
Sdk
  owns Arc<Environment>
  owns prepared cache keyed by (artifact digest, environment fingerprint, mode)

Program
  owns Arc<Artifact>
  borrows Arc<SdkShared>
  owns only light schema/name-resolution caches

Runner
  prepared execute/prove object for one (artifact, environment)

Verifier
  prepared verification object for one (artifact, environment)
```

The key design rule is:

- `Artifact` is immutable and portable
- `Program` is a semantic handle
- `Runner` and `Verifier` are prepared engines

The current `Program`-owns-a-mutable-runtime-cache shape is not ideal. Cache
ownership belongs in `Sdk` or `Environment`, not inside each `Program`.

## 6. Ideal Application Flow

The default embedding path should look like this:

```rust
use tabula_sdk::prelude::*;

let sdk = Sdk::builder()
    .with_extension(my_extension()?)?
    .build()?;

let artifact = sdk.compile(source)?;
let program = sdk.open(artifact)?;

let state = program
    .state()
    .table("balances")
    .row(1u64)
    .set("amount", 100u64)?
    .build()?;

let batch = program
    .batch()
    .push(program.tx("transfer")?.call((1u64, 2u64, 50u64))?)
    .build()?;

let runner = program.runner()?;
let receipt = runner.execute(&state, &batch, &Context::empty())?;

#[cfg(feature = "prove")]
let proof = runner.prove(&receipt)?;

#[cfg(feature = "verify")]
program.verifier()?.verify(&proof)?;
```

The default user should not need to understand:

- which crate owns sealing vs runtime policy
- how capability handlers are installed
- how scheme backends are materialized
- how numeric ids are assigned internally

## 7. Public API Shape

The ideal stable API should look roughly like this.

```rust
impl Sdk {
    pub fn builder() -> SdkBuilder;
    pub fn standard() -> Result<Self, InstallError>;

    pub fn compile(&self, source: &str) -> Result<Artifact, CompileError>;
    pub fn load_artifact(&self, bytes: &[u8]) -> Result<Artifact, ArtifactError>;
    pub fn open(&self, artifact: Artifact) -> Result<Program, OpenError>;

    pub fn environment(&self) -> &Environment;
}

impl Program {
    pub fn artifact(&self) -> &Artifact;
    pub fn schema(&self) -> &Schema;

    pub fn tx(&self, symbol: &str) -> Result<TxHandle, LookupError>;
    pub fn query(&self, symbol: &str) -> Result<QueryHandle, LookupError>;
    pub fn table(&self, symbol: &str) -> Result<TableHandle, LookupError>;

    pub fn state(&self) -> StateBuilder<'_>;
    pub fn context(&self) -> ContextBuilder<'_>;
    pub fn batch(&self) -> BatchBuilder<'_>;

    pub fn runner(&self) -> Result<std::sync::Arc<Runner>, PrepareError>;
    pub fn verifier(&self) -> Result<std::sync::Arc<Verifier>, PrepareError>;
}

impl Runner {
    pub fn execute(
        &self,
        state: &State,
        batch: &TransactionBatch,
        context: &Context,
    ) -> Result<ExecutionReceipt, ExecutionError>;

    pub fn query(
        &self,
        state: &State,
        query: &PreparedQuery,
        context: &Context,
    ) -> Result<QueryResult, ExecutionError>;

    #[cfg(feature = "prove")]
    pub fn prove(&self, receipt: &ExecutionReceipt) -> Result<Proof, ProofError>;

    #[cfg(feature = "prove")]
    pub fn execute_and_prove(
        &self,
        state: &State,
        batch: &TransactionBatch,
        context: &Context,
    ) -> Result<(ExecutionReceipt, Proof), ProofError>;
}

impl Verifier {
    pub fn verify(&self, proof: &Proof) -> Result<(), VerificationError>;
}
```

Two important DX rules:

- `execute_and_prove()` should return both receipt and proof
- `runner()` / `verifier()` should return shared prepared handles, not rebuild
  on every call

## 8. Builders, Handles, And Value Codecs

### 8.1 Schema-Aware Builders

The default path should be symbol-first and schema-aware.

Builders should exist for:

- `State`
- `Context`
- `TransactionBatch`
- transaction calls
- query calls

They should resolve names once, cache handles internally, and fail early on:

- unknown symbol
- wrong arity
- wrong Rust value type for the schema slot
- query/tx kind mismatch

### 8.2 Handles

The default path should expose semantic handles, not raw ids:

- `TxHandle`
- `QueryHandle`
- `TableHandle`
- `FieldHandle`

These handles may internally carry ids, but the public API remains symbol-first
and IDE-friendly.

### 8.3 Value Conversion

The SDK needs an explicit Rust-value conversion model.

The ideal design should provide:

- built-in codecs for canonical Rust primitives
- optional extension-provided codecs for custom Rust types
- exact `PortableValue` fallback only under `advanced`

That suggests a small public conversion layer, for example:

- `EncodeValue`
- `DecodeValue`
- or one bundled `ValueCodec<T>` concept

This matters for DX because Tabula supports custom types. Without a codec layer,
extension-defined types always leak back into manual portable-value assembly.

## 9. ExecutionReceipt

`ExecutionReceipt` should be runtime-owned and canonical.

It should contain:

- `state_before`
- `state_after`
- `batch`
- `context`
- per-tx outcomes
- canonical execution journal summary
- optional statement view when available on the chosen feature set

The SDK should never materialize post-state itself. Runtime must own that logic
once and expose the result directly. Otherwise every adapter risks semantic
drift.

## 10. Environment And Installation

`Environment` should be the only installation boundary on the application side.

It should own:

- semantic contributions needed by compilation
- runtime type behavior
- runtime encoding behavior
- scheme families
- capability handlers
- optional root/backend selections

`SdkBuilder` should be a facade over `EnvironmentBuilder`, not a second parallel
registration model.

The default install path should be:

```rust
let sdk = Sdk::builder()
    .with_extension(foo()?)?
    .with_extension(bar()?)?
    .build()?;
```

Not:

- `with_capability_descriptor(...)`
- `with_type_runtime(...)`
- `with_encoding_runtime(...)`
- `with_column_backend(...)`

on the main happy path.

Those lower-level hooks can remain in `advanced`.

## 11. Extension Model

### 11.1 One Atomic Bundle

An `Extension` should be one immutable bundle with:

- identity: name, version, optional description
- semantic contributions
- runtime contributions
- optional proof/back-end contributions

Installing an extension should be atomic. It should either fully install or
fully fail.

### 11.2 ExtensionBuilder

The high-level authoring pattern should be:

```rust
use tabula_ext::prelude::*;

pub fn my_extension() -> Result<Extension, ExtError> {
    Extension::builder("my-extension", Version::new(1, 0, 0))
        .with_type(my_type_descriptor()?, MyTypeRuntime, MyRustCodec)?
        .with_encoding(my_encoding_profile()?, MyEncodingRuntime)?
        .with_capability(
            Capability::builder("my::capability")
                .inputs([type_ref::<u64>()])
                .outputs([type_ref::<u64>()])
                .checked()
                .journaled()
                .build()?,
            MyCapabilityHandler,
        )?
        .with_scheme(my_scheme_profile()?, MySchemeFactory)?
        .build()
}
```

Builder-based authoring is the right default because it improves:

- local validation
- partial composition
- bundle readability
- packaging for downstream use

Trait-based expert seams should still exist underneath.

## 12. Capability Model

Capability is the public semantic concept.

The ideal model for a generic capability should bundle:

- semantic identity and source import path
- ordered input and output types
- totality
- query policy
- proof visibility
- runtime handler
- optional Rust codecs for ergonomic call building

Proof-facing transcript behavior should not introduce a second public noun. For
proof-visible generic capabilities, the capability transcript is just the proof
materialization of that capability's sealed signature.

Two Tabula-specific rules matter here:

1. `OpaqueRuntimeOnly` capabilities remain true capabilities, but produce no
   proof transcript lane.
2. Blessed built-ins such as dedicated hash families may still lower to special
   IR/runtime paths rather than to generic capability calls.

That means `Capability` is the public noun, while `Capability Transcript`
remains an internal proof-facing term.

## 13. Scheme Model

The public scheme authoring unit should be a scheme family, not an ad hoc
collection of per-column hooks.

One `SchemeContribution` should package:

- family identity
- execution-facing runtime column behavior
- verifier-visible contract materialization
- prover-side preparation behavior
- low-level backend authoring hooks when needed

The internal materialization step can remain per-column, but that should not
drive the default authoring vocabulary.

## 14. Root Backend Model

Root backend choice is real, but it is not ordinary application customization.

The design should separate:

- normal extension installation
- named proving profiles
- expert backend selection

High-level application users should usually choose:

- the default root backend
- or a named proving profile

Raw root backend bundles belong under `advanced` or `tabula_ext::backend`.

## 15. Feature Matrix

### 15.1 `tabula-sdk`

Recommended features:

- `default = ["compile", "execute"]`
- `compile`
- `execute`
- `verify`
- `prove`
- `advanced`

Rules:

- `verify` implies `execute`
- `prove` implies `verify`
- low-level machine knobs live behind `advanced`

### 15.2 `tabula-ext`

Recommended features:

- `default = ["authoring"]`
- `authoring`
- `runtime`
- `verify`
- `prove`
- `backend`

Rules:

- runtime-only extension authors should not compile full proof machinery
- expert chip/AIR helpers should live behind `backend`
- the default surface should stay clean and authoring-oriented

## 16. Coding Patterns

The target architecture should enforce the following patterns:

- immutable finished objects, mutable builders
- cheap `Arc` clones for prepared heavy objects
- phase-specific errors:
  - `CompileError`
  - `InstallError`
  - `OpenError`
  - `PrepareError`
  - `ExecutionError`
  - `ProofError`
  - `VerificationError`
- explicit expert escapes:
  - `tabula_sdk::advanced`
  - `tabula_ext::backend`
- no manual `PortableValue` construction on the happy path

## 17. Performance Model

The public architecture should actively encourage efficient usage.

### 17.1 Prepared Cache Ownership

Prepared caches should live in `Sdk` or `Environment`, keyed by:

- artifact digest
- environment fingerprint
- mode: `execute`, `verify`, `prove`

That gives safe reuse across many `Program` handles and avoids hidden mutable
state inside `Program`.

### 17.2 Environment Fingerprint

`Environment` should have a stable fingerprint derived from:

- installed extension identities
- descriptor hashes
- runtime implementation identities
- scheme family ids
- selected proving/root backend family

### 17.3 Resolve Names Once

Builders and handles should resolve names once. Hot-path execution should never
repeatedly walk the schema by symbol.

### 17.4 Keep Trait Objects Off Inner Loops

Trait objects are acceptable at:

- extension installation
- capability dispatch
- scheme family selection

But once a runner is prepared, repeated per-value or per-cell work should use
prepared concrete data where possible.

## 18. Current-To-Target Mapping

The main target changes from the current code are:

- landed: `Sdk::compile()` returns `Artifact`
- landed: `Program` is a light semantic handle and `Runner` / `Verifier`
  own prepared execution and verification behavior
- landed: raw compiler/runtime nouns live behind `advanced`
- landed: the default path uses `Artifact`, `State`, `Context`, and
  `TransactionBatch`
- landed: extension installation centers on `with_extension(...)`
- landed: runtime owns `ExecutionReceipt`, including post-state materialization
- remaining refinement: share prepared `Runner` / `Verifier` handles more
  explicitly at the public API level if cross-call object identity matters

## 19. Migration Order

Recommended migration order:

1. Freeze the public nouns at the SDK crate root:
   `Artifact`, `Program`, `Runner`, `Verifier`, `ExecutionReceipt`,
   `TransactionBatch`, `Context`, `State`.
2. Introduce `advanced` and move raw compiler/runtime re-exports there.
3. Make `Sdk::compile()` return `Artifact` and `Sdk::open()` return `Program`.
4. Move prepared runtime/verifier caches out of `Program` and into `Sdk`.
5. Make runtime return canonical `ExecutionReceipt` with post-state included.
6. Introduce `Extension` and `ExtensionBuilder`.
7. Add `SdkBuilder::with_extension(...)`.
8. Relegate low-level registration hooks to `advanced`.

## 20. Definition Of Done

The public architecture is in a good state when:

- ordinary embedding uses `Sdk`, `Artifact`, `Program`, `Runner`, and
  `Verifier`
- ordinary extension installation uses `with_extension(...)`
- happy-path builders are symbol-first and schema-aware
- custom Rust types can participate ergonomically through extension-provided
  codecs
- post-state materialization exists in exactly one runtime path
- proof-heavy dependencies stay out of the default library path
- expert hooks remain available, but under explicit advanced namespaces

## 21. Bottom Line

The ideal structure is not "more layers." It is a clearer separation of the
right layers:

- immutable artifact vs opened program vs prepared engine
- application embedding vs extension authoring vs backend authoring
- public capability noun vs proof-facing capability transcript materialization
- atomic installation vs scattered registry mutation

If we follow this structure, Tabula becomes easier to embed, easier to extend,
harder to misuse, and cheaper to evolve internally without breaking users.
