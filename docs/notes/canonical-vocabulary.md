# Canonical Vocabulary

This note defines the final naming model for the current Tabula codebase.

The goal is to keep one stable vocabulary across SDK design, crate boundaries,
runtime setup, and future API reviews.

## Purpose

Tabula has multiple layers that naturally reuse broad words such as `Program`,
`Runtime`, and `Proof`. This note fixes the intended meaning of those names and
maps them onto the current code.

## Naming Principles

1. Use names by semantic role, not by implementation accident.
2. Prefer user-facing semantic names over backend-centric names.
3. Keep public vocabulary smaller than internal vocabulary.
4. Reserve backend terms such as `Prover`, `Machine`, `Chip`, and `Trace` for
   lower layers.
5. Treat `Program` as the top-level semantic unit.
6. Treat `Artifact` as the sealed portable form of a program.
7. Treat `HostEnvironment` as the canonical bootstrap seam.

## Naming Grammar

These words are not interchangeable. They describe different layers of
ownership.

- `Descriptor`
  - One reusable semantic definition.
  - Use when the type answers: "what is this thing?"
  - A descriptor defines identity, typed shape, and compatibility-relevant
    contract for one item.
  - Good fits: `TypeDescriptor`, `CapabilityDescriptor`,
    future `RelationDescriptor`.
- `Catalog`
  - Canonical reusable collection of descriptors.
  - Use when the collection is registry-like, reusable across programs, and
    not itself a program-owned sealed inclusion set.
  - Good fit: `ProfileCatalog`.
- `Manifest`
  - One sealed inclusion set owned by a program, artifact, or statement scope.
  - Use when the collection answers: "which exact items are present or
    required in this sealed scope?"
  - A manifest is typically ordered, unique, and binding-relevant.
  - Good fit: `capability_manifest`.
- `Binding`
  - Exact identity or commitment for one semantic scope.
  - Use when the type answers: "what exact artifact / program / relation set
    is this proof or verifier context bound to?"
  - A binding is stronger than a descriptor. It pins one concrete sealed
    identity.
  - Good fits: `Binding`, `ColumnRootBinding`, future `RelationBinding`.
- `Entry`
  - One value-bearing item inside a pool or manifest.
  - Use when the item is primarily data, not a reusable semantic contract.
  - Good fit: future `ConstantEntry`.
- `Pool`
  - Canonical owned collection of value-bearing entries.
  - Use when the collection is a program-owned immutable value store rather
    than a descriptor inventory.
  - Good fit: future `ConstantPool`.

### Naming Rule of Thumb

- If the type defines semantics, use `Descriptor`.
- If the type stores reusable definitions, use `Catalog`.
- If the type seals "which items exist here", use `Manifest`.
- If the type seals "what exact identity is this bound to", use `Binding`.
- If the type mostly carries raw immutable data, prefer `Entry` or `Pool`
  over `Descriptor`.

## System Elements

### 1. Authoring and Language

- `Source`
  - Human-authored `.tab` input.
  - Owned by `tabula-lang`.
- `AST`
  - Parsed syntax tree.
  - Internal authoring representation.
- `HIR`
  - High-level semantic source IR.
  - Use for the first compiler-owned representation that preserves source-level
    program structure and semantic categories after parsing.
- `MIR`
  - Mid-level compiler IR.
  - Use for normalized compiler-facing bodies after name resolution, typing,
    sugar removal, and effect classification, but before lowering to the
    canonical execution/proof contract.
- `IR`
  - Canonical execution and proof instruction-level form.
  - This is the small, stable runtime-facing contract, not a generic label for
    every intermediate form.
  - Owned by `tabula-ir`.
- `EffectSummary`
  - Compiler-owned summary of a callable or body's semantic effects.
  - Intended primarily for MIR and middle-end checking.
  - Should distinguish world effects, proof-observable semantic effects, and
    failure or checked behavior.
- `WorldEffect`
  - Static effect category for interaction with mutable program state or
    externally visible world surfaces.
- `ProofEffect`
  - Static effect category for semantically important operations that remain
    visible to journaling and proof preparation even when they do not mutate
    state.
- `MayFail`
  - Static marker for checked or partial behavior that may fail and therefore
    matters for guarded lowering and callable legality.

### 2. Semantic Program Model

- `CompiledProgram`
  - Compiler-owned in-memory semantic program.
  - Current type: `tabula_compiler::CompiledProgram`.
- `RegisteredProgram`
  - Sealed portable representation of a compiled program.
  - Current type: `tabula_compiler::RegisteredProgram`.

