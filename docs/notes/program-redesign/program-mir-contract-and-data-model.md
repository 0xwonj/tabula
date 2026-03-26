# Program MIR Contract and Data Model

> **Status**: Implemented implementation contract
> **Date**: 2026-03-25
> **Scope**: Defines the exact MIR contract used as Tabula's compiler-owned
> middle-end, together with the exact Rust data model, validation invariants,
> effect summary model, and MIR -> canonical IR lowering contract.
> **Related**: [program-mir-design.md](program-mir-design.md),
> [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md)

---

## 1. Why This Note Exists

MIR is now the compiler's exact target between HIR and canonical IR.

That means this note must answer concrete implementation questions:

- what Rust types represent MIR
- what is allowed to remain in MIR
- what must disappear before canonical IR
- what MIR analyses are authoritative
- what validation MIR performs
- and exactly how MIR lowers into canonical IR

This note is therefore the implementation authority for `compiler::mir`.

---

## 2. Fixed Assumptions

This note does not reopen the following.

- compiler layering is `AST -> HIR -> MIR -> canonical IR`
- canonical IR is already fixed as:
  - flat
  - typed
  - SSA-disciplined
  - CFG-free
  - portable semantic contract
- runtime owns:
  - `RuntimeProgram { execution, proof }`
  - `ResolvedExecutionProgram`
  - `ResolvedProofProgram`
- executor consumes resolved execution contracts, not raw compiler MIR
- builtin `Hash` is a pure total value op, not a journaled effect family
- capability semantics are fixed:
  - `Checked` capability failure is semantic failure
  - `Total` capability failure is host/runtime contract violation

MIR is therefore designed against a closed lower boundary.

---

## 3. Exact MIR Role

MIR is:

- the last compiler-owned representation
- the first fully normalized compiler representation
- the first layer with an explicit verification/analysis boundary
- the last layer that keeps structured control
- the layer where function inlining and control normalization happen

MIR is not:

- source-shaped like HIR
- proof-shaped like canonical IR
- CFG-SSA
- a second executor IR

The exact design target is:

- **single-assignment ANF**
- **region-based control**
- **single-block regions**
- **explicit region results**
- **no CFG**
- **no phi**
- **no guards**

---

## 4. Current MIR Scope

The exact MIR contract now covers the rewritten core and boundary surface that
the compiler executes end-to-end today.

Included:

- `Function`
- `Query`
- `Tx`
- `If`
- `Match`

Not in the exact MIR contract:

- `For`
- `Predicate`
- `Invariant`

Those remain architecture-level or HIR-level reserved vocabulary for later
phases. MIR should not carry disabled surface futures in its exact contract.

---

## 5. Exact Rust Root Shape

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

pub struct Callable {
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: Body,
}

pub enum CallableKind {
    Function,
    Query,
    Tx,
}
```

This is the correct root shape because:

- MIR sees the same semantic universe as canonical IR
- callable graph reasoning is simpler with one `Vec<Callable>`
- inlining and query legality are both defined over one callable universe

Canonical IR removes `Function`. MIR does not.

---

## 6. Exact Body and Region Shape

```rust
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub region: Region,
}

pub struct LocalDecl {
    pub id: LocalId,
    pub symbol: Option<String>,
    pub ty: TypeRef,
}

pub struct Region {
    pub ops: Vec<Op>,
    pub terminator: Terminator,
}

pub enum Terminator {
    Yield { values: ValueTupleRef },
    Return { values: ValueTupleRef },
}
```

The exact control rule is:

- root callable region must terminate with `Return`
- nested `if`/`match` regions must terminate with `Yield`

`Return` and `Yield` are terminators, not ordinary ops.

This is the cleanest way to combine:

- ANF local sequencing
- region-based control
- explicit region results
- value-producing branches

without inventing CFG.

---

## 7. Exact Value Model

MIR already uses resolved value references.

```rust
pub enum ValueRef {
    Literal(LiteralValue),
    Param(ParamId),
    Context(ContextFieldId),
    Local(LocalId),
    Const(ConstId),
}
```

This intentionally mirrors canonical IR's value-source model. MIR differs from
canonical IR mainly in:

- `Function`
- `CallFunction`
- structured control
- region results
- explicit compiler analysis results

`LocalId` is single-assignment.

- a local is defined exactly once
- re-assignment is not allowed
- branch-produced values flow through region `Yield` results and control-op
  destination locals

---

## 8. Exact Op Taxonomy

Pure value computation is grouped under `BindValue`.

```rust
pub enum Op {
    BindValue { dst: LocalId, value: ValueOp },

    DivMod { dst_q: LocalId, dst_r: LocalId, lhs: ValueRef, rhs: ValueRef },

