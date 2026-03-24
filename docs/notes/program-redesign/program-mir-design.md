# Program MIR Design

> **Status**: Proposed architecture note
> **Date**: 2026-03-24
> **Scope**: Defines the intended role, structure, normalization rules, and
> lowering responsibilities of Tabula's new MIR layer.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-design.md](program-hir-design.md),
> [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md),
> [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md),
> [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md)

---

## 1. Why This Note Exists

Once HIR exists, the next architectural question is not "how do we lower to the
final IR?" but:

> **what is the compiler's actual working language between source semantics and
> the canonical proof contract?**

That language is MIR.

Without MIR, Tabula would be forced into one of two bad shapes:

- either HIR becomes overloaded with compiler normalization responsibilities,
- or the canonical proof IR becomes bloated with source-shaped structure and
  frontend concerns.

MIR exists to prevent both failures.

---

## 2. Position in the Compiler Stack

The intended compiler stack is:

1. **AST**
   - parser-oriented surface tree
2. **HIR**
   - semantic source IR
3. **MIR**
   - normalized compiler IR
4. **canonical IR**
   - fixed-shape execution/proof contract

### 2.1 HIR versus MIR

HIR is still source-shaped.

MIR is compiler-shaped.

HIR preserves:

- declaration categories,
- lexical source structure,
- source-level control constructs,
- and local bindings by source name.

MIR should preserve:

- semantic meaning,
- declaration identity,
- structured control,
- effect structure,
- and type discipline,

while discarding:

- surface sugar,
- incidental syntactic distinctions,
- and direct dependence on source spelling.

### 2.2 MIR versus canonical IR

Canonical IR is small, stable, flat, proof-facing, and executor-facing.

MIR is not yet any of those things.

MIR is the last compiler-owned representation before canonicalization into the
final proof/execution contract. It is where the hard normalization work belongs.

---

## 3. Core Thesis

The ideal Tabula MIR is:

- **fully semantic**
- **fully resolved**
- **effect-structured**
- **region-capable**
- **compiler-normalized**
- **but still not proof-shaped**

That means:

- HIR owns source-like structure,
- MIR owns normalization and legality,
- canonical IR owns proof/execution contract shape.

The MIR layer is therefore the real middle-end of the redesigned compiler.

---

## 4. Why MIR Is Needed

### 4.1 HIR is too rich

HIR still preserves:

- source-style declarations,
- lexical names,
- structured statements,
- and rich language categories.

That is good for source semantics and diagnostics, but not for compiler passes
such as:

- inlining,
- constant propagation,
- control normalization,
- effect checking,
- and lowering feasibility analysis.

### 4.2 Canonical IR is too small

Canonical IR should remain:

- flat,
- SSA-based,
- CFG-free,
- fixed-shape,
- and very conservative in its operation set.

It should not be asked to carry:

- structured regions,
- source-like declaration references,
- local lexical environments,
- or partial frontend normalization.

### 4.3 The gap is real

Tabula's future feature set makes the gap larger, not smaller:

- constants,
- relations,
- context,
- queries,
- events,
- structured control,
- and later spec-layer constructs

all introduce real frontend structure that should not leak into the final proof
contract unchanged.

MIR is therefore not optional architecture. It is a structural necessity.

---

## 5. What MIR Is Responsible For

MIR should own the following responsibilities.

### 5.1 Symbol resolution complete

By MIR time, top-level references should no longer be stringly typed.

MIR should already resolve:

- table references,
- field references,
- constant references,
- relation references,
- event references,
- function references,
- query references,
- capability references.

The exact representation may be:

- IDs,
- declaration handles,
- interned references,
- or typed symbolic references.

But MIR should not depend on unresolved source names for semantics.

### 5.2 Typing complete enough for lowering

By MIR time, enough type information must be known to support:

- operation legality,
- relation mode checking,
- event argument checking,
- query and tx discipline,
- and later canonical IR lowering.

