# Profile-Native Runtime Architecture

> **Status**: Complete
> **Date**: 2026-03-23
> **Scope**: Final end-to-end architecture after the legacy carrier, compat,
> and capability-contract migrations.
> **Related**: [verification vocabulary](../design/architecture.md#verification-vocabulary),
> [column-profile-architecture-workstreams.md](column-profile-architecture-workstreams.md),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [executor-proof-codesign-architecture.md](executor-proof-codesign-architecture.md),
> [execution-proof-redesign-workplan.md](execution-proof-redesign-workplan.md),
> [../research/symbolic-air-compilation.md](../research/symbolic-air-compilation.md)

---

## 1. Final State

Tabula now uses one consistent architecture across authoring, execution,
proving, and verification:

- `tabula-profile` is the semantic source of truth.
- `tabula-types` owns runtime type and encoding behavior.
- `tabula-core` owns portable protocol data only.
- `tabula-ext` owns extension contracts only.
- `tabula-runtime` consumes sealed capability contracts and orchestrates
  execution, proving preparation, and verifier setup.

The legacy built-in carrier model is gone. There is no deprecated public alias
layer, no legacy value adapter layer, and no meta-based commitment public
surface.

---

## 2. Design Rules

These rules now describe the implemented system, not an aspirational target.

### 2.1 Semantic truth is profile-centric

Column meaning comes from sealed profile data:

- `TypeDescriptor`
- `EncodingProfile`
- `SchemeProfile`
- `ColumnProfile`
- `ProfileCatalog`

No production path reconstructs semantic meaning from closed enums or ad hoc
built-in tags.

### 2.2 Two carriers only

- `PortableValue` is the only public and serialized carrier.
- `TypedValue` is the only internal runtime, proof, and capability carrier.

There is no third long-lived production carrier.

### 2.3 Built-ins are bootstrap seeds, not architecture exceptions

Built-ins are registered during bootstrap. After bootstrap:

- built-in and custom types use the same runtime registries,
- built-in and custom schemes use the same backend materialization path,
- built-in and custom capabilities use the same typed contract path.

### 2.4 Hard-break cleanup is complete

The old carrier model was deleted rather than deprecated. Production code does
not keep compatibility aliases, adapter modules, or fallback public APIs.

---

## 3. Crate Responsibilities

### 3.1 `tabula-profile`

Semantic data only.

It owns:

- type, encoding, and scheme descriptors,
- column profiles,
- profile catalogs,
- semantic validation and hashing.

It does not own runtime behavior, proof backends, or execution behavior.

### 3.2 `tabula-types`

Runtime behavior only.

It owns:

- `TypedValue`,
- `TypeRuntime`,
- `EncodingRuntime`,
- `TypeRuntimeRegistry`,
- `EncodingRuntimeRegistry`,
- typed helpers used by execution and witness preparation.

It does not own artifacts, program schemas, or backend setup.

### 3.3 `tabula-core`

Portable protocol data only.

It owns:

- `PortableValue`,
- transactions,
- state and event models,
- boundary traits that remain carrier-only.

It does not own arithmetic, comparison, null encoding, or proof behavior.

### 3.4 `tabula-ext`

Extension contracts only.

It owns:

- canonical scheme backend contracts,
- execution-tier and root-tier backend hooks for extension authors.

### 3.5 `tabula-runtime`

Capability consumer and orchestration layer.

It owns:

- host bootstrap,
- runtime setup,
- execution orchestration,
- proof preparation,
- verifier setup,
- machine handoff.

### 3.6 `tabula-commitment`

Canonical root-binding and native digest primitives only.

Its public surface is limited to:

- `ColumnRootBinding`
- `NormalizedVerifierDigest`
- `compute_column_root_binding_prefix_digest`
- `compute_column_root_binding_leaf`
- `compute_state_roots_from_bindings`
- native hash and digest primitives

It does not expose `ColumnState`, `ColumnMeta`, or `compat`.

---

## 4. Host Bootstrap Model

`HostEnvironment` is the canonical installation seam on the `verify` / `prove`
runtime surface.

It owns:

- installed type runtimes,
- installed encoding runtimes,
- installed schemes.

Capability descriptors and capability transcript signatures are sealed into
compiler/runtime inputs today; a separate installed capability registry is
follow-up work, not part of the current `HostEnvironment` type.

`RuntimeBuilder`, `VerifierBuilder`, and `SdkBuilder` are facades over this
host-owned model. They do not keep parallel registry ownership.

---

## 5. Capability Transcript Contract

Capability transcript I/O is explicit, typed, and sealed.

The canonical contract is:

- `CapabilityTranscriptValueProfile { type_id, encoding_profile_id }`
- `CapabilityTranscriptSignature { inputs, outputs }`
- artifact descriptors that carry the full typed signature

The typed transcript structures live in `tabula-core`; extension backends
consume them, but `tabula-ext` does not redefine that vocabulary.

Compiler catalogs, source lowering, IR typecheck, runtime dispatch, proof
preparation, and transcript generation validate the same sealed contract.

Capability transcript encoding is signature-driven and registry-driven.
`type_id`, `encoding_profile_id`, and atom counts are encoded bytewise as LE32
prefixes before transcript atoms.

Capability transcript I/O that does not fit the current generic execution-slot width is
rejected early at compile/register time and rechecked at runtime setup.

---

## 6. Commitment Contract

The canonical verifier-visible commitment contract is expressed in root
bindings, not meta tags.

Every committed column is described publicly by:

- root-binding family,
- column profile hash,
- normalized verifier digest,
- canonical binding digest.

State-root aggregation uses bindings only. There is no public legacy
meta-derived state-root path.

---

## 7. Phase History

The migration completed in four phases:

1. Host-centric contract freeze
2. Proof and witness typed-carrier migration
3. Sealed typed capability transcript contract completion
4. Final legacy deletion and canonical surface cleanup

The older slice numbering remains useful only as historical context.

---

## 8. Definition of Done

The architecture is complete because all of the following are now true:

- `PortableValue` is the only public and serialized carrier
- `TypedValue` is the only internal execution, proof, and capability carrier
- profile data is the only semantic source of truth
- built-in and custom types share the same runtime registries
- built-in and custom schemes share the same backend materialization path
- built-in and custom capabilities share the same typed signature path
- witness and proof preparation do not depend on the deleted legacy carrier
- no public alias layer remains
- no public commitment meta surface remains
- architecture guards fail on reintroduction of the removed seams

---

## 9. Deliberately Deferred Work

Three items remain intentionally out of scope for this completed migration:

- execution AIR width generalization
- symbolic AIR compilation
- proof front-end journalization and deterministic parallel reduction

The current generic execution AIR remains fixed-width with width `3`.
Wide custom values are supported in profile, runtime, storage, proof, and typed
capability transcript contracts, but not as arbitrary generic execution-slot values.

Future work on symbolic AIR compilation should be treated as a new architecture
track, not as unfinished migration debt from this bundle.

Likewise, future work on a canonical execution journal and runtime-owned proof
front-end reduction should be treated as a post-migration architecture track,
not as leftover legacy-carrier debt.
