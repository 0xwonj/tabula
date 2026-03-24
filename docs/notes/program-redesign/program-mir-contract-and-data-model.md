# Program MIR Contract and Data Model

> **Status**: Proposed implementation contract
> **Date**: 2026-03-24
> **Scope**: Defines the exact MIR contract that should be used as the
> immediate compiler-middle-end target, together with the recommended Rust data
> model and the MIR -> canonical IR lowering contract.
> **Related**: [program-mir-design.md](program-mir-design.md),
> [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md)

---

## 1. Why This Note Exists

The MIR architecture note explains:

- why MIR exists,
- what MIR is responsible for,
- and how MIR differs from HIR and canonical IR.

That is no longer enough.

The rewrite now needs an exact middle-end contract that can answer:

- what Rust types should represent MIR,
- what exactly is allowed to remain in MIR,
- what exactly must disappear before canonical IR,
- and how MIR lowers into the canonical executor/prover contract.

This note freezes that exact contract.

---

## 2. What This Note Assumes Is Already Fixed

This note does not reopen the following.

- Tabula is a closed-world `program` language.
- The compiler layering is `AST -> HIR -> MIR -> canonical IR`.
- Canonical IR is already defined as:
  - flat
  - SSA-disciplined
  - CFG-free
  - executor/prover-facing
- `Lookup` is gone and `Relation` is first-class.
- The typing/effect system distinguishes:
  - world effects
  - proof-observable semantic effects
  - `MayFail`
- The seam decisions are already fixed:
  - builtin blessed `Hash`
  - public-only statement-bound initial `context`
  - digest-only initial event binding
  - separate future query-proof mode
  - guards only on effectful or checked ops

MIR is therefore designed **against** fixed canonical IR policy, not alongside
it.

---

## 3. MIR's Exact Role

The exact role of MIR is:

- the last compiler-owned representation
- the first fully resolved representation
- the first effect-explicit representation
- the layer that still preserves structured control
- and the layer that owns normalization before canonical flattening

It should therefore be:

- richer than canonical IR
- poorer than HIR
- strict enough to support effect checking and legality
- but still expressive enough to represent:
  - `fn`
  - `query`
  - `tx`
  - `if`
  - `match`

without inventing CFG.

---

## 4. Naming Recommendation

Within the eventual `mir` module, the preferred type names are:

- `mir::Program`
- `mir::Callable`
- `mir::Body`
- `mir::Op`
- `mir::Region`

not:

- `MirProgram`
- `MirBody`
- `MirOp`

Layer identity should come from the module path, not from type-name suffixes.

This keeps naming aligned across:

- `ast::Program`
- `hir::Program`
- `mir::Program`
- `ir::Program`

---

## 5. Exact MIR Root Shape

The recommended exact MIR root is:

```rust
pub struct Program {
    pub program_id: ProgramId,
    pub state: StateSchema,
    pub context: ContextSchema,
    pub const_pool: ConstantPool,
    pub relation_manifest: RelationManifest,
    pub capability_manifest: CapabilityManifest,
    pub event_manifest: EventManifest,
    pub callables: Vec<Callable>,
}
```

### 5.1 Why MIR keeps the same top-level semantic universe

MIR should still carry:

- state schema
- context schema
- constant pool
- relation manifest
- capability manifest
- event manifest

because MIR legality depends on all of them.

Examples:

- query legality depends on capability metadata
- relation modes depend on relation signatures
- emit legality depends on event descriptors
- state accesses depend on table/field schemas

### 5.2 Why one `Callable` set is better than separate vectors

MIR should not split top-level callables into three disconnected vectors.

Use:

```rust
pub struct Callable {
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: Body,
    pub effects: EffectSummary,
}
```

with:

```rust
pub enum CallableKind {
    Function,
    Query,
    Tx,
}
```

This is better because:

- `Function`, `Query`, and `Tx` are all bodies with parameters and returns
- effect policy can be checked uniformly
- inlining logic can refer to one callable universe
- diagnostics can still branch on `CallableKind`

