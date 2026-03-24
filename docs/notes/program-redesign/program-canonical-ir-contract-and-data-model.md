# Program Canonical IR Contract and Data Model

> **Status**: Proposed implementation contract
> **Date**: 2026-03-24
> **Scope**: Defines the exact canonical IR contract that should be used as the
> immediate implementation target, together with the recommended Rust-facing
> data model shape for that contract.
> **Related**: [program-canonical-ir-design.md](program-canonical-ir-design.md),
> [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-mir-design.md](program-mir-design.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md)

---

## 1. Why This Note Exists

At this point, the project should not separate:

- the semantic contract of canonical IR
- from the Rust data model that will embody that contract.

If those are designed independently, one of two bad outcomes usually follows:

- the Rust data model drifts into a convenience structure that no longer
  faithfully encodes the intended semantics,
- or the semantic note stays abstract and implementation begins to improvise.

The right move now is to define both together.

This note is therefore intentionally:

- semantic enough to serve as the canonical IR contract,
- and concrete enough to guide immediate Rust data-model implementation.

---

## 2. What This Note Is Freezing

This note assumes the following architecture is already fixed:

- Tabula is a closed-world `program` language.
- The compiler pipeline is `AST -> HIR -> MIR -> canonical IR`.
- Canonical IR is:
  - flat
  - SSA-disciplined
  - CFG-free
  - the shared executor/prover contract
- `Lookup` is deleted and replaced by first-class `Relation`.
- `fn` disappears before canonical IR.
- The finalized seam decisions already hold:
  - hybrid builtin `Hash` plus `CallCapability`
  - public-only statement-bound initial `context`
  - digest-only initial event binding
  - separate future query-proof mode
  - guards only on effectful or checked ops

This note does **not** reopen those questions.

---

## 3. Design Principles

### 3.1 The model should encode semantic categories, not source syntax

Canonical IR should still make visible:

- state
- const
- relation
- capability
- event
- query/tx entry kind

It should not preserve:

- source blocks
- source control syntax
- helper functions
- parser-level naming conveniences

### 3.2 The model should make illegal states hard to express

The Rust structure should make it natural to validate:

- guard placement
- entry-kind legality
- manifest membership
- result arity
- type correctness
- and SSA single-assignment

### 3.3 The model should be executor-shaped

Canonical IR is not just a compiler interchange format.

It is directly consumed by:

- the executor
- the execution journal
- proof-journal reduction
- and proof-artifact generation

So data-model choices should favor deterministic interpretation over source-like
presentation.

### 3.4 The model should keep MIR pressure out

MIR owns:

- structured control
- effect summaries
- inlining
- normalization
- callable-policy enforcement

Canonical IR should not re-grow those concerns in encoded form.

---

## 4. Recommended Overall Shape

The most coherent exact shape is:

```rust
pub struct ProgramIr {
    pub program_id: ProgramId,
    pub state: StateSchema,
    pub context: ContextSchema,
    pub const_pool: ConstantPool,
    pub relation_manifest: RelationManifest,
    pub capability_manifest: CapabilityManifest,
    pub event_manifest: EventManifest,
    pub entries: Vec<EntryIr>,
}
```

The important recommendation here is:

- use a **single `EntryIr` type**
- distinguish query versus tx by `EntryKind`

rather than having two wholly separate top-level body types.

That keeps canonical IR smaller and makes validation more uniform, while still
preserving the semantic distinction that matters.

---

## 5. Core Identifiers

Canonical IR should use small, explicit typed IDs.

```rust
pub struct ProgramId(pub u32);
pub struct EntryId(pub u32);
pub struct ParamId(pub u32);
pub struct LocalId(pub u32);
pub struct ContextFieldId(pub u32);
pub struct ConstId(pub u32);
pub struct TableId(pub u32);
pub struct FieldId(pub u32);
pub struct RelationId(pub u32);
pub struct CapabilityId(pub u32);
pub struct EventId(pub u32);
```

These should be:

- stable within one `ProgramIr`
- dense enough for compact runtime storage
- never reused across different semantic families

---

## 6. Entry Model

### 6.1 Entry kind