MIR may still allow some later canonical typing work, but it should not remain
"mostly inferred parser output".

### 5.3 Surface sugar removed

MIR is where source sugar should disappear.

Examples:

- `x in AllowedTier` -> relation assertion form
- functional relation sugar -> explicit relation evaluation form
- syntactic convenience around simple patterns -> normalized binding form
- parser-level operator quirks -> canonical op forms

This makes MIR the first good place for true compiler reasoning.

### 5.4 Effects explicit

This is one of MIR's most important jobs.

MIR should classify operations by semantic effect kind:

- pure value computation
- state read
- state write
- relation assertion
- relation evaluation
- capability call
- event emission
- return
- control region

This classification matters for:

- legality checks,
- read-only enforcement,
- branch lowering,
- query discipline,
- and eventual journal/proof reasoning.

MIR should also be the first layer where callable bodies have an explicit effect
summary computed or inferred by the compiler.

### 5.5 Structured control preserved

MIR should still preserve structured control as explicit regions:

- `IfRegion`
- `MatchRegion`
- later bounded `ForRegion`

This is where Tabula should absorb MLIR's region mindset most strongly.

### 5.6 Callable effect policy enforced

MIR is the natural place to enforce the distinction between:

- internal helper functions,
- read-only external queries,
- and mutating transactions.

That means MIR should be able to answer not only "what ops exist?" but also:

- whether a callable is read-only in the world-effect sense,
- whether it performs proof-observable semantic work,
- and whether it may fail.

### 5.7 Lowering feasibility checked

MIR is where the compiler should be able to answer questions such as:

- is this branch pure or effectful?
- can this relation use be lowered legally?
- is this query read-only?
- does this tx body violate phase discipline?
- can this control region be lowered to predicated canonical IR?

This is too late for HIR and too early for canonical IR. MIR is the correct
layer.

---

## 6. What MIR Is Not Responsible For

### 6.1 Not canonical proof shape

MIR should not yet encode:

- flat slot numbering,
- explicit SSA temporaries,
- `select`-level predication,
- one-hot selector materialization,
- guarded canonical effect ops,
- or final executor instruction order.

### 6.2 Not parser-facing source presentation

MIR should not preserve:

- all original name spellings for semantics,
- source-level syntactic distinctions that normalize away,
- or parser incidental structure.

Spans and user-facing metadata may remain for diagnostics, but MIR is not a
presentation tree.

### 6.3 Not generic CFG

MIR should remain region-based and structured.

It should not introduce:

- arbitrary basic blocks,
- generic jump graphs,
- or branch terminators as canonical middle-end structure.

This keeps MIR aligned with the later predicated lowering strategy.

### 6.4 Not runtime/prover orchestration

MIR should not know about:

- proof slots,
- trace rows,
- witness stores,
- journals,
- proof-plan indices,
- or backend preparation.

That belongs downstream.

---

## 7. MLIR Concepts to Absorb

MIR is the layer where MLIR-style ideas are most useful.

### 7.1 Region-first middle-end structure

MLIR's biggest lesson for Tabula MIR is:

- preserve high-level structure in the middle-end,
- normalize within that structure,
- and lower only when a later layer truly needs it.

This fits Tabula extremely well.

MIR should therefore strongly adopt:

- region-owning control operations,
- nested region bodies,
- and structure-preserving normalization.

### 7.2 Operation categories

MLIR's operation-centric worldview is useful here, but Tabula should use typed
Rust node families rather than a maximally generic op/attribute system.

Examples of MIR operation categories:

- value ops
- effect ops
- control ops
- terminator-like body-ending ops

### 7.3 Symbol references

MIR should continue the HIR transition toward symbolic declaration references.

This is especially useful for:

- `ConstRef`
- `RelationRef`
- `EventRef`
- `CallableRef`
- `CapabilityRef`

### 7.4 Region-based control, not CFG

