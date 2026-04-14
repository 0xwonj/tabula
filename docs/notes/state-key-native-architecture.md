# Native State-Key Architecture

> **Status**: Planned final architecture
> **Date**: 2026-04-06
> **Scope**: Final end-to-end architecture for native state-key support across
> language, compiler sealing, runtime execution, property queries, proving, and
> user-facing tooling.
> **Related**: [executor-proof-codesign-architecture.md](executor-proof-codesign-architecture.md),
> [profile-native-runtime-migration-plan.md](profile-native-runtime-migration-plan.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [runtime-machine-proof-backend-roadmap.md](runtime-machine-proof-backend-roadmap.md),
> [program-redesign/program-hir-contract-and-data-model.md](program-redesign/program-hir-contract-and-data-model.md),
> [program-redesign/program-canonical-ir-contract-and-data-model.md](program-redesign/program-canonical-ir-contract-and-data-model.md),
> [program-redesign/program-final-seam-decisions.md](program-redesign/program-final-seam-decisions.md)

> **Implementation status**: this note describes the final target architecture.
> The current proof-capable native implementation is intentionally fail-closed
> to unary user-state keys (`1 component / 3 FE`) until symbolic AIR
> compilation reopens composite-key proving.

---

## 1. Why This Note Exists

Tabula's source-language and IR model already treat table keys as logical typed
tuples.

However, the current execution and proof stack still treats user state keys as
`u64` rows.

That mismatch creates an unstable architecture:

- the language surface can express composite keys,
- some compiler artifacts preserve key arity,
- but executor, witness, commitment, and AIR still collapse user-state keys
  into `RowKey(u64)`.

This note fixes the target architecture so the migration can proceed without
reopening the design.

The goal is not "better lowering to row numbers."

The goal is:

> **Tabula user state should support native logical keys, with one canonical
> committed-key model shared by execution and proof.**

---

## 2. Final Decision Summary

The final architecture decisions are:

1. **User-state protocol keys are `CommittedKey`, not `RowKey(u64)`**
   - `RowKey` stops being the protocol-visible identity for user state
   - dense rows may remain as private scheme/storage optimizations only
2. **Logical keys and committed keys are distinct layers**
   - source, HIR, IR, SDK, and CLI use logical key tuples
   - runtime, witness, commitment, and AIR use canonical committed keys
3. **Table key semantics are compiler-sealed**
   - each state table gets a sealed `TableKeyContract`
   - execution and proof consume that contract rather than inferring key shape
4. **Ordered property queries are defined over committed-key order**
   - not row order
   - not ad hoc backend-local order
5. **Support is fail-closed at registration time**
   - unsupported key component types, encodings, or property-query
     combinations fail in compiler registration
   - runtime should not be the first place that rejects them
6. **The proof stack must prove committed keys directly**
   - not re-derive them from hidden runtime mappings
   - not assume `u64`-only key AIR forever

These decisions are the baseline for implementation.

---

## 3. Core Model

### 3.1 Three key representations

Tabula should use three distinct key representations:

1. `LogicalKeyTuple`
   - typed tuple used in source, HIR, MIR, canonical IR, SDK, and CLI
   - examples:
     - `users[id]`
     - `allowances[owner, spender]`
2. `CommittedKey`
   - canonical protocol-visible key for user state
   - shared by runtime execution, journals, witness inputs, commitments, and
     AIR
3. `LocalLocator`
   - optional backend-private index or row handle
   - may be used for storage or performance
   - must not appear in portable protocol contracts

### 3.2 Static tables are not part of this migration

Static lookup tables may continue using row-oriented identifiers for now.

This architecture note is about declared user state.

### 3.3 What `CommittedKey` means

`CommittedKey` is the canonical, table-scoped key representation that the proof
system commits to and the runtime executes against.

It must be:

- deterministic,
- canonical,
- comparable when ordered queries are supported,
- encodable into proof-visible limbs,
- and decodable back into logical key components when execution needs to write
  property-read result keys into locals.

---

## 4. Crate Responsibilities

### 4.1 `tabula-core`

Owns portable protocol nouns only.

It should define:

- `CommittedKey`
- `CommittedCellKey`
- `TableKeySchema`
- `TableKeyContract`
- `ProgramMachineShape`
- committed-key-based property query result shapes

It should stop modeling user-state cells as `(table, col, row: u64)`.

### 4.2 `tabula-profile`

Remains the semantic source of truth for type, encoding, and scheme facts.

It should not grow a separate parallel "key profile universe."

Instead, key support should be expressed through existing profile facts:

- type ordering capability,
- encoding ordering-preserving behavior,
- scheme property-query capability.

### 4.3 `tabula-types`

Owns runtime behavior for table keys.

It should define:

- `TableKeyCodec`
- `TableKeyCodecRegistry`
- committed-key-based runtime carriers used by execution and proof preparation

`TableKeyCodec` should be built from sealed key contracts plus installed type
and encoding runtimes.

### 4.4 `tabula-compiler`

Owns key-contract sealing and fail-closed validation.

It should:

- derive one `TableKeyContract` per table,
- compute table key usage summaries,
- compute machine shape requirements,
- reject unsupported key/query/scheme combinations,
- include key contracts in program binding and canonical artifact hashing.

### 4.5 `tabula-runtime`

Owns the boundary between logical state input and committed execution state.

It should:

- build key codecs from the registered program,
- convert logical state input into committed snapshots,
- pass key codec access into executor and proof preparation,
- ensure execution and proving consume the same committed-key semantics.

### 4.6 `tabula-executor`

Owns deterministic semantic execution over committed keys.

It should:

- evaluate logical key tuples from IR,
- encode them through `TableKeyCodec`,
- read/write/delete against committed-key state,
- record committed-key journals,
- decode result keys only when IR destinations require logical components.

### 4.7 `tabula-ext`

Owns extension contracts, including scheme runtime and proof-backend interfaces.

It should accept table key contracts and committed-key-based column state rather
than raw `u64` rows.

### 4.8 `tabula-witness`, `tabula-commitment`, `tabula-chips`, `tabula-gadgets`

Own the proof-visible committed-key representation.

They should:

- materialize committed-key witness inputs,
- commit to committed keys directly,
- and generalize key witnesses away from fixed `u64` assumptions.

---

## 5. Final Data Contracts

The following contracts are the intended final nouns.

### 5.1 Core portable contracts

```rust
pub struct CommittedKey(pub Vec<u8>);

pub struct CommittedCellKey {
    pub table: TableId,
    pub col: ColId,
    pub key: CommittedKey,
}

pub struct TableKeySchema {
    pub component_types: Vec<TypeId>,
}

pub struct TableKeyContract {
    pub table_id: TableId,
    pub schema: TableKeySchema,
    pub component_encoding_profile_ids: Vec<EncodingProfileId>,
    pub ordering_family: KeyOrderingFamily,
    pub key_width_bytes: u16,
    pub key_width_fes: u16,
    pub supports_ordered_queries: bool,
}

pub struct ProgramMachineShape {
    pub max_slots: u16,
    pub max_key_components: u16,
    pub max_key_fes: u16,
}
```

The exact field names may change during implementation, but these roles should
remain stable.

### 5.2 Runtime behavior contract

```rust
pub trait TableKeyCodec {
    fn encode_tuple(&self, values: &[TypedValue]) -> Result<CommittedKey, TabulaError>;
    fn decode_key(&self, key: &CommittedKey) -> Result<Vec<TypedValue>, TabulaError>;
    fn compare_keys(
        &self,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<std::cmp::Ordering, TabulaError>;
    fn encode_proof_limbs(&self, key: &CommittedKey) -> Result<Vec<KoalaBear>, TabulaError>;
}
```

This runtime is the only approved seam between logical tuples and committed
keys.

### 5.3 Runtime/proof carriers

Execution and proof-facing carriers should become committed-key based:

- `CommittedColumnEntry`
- `CommittedPropertyQuery`
- `CommittedPropertyQueryResult`
- committed-key-based journal effects
- committed-key-based witness preparation structs

No hidden side map should be required to explain which logical key a proof
entry refers to.

---

## 6. Compiler Sealing Model

### 6.1 What the compiler must seal

For every declared state table, compiler registration must seal:

- logical key component types,
- key encoding selection per component,
- whether ordered property queries are required,
- final committed-key width in bytes,
- final committed-key width in proof limbs,
- machine shape contribution.

### 6.2 Why sealing is mandatory

Execution and proof must not independently guess:

- how keys are encoded,
- which key order semantics are valid,
- or whether a chosen scheme can satisfy the table's query surface.

The registered program must be sufficient to reconstruct committed-key
semantics exactly.

### 6.3 Fail-closed validation rules

Registration should reject at least the following:

- key component types without installed runtime support,
- ordered-key use with non-ordering-preserving encodings,
- schemes that do not support the required property-query kinds,
- tables whose key width exceeds machine-shape limits,
- IR property-read forms whose result-key destinations do not match table key
  arity.

---

## 7. Runtime and Execution Pipeline

The final runtime path is:

1. source program defines logical table keys,
2. compiler seals `TableKeyContract` and `ProgramMachineShape`,
3. runtime builds `TableKeyCodec` instances from the registered program,
4. public input provides logical key tuples,
5. runtime converts logical input into `CommittedStateSnapshot`,
6. executor evaluates state ops and property ops against committed keys,
7. executor emits committed-key journals,
8. proving consumes committed state and committed-key journals directly.

### 7.1 State input boundary

The public authoring boundary should be:

- `LogicalStateInput`

The execution/proof boundary should be:

- `CommittedStateSnapshot`

This boundary is owned by runtime, not SDK and not executor.

### 7.2 Executor hot path

For read/write/delete:

1. evaluate logical key operands,
2. encode tuple using `TableKeyCodec`,
3. build `CommittedCellKey`,
4. perform state access,
5. record committed-key effects.

There should be no executor-local `u64` row decoding special case for user
state.

---

## 8. Property Query Architecture

Property queries are the hardest part of the migration and therefore require
explicit final rules.

### 8.1 Query semantics

Ordered property queries are defined over committed-key order.

That means:

- `minimum`
- `maximum`
- `successor`
- `predecessor`
- range non-existence

all refer to the canonical committed-key ordering for the table.

### 8.2 IR result shape

The current scalar-key result model is not the final design.

The final `PropertyRead` shape should support:

- one value destination,
- zero or more key-component destinations,
- one null destination.

The executor should decode the committed result key back into logical
components only at the final local-write step.

### 8.3 Proof-facing property claims

Proof preparation should not re-evaluate logical tuples from IR.

Instead, proof-facing property claims should carry:

- committed query bounds,
- committed result key,
- committed result value,
- null flag.

This keeps execution and proof aligned on one canonical key meaning.

---

## 9. Proof and AIR Architecture

### 9.1 Commitment model

Committed columns should be committed as:

- `(table, col, committed_key) -> value`

not:

- `(table, col, row_u64) -> value`

for user state.

### 9.2 Witness model

Witness preparation should group:

- committed old entries,
- committed accesses,
- committed writes,
- committed property-read claims.

User-state witness inputs should not carry raw row ids as the semantic key.

### 9.3 AIR model

Execution, state, and property AIR should share one committed-key witness
representation.

The current `u64`-specific key gadgets are not the final architecture.

The intended final direction is:

- one committed-key witness family parameterized by machine shape,
- fixed proof-visible width from `ProgramMachineShape.max_key_fes`,
- table-local exact widths checked against that program shape.

This allows the generic machine to remain fixed-width while supporting multiple
table key contracts within one program.

---

## 10. Machine Shape Policy

Key support changes proof geometry, so it should not be treated as a normal DoS
budget.

Tabula should distinguish:

- `ProgramBudgets`
  - operational resource ceilings
- `ProgramMachineShape`
  - proof geometry requirements

The machine shape should include at least:

- max slot count,
- max key component count,
- max committed-key FE width.

Compiler registration computes it. Runtime and proof setup enforce it.

---

## 11. Large Implementation Plan

Implementation should proceed in four large workstreams.

The workstreams below describe the intended end state of the migration. The
current proof-capable native implementation is still intentionally fail-closed
to unary user-state keys until symbolic AIR compilation reopens composite-key
proving.

### 11.1 Workstream 1: Key Contract Foundation

Crates:

- `tabula-core`
- `tabula-compiler`
- `tabula-contract`
- `tabula-profile`

Goals:

- define committed-key portable nouns,
- define table key contract and machine shape,
- add compiler sealing for key contracts,
- add fail-closed registration validation,
- include key contracts in registered artifact hashing and binding.

Success criteria:

- the registered artifact fully describes table key semantics,
- binding changes if key semantics change,
- unsupported key/query combinations fail before runtime.

### 11.2 Workstream 2: Execution Data Plane Migration

Crates:

- `tabula-types`
- `tabula-runtime`
- `tabula-executor`
- `tabula-ir`
- `tabula-ext`

Goals:

- implement `TableKeyCodec`,
- introduce logical-input to committed-snapshot conversion,
- migrate state ops to committed keys,
- migrate property-read result shapes,
- migrate runtime scheme APIs to committed-key carriers.

Success criteria:

- composite-key direct reads/writes/deletes work through execute,
- runtime internals stop depending on user-state `u64` rows,
- property queries flow through committed keys end-to-end on the execution
  path.

### 11.3 Workstream 3: Proof Stack Migration

Crates:

- `tabula-witness`
- `tabula-commitment`
- `tabula-ext`
- `tabula-gadgets`
- `tabula-chips`
- `tabula-stark`

Goals:

- migrate witness inputs and proof backend interfaces,
- migrate commitments from row keys to committed keys,
- generalize key witness gadgets,
- update execution/state/property AIR to prove committed-key behavior directly.

Success criteria:

- composite-key programs prove end-to-end,
- proof-visible user-state keys are committed keys rather than `u64` rows,
- scheme behavior and AIR behavior agree on key ordering semantics.

### 11.4 Workstream 4: Surface Cleanup and Stabilization

Crates:

- `tabula-sdk`
- `tabula-cli`
- docs and full test suites

Goals:

- remove row-based user-state authoring APIs,
- expose logical-key-based SDK and CLI flows,
- update docs, fixtures, and examples,
- delete transitional compatibility paths.

Success criteria:

- no user-facing user-state API requires raw row ids,
- docs and tests describe the final model rather than the migration model,
- the legacy user-state row path is deleted.

---

## 12. Non-Goals

This note does not require:

- changing static-table lookup contracts immediately,
- supporting every possible custom ordered type in the first implementation,
- preserving compatibility with the legacy user-state row API forever.

The architecture should support future custom ordered key types, but the first
native implementation may start with a smaller set of ordered encodings.

---

## 13. Final Acceptance Criteria

The migration is complete only when all of the following are true:

1. source, HIR, and IR can express composite logical keys,
2. compiler registration seals final key contracts and machine shape,
3. runtime builds committed snapshots from logical key inputs,
4. executor uses committed keys for state and property operations,
5. witness and commitment consume committed keys directly,
6. AIR proves committed-key semantics directly,
7. ordered property queries are defined over committed-key order,
8. user-facing SDK and CLI surfaces no longer require raw user-state row ids.

Until then, the migration should be treated as incomplete.