```rust
pub enum EntryKind {
    Query,
    Tx,
}
```

Canonical IR should not have `Fn` entries.

### 6.2 Entry structure

```rust
pub struct EntryIr {
    pub id: EntryId,
    pub symbol: String,
    pub kind: EntryKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: BodyIr,
}
```

`symbol` may later become a stronger symbol type, but a human-readable name is
useful for debugging, diagnostics, and tooling.

### 6.3 Why one entry type is better

Using `EntryKind` rather than separate `QueryIr` / `TxIr` structs gives:

- one body representation
- one validation entrypoint
- one dispatch shape for runtime integration

while still preserving:

- query legality checks
- tx legality checks
- different return expectations

The semantic difference belongs in:

- `kind`
- validation
- and runtime dispatch policy

not in duplicated container structure.

---

## 7. Type-Carrying Nodes

Canonical IR should stay typed, but it does not need to reinvent the entire
profile/type layer in this note.

The exact type reference should therefore be abstracted as:

```rust
pub type TypeRef = TypeId;
```

or whatever the eventual profile/type handle becomes.

The key point is:

- locals
- params
- context fields
- const entries
- manifest signatures

must all carry explicit type references.

### 7.1 Param declarations

```rust
pub struct ParamDecl {
    pub id: ParamId,
    pub symbol: String,
    pub ty: TypeRef,
}
```

### 7.2 Local declarations

```rust
pub struct LocalDecl {
    pub id: LocalId,
    pub ty: TypeRef,
}
```

Locals should be declared separately from ops so that:

- validators know result types up front
- inactive-default behavior can be checked
- and runtime storage can be allocated predictably

---

## 8. Body Model

The canonical body remains intentionally simple.

```rust
pub struct BodyIr {
    pub locals: Vec<LocalDecl>,
    pub ops: Vec<Op>,
}
```

### 8.1 Why no nested regions

Canonical IR is where regions are already gone.

By the time execution reaches this layer:

- `if`
- `match`
- future bounded loops

must already have been lowered into:

- selector computation
- `Select`
- and guarded ops

### 8.2 Why `Return` remains an op

The cleanest model is still:

- a flat op list
- ending in `Return`

That keeps:

- evaluation order explicit
- result production explicit
- and validation simple

---

## 9. Value Sources

The most useful value model is:

```rust
pub enum ValueRef {
    Literal(LiteralValue),
    Param(ParamId),
    Context(ContextFieldId),
    Local(LocalId),
    Const(ConstId),
}
```

This gives canonical IR exactly the immutable sources it needs:

- literals
- per-entry parameters
- per-instance context
- already-computed locals
- program-sealed constants

### 9.1 Why context should be a value source

`context` is not a state read.

It is:

- immutable within the instance
- statement-bound in the initial model
- and globally visible to all entries

So it belongs beside `Param` and `Const`, not beside state ops.

### 9.2 Value tuples

The IR should use an explicit tuple wrapper:

```rust
pub struct ValueTupleRef(pub Vec<ValueRef>);
```

This is sufficient for:

- composite table keys
- relation inputs
- capability inputs
- event arguments
- return tuples

No special row-expression type is needed.

---

## 10. Guard Model

### 10.1 Guard reference

```rust
pub struct GuardRef(pub LocalId); // must have bool type
```

This remains the cleanest representation.

It says:

- guards are ordinary SSA-produced booleans
- but they are used in a semantically privileged position

### 10.2 Guard semantics

If an op has:

- no guard, it always applies
- a true guard, it applies
- a false guard, it is semantically inactive

### 10.3 Finalized initial guard frontier

Guarded canonical ops are:

- `Assert`
- `DivMod` and other checked partial ops
- `ReadState`
- `WriteState`
- `DeleteState`
- `ReadStateProperty`
- `AssertRelation`
- `EvalRelation`
- `CallCapability`
- `EmitEvent`

Non-guarded canonical ops are:

- arithmetic
- comparisons
- boolean ops
- `Select`
- builtin `Hash`
- other total pure value ops

### 10.4 Inactive output semantics

For output-producing guarded ops:

- a false guard makes the op semantically inactive
- but outputs still receive typed inactive default values