The MLIR `scf` family is a strong conceptual reference:

- `if` as a region operation
- finite dispatch as a region operation
- loops as structured region operations

This is precisely the right mindset for Tabula MIR, where structured control
must survive long enough for legality analysis and later predicated lowering.

---

## 8. MLIR Concepts to Exclude

### 8.1 No generic dialect machinery

Tabula MIR should not become a generic dialect engine or a macro-system for
operation definitions. The project needs clear semantic node types, not a
tooling dependency on MLIR infrastructure.

### 8.2 No multi-block generic regions as the default

Structured regions are useful. Arbitrary CFG regions are not the right default
for this language and this proof model.

### 8.3 No middle-end dependence on SSA dominance

MIR should be able to reason about values and effects without requiring the
entire program to adopt SSA dominance or phi semantics. Those concerns belong
later, if at all.

---

## 9. MIR Design Principles

### 9.1 Normalize semantics, not just syntax

MIR should remove syntactic accidents while preserving semantic distinctions.

That means:

- `const` remains distinct from local values,
- relation uses remain distinct from ordinary calls,
- queries remain distinct from functions,
- transactions remain distinct from queries.

### 9.2 Make effects first-class

If HIR preserves source structure, MIR should preserve effect structure.

This makes later control lowering and proof-friendly canonicalization far
cleaner.

### 9.3 Delay proof-shaping

MIR should prepare for canonical lowering without itself becoming canonical IR.

That means region-based control and effect-aware normalization are good.
Canonical predication is not yet MIR's job.

### 9.4 Be the compiler's workbench

Most semantic compiler work should happen here:

- inlining,
- constant propagation,
- relation mode checking,
- read-only enforcement,
- branch simplification,
- dead local elimination,
- and lowering feasibility.

---

## 10. MIR Program Object Model

MIR should still preserve the whole-program shape, but in a more compiler-ready
form than HIR.

```rust
pub struct Program {
    pub program_id: ProgramId,
    pub uses: Vec<Use>,
    pub state: Option<StateDecl>,
    pub context: Option<ContextDecl>,
    pub consts: Vec<ConstDecl>,
    pub relations: Vec<RelationDecl>,
    pub events: Vec<EventDecl>,
    pub predicates: Vec<PredicateDecl>,
    pub invariants: Vec<InvariantDecl>,
    pub functions: Vec<FunctionDef>,
    pub queries: Vec<QueryDef>,
    pub transactions: Vec<TxDef>,
}
```

The exact container shape may differ, but the program should still behave like
a resolved semantic module, not yet like a flattened instruction bundle.

The key difference from HIR is not that declarations disappear. It is that they
are now normalized and compiler-ready.

---

## 11. MIR Declaration Layer

### 11.1 Resolved declarations

By MIR, declaration signatures should be fully normalized:

- table keys and fields
- constant type and evaluated constant form or constant expression plan
- relation signature and semantic body kind
- event signature
- callable signatures

Examples:

```rust
pub struct RelationDecl {
    pub relation_id: RelationId,
    pub params: Vec<Param>,
    pub results: Vec<Param>,
    pub body: RelationBody,
}

pub enum RelationBody {
    Enum(Vec<ConstValue>),
    Range { lower: ConstValue, upper: ConstValue },
    Map(Vec<MapEntry>),
    Set(Vec<TupleConstValue>),
    Extern,
}
```

Constant-folding of relation bodies may already begin here if the source
relation constructors are compile-time evaluable.

### 11.2 Callable categories remain distinct

Transactions, queries, and helper functions should remain distinct MIR
declaration kinds, even if their body model becomes more uniform.

That distinction matters for:

- effect discipline,
- external interface generation,
- query proof planning,
- and runtime API generation.

They should also be the natural attachment point for inferred effect summaries.

Conceptually:

```rust
pub struct QueryDef {
    pub query_id: QueryId,
    pub body: Body,
    pub effects: EffectSummary,
}

pub struct TxDef {
    pub tx_id: TxId,
    pub body: Body,
    pub effects: EffectSummary,
}
```

The exact field placement may vary, but MIR should make the summary explicit
somewhere in the callable definition graph.

---

## 12. MIR Body Model

MIR bodies should be region-based and operation-based.

```rust
pub struct Body {
    pub params: Vec<LocalId>,
    pub ops: Vec<Op>,
    pub result: BodyResult,
    pub effects: EffectSummary,
}
```

The exact shape can vary, but three properties matter:

1. bodies are no longer source statement trees,
2. bodies are still structured and region-capable,
3. bodies are not yet flat canonical IR sequences.

### 12.1 Locals in MIR

MIR may still use named or numbered locals, but they are now compiler locals,
not source-local strings.

This means:

- source names may remain for diagnostics,
- but local identity is now compiler-owned.

MIR locals are therefore a step toward later SSA lowering without already being
SSA.

---

## 13. MIR Operation Taxonomy

The MIR operation set should be grouped by semantic role.

### 13.1 Value operations

Pure value-producing operations such as:

- literal materialization
- local move/copy
- unary arithmetic or boolean ops
- binary arithmetic or comparison ops
- tuple construction or projection where needed
- `select`
- constant reference

These have no semantic side effects.

### 13.2 State operations

- `ReadState`
- `WriteState`

These should be explicit MIR operations, not generic calls or lvalues hidden in
expressions.

### 13.3 Relation operations

- `AssertRelation`
- `EvalRelation`

These should remain explicit. MIR is too late to still pretend relation use is
just a generic call.

### 13.4 Capability operations

- `CallCapability`
- maybe explicit builtin families such as `Hash` if the canonical IR keeps
  them separate

### 13.5 Event operations

- `EmitEvent`

### 13.6 Control operations

- `IfRegion`
- `MatchRegion`
- later `ForRegion`

### 13.7 Terminator-like operations

Depending on body representation, MIR may need explicit body-ending operations
such as:

- `Return`
- `Abort`

These are not yet CFG terminators. They are just explicit body-ending semantic
ops.

---

## 14. Proposed MIR Node Sketch

The following is a plausible pseudo-Rust shape.

```rust
pub enum Op {
    BindValue(BindValueOp),
    ReadState(ReadStateOp),
    WriteState(WriteStateOp),
    Assert(AssertOp),
    AssertRelation(AssertRelationOp),
    EvalRelation(EvalRelationOp),
    CallCapability(CallCapabilityOp),
    EmitEvent(EmitEventOp),
    IfRegion(IfRegionOp),
    MatchRegion(MatchRegionOp),
    ForRegion(ForRegionOp),
    Return(ReturnOp),
}
```

Where:

```rust
pub struct BindValueOp {
    pub dst: LocalId,
    pub expr: ValueExpr,
}

pub struct IfRegionOp {
    pub condition: LocalRef,
    pub then_body: Region,
    pub else_body: Option<Region>,
}

pub struct MatchRegionOp {
    pub scrutinee: LocalRef,
    pub arms: Vec<MatchArm>,
}
```

This is only a sketch, but it captures the right taxonomy:

- pure value binding,
- explicit effects,
- structured control as region ops.

---

## 15. Value Model in MIR

MIR needs a clearer value model than HIR.

### 15.1 Explicit local references

HIR can still talk in source-like names.

MIR should use compiler-owned local references:

- `LocalId`
- `ConstRef`
- `ParamRef`
- `ContextRef`

This is one of the biggest transitions from source semantics to compiler
semantics.

### 15.2 Constants as explicit value sources

Constants should already be distinct in MIR:

- `ConstRef(ConstId)`

They are not locals, not state reads, and not generic symbols.

### 15.3 Table access is already an operation

By MIR, table access should no longer be encoded only as nested postfix syntax.