Unlike canonical IR, MIR still needs `Function`.

---

## 6. Exact MIR Body Shape

MIR bodies should remain structured.

```rust
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub region: Region,
}
```

The root region is:

```rust
pub struct Region {
    pub ops: Vec<Op>,
}
```

### 6.1 Why `Region` exists

MIR needs a reusable structured container for:

- root bodies
- `if` arms
- `match` arms
- future bounded loop bodies

This is one of the strongest places to absorb MLIR's region thinking without
adopting MLIR itself.

### 6.2 Why MIR should not use CFG blocks

MIR may be richer than canonical IR, but it still should not become:

- arbitrary CFG
- block arguments everywhere
- dominance-sensitive machine IR

Structured regions are enough for the intended language.

---

## 7. Value Model

MIR should already use resolved value references.

```rust
pub enum ValueRef {
    Literal(LiteralValue),
    Param(ParamId),
    Context(ContextFieldId),
    Local(LocalId),
    Const(ConstId),
}
```

This intentionally matches the canonical value-source model closely.

That is good.

MIR should differ from canonical IR mainly in:

- control structure
- callable structure
- effect summaries
- and remaining compiler-owned normalization work

not by inventing a different value world.

### 7.1 Local declarations

```rust
pub struct LocalDecl {
    pub id: LocalId,
    pub symbol: Option<String>,
    pub ty: TypeRef,
}
```

Unlike canonical IR, MIR may still preserve optional local symbols because:

- diagnostics
- debugging
- and source mapping

still matter here.

---

## 8. Exact MIR Op Taxonomy

The recommended exact MIR op set is:

```rust
pub enum Op {
    // Pure total value binding
    BindValue {
        dst: LocalId,
        value: ValueOp,
    },

    // Checked / partial
    DivMod {
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },

    // State
    ReadState {
        dst_value: LocalId,
        dst_present: LocalId,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    WriteState {
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
        value: ValueRef,
    },
    DeleteState {
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    ReadStateProperty {
        dsts: Vec<LocalId>,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },

    // Assertions
    Assert {
        cond: ValueRef,
    },

    // Relations
    AssertRelation {
        relation: RelationId,
        args: ValueTupleRef,
    },
    EvalRelation {
        relation: RelationId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Capabilities
    CallCapability {
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Calls
    CallFunction {
        callee: CallableId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Output
    EmitEvent {
        event: EventId,
        args: ValueTupleRef,
    },

    // Structured control
    If {
        cond: ValueRef,
        then_region: Region,
        else_region: Region,
    },
    Match {
        scrutinee: ValueRef,
        arms: Vec<MatchArm>,
        default: Option<Region>,
    },

    // End
    Return {
        values: ValueTupleRef,
    },
}
```

### 8.1 Pure total value ops are nested under `BindValue`

Pure total value production should stay compact.

```rust
pub enum ValueOp {
    Arith {
        op: ArithOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Cmp {
        op: CmpOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Not {
        src: ValueRef,
    },
    And {
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Or {
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Select {
        cond: ValueRef,
        if_true: ValueRef,
        if_false: ValueRef,
    },
    Hash {
        family: HashFamily,
        inputs: ValueTupleRef,
    },
}
```

This makes the pure-total subset visually and structurally separate from:

- world effects
- proof-observable effects
- checked ops
- structured control

### 8.2 Why `CallFunction` remains in MIR

`Function` calls are still useful in MIR because MIR is where:

- inlining
- effect propagation
- and body normalization

should happen.

`CallFunction` must not survive into canonical IR.

### 8.3 Why `If` and `Match` remain first-class in MIR

This is the key difference from canonical IR.

MIR still needs:

- structured control regions
- region-local legality checks
- branch-local effect reasoning

before final predicated lowering.

---

## 9. Match Arms

The exact match-arm shape should be:

```rust
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub region: Region,
}
```