    ReadState { dst_value: LocalId, dst_present: LocalId, table: TableId, key: ValueTupleRef, field: FieldId },
    WriteState { table: TableId, key: ValueTupleRef, field: FieldId, value: ValueRef },
    DeleteState { table: TableId, key: ValueTupleRef, field: FieldId },
    ReadStateProperty { dsts: Vec<LocalId>, table: TableId, field: FieldId, query: StatePropertyQuery },

    Assert { cond: ValueRef },

    AssertRelation { relation: RelationId, args: ValueTupleRef },
    EvalRelation { relation: RelationId, inputs: ValueTupleRef, dsts: Vec<LocalId> },

    CallCapability { capability: CapabilityId, inputs: ValueTupleRef, dsts: Vec<LocalId> },
    CallFunction { callee: CallableId, inputs: ValueTupleRef, dsts: Vec<LocalId> },

    EmitEvent { event: EventId, args: ValueTupleRef },

    If { dsts: Vec<LocalId>, cond: ValueRef, then_region: Region, else_region: Region },
    Match { dsts: Vec<LocalId>, scrutinee: ValueRef, arms: Vec<MatchArm>, default: Option<Region> },
}
```

```rust
pub enum ValueOp {
    Arith { op: ArithOp, lhs: ValueRef, rhs: ValueRef },
    Cmp { op: CmpOp, lhs: ValueRef, rhs: ValueRef },
    Not { src: ValueRef },
    And { lhs: ValueRef, rhs: ValueRef },
    Or { lhs: ValueRef, rhs: ValueRef },
    Select { cond: ValueRef, if_true: ValueRef, if_false: ValueRef },
    Hash { family: HashFamily, inputs: ValueTupleRef },
}
```

Key points:

- `Hash` is a `ValueOp`
- `Hash` is pure, total, and builtin
- `Hash` is not an effect family
- `CallFunction` exists only in MIR
- `If` and `Match` are value-producing control ops
- `dsts` may be empty, so effect-only branches are allowed

Exact current match patterns are:

```rust
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub region: Region,
}

pub enum MatchPattern {
    Literal(LiteralValue),
    Wildcard,
}
```

---

## 9. Exact MIR Analysis Shape

Raw MIR is structural only. Derived metadata such as effect summaries and call
graphs live in the analyzed wrapper, not in `Callable` payload.

```rust
pub struct VerifiedProgram(Program);

pub struct AnalyzedProgram {
    pub verified: VerifiedProgram,
    pub analysis: ProgramAnalysis,
}

pub struct ProgramAnalysis {
    pub effect_summaries: BTreeMap<CallableId, EffectSummary>,
    pub failure_summaries: BTreeMap<CallableId, FailureSummary>,
    pub policy_summaries: BTreeMap<CallableId, PolicySummary>,
    pub context_demands: BTreeMap<CallableId, ContextDemandSummary>,
    pub call_graph: BTreeMap<CallableId, BTreeSet<CallableId>>,
}

pub struct EffectSummary {
    pub world: WorldEffects,
    pub proof: ProofEffects,
}

pub struct FailureSummary {
    pub semantic_may_fail: bool,
    pub host_contract_sensitive: bool,
}

pub struct PolicySummary {
    pub uses_builtin_hash: bool,
    pub uses_tx_only_capability: bool,
    pub uses_query_safe_capability: bool,
    pub uses_journaled_capability: bool,
    pub uses_opaque_runtime_capability: bool,
}