It should have become:

- a state read op,
- or a state write target in an explicit state effect op.

This is a crucial normalization boundary.

---

## 16. Control Regions in MIR

MIR should strongly preserve control as regions.

### 16.1 If regions

An if region should carry:

- normalized condition
- then region
- else region

This is the right form for:

- branch legality checking,
- branch-local effect analysis,
- and later predicated lowering.

### 16.2 Match regions

A match region should carry:

- normalized scrutinee
- normalized arm patterns
- normalized arm regions

This form is particularly useful because `match` will likely lower to one-hot
selectors later. MIR is the perfect layer to preserve its finite structured
meaning before canonicalization.

### 16.3 For regions

If bounded loops are added later, MIR should carry them in a structured form
long enough to:

- verify static boundedness,
- compute cost estimates,
- and choose an appropriate lowering strategy.

The region model therefore needs to reserve this possibility from the start,
even if V1 and V2 do not use it.

---

## 17. Effect Model

The effect model is one of MIR's most important architectural roles.

### 17.1 Why MIR must make effects explicit

Later canonical lowering depends on knowing:

- which ops are world-interacting,
- which ops are proof-observable,
- which ops may fail or require checked treatment,
- and which ops may require guarded lowering under control flow.

Without explicit MIR effects, branch lowering becomes brittle and canonical IR
extension becomes ad hoc.

### 17.2 Suggested effect axes

MIR should not use a single pure/impure bit.

Instead, it should distinguish:

- **world effects**
- **proof-observable semantic effects**
- **failure or checked behavior**

Pure value binding remains separate from all three.

### 17.3 Suggested world effects

At minimum:

- `StateRead`
- `StateWrite`
- `StateDelete`
- `EmitEvent`

### 17.4 Suggested proof-observable effects

At minimum:

- `RelationAssert`
- `RelationEval`
- `StatePropertyRead`
- `CapabilityCall`
- maybe `Hash`

### 17.5 Failure or checked behavior

MIR should also track whether a body may fail.

Examples:

- `Assert`
- checked arithmetic such as `DivMod`
- checked capabilities
- partial relation evaluation if it ever exists

This may be modeled as a coarse `may_fail` flag rather than a large enum.

### 17.6 Recommended summary shape

The exact Rust shape may differ, but MIR should conceptually compute something
like:

```rust
pub struct EffectSummary {
    pub world: WorldEffectSet,
    pub proof: ProofEffectSet,
    pub may_fail: bool,
}
```

This summary should attach to:

- `FunctionDef`
- `QueryDef`
- `TxDef`

and to normalized bodies where helpful.

### 17.7 Query and tx discipline

Queries and transactions differ primarily in effect discipline.

MIR should make it easy to enforce:

- queries may read state but not write it,
- queries may use relations,
- queries may read state properties,
- queries may call only query-safe deterministic capabilities,
- queries may not emit events,
- transactions may write,
- helper functions inherit the effect constraints of their call context or of
  their own declared discipline.

MIR is the natural place for that checking.

This is also why `query` should be described as read-only rather than pure.

---

## 18. Normalization Responsibilities

MIR is where the compiler should perform the following major normalizations.

### 18.1 Desugaring

Examples:

- relation sugar to explicit relation ops
- query or tx clauses into normalized metadata fields
- pattern sugar into explicit local bindings

### 18.2 Local binding normalization

HIR lexical bindings should become stable compiler locals in MIR.

This does not yet require SSA, but it does require:

- compiler-owned local identity,
- and normalized local use structure.

### 18.3 Inlining or helper expansion

The future canonical IR should remain small and should likely avoid a rich call
model. MIR is the right place to inline helper functions or normalize calls
into an implementation form that can later lower cleanly.

### 18.4 Branch simplification

MIR is where the compiler can simplify:

- constant conditions,
- trivially empty branches,
- degenerate matches,
- and other structured-control simplifications