The initial `MatchPattern` should stay intentionally small:

```rust
pub enum MatchPattern {
    Literal(LiteralValue),
    Wildcard,
}
```

That is enough for:

- finite numeric dispatch
- enum-tag-like lowering
- later one-hot selector synthesis

without reopening full pattern matching.

---

## 10. Effect Summary Is Mandatory in MIR

MIR is the exact layer where callable effects become explicit.

```rust
pub struct EffectSummary {
    pub world: WorldEffects,
    pub proof: ProofEffects,
    pub may_fail: bool,
}
```

Recommended exact shapes:

```rust
pub struct WorldEffects {
    pub state_read: bool,
    pub state_write: bool,
    pub state_delete: bool,
    pub emit_event: bool,
}

pub struct ProofEffects {
    pub relation_use: bool,
    pub state_property_read: bool,
    pub capability_call: bool,
    pub builtin_hash: bool,
}
```

### 10.1 Why `builtin_hash` is here

Even though builtin `Hash` is a pure value op, tracking it in MIR summary is
still useful for:

- compiler accounting
- future optimization policy
- query diagnostics

It does **not** make `Hash` a world effect or guarded op.

It simply records that the callable uses the blessed builtin family.

### 10.2 Why summary is attached to callables, not every op

Per-op classification still exists in the op taxonomy.

But the summary is attached to the callable because it supports:

- query legality
- tx legality
- inlining
- effect propagation through `CallFunction`

---

## 11. Exact Callable Policy

### 11.1 `Function`

Functions are:

- internal
- callable from other MIR functions, queries, and txs
- not externally invokable
- inlined or otherwise erased before canonical IR

Functions may have broad effect summaries.

They are not required to be pure.

### 11.2 `Query`

Queries are:

- external
- read-only
- result-bearing

Queries may use:

- `BindValue`
- `DivMod`
- `ReadState`
- `ReadStateProperty`
- `Assert`
- `AssertRelation`
- `EvalRelation`
- builtin `Hash`
- query-safe `CallCapability`
- `CallFunction` only if the callee's inferred effect summary is query-legal
- `If`
- `Match`
- `Return`

Queries may not use:

- `WriteState`
- `DeleteState`
- `EmitEvent`
- tx-only `CallCapability`
- `CallFunction` to a function whose summary would violate query policy

### 11.3 `Tx`

Transactions may use any MIR op family that is otherwise valid.

### 11.4 Why policy belongs in MIR validation

This policy should be enforced:

- after name resolution
- after effect inference
- before canonical lowering

MIR is the only layer where all three are present together.

---

## 12. Lowering Contract to Canonical IR

This is the most important part of the note.

### 12.1 What must disappear before canonical IR

The following must not survive:

- `CallableKind::Function`
- `CallFunction`
- `If`
- `Match`
- optional local symbols

### 12.2 What must survive almost unchanged

The following should lower almost directly:

- `ReadState`
- `WriteState`
- `DeleteState`
- `ReadStateProperty`
- `Assert`
- `AssertRelation`
- `EvalRelation`
- `CallCapability`
- `EmitEvent`
- `Return`

### 12.3 Pure value lowering

`BindValue` lowers to one canonical value op per destination local.

This is close to a structural rewrite:

- `ValueOp::Arith` -> canonical `Arith`
- `ValueOp::Cmp` -> canonical `Cmp`
- `ValueOp::Not` -> canonical `Not`
- `ValueOp::And` -> canonical `And`
- `ValueOp::Or` -> canonical `Or`
- `ValueOp::Select` -> canonical `Select`
- `ValueOp::Hash` -> canonical `Hash`

### 12.4 Checked-op lowering

`DivMod` lowers directly to canonical `DivMod`.

When later reached under control lowering:

- canonical guard insertion happens there
- not in ordinary straight-line MIR

### 12.5 Function-call lowering

`CallFunction` should be eliminated by:

- inlining the callee body into the caller
- propagating the callee effect summary upward
- remapping locals and params hygienically

The preferred policy is:

- **no canonical function call op**

### 12.6 `If` lowering

`If` lowers by:

1. lowering the condition to a boolean local
2. synthesizing branch guards
3. lowering each region recursively
4. inserting guards onto the canonical guardable frontier
5. merging branch-produced pure values with canonical `Select`

This is where MIR becomes canonical predicated SSA.

### 12.7 `Match` lowering

`Match` lowers by:

1. synthesizing one boolean selector per arm
2. enforcing one-hot/exhaustive policy in lowering
3. lowering each arm region recursively
4. inserting guards on guardable/checked canonical ops
5. merging produced pure values through nested `Select` or one-hot value
   combination

### 12.8 Why MIR should not pre-encode guards

MIR should not itself carry canonical guards on ordinary ops.

That would:

- blur MIR and canonical IR
- make MIR less structured
- and force guard policy too early

Instead:

- MIR owns structured control
- canonical IR owns guards

That is the clean boundary.

---

## 13. Inlining Contract

Inlining should be treated as part of MIR normalization, not a later optional
optimization.

### 13.1 Why it belongs here

Canonical IR should not know about internal helpers.

So function elimination is not an optimization detail. It is part of the
required lowering contract.

### 13.2 Minimal exact contract

Inlining must:

- allocate fresh locals for the callee body
- map call inputs to callee params
- append the callee region into the caller region
- rewrite returned values into the call destination locals
- compose effect summaries transitively

### 13.3 What is allowed later

The compiler may eventually choose between:

- eager inlining
- normalization into a call-free MIR
- or a dedicated inlining pass

But by the time MIR lowers to canonical IR:

- no `Function`
- no `CallFunction`

may remain.

---

## 14. Validation Invariants

MIR validation should enforce at least the following.

### 14.1 Structural

- every callable kind is well-formed
- every region is well-formed
- every `Return` occurs only in a valid terminal position within a region
- `Match` arms use supported pattern forms

### 14.2 Reference resolution

- all referenced IDs exist
- all tables/fields exist in the state schema
- all relation IDs exist
- all capability IDs exist
- all event IDs exist
- all called functions exist and are `CallableKind::Function`

### 14.3 Typing

- all operands are type-correct
- destination locals have the correct declared types
- tuple arities match descriptor signatures
- `HashFamily` input/output rules hold

### 14.4 Effect and callable policy

- every callable has an inferred `EffectSummary`
- query bodies satisfy query legality
- tx bodies satisfy tx policy
- function bodies may be broad but are still summarized
- capability usage obeys descriptor metadata

### 14.5 Lowering readiness

- no MIR construct exists without a defined canonical lowering
- every `If` / `Match` is lowerable under the chosen guard frontier
- no op requires canonical CFG

---

## 15. Recommended Implementation Order

After canonical IR exact data model is frozen, the next implementation work
should be:

1. define exact MIR Rust data structures
2. define and test `EffectSummary`
3. implement MIR validation
4. implement function inlining / call elimination
5. implement MIR -> canonical IR lowering for straight-line V1
6. only then extend MIR lowering to `If` / `Match`

This sequencing matters.

If MIR exact lowering is not fixed before HIR grows richer, the middle-end will
start improvising.

---

## 16. What This Note Commits To

This note is intended to settle the following.

- MIR should use one `Program` root with one `Callable` universe.
- MIR should keep `Function`, `Query`, and `Tx` together as `CallableKind`.
- MIR should keep structured control through explicit `Region`, `If`, and
  `Match`.
- MIR should carry explicit `EffectSummary`.
- MIR should keep function calls only long enough to inline them away.
- MIR should lower to canonical IR without introducing CFG.
- Guards are introduced only during MIR -> canonical IR lowering, not earlier.
- MIR validation owns callable legality and lowering readiness.

With this note in place, the next natural design step is the exact HIR data
model that feeds this MIR contract.
