# Canonical Vocabulary

This note defines the canonical naming model for the current Tabula codebase.

It is intentionally grounded in the code as it exists today, not in older design
documents. The goal is to stabilize the vocabulary used for SDK design, crate
boundaries, and future API reviews.

## Purpose

Tabula currently uses multiple overlapping names for similar concepts:

- `Program` exists at the AST layer, IR layer, compiler layer, and artifact layer.
- `Runtime` refers both to a crate boundary and to a prepared per-program engine.
- `Proof` can mean a planning decision, a public statement, or backend proof bytes.

This note introduces a single canonical vocabulary and maps current code elements
onto it.

## Naming Principles

1. Use names by semantic role, not by implementation accident.
2. Prefer user-facing semantic names over backend-centric names.
3. Keep public vocabulary smaller than internal vocabulary.
4. Reserve backend terms such as `Prover`, `Machine`, `Chip`, and `Trace` for
   lower layers.
5. Treat `Program` as the top-level semantic unit.
6. Treat `Artifact` as the sealed portable form of a program.
7. Do not use generic terms such as `Handle` unless they add real precision.

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

These are authoring and compilation inputs. They are not the final semantic
contract exposed to SDK users.

### 2. Semantic Program Model

- `ProgramDefinition`
  - Source-derived program shape before metadata sealing.
  - Includes table schemas, transaction definitions, and source-level column
    scheme selections.
  - Current type: `tabula_compiler::ProgramDefinition`.
- `CompiledProgram`
  - Compiler-owned in-memory semantic program.
  - Includes the registered IR program, schemas, tx definitions, required
    capabilities, proof plan, and contract metadata.
  - Current type: `tabula_compiler::CompiledProgram`.

This is the strongest current code-level representation of a complete Tabula
program. It is the main reason `Program` should remain the top-level semantic
term in the future SDK.

### 3. Portable Contract Objects

- `Artifact`
  - Sealed portable representation of a program.
  - Includes schemas, tx definitions, capability manifest, proof plan, and
    contract metadata.
  - Current type: `tabula_artifact::Artifact`.
- `State`
  - Canonical state value used at an execution boundary.
  - In the current code, the portable representation is a state snapshot.
  - Current type anchor: `tabula_artifact::StateView`.
- `TransactionBatch`
  - Canonical batch of transaction inputs.
  - Current type: `tabula_artifact::TransactionBatch`.
- `Statement`
  - Canonical public claim for one execution.
  - Current type: `tabula_artifact::Statement`.

These are the transport, storage, and verification boundary objects.

### 4. Contract and Compatibility

- `Metadata`
  - Canonical versioned contract metadata attached to a program artifact.
  - Current type: `tabula_contract::ContractMetadataEnvelope`.
- `CompatibilityPolicy`
  - Fail-closed metadata compatibility rules.
  - Current type: `tabula_contract::ContractCompatibilityPolicy`.
- `Binding`
  - Expected verifier-side identity for a program context.
  - Current type: `tabula_runtime::Binding`.

In the current code, `Binding` is effectively the verifier-expected identity
derived from `program_hash + metadata_hash`.

### 5. Execution

- `Executor`
  - Deterministic zero-crypto execution engine.
  - Owned by `tabula-executor`.
- `ExecutedBatch`
  - Runtime-owned envelope containing state before/after, batch result, and
    consistency status.
  - Current type: `tabula_runtime::ExecutedBatch`.
- `Capability`
  - A requirement that must be present for execution/proving to succeed.
  - Current examples:
    - required precompiles
    - required property-query support

Execution is a semantic phase, not yet proof generation.

### 6. Proving Orchestration

- `Runtime`
  - Internal orchestration layer that prepares execution resources, property
    resolvers, proof inputs, and machine interaction for one program.
  - Current main type: `tabula_runtime::TabulaRuntime`.
- `Verifier`
  - Prepared verification object bound to one program artifact / binding.
  - Current main type: `tabula_runtime::Verifier`.

Important: `Runtime` is currently a real code concept, but it should be treated
as an internal orchestration term, not as the primary public noun for the SDK.

### 7. Extension Model

- `Extension`
  - Safe, semantic customization unit exposed above the raw backend.
- `TypeDescriptor`
  - Canonical semantic definition of one registered type family.
  - Future architecture term.
- `EncodingProfile`
  - Canonical proof- and transcript-facing representation contract for a type.
  - Future architecture term.
- `Scheme`
  - Column commitment / property-query support mechanism.
- `SchemeProfile`
  - Canonical verifier-visible commitment/opening contract for one scheme family.
  - Future architecture term.
- `ColumnProfile`
  - Per-column sealed composition of type, encoding, scheme, proof layout, and
    root-binding choices.
  - Intended future per-column source of truth.
- `SchemeDescriptor`
  - Verifier-visible contract for a scheme.
  - Current transitional type: `tabula_artifact::SchemeDescriptor`.
- `ProofPlan`
  - Compiler-owned per-column proof planning decision.
  - Current transitional type: `tabula_artifact::ColumnProofPlan`.
- `Precompile`
  - Custom instruction capability with execution- and verification-side effects.
- `PrecompileRegistration`
  - Internal runtime registration unit bundling executor handler and verifier
    extension.
  - Current type: `tabula_runtime::PrecompileRegistration`.

Extension vocabulary is public-facing up to the semantic bundle level. Raw proof
factories, chip extensions, and backend knobs should remain internal for now.
The architecture direction is for `ColumnProfile` to become the main per-column
sealed contract, with `TypeDescriptor`, `EncodingProfile`, and `SchemeProfile`
as reusable supporting definitions.