without yet flattening the entire body into canonical IR.

### 18.5 Constant propagation

MIR is the right place to do constant propagation over:

- local pure computations,
- relation constructor results where possible,
- query preconditions,
- and branch simplifications.

### 18.6 Dead local cleanup

Once sugar and local structure are normalized, MIR is also the natural place to
perform cleanup of unused locals and trivially dead pure computations.

---

## 19. MIR Validation Rules

MIR should enforce invariants that are stronger than HIR but weaker than
canonical IR.

Examples:

- all top-level references resolved
- all local references defined
- explicit effect discipline respected
- queries do not perform disallowed effects
- non-functional relations not used in evaluation mode
- event emission references a known event declaration
- region structure is internally well-formed
- loop bounds are explicit enough for the currently allowed lowering policy

These are middle-end invariants. They do not belong in the parser, and they are
too semantic to defer to canonical IR validation.

---

## 20. MIR to Canonical IR Boundary

MIR should hand off to canonical IR only after the compiler can answer:

- what the exact effectful operations are,
- what the control regions are,
- which regions are legal to lower,
- how relation and capability uses are typed,
- and which locals/values survive to canonical execution.

The MIR -> canonical IR lowering is then responsible for:

- SSA conversion,
- flattening,
- selector synthesis,
- one-hot match lowering,
- guarded effect lowering,
- and final canonical op scheduling.

This boundary is the central reason MIR exists.

Because MIR now owns effect summaries, this boundary should consume:

- normalized body structure,
- explicit effect summaries,
- and callable-category policy

when deciding whether lowering into canonical IR is legal.

---

## 21. Example

Given:

```tabula
tx register(id: UserId, tier: u8) {
  assert relation AllowedTier(tier);
  users[id].active = true;
  users[id].tier = tier;
}
```

HIR is still source-shaped.

MIR should look more like:

```text
TxDef register(params = [id, tier])
  ops:
    AssertRelation {
      relation = AllowedTier,
      args = [Local(tier)]
    }
    WriteState {
      table = users,
      key = [Local(id)],
      field = active,
      value = Literal(true)
    }
    WriteState {
      table = users,
      key = [Local(id)],
      field = tier,
      value = Local(tier)
    }
    Return
```

This is already:

- resolved,
- effect-structured,
- and compiler-friendly,

but still not:

- flat SSA,
- predicated,
- or canonical proof IR.

---

## 22. Implementation Guidance

### 22.1 MIR should be a first-class module, not incidental compiler glue

MIR deserves explicit documentation, tests, and ownership. It should not emerge
accidentally as whatever shape the lowering pass happens to use internally.

### 22.2 Keep MIR explicit and typed

MIR should use explicit Rust data structures and operation families. Avoid
stringly references, generic "node blobs", or parser-ish trees.

### 22.3 Be conservative with genericity

Tabula benefits more from semantically explicit node kinds than from a generic
operation framework. MIR should be structured enough for compiler work without
losing the language's major semantic distinctions.

### 22.4 Reserve structured control from the start

Even if V1 implementation supports only straight-line bodies, MIR should still
be designed so that:

- `IfRegion`
- `MatchRegion`
- later `ForRegion`

fit naturally into the model. That prevents another structural rewrite when V3
arrives.

---

## 23. What This Note Commits To

This note is intended to settle the following design commitments.

- MIR is a required new compiler layer.
- MIR is the normalization and effect-structuring layer.
- MIR remains structured and region-based.
- MIR is symbol-resolved and type-resolved enough for lowering.
- MIR is where explicit state/relation/capability/event operations appear.
- MIR is where callable effect summaries are inferred and enforced.
- MIR should borrow MLIR's region-centric middle-end ideas.
- MIR should not become generic CFG IR.
- MIR should not yet become canonical proof IR.

With this note in place, the next step is the exact MIR contract and data model,
followed by the exact HIR contract that feeds it.