This should be an explicit executor rule, not an accidental convention.

That requires a canonical notion of:

```rust
fn inactive_default(ty: TypeRef) -> LiteralValue
```

or the moral equivalent in the runtime representation.

---

## 11. Program-Level Metadata Objects

Canonical IR needs the following program-level objects to be exact enough for
execution and validation.

### 11.1 Context schema

```rust
pub struct ContextSchema {
    pub fields: Vec<ContextField>,
}

pub struct ContextField {
    pub id: ContextFieldId,
    pub symbol: String,
    pub ty: TypeRef,
}
```

Initial policy:

- all context fields are public
- all context fields are statement-bound

So visibility does not need to be encoded yet.

### 11.2 Constant pool

```rust
pub struct ConstantPool {
    pub entries: Vec<ConstantEntry>,
}

pub struct ConstantEntry {
    pub id: ConstId,
    pub ty: TypeRef,
    pub value: LiteralValue,
}
```

This keeps `Const` a true immutable value source rather than a hidden op.

### 11.3 Relation manifest

```rust
pub struct RelationManifest {
    pub entries: Vec<RelationManifestEntry>,
}

pub struct RelationManifestEntry {
    pub id: RelationId,
    pub descriptor: RelationDescriptor,
    pub binding: RelationBinding,
}
```

This preserves the earlier decision:

- relation needs descriptor plus binding

not just a flat table identifier.

### 11.4 Event manifest

```rust
pub struct EventManifest {
    pub entries: Vec<EventDescriptor>,
}

pub struct EventDescriptor {
    pub id: EventId,
    pub symbol: String,
    pub fields: Vec<TypeRef>,
}
```

### 11.5 Capability manifest

```rust
pub struct CapabilityManifest {
    pub entries: Vec<CapabilityDescriptor>,
}

pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub symbol: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub totality: CapabilityTotality,
    pub query_policy: CapabilityQueryPolicy,
    pub proof_visibility: CapabilityProofVisibility,
}
```

Recommended capability metadata enums:

```rust
pub enum CapabilityTotality {
    Total,
    Checked,
}

pub enum CapabilityQueryPolicy {
    QuerySafe,
    TxOnly,
}

pub enum CapabilityProofVisibility {
    Journaled,
    OpaqueRuntimeOnly,
}
```

This is the minimum needed for:

- query legality
- guardability
- journaling policy

---

## 12. Exact Op Taxonomy

The recommended exact op set is:

```rust
pub enum Op {
    // Total pure value ops
    Arith {
        dst: LocalId,
        op: ArithOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Cmp {
        dst: LocalId,
        op: CmpOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Not {
        dst: LocalId,
        src: ValueRef,
    },
    And {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Or {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Select {
        dst: LocalId,
        cond: ValueRef,
        if_true: ValueRef,
        if_false: ValueRef,
    },
    Hash {
        dst: LocalId,
        family: HashFamily,
        inputs: ValueTupleRef,
    },

    // Checked / partial
    DivMod {
        guard: Option<GuardRef>,
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },

    // State
    ReadState {
        guard: Option<GuardRef>,
        dst_value: LocalId,
        dst_present: LocalId,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    WriteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
        value: ValueRef,
    },
    DeleteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    ReadStateProperty {
        guard: Option<GuardRef>,
        dsts: Vec<LocalId>,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },

    // Assertions
    Assert {
        guard: Option<GuardRef>,
        cond: ValueRef,
    },

    // Relations
    AssertRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        args: ValueTupleRef,
    },
    EvalRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Capabilities
    CallCapability {
        guard: Option<GuardRef>,
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Output
    EmitEvent {
        guard: Option<GuardRef>,
        event: EventId,
        args: ValueTupleRef,
    },

    // Body end
    Return {
        values: ValueTupleRef,
    },
}
```

### 12.1 Why `Hash` is separate

`Hash` is separate because the seam decision is already closed:

- a tiny blessed builtin family belongs in canonical IR
- everything else remains `CallCapability`

`HashFamily` should therefore be a closed enum, not a stringly-typed escape
hatch.

### 12.2 Why `ReadStateProperty` uses `dsts`