pub struct ContextDemandSummary {
    pub fields: BTreeSet<ContextFieldId>,
}

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
}
```

The meanings are:

`EffectSummary`

- world mutation and read surface only
- proof-observable operation families only

`FailureSummary`

- `semantic_may_fail`
  - assertion failure
  - `DivMod`
  - relation assertion failure
  - relation evaluation failure for current bindings
  - checked capability failure
- `host_contract_sensitive`
  - total capability use
  - means lowering targets a runtime/host contract, not semantic failure

`PolicySummary`

- `uses_builtin_hash`
  - builtin hash use only
  - analysis bit, not effect family
- `uses_tx_only_capability`
  - makes a callable graph query-illegal
- `uses_query_safe_capability`
  - records query-safe capability dependence without turning it into a world effect
- `uses_journaled_capability` / `uses_opaque_runtime_capability`
  - preserve capability proof-visibility facts as MIR policy metadata

`ContextDemandSummary`

- records which public context fields a callable may read
- is merged transitively across helper calls
- stays separate from effects/policy/failure because context use is a demand,
  not a world mutation or proof event family

This split is intentional.

- raw MIR owns structure
- verification owns structural/type/region invariants
- analysis owns effect summaries, call graph, and query policy checking

This follows the intended MLIR-like pass separation:

- structural IR payload
- verifier
- analyses
- normalization
- conversion

---

## 10. Capability Semantics in MIR

Capability metadata is already semantic in MIR.

- `CapabilityQueryPolicy`
  - contributes to query legality
- `CapabilityTotality`
  - contributes to failure summary
- `CapabilityProofVisibility`
  - visible as metadata
  - but proof visibility filtering remains runtime-owned, not MIR-owned

Exact totality interpretation:

- `Checked`
  - contributes to `semantic_may_fail`
- `Total`
  - does not contribute to `semantic_may_fail`
  - contributes to `host_contract_sensitive`

This matches the fixed lower boundary used by canonical IR, runtime, and
executor.

---

## 11. Query Legality

MIR is where query legality becomes compiler-exact.

Allowed in queries:

- pure value ops
- `DivMod`
- `ReadState`
- `ReadStateProperty`
- `Assert`
- `AssertRelation`
- `EvalRelation`
- builtin `Hash`
- query-safe `CallCapability`
- `CallFunction` only if callee summary is query-legal
- `If`
- `Match`

Forbidden in queries:

- `WriteState`
- `DeleteState`
- `EmitEvent`
- tx-only capability
- `CallFunction` whose reachable summary violates query policy

This check happens in MIR analysis after structural verification.

It is intentionally not encoded as a raw `EffectSummary` bit, because query
legality depends on both effect and policy facts. The implemented analysis
therefore derives query legality from:

- read-only `EffectSummary.world`
- absence of `PolicySummary.uses_tx_only_capability`
- verifier-enforced callable-category rules

This keeps query legality analysis-derived without pretending that callable
policy is just another effect bit.

- capability query policy
- helper call graph reachability
- callable kind

and not just on effect-family presence.

---

## 12. Lowering Contract to Canonical IR

The following survive structurally into canonical IR:

- `BindValue(ValueOp::Arith/Cmp/Not/And/Or/Select/Hash)`
- `DivMod`
- state ops
- relation ops
- `CallCapability`
- `EmitEvent`

The following must disappear before canonical IR:

- `CallableKind::Function`
- `CallFunction`
- `If`
- `Match`
- optional MIR local symbols

### 12.1 Function elimination

`CallFunction` is removed by MIR normalization via inlining.

- callee params are bound from caller inputs
- callee locals are hygienically re-bound
- analyzed MIR is re-verified and re-analyzed after inlining
- canonical IR never sees a function call op

### 12.2 Control lowering

MIR does not carry guard fields.

Instead:

- branch regions are recursively lowered
- canonical guards are introduced only during MIR -> canonical lowering
- only effectful or checked canonical ops receive guards
- yielded branch values are merged with canonical `Select`

This is the correct split:

- MIR owns structured control and region results
- canonical IR owns guard insertion and flat predicated form

---

## 13. Validation Invariants

The exact MIR validator must enforce:

- unique callable IDs
- unique param IDs per callable
- unique local IDs per callable
- root region terminator is `Return`
- nested region terminator is `Yield`
- every local is assigned exactly once
- every used local is defined earlier in region order
- tx callables do not declare explicit returns
- `If` result arity matches both arm yields
- `Match` result arity matches all arm/default yields
- literal match pattern type matches scrutinee
- wildcard arm occurs at most once and only last
- wildcard and default do not coexist
- value-producing match must be exhaustive via wildcard or default
- `CallFunction` target exists and is `CallableKind::Function`
- tuple arities match table keys, relations, capabilities, and events
- every MIR construct has a defined canonical lowering

The MIR analysis phase must additionally enforce:

- exact `EffectSummary` inference
- call graph construction
- query legality over reachable helpers and capability policy

---

## 14. Exact Lowering Target

MIR lowers to the already-frozen lower boundary:

- canonical `Program`
- canonical `ValidatedProgram`
- runtime `RuntimeProgram { execution, proof }`
- executor `ResolvedExecutionProgram`
- runtime `ResolvedProofProgram`

That means MIR should be designed for the current closed semantics of:

- builtin hash
- capability totality
- runtime-owned proof visibility filtering
- executor-owned semantic journaling

MIR is not allowed to assume a looser or more abstract lower boundary than the
one already implemented.

---

## 15. Implementation Order

The intended implementation order is:

1. exact MIR data model
2. structural verifier
3. MIR analysis
4. `CallFunction` inlining
5. re-verification and re-analysis
6. straight-line MIR -> canonical IR lowering
7. `If` lowering
8. `Match` lowering

That order keeps MIR grounded in the real canonical/runtime/executor target.

---

## 16. Final Recommendation

The exact current MIR should remain:

- core-only
- single-assignment
- ANF
- region-based
- non-CFG
- analysis-backed
- canonical-IR-targeting

This is the smallest structure that is:

- richer than HIR where compiler work actually needs help
- richer than canonical IR where control normalization still matters
- but not so rich that Tabula accidentally creates a second low-level IR.