### 8. Proof Backend

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
  - Prepared proving inputs derived from execution.

These are backend words. They are valid internally, but should not dominate the
top-level SDK vocabulary.

### 9. Adapters and Tools

- `CLI`
  - File-driven operator tool.
- `Daemon`
  - Local service adapter.
- `Web`
  - UI/client adapter.
- `Testing`
  - Fixtures and harnesses.

These are delivery surfaces, not core semantic concepts.

## Canonical Public Vocabulary

The following names should anchor future SDK design.

| Canonical name | Meaning | Current code anchor |
|---|---|---|
| `Source` | Human-authored `.tab` input | `tabula-lang` |
| `Definition` | Source-derived program before sealing | `ProgramDefinition` |
| `Program` | Top-level semantic unit | closest current anchor: `CompiledProgram` |
| `Artifact` | Sealed portable program form | `Artifact` |
| `State` | Canonical state boundary object | `StateView` |
| `TransactionBatch` | Canonical execution request batch | `TransactionBatch` |
| `Statement` | Canonical public execution claim | `Statement` |
| `Binding` | Expected verifier-side program identity | `Binding` |
| `Verifier` | Reusable verification object bound to one program context | `Verifier` |
| `Extension` | Safe Tabula customization unit | future SDK surface |
| `TypeDescriptor` | Registered semantic type definition | future architecture surface |
| `EncodingProfile` | Registered proof/transcript representation contract | future architecture surface |
| `SchemeProfile` | Registered commitment/opening contract | future architecture surface |
| `ColumnProfile` | Sealed per-column semantic/proof contract | future architecture surface |
| `Backend` | Raw proving machinery below the SDK surface | `machine`, `stark`, `chips`, `witness` |

## Current Internal Vocabulary

The following names are valid internally, but should not become the main public
SDK vocabulary.

| Internal name | Role |
|---|---|
| `AST Program` | Syntax-layer program |
| `IR Program` | Instruction-layer program |
| `CompiledProgram` | Compiler-owned in-memory semantic program |
| `ResolvedProgram` | Runtime-materialized form of a compiled program |
| `TabulaRuntime` | Prepared per-program execution/proving engine |
| `VerifierBuilder` | Internal-oriented verifier construction API |
| `Machine` / `Prover` / `Chip` / `Trace` | Backend proving vocabulary |

## Canonical Decisions

### `Program` is the top-level semantic unit

`Program` is the right top-level public noun for Tabula.

Reason:

- The complete unit is more than an IR body or executable image.
- The compiler already produces a semantic object containing schemas, tx
  definitions, capability requirements, proof planning, and metadata.
- The portable form of the same unit already exists as `Artifact`.

Therefore:

- use `Program` for the user-facing semantic unit
- use `Artifact` for the sealed form of that unit

### `ColumnProfile` is the per-column source of truth

The future architecture should treat `ColumnProfile` as the canonical
per-column contract.

Reason:

- column meaning is currently scattered across schema type, scheme descriptor,
  width assumptions, runtime planning, and proof-layout choices
- verifier-visible column behavior should be sealed once, not reconstructed from
  multiple partial objects
- built-in and custom types/schemes should pass through the same per-column
  contract model

Therefore:

- use reusable definitions for type, encoding, and scheme semantics
- use `ColumnProfile` as the final sealed composition for one column

### `Runtime` is not the top-level public noun

`Runtime` is still a useful implementation term, but it should describe an
internal orchestration layer, not the main public identity of the system.

Reason:

- `runtime` currently means both a crate and a prepared per-program engine
- it is too implementation-shaped for the intended SDK
- it pushes the vocabulary toward execution machinery instead of semantic
  program identity

### `Handle` is not canonical

`Handle` is too generic and should not be part of the canonical public
vocabulary.

Reason:

- it does not express semantic meaning
- it hides whether the object is program-bound, verifier-bound, or backend-bound

### `Prover` is backend vocabulary, not SDK-top vocabulary

Tabula should not name the prepared per-program SDK object `Prover`.

Reason:

- the prepared object is not only a proof producer
- it also owns execution resources, property-resolution context, and statement
  binding
- `Prover` already has a precise lower-layer meaning in `tabula-machine`

## Naming Guidance For Future SDK Work

### Public SDK

Preferred top-level nouns:

- `Program`
- `Artifact`
- `State`
- `Statement`
- `Binding`
- `Verifier`
- `Extension`

Preferred verbs:

- `compile`
- `open`
- `seal`
- `execute`
- `prove`
- `verify`
- `extend`

### Internal Engine Vocabulary

Valid internal nouns:

- `Runtime`
- `RuntimeBuilder`
- `ResolvedProgram`
- `Machine`
- `Trace`
- `Witness`
- `Chip`

These are implementation-facing and should remain below the primary SDK layer.

## Near-Term API Direction

If the SDK is introduced soon, its vocabulary should center on:

- `Program` as the primary user-facing object
- `Artifact` as the portable/serializable form
- `Verifier` as the reusable verification object
- `ext::*` as the highest public customization layer

If a separate prepared per-program object is needed later, it should be treated
as an implementation detail first. If it eventually becomes public, it should
be named by its semantic role, not by a generic name such as `Handle`.

## Summary

The canonical model is:

- author `Source`
- compile to a `Definition`
- register into a `Program`
- seal into an `Artifact`
- execute a `TransactionBatch` against a `State`
- produce a `Statement`
- verify against a `Binding` using a `Verifier`
- customize behavior through `Extension`
- keep `Backend` terms below the public SDK layer