Different structural queries may return:

- a value
- a key/value pair
- a presence flag
- a tuple of aggregate results

So `ReadStateProperty` should use a result tuple rather than overfit a single
shape.

### 12.3 Why `Return` stays explicit

Canonical IR should not split:

- a flat op list
- from a hidden terminator object

That would add shape without much benefit.

`Return` as the final op keeps interpretation simpler.

---

## 13. Entry-Kind Validation Policy

### 13.1 Queries

Queries may contain:

- total pure value ops
- `ReadState`
- `ReadStateProperty`
- `Assert`
- `AssertRelation`
- `EvalRelation`
- builtin `Hash`
- query-safe `CallCapability`
- `Return`

Queries may not contain:

- `WriteState`
- `DeleteState`
- `EmitEvent`
- tx-only `CallCapability`

### 13.2 Transactions

Transactions may contain all canonical op families that are otherwise valid.

### 13.3 Why entry-kind policy belongs in validation

This policy should **not** be encoded by making separate op enums per entry
kind.

That would fragment the IR.

Instead:

- the op taxonomy stays small and shared
- entry-kind legality is enforced by canonical validation

---

## 14. Journal Projection Contract

The canonical IR contract should already imply the journal mapping.

### 14.1 State journal families

- `ReadState`
- `WriteState`
- `DeleteState`
- `ReadStateProperty`

map into state-oriented journal families.

### 14.2 Relation journal families

- `AssertRelation`
- `EvalRelation`

map into relation-oriented journal families.

### 14.3 Capability journal families

- `CallCapability`

maps into capability-oriented journal families when:

- the referenced capability descriptor is `Journaled`

and may omit a dedicated journal family when capability metadata says
`OpaqueRuntimeOnly`.

### 14.4 Event journal family

- `EmitEvent`

maps into the event journal.

### 14.5 Builtin hash

Builtin `Hash` should not require its own journal family in the initial model.

---

## 15. Validation Invariants

Canonical IR validation should enforce at least the following.

### 15.1 Structural invariants

- every `EntryIr` has exactly one `Return`
- `Return` is the last op in the body
- all referenced IDs exist in the appropriate manifest or schema
- all locals and params are uniquely declared

### 15.2 SSA invariants

- every local is assigned exactly once
- every local use refers to a previously assigned local
- guard refs refer to bool-typed locals

### 15.3 Typing invariants

- all op operands have the expected types
- result locals have the correct declared types
- tuple arities match relation, capability, event, and return signatures
- builtin `Hash` family and input/result types are valid for that family

### 15.4 Entry-policy invariants

- query bodies contain only allowed op families
- tx bodies satisfy tx return policy
- tx-only capabilities are rejected from queries

### 15.5 Guard invariants

- guards appear only on the finalized guardable frontier
- non-guardable ops never carry a guard
- output-producing guarded ops use the canonical inactive-default policy

---

## 16. Recommended Implementation Order

This note implies the following implementation order.

1. freeze these exact Rust-facing canonical IR shapes
2. implement canonical validation against this contract
3. implement executor semantics against this contract
4. implement journal projection against this contract
5. only then design MIR exact data structures against this target

This is the right order because canonical IR is the boundary all higher layers
must eventually satisfy.

---

## 17. What This Note Commits To

This note is intended to settle the following.

- The next concrete rewrite step should be **canonical IR contract plus exact
  Rust data model together**, not separately.
- Canonical IR should use one `EntryIr` with `EntryKind`, not duplicate query
  and tx container structure.
- `ValueRef` should include `Literal`, `Param`, `Context`, `Local`, and `Const`.
- `context` is an immutable value source, not a state read.
- `Hash` remains a small blessed builtin family and stays outside the initial
  guarded frontier.
- `CallCapability` remains the general operational escape hatch.
- Guards apply only to effectful or checked ops.
- Inactive guarded outputs use typed defaults.
- Canonical validation owns entry-kind legality and guard legality.
- Executor and journal implementation should be written against this contract
  before MIR/HIR exact data models are frozen.

With this in place, the next design target is no longer "what should canonical
IR mean?" but "how should MIR target this exact contract?"