### 3. Portable Contract Objects

- `Artifact`
  - Canonical sealed portable program object on the SDK happy path.
  - Current type: `tabula_sdk::Artifact`, wrapping
    `tabula_compiler::RegisteredProgram`.
- `StateSnapshot`
  - Canonical runtime-native execution and proving state carrier.
  - Current type: `tabula_runtime::StateSnapshot`.
- `State`
  - Canonical SDK-facing committed state carrier.
  - Current type: `tabula_sdk::State`, wrapping `tabula_runtime::StateSnapshot`.
- `EntryBatch`
  - Canonical runtime-native batch execution request.
  - Current type: `tabula_ir::EntryBatch`.
- `TransactionBatch`
  - Canonical SDK-facing batch execution request.
  - Current type: `tabula_sdk::TransactionBatch`, wrapping
    `tabula_ir::EntryBatch`.
- `ContextInput`
  - Canonical runtime-native public context request carrier.
  - Current type: `tabula_ir::ContextInput`.
- `Context`
  - Canonical SDK-facing public context request carrier.
  - Current type: `tabula_sdk::Context`, wrapping `tabula_ir::ContextInput`.
- `ProofStatement`
  - Canonical semantic public claim for one execution.
  - Current type: `tabula_runtime::ProofStatement`.
- `Statement`
  - Canonical SDK-facing semantic public claim for one execution.
  - Current type: `tabula_sdk::Statement`.
- `PortableValue`
  - Canonical public and serialized value carrier.
  - Current type: `tabula_core::PortableValue`.

### 4. Execution and Proof Carriers

- `TypedValue`
  - Canonical internal execution, proof, and capability carrier.
  - Current type: `tabula_types::TypedValue`.
- `TypeRuntime`
  - Runtime behavior for one registered type.
  - Current type: `tabula_types::TypeRuntime`.
- `EncodingRuntime`
  - Runtime encoding behavior for one registered encoding profile.
  - Current type: `tabula_types::EncodingRuntime`.
- `TypeRuntimeRegistry`
  - Installed type-runtime set.
  - Current type: `tabula_types::TypeRuntimeRegistry`.
- `EncodingRuntimeRegistry`
  - Installed encoding-runtime set.
  - Current type: `tabula_types::EncodingRuntimeRegistry`.

### 5. Contract and Compatibility

- `Metadata`
  - Versioned contract metadata attached to a registered program.
  - Current type: `tabula_contract::ContractMetadataEnvelope`.
- `CompatibilityPolicy`
  - Fail-closed metadata compatibility rules.
  - Current type: `tabula_contract::ContractCompatibilityPolicy`.
- `Binding`
  - Expected verifier-side identity for a program context.
  - Current type: `tabula_contract::ProgramBinding`.

### 6. Runtime Orchestration

- `Executor`
  - Deterministic zero-crypto execution engine.
  - Owned by `tabula-executor`.
- `Runtime`
  - Internal orchestration layer that prepares resources for one program.
  - Current main type: `tabula_runtime::TabulaRuntime`.
- `Verifier`
  - Prepared verification object bound to one artifact and binding.
  - Current main type: `tabula_runtime::Verifier`.
- `ExecutionJournal`
  - Canonical internal execution-effect journal used as the execution truth for
    runtime proving.
  - Current canonical runtime execution output.
- `ExecutionStateSummary`
  - Nested derived batch-level state projection carried inside
    `ExecutionJournal`.
  - Current canonical journal summary for `read_set_old` /
    `write_set_final`.
- `ProofJournal`
  - Canonical runtime-owned proof-front-end journal aligned to proof-plan
    order.
  - Current canonical runtime proof input.
- `ProofArtifacts`
  - Backend-prepared machine-facing proof bundle derived from
    `ProofJournal`.
  - Current canonical machine-ready proof-preparation output.

### 6.1 Host Bootstrap

- `HostEnvironment`
  - Canonical process-local installation model for runtime behavior on the
    `verify` / `prove` runtime surface.
  - Current type: `tabula_runtime::HostEnvironment`.
- `RuntimeRegistries`
  - Installed type and encoding runtime set on the `verify` / `prove` runtime
    surface.
  - Current type: `tabula_runtime::RuntimeRegistries`.
- `InstalledSchemes`
  - Installed canonical column backend families.
  - Current type: `tabula_runtime::InstalledSchemes`.

Today `HostEnvironment` owns runtime registries and installed schemes. Sealed
capability descriptors and capability transcript signatures travel with the
compiled/runtime inputs rather than through a separate installed host registry.

