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

## System Elements

### 1. Authoring and Language

- `Source`
  - Human-authored `.tab` input.
  - Owned by `tabula-lang`.
- `AST`
  - Parsed syntax tree.
  - Internal authoring representation.
- `IR`
  - Executable instruction-level form.
  - Owned by `tabula-ir`.

### 2. Semantic Program Model

- `ProgramDefinition`
  - Source-derived program shape before metadata sealing.
  - Current type: `tabula_compiler::ProgramDefinition`.
- `CompiledProgram`
  - Compiler-owned in-memory semantic program.
  - Current type: `tabula_compiler::CompiledProgram`.

### 3. Portable Contract Objects

- `Artifact`
  - Sealed portable representation of a program.
  - Current type: `tabula_artifact::Artifact`.
- `State`
  - Canonical execution-boundary state object.
  - Current type anchor: `tabula_artifact::StateView`.
- `TransactionBatch`
  - Canonical batch execution request.
  - Current type: `tabula_artifact::TransactionBatch`.
- `Statement`
  - Canonical public claim for one execution.
  - Current type: `tabula_artifact::Statement`.
- `PortableValue`
  - Canonical public and serialized value carrier.
  - Current type: `tabula_core::PortableValue`.

### 4. Execution and Proof Carriers

- `TypedValue`
  - Canonical internal execution, proof, and precompile carrier.
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
  - Versioned contract metadata attached to an artifact.
  - Current type: `tabula_contract::ContractMetadataEnvelope`.
- `CompatibilityPolicy`
  - Fail-closed metadata compatibility rules.
  - Current type: `tabula_contract::ContractCompatibilityPolicy`.
- `Binding`
  - Expected verifier-side identity for a program context.
  - Current type: `tabula_runtime::Binding`.

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
- `ExecutedBatch`
  - Runtime-owned envelope containing execution outputs and consistency status.
  - Current type: `tabula_runtime::ExecutedBatch`.

### 6.1 Host Bootstrap

- `HostEnvironment`
  - Canonical process-local installation model for runtime behavior and backend
    capabilities.
  - Current type: `tabula_runtime::HostEnvironment`.
- `HostTypeRuntimes`
  - Installed type and encoding runtime set.
  - Current type: `tabula_runtime::HostTypeRuntimes`.
- `InstalledSchemes`
  - Installed canonical column backend families.
  - Current type: `tabula_runtime::InstalledSchemes`.
- `InstalledPrecompiles`
  - Installed canonical precompile backend families.
  - Current type: `tabula_runtime::InstalledPrecompiles`.

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
- `Precompile`
  - Custom instruction capability with typed execution and proof contracts.
- `PrecompileValueProfile`
  - Typed value contract for one precompile input or output position.
  - Current code anchor: `tabula_ext::PrecompileValueProfile`.
- `PrecompileSignature`
  - Full typed I/O contract for a precompile.
  - Current code anchor: `tabula_ext::PrecompileSignature`.
- `PrecompileBackendFactory`
  - Host-installed backend family for one exact precompile descriptor.
  - Current type: `tabula_ext::PrecompileBackendFactory`.

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
| `State` | Canonical state boundary object | `StateView` |
| `TransactionBatch` | Canonical execution request batch | `TransactionBatch` |
| `Statement` | Canonical public execution claim | `Statement` |
| `PortableValue` | Canonical public value carrier | `tabula_core::PortableValue` |
| `TypedValue` | Canonical internal value carrier | `tabula_types::TypedValue` |
| `Binding` | Expected verifier-side program identity | `Binding` |
| `HostEnvironment` | Canonical bootstrap installation model | `tabula_runtime::HostEnvironment` |
| `Verifier` | Reusable verification object bound to one program context | `Verifier` |
| `TypeDescriptor` | Registered semantic type definition | `tabula_profile::TypeDescriptor` |
| `EncodingProfile` | Registered proof/transcript representation contract | `tabula_profile::EncodingProfile` |
| `SchemeProfile` | Registered commitment/opening contract | `tabula_profile::SchemeProfile` |
| `ColumnProfile` | Sealed per-column semantic/proof contract | `tabula_profile::ColumnProfile` |
| `PrecompileSignature` | Sealed typed precompile I/O contract | `tabula_ext::PrecompileSignature` |
| `ColumnRootBinding` | Canonical committed-column binding contract | `tabula_commitment::ColumnRootBinding` |
| `Backend` | Raw proving machinery below the SDK surface | `machine`, `stark`, `chips`, `witness` |

## Current Internal Vocabulary

The following names are valid internally, but should not dominate the main
public SDK vocabulary.

| Internal name | Role |
|---|---|
| `AST Program` | Syntax-layer program |
| `IR Program` | Instruction-layer program |
| `CompiledProgram` | Compiler-owned in-memory semantic program |
| `ResolvedProgram` | Runtime-materialized form of a compiled program |
| `TabulaRuntime` | Prepared per-program execution/proving engine |
| `HostTypeRuntimes` | Installed runtime behavior bundle |
| `InstalledSchemes` | Installed canonical scheme families |
| `InstalledPrecompiles` | Installed canonical precompile families |
