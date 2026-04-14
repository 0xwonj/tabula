# Column Profile Architecture Bundles and Workstreams

> **Status**: Design note
> **Date**: 2026-03-22
> **Scope**: Defines the top-level design bundles and subordinate execution workstreams for migrating Tabula to a column-profile-driven architecture.
> **Related**: [verification vocabulary](../design/architecture.md#verification-vocabulary), [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md), [../archive/master-roadmap.md](../archive/master-roadmap.md), [../archive/column-scheme-refactor-roadmap.md](../archive/column-scheme-refactor-roadmap.md), [../archive/extensibility-architecture.md](../archive/extensibility-architecture.md)

---

## 1. Why This Note Exists

The previous A-I refactor breakdown is still useful, but only as an implementation
workstream inventory.

It is not the right top-level planning structure.

At the design level, those workstreams are too fine-grained:

- several of them must be designed together to avoid freezing the wrong boundary,
- several of them describe one verifier-visible contract split across multiple layers,
- several of them are execution phases rather than architecture units.

This note introduces a hybrid planning model:

- **top-level design bundles** define the architecture in large coherent units,
- **execution workstreams** remain available underneath as implementation slices.

The goal is to make future planning decision-complete without forcing a big-bang
rewrite or prematurely hardening the wrong seam.

---

## 2. Planning Model

Tabula should use two planning layers.

### 2.1 Design Bundles

These are the units we design at the architecture level.

They answer questions such as:

- what is the source of truth,
- what belongs in the sealed verifier contract,
- what is reusable versus per-column,
- what must be designed together to avoid architectural drift.

### 2.2 Execution Workstreams

These are the units we implement incrementally.

They answer questions such as:

- what can be landed independently,
- what temporary adapters are needed,
- what code paths can migrate first,
- what can be tested and deleted phase by phase.

The important rule is:

> **We should not mistake execution workstreams for architecture boundaries.**

---

## 3. Architectural North Star

The target architecture is:

> **Per-column sealed profiles are the source of truth for compiler, runtime, prover, and verifier.**

Each committed column should be explained by one canonical contract:

- what type it uses,
- how that type is encoded,
- what commitment/opening semantics it uses,
- what proof layout it requires,
- how it binds into transcript and root contracts.

This contract is centered on four canonical object families:

1. `TypeDescriptor`
2. `EncodingProfile`
3. `SchemeProfile`
4. `ColumnProfile`

`TypeDescriptor`, `EncodingProfile`, and `SchemeProfile` are reusable definitions.
`ColumnProfile` is the per-column sealed composition of those definitions.

The design consequence is:

- shared orchestration should ask "what does this column profile require?",
- not "is this SSMC, SMT, U64, or Bytes32?"

---

## 4. Canonical Object Model

These four objects must be designed together as one model.

## 4.1 `TypeDescriptor`

Defines what a value means at the language and execution level.

Minimum responsibilities:

- `type_id`
- semantic identity hash
- host representation contract
- zero/null semantics
- equality capability
- ordering capability
- arithmetic capability
- generic-IR capability classification

Important constraint:

`TypeDescriptor` describes semantics.
It must not become an arbitrary user-code execution or proving hook.

## 4.2 `EncodingProfile`

Defines how a type is represented at proof and commitment boundaries.

Minimum responsibilities:

- `encoding_profile_id`
- compatible `type_id`
- field representation family
- width
- canonical null encoding
- transcript serialization rules
- ordering-preserving flag if relevant

Important constraint:

Encoding is part of the verifier-visible contract.
It is not merely an internal codec detail.

## 4.3 `SchemeProfile`

Defines one verifier-visible commitment and opening contract.

Minimum responsibilities:

- `scheme_profile_id`
- semantic identity hash
- commitment semantics
- canonical verifier-visible digest normalization
- property opening semantics
- proof layout family
- root binding contract

Important constraint:

A scheme profile must expose one canonical verifier-facing commitment meaning.
There must not be separate "native" and "proof-visible" meanings that silently diverge.

## 4.4 `ColumnProfile`

Defines what one concrete committed column uses.

Minimum responsibilities:

- `column_profile_id`
- referenced `type_id`
- referenced `encoding_profile_id`
- referenced `scheme_profile_id`
- referenced proof layout family
- referenced root binding family
- canonical profile hash

Important constraint:

`ColumnProfile` is the source of truth for one column.
Shared layers must not reconstruct the same facts from unrelated fields.

---

## 5. Non-Negotiable Design Principles

## 5.1 Built-ins Must Not Be Privileged

Built-in types and schemes may be pre-registered, but they must not require
special-case orchestration logic.

Built-ins are defaults, not architecture exceptions.

## 5.2 Open Types and Schemes, Closed Generic Ops

Tabula should be extensible without becoming an arbitrary plugin VM.

Therefore:

- types should be open for registration,
- schemes should be open for registration,
- generic IR operation families should remain closed and capability-based.

This implies:

- custom types may participate in generic IR through declared semantics families,
- arbitrary domain-specific logic should go through capabilities, not type-defined operators.

## 5.3 Setup-Time Specialization

Specialization should happen when a column profile is materialized.

By the time proving starts, the following should already be fixed:

- width,
- chip family,
- transcript rules,
- commitment semantics,
- root binding behavior.

Shared proving and runtime code should not keep rediscovering built-in identity.

## 5.4 One Verifier-Visible Contract Per Profile

Each encoding profile and scheme profile must correspond to one canonical verifier-visible contract.

If multiple layers redundantly describe the same semantic fact, the architecture is still wrong.

## 5.5 No Hybrid Auto-Selection

Column scheme choice is explicit, not heuristic.

The system must optimize the selected profile well.
It does not need to guess which profile to choose.

---

## 6. Top-Level Design Bundles

These bundles are the correct architecture-level planning units.

## 6.1 Bundle 0: Canonical Profile Model

This bundle must be designed as one unit.

### Goal

Fix the roles, ownership, references, and canonical identity rules of:

- `TypeDescriptor`
- `EncodingProfile`
- `SchemeProfile`
- `ColumnProfile`

### Why These Belong Together

- `TypeDescriptor` cannot be finalized until `EncodingProfile` ownership is clear.
- `SchemeProfile` cannot be finalized until `ColumnProfile` sealing rules are clear.
- `ColumnProfile` cannot be finalized until proof layout and root binding ownership are clear.

If these are designed separately, we will likely harden the wrong seam.

### Success Criteria

- the four objects have non-overlapping responsibilities,
- built-in types and schemes are representable through the same model,
- `ColumnProfile` alone can describe one column's verifier-visible contract,
- compiler/runtime/prover/verifier can share this vocabulary without reconstructing truth elsewhere.

### Design Philosophy

- source of truth must exist once per column,
- reusable definitions should stay reusable,
- per-column sealing should happen only at the composition layer,
- no built-in-only constructors should remain part of the long-term architecture.

### Important Public Interface Direction

- `TypeId`
- `EncodingProfileId`
- `SchemeProfileId`
- `ColumnProfileId`
- sealed `ColumnProfileDescriptor`
- capability-based type semantics model

### Execution Workstream Mapping

- A
- parts of B
- parts of C
- parts of D
- parts of E

---

## 6.2 Bundle 1: Semantic Surface Migration

This bundle moves language/schema/IR/compiler/artifact surfaces onto the new profile model.

### Goal

- make schema and artifact speak in terms of `ColumnProfile`,
- move type semantics from closed built-in enums to descriptors and capabilities,
- elevate encoding, transcript, and null semantics into explicit profile contracts.

### Why These Belong Together

- changing type semantics without artifact sealing leaves the old truth source intact,
- changing encoding without transcript migration preserves a split verifier contract,
- changing schema without compiler planning preserves duplicated intent reconstruction.

### Success Criteria

- sealed artifacts carry per-column profile contracts,
- generic IR typechecking uses capability families rather than built-in identity,
- width/null/transcript rules are derived from `EncodingProfile`,
- column meaning is no longer scattered across schema, plan, descriptor, and width assumptions.

### Design Philosophy

- open types, closed generic ops,
- encoding is verifier-visible semantics,
- compiler output should emit sealed contracts, not reconstruct intent from multiple side channels.

### Important Public Interface Direction

- schema columns reference profiles rather than only `value_type`,
- IR and typechecker consume descriptor-backed type metadata,
- transcript and event encoding become profile-aware.

### Execution Workstream Mapping

- B
- C
- D

---

## 6.3 Bundle 2: Commitment and Proof Backend Migration

This bundle treats scheme semantics, root binding, backend ownership, and proof layout
as one verifier contract.

### Goal

- make `SchemeProfile` own one canonical verifier-visible commitment meaning,
- replace shared built-in concrete state handling with materialized column backends,
- move proof layout and width specialization under explicit profile ownership,
- make root binding consume canonical normalized profile-aware commitment products.

### Why These Belong Together

- scheme semantics, proof layout, and root binding are one verifier contract,
- splitting them creates a new "native vs proof" semantic drift problem,
- backend ownership cannot be fixed cleanly if proof layout and root binding remain ad hoc.

### Success Criteria

- each scheme profile has exactly one canonical verifier-visible digest contract,
- shared proving code no longer reconstructs concrete built-in state enums,
- width and layout are fixed at setup-time profile materialization,
- built-in SSMC and SMT behave like ordinary registered backends.

### Design Philosophy

- setup-time specialization,
- no shared built-in branching,
- one scheme profile, one commitment meaning.

### Important Public Interface Direction

- `ColumnBackend` or equivalent profile-owned materialized backend,
- canonical column transition / proof input objects owned by the backend seam,
- root binding consuming normalized profile-aware digests rather than ad hoc scheme tags.

### Execution Workstream Mapping

- E
- F
- G

---

## 6.4 Bundle 3: Extensibility Boundary and Migration Hardening

This bundle defines where custom types stop and capability families begin, and how the
old world is retired.

### Goal

- define the exact boundary of custom types inside generic IR,
- route domain-specific semantics to capability families,
- make capability transcript and proof contracts profile-aware,
- fix migration phases, compatibility shims, and deletion order.

### Why These Belong Together

- allowing custom types changes the capability and transcript boundary too,
- migration strategy is part of architecture here, not follow-up cleanup,
- boundary mistakes at this layer turn extensibility into an unbounded VM problem.

### Success Criteria

- registered custom types can flow through capability transcript I/O,
- generic IR remains closed in operation vocabulary,
- architecture tests can forbid built-in privilege and duplicated truth sources,
- legacy `ValueType`-centric and built-in-only paths are reduced to explicit deletion targets.

### Design Philosophy

- extensibility without arbitrary VM plugins,
- domain semantics belong in capabilities,
- migration is a first-class architecture concern.

### Important Public Interface Direction

- capability transcript I/O and transcript become profile-aware,
- custom types participate by descriptor and capability, not operator injection,
- migration gates and deletion criteria are made explicit.

### Execution Workstream Mapping

- H
- I
- parts of C and D at the capability boundary

---

## 7. Execution Workstream Inventory

The original A-I workstreams remain useful, but only as implementation slices under the
design bundles above.

| Workstream | Short role | Primary bundle |
|---|---|---|
| A | canonical descriptor and registry layer | Bundle 0 |
| B | compiler and artifact refactor around column profiles | Bundle 1 |
| C | type-system refactor to descriptors and semantics families | Bundle 1 |
| D | encoding, width, and transcript refactor | Bundle 1 |
| E | scheme profile and commitment/root unification | Bundle 2 |
| F | column backend and proof materialization refactor | Bundle 2 |
| G | proof layout specialization and width polymorphism | Bundle 2 |
| H | capability and custom-type boundary | Bundle 3 |
| I | migration, compatibility, and legacy deletion | Bundle 3 |

These workstreams are still the right units for:

- implementation sequencing,
- temporary compatibility adapters,
- partial migrations,
- test ownership,
- legacy deletion planning.

They are not the right units for top-level architecture design review.

---

## 8. How To Use This Structure

From this point forward, planning should proceed in this order:

1. write the detailed design spec for **Bundle 0**,
2. once Bundle 0 is stable, produce separate detailed plans for **Bundle 1** and **Bundle 2**,
3. only after those contracts are stable, finalize **Bundle 3**,
4. implement through the subordinate execution workstreams as needed.

This means the next planning unit is not "implement Workstream A directly."

It is:

> **Produce a decision-complete spec for Bundle 0.**

---

## 9. Acceptance and Validation Gates

Before detailed planning begins for a bundle, the following questions must be answerable.

## 9.1 Bundle 0 Gate

- Do the four descriptors have non-overlapping responsibilities?
- Is `ColumnProfile` clearly the per-column source of truth?

## 9.2 Bundle 1 Gate

- Does compiler/artifact sealing bind one profile contract per column?
- Are type, encoding, and transcript truth centralized instead of split?

## 9.3 Bundle 2 Gate

- Are scheme semantics and root/proof semantics unified into one contract?
- Has shared orchestration stopped branching on built-in concrete types and schemes?

## 9.4 Bundle 3 Gate

- Does the custom-type boundary align cleanly with the capability boundary?
- Are migration and deletion criteria explicit enough to avoid indefinite dual truth sources?

If any answer is still vague, that bundle is not ready for detailed implementation planning.

---

## 10. Defaults and Assumptions

- `hybrid auto-selection` is out of scope.
- Column-by-column scheme choice remains explicit.
- Built-ins remain available, but not architecturally privileged.
- Custom types are allowed, but arbitrary custom operators are not part of generic IR.
- Complex domain semantics should go through capability families.
- This note is now the top-level planning structure for the column-profile migration.

---

## 11. What "Done" Looks Like

The migration should be considered complete only when all of the following are true:

1. every committed column is fully described by one sealed `ColumnProfile`,
2. built-ins are implemented as pre-registered profiles rather than special-case core logic,
3. generic IR remains closed in operation families and capability-based in type participation,
4. width, transcript encoding, null encoding, and proof layout are profile-owned,
5. scheme commitment semantics are canonical and profile-owned across native and proof layers,
6. shared runtime/prover/verifier orchestration no longer depends on built-in concrete state enums,
7. custom registered types can flow through the system, including capability boundaries, without being faked as built-in types.

If any of these statements is false, the architecture is still only partially migrated.