Builders and SDK surfaces are facades over this host-owned model.

### 7. Extension Model

- `TypeDescriptor`
  - Canonical semantic definition of one registered type.
  - Current code anchor: `tabula_profile::TypeDescriptor`.
- `EncodingProfile`
  - Canonical proof and transcript representation contract for a type.
  - Current code anchor: `tabula_profile::EncodingProfile`.
- `SchemeProfile`
  - Canonical verifier-visible commitment and opening contract for one scheme
    family.
  - Current code anchor: `tabula_profile::SchemeProfile`.
- `ColumnProfile`
  - Sealed per-column composition of type, encoding, scheme, proof layout, and
    root-binding choices.
  - Current code anchor: `tabula_profile::ColumnProfile`.
- `Capability`
  - Custom instruction capability with typed execution and proof contracts.
  - Current code anchors: `tabula_ir::CapabilityDescriptor`,
    `tabula_executor::CapabilityHandler`.
- `CapabilityTranscriptValueProfile`
  - Typed value contract for one proof-visible capability input or output
    position.
  - Current code anchor: `tabula_core::CapabilityTranscriptValueProfile`.
- `CapabilityTranscriptSignature`
  - Full typed transcript I/O contract for one proof-visible capability.
  - Current code anchor: `tabula_core::CapabilityTranscriptSignature`.

Proof-visible capability transcript backends currently plug into
`tabula_ext::backend::ExecutionBackend`; there is no separate public
backend-factory type for capability transcripts.

### 7.1 Reserved Vocabulary for Relations and Constants

These names are not fully implemented yet, but they are the intended canonical
terms for future relation and constant support.

- `RelationDescriptor`
  - Canonical semantic definition of one immutable relation family.
  - Use for relation identity, arity, typing, and semantic class such as
    functional vs membership-only.
- `RelationManifest`
  - Program-owned sealed inclusion set of referenced relation families.
  - Use when the program artifact must declare which exact relations are part
    of its semantic contract.
- `RelationBinding`
  - Exact binding for one concrete relation universe or committed relation set.
  - Use when verifier-visible identity must distinguish not only relation
    shape, but also exact committed contents or version.
- `ConstantEntry`
  - One immutable program-owned value in a sealed constant store.
  - Prefer `Entry` rather than `Descriptor` because constants are primarily
    data, not reusable semantic definitions.
- `ConstantPool`
  - Program-owned immutable store of sealed constant entries.
  - Prefer `Pool` rather than `Manifest` for the in-program constant store.
- `ConstantManifest`
  - Reserved for the rare case where a verifier-visible or artifact-exported
    sealed inventory of constants is needed.
  - Do not use this name for the normal in-program constant store; prefer
    `ConstantPool`.

### 7.2 Relation vs Constant

- `Relation`
  - Immutable allowed structure over one tuple of values.
  - A relation is about membership or functional evaluation.
  - Use relation vocabulary for range checks, decomposition constraints,
    fixed maps, and other tuple-level semantic contracts.
- `Constant`
  - Immutable data value owned by the program or proof instance.
  - A constant is about loading fixed data, not asserting allowed tuple
    structure.
  - Use constant vocabulary for domain separators, config thresholds, fixed
    blobs, and sealed value vectors.

### 7.3 Reserved Relation Operations

- `AssertRelation`
  - Membership assertion against one `RelationDescriptor`.
- `EvalRelation`
  - Functional relation evaluation against one `RelationDescriptor`.

These are the preferred future semantic IR primitives. Backend techniques such
as lookup arguments are lower-level realizations, not the canonical public
vocabulary.

### 8. Commitment Contract

- `ColumnRootBinding`
  - Canonical verifier-visible binding for one committed column.
  - Current type: `tabula_commitment::ColumnRootBinding`.
- `NormalizedVerifierDigest`
  - Canonical verifier-visible digest wrapper.
  - Current type: `tabula_commitment::NormalizedVerifierDigest`.

There is no canonical public `ColumnMeta` or `ColumnState` vocabulary anymore.

### 9. Proof Backend

- `Machine`
  - Multi-proof STARK orchestrator.
  - Current type: `tabula_machine::TabulaMachine`.
- `Prover`
  - Backend proof producer.
  - Current type: `tabula_machine::Prover`.
- `Proof`
  - Backend proof object.
  - Current type: `tabula_machine::TabulaProof`.
- `Chip`
  - AIR implementation unit.
  - Owned by `tabula-chips`.
- `Trace`
  - Backend trace representation used during proving.
- `Witness`
  - Prepared backend proving inputs derived from execution.
- `Effect`
  - Canonical semantic fact emitted by execution and consumed by proving.
- `Shard`
  - Immutable tx-local unit later reduced into proof-plan order.
- `Plan`
  - Runtime-owned resolved proof-slot contract.

### 10. Adapters and Tools

- `CLI`
  - File-driven operator tool.
- `Daemon`
  - Local service adapter.
- `Web`
  - UI and browser adapter.
- `Testing`
  - Fixtures and harnesses.

These are delivery surfaces, not core semantic concepts.

## Canonical Public Vocabulary

| Canonical name | Meaning | Current code anchor |
|---|---|---|
| `Source` | Human-authored `.tab` input | `tabula-lang` |
| `Definition` | Source-derived program before sealing | `ProgramDefinition` |
| `Program` | Top-level semantic unit | closest current anchor: `CompiledProgram` |
| `Artifact` | Sealed portable program form | `Artifact` |
| `State` | Canonical state boundary object | `tabula_sdk::State` |
| `TransactionBatch` | Canonical execution request batch | `tabula_sdk::TransactionBatch` |
| `Statement` | Canonical public execution claim | `tabula_sdk::Statement` |
| `PortableValue` | Canonical public value carrier | `tabula_core::PortableValue` |
| `TypedValue` | Canonical internal value carrier | `tabula_types::TypedValue` |
| `Binding` | Expected verifier-side program identity | `Binding` |
| `HostEnvironment` | Canonical bootstrap installation model on the `verify` / `prove` runtime surface | `tabula_runtime::HostEnvironment` |
| `Verifier` | Reusable verification object bound to one program context | `tabula_sdk::Verifier` |
| `TypeDescriptor` | Registered semantic type definition | `tabula_profile::TypeDescriptor` |
| `EncodingProfile` | Registered proof/transcript representation contract | `tabula_profile::EncodingProfile` |
| `SchemeProfile` | Registered commitment/opening contract | `tabula_profile::SchemeProfile` |
| `ColumnProfile` | Sealed per-column semantic/proof contract | `tabula_profile::ColumnProfile` |
| `Capability` | Custom instruction capability | `tabula_ir::CapabilityDescriptor` |
| `CapabilityTranscriptSignature` | Sealed typed capability transcript I/O contract | `tabula_core::CapabilityTranscriptSignature` |
| `ColumnRootBinding` | Canonical committed-column binding contract | `tabula_commitment::ColumnRootBinding` |
| `RelationDescriptor` | Reserved future semantic relation definition | planned vocabulary |
| `RelationManifest` | Reserved future sealed relation inclusion set | planned vocabulary |
| `RelationBinding` | Reserved future exact relation-universe identity | planned vocabulary |
| `ConstantEntry` | Reserved future immutable program-owned constant item | planned vocabulary |
| `ConstantPool` | Reserved future immutable program-owned constant store | planned vocabulary |
| `Backend` | Raw proving machinery below the SDK surface | `machine`, `stark`, `chips`, `witness` |

## Current Internal Vocabulary

The following names are valid internally, but should not dominate the main
public SDK vocabulary.

| Internal name | Role |
|---|---|
| `AST Program` | Syntax-layer program |
| `IR Program` | Instruction-layer program |
| `CompiledProgram` | Compiler-owned in-memory semantic program |
| `RuntimeProgram` | Runtime-owned root contract split into execution and proof subcontracts |
| `ResolvedExecutionProgram` | Executor-owned resolved hot-path execution contract |
| `ResolvedProofProgram` | Runtime-owned resolved proof/planning contract |
| `TabulaRuntime` | Prepared per-program execution/proving engine |
| `RuntimeRegistries` | Installed runtime behavior bundle |
| `InstalledSchemes` | Installed canonical scheme families |
| `ExecutionJournal` | Canonical executor-owned execution truth consumed by runtime proving |
| `ExecutionStateSummary` | Nested derived batch-level state view inside `ExecutionJournal` |
| `FailedAccessObservation` | Diagnostic failed-tx access observation excluded from proof reduction |
| `ProofJournal` | Canonical runtime-owned proof input aligned to proof-plan slot order |
| `ProofArtifacts` | Backend-prepared machine-facing proof bundle derived from `ProofJournal` |
| `SuccessfulTxExecution` | Canonical tx-local immutable success-path execution-effect unit |
| `ProofPlan` | Canonical runtime-owned slot-order proof contract |
