# Program HIR Contract and Data Model

> **Status**: Proposed implementation contract
> **Date**: 2026-03-24
> **Scope**: Defines the exact HIR contract that should be used as the
> immediate frontend-semantic target, together with the recommended Rust data
> model, the HIR -> MIR lowering contract, and the frontend skeleton that
> should build it.
> **Related**: [program-hir-design.md](program-hir-design.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md)

---

## 1. Why This Note Exists

The HIR architecture note explains:

- what HIR is for
- what HIR should preserve
- and why it should be structured, symbol-based, and region-based

That is still not enough to implement the frontend rewrite.

The rewrite now needs one exact HIR contract that answers:

- what Rust types should represent HIR
- which semantic distinctions must already be explicit in HIR
- what HIR validates before MIR exists
- how HIR lowers into exact MIR
- and what frontend skeleton should build and validate it

This note freezes that exact contract.

---

## 2. What This Note Assumes Is Already Fixed

This note does not reopen the following.

- Tabula is a closed-world `program` language.
- The compiler layering is `AST -> HIR -> MIR -> canonical IR`.
- The grammar note remains the source of truth for concrete surface syntax.
- MIR is already fixed as:
  - fully resolved enough for lowering
  - effect-explicit
  - structured
  - function-bearing
  - and canonical-IR-targeting
- Canonical IR is already fixed as:
  - flat
  - SSA-disciplined
  - CFG-free
  - executor/prover-facing
- The typing/effect system and final seam decisions already hold.

HIR is therefore designed:

- against a fixed grammar target
- and against a fixed MIR target

not as an open semantic playground.

---

## 3. Exact Role of HIR

HIR is:

- the first semantic program representation
- the last strongly source-shaped representation
- the layer where declaration categories are still explicit
- the layer where lexical bindings still matter
- and the layer where generic surface forms are semantically classified

HIR is **not**:

- parser syntax
- generic name trees
- SSA
- effect-summary carrier
- canonical executor/prover IR

The best short definition is:

> **HIR is the resolved source-semantic contract that feeds MIR.**

---

## 4. Naming Recommendation

Inside the eventual `hir` module, the preferred type names are:

- `hir::Program`
- `hir::Item`
- `hir::CallableDecl`
- `hir::Body`
- `hir::Region`
- `hir::Stmt`
- `hir::Expr`

not:

- `HirProgram`
- `HirExpr`
- `HirStmt`

Layer identity should come from the module path.

This keeps naming aligned across:

- `ast::Program`
- `hir::Program`
- `mir::Program`
- `ir::Program`

---

## 5. Exact HIR Root Shape

The recommended exact HIR root is:

```rust
pub struct Program {
    pub program_id: ProgramId,
    pub symbol: String,
    pub uses: Vec<UseDecl>,
    pub state: Option<StateDecl>,
    pub context: Option<ContextDecl>,
    pub items: Vec<Item>,
}
```

This root should preserve:

- one program symbol
- optional unique program-scoped blocks (`state`, `context`)
- ordered top-level items
- use/import declarations

### 5.1 Why `state` and `context` stay separate from `items`

`state` and `context` are not ordinary repeatable declarations.

They are:

- unique scoped blocks
- structurally privileged
- and semantically upstream of most body validation

Keeping them separate makes frontend validation simpler and mirrors the grammar
more honestly.

### 5.2 Why ordered `items` still matter

Even though top-level declarations become semantic objects, HIR should still
preserve source order for:

- diagnostics
- documentation tooling
- and stable source-oriented behavior

Semantic identity should come from typed IDs, not from declaration position, but
source order is still worth preserving.

---

## 6. Top-Level Declarations

### 6.1 Use declarations

```rust
pub struct UseDecl {
    pub kind: UseKind,
    pub path: IdentPath,
    pub span: Span,
}

pub enum UseKind {
    Capability,
    Relation,
    Type,
}
```

These should remain close to source shape in HIR.

Resolution of what exactly they bind to may still depend on later registry or
compiler context.

### 6.2 State declarations

```rust
pub struct StateDecl {
    pub tables: Vec<TableDecl>,
    pub span: Span,
}

pub struct TableDecl {
    pub id: TableId,
    pub symbol: String,
    pub keys: Vec<KeyFieldDecl>,
    pub fields: Vec<StateFieldDecl>,
    pub span: Span,
}

pub struct KeyFieldDecl {
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}

pub struct StateFieldDecl {
    pub id: FieldId,
    pub symbol: String,
    pub ty: TypeRef,
    pub scheme: Option<SchemeRef>,
    pub span: Span,
}
```

The important exact choice here is:

- `TableId` and `FieldId` should already be assigned in HIR

because later MIR and canonical IR lowering benefit from stable semantic IDs.

### 6.3 Context declarations

```rust
pub struct ContextDecl {
    pub fields: Vec<ContextFieldDecl>,
    pub span: Span,
}

pub struct ContextFieldDecl {
    pub id: ContextFieldId,
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}
```

Initial visibility is not represented because initial context is public-only.

### 6.4 Items

```rust
pub enum Item {
    Const(ConstDecl),
    Relation(RelationDecl),
    Event(EventDecl),
    Callable(CallableDecl),
    Predicate(PredicateDecl),
    Invariant(InvariantDecl),
}
```

This keeps declaration categories explicit while still using one ordered item
list.

### 6.5 Constants

```rust
pub struct ConstDecl {
    pub id: ConstId,
    pub symbol: String,
    pub ty: TypeRef,
    pub value: ConstExpr,
    pub span: Span,
}
```

HIR should use a dedicated `ConstExpr` subset rather than ordinary `Expr`.

That gives the frontend a precise place to enforce:

- const-evaluable shape
- no state reads
- no event emission
- no arbitrary calls

### 6.6 Relations

```rust
pub struct RelationDecl {
    pub id: RelationId,
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub outputs: Vec<TypeRef>,
    pub body: RelationBody,
    pub span: Span,
}
```

Recommended exact relation body:

```rust
pub enum RelationBody {
    Extern,
    Enum {
        values: Vec<ConstExpr>,
    },
    Range {
        start: ConstExpr,
        end: ConstExpr,
    },
    Map {
        entries: Vec<RelationMapEntry>,
    },
    TupleSet {
        tuples: Vec<Vec<ConstExpr>>,
    },
}

pub struct RelationMapEntry {
    pub inputs: Vec<ConstExpr>,
    pub outputs: Vec<ConstExpr>,
}
```

HIR should preserve relation-definition kind explicitly.

It should not reduce relation definitions to generic initializer expressions.

### 6.7 Events

```rust
pub struct EventDecl {
    pub id: EventId,
    pub symbol: String,
    pub fields: Vec<ParamDecl>,
    pub span: Span,
}
```

### 6.8 Callables

```rust
pub struct CallableDecl {
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub spec: CallableSpec,
    pub body: Body,
    pub span: Span,
}

pub enum CallableKind {
    Function,
    Query,
    Tx,
}

pub struct CallableSpec {
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
}
```

The exact choice here is important:

- HIR unifies `fn`, `query`, and `tx` as one callable data shape
- but preserves category through `CallableKind`

That gives:

- one lowering path
- one validation framework
- and one symbol universe

without losing semantic distinction.

### 6.9 Later spec items

Predicate and invariant should be represented explicitly even if not implemented
in V1.

```rust
pub struct PredicateDecl {
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub body: Body,
    pub span: Span,
}

pub struct InvariantDecl {
    pub symbol: Option<String>,
    pub params: Vec<ParamDecl>,
    pub body: Body,
    pub span: Span,
}
```

These can remain parser-accepted-later / lowering-disabled-later if needed, but
the HIR vocabulary should already reserve them.

---

## 7. Parameters, Bindings, and References

### 7.1 Parameter declarations

```rust
pub struct ParamDecl {
    pub id: ParamId,
    pub symbol: String,
    pub ty: TypeRef,
    pub span: Span,
}
```

### 7.2 Local lexical bindings

HIR still has lexical locals.

```rust
pub struct BindingId(pub u32);

pub struct BindingDecl {
    pub id: BindingId,
    pub symbol: String,
    pub ty: Option<TypeRef>,
    pub span: Span,
}
```

### 7.3 Local reference model

```rust
pub enum LocalRef {
    Param(ParamId),
    Binding(BindingId),
}
```

This is better than a raw string local name because HIR should already be
lexically resolved.

### 7.4 Top-level reference model

HIR should use typed IDs for top-level semantic references:

- `ConstId`
- `RelationId`
- `EventId`
- `CallableId`
- `TableId`
- `FieldId`
- `ContextFieldId`
- `CapabilityId`

This means HIR is not stringly typed even though it is still source-shaped.

---

## 8. Body and Region Model

```rust
pub struct Body {
    pub region: Region,
    pub span: Span,
}

pub struct Region {
    pub statements: Vec<Stmt>,
    pub span: Span,
}
```

HIR regions are:

- structured
- single-block
- lexical

They are not CFG blocks.

### 8.1 Why one simple region type is enough

HIR needs:

- root callable bodies
- nested `if` arms
- nested `match` arms
- later bounded loop bodies

One simple `Region` type is enough for all of these.

That captures the useful part of MLIR region structure without importing
multi-block CFG machinery.

---

## 9. Exact Statement Model

```rust
pub enum Stmt {
    Let(LetStmt),
    StateAssign(StateAssignStmt),
    Assert(AssertStmt),
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    Emit(EmitStmt),
    Return(ReturnStmt),
    Expr(ExprStmt),
}
```

### 9.1 Why assignment is state-only

The current target grammar only needs assignment for state mutation.

So the exact HIR contract should model that honestly.

Do **not** use a generic assignment target if the language does not yet support:

- local variable reassignment
- arbitrary lvalues

Use:

```rust
pub struct StateAssignStmt {
    pub target: StatePlace,
    pub value: Expr,
    pub span: Span,
}

pub struct StatePlace {
    pub table: TableId,
    pub key: Vec<Expr>,
    pub field: FieldId,
    pub span: Span,
}
```

This lowers much more cleanly to MIR `WriteState`.

### 9.2 `let`

```rust
pub struct LetStmt {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}
```

Recommended exact pattern support:

```rust
pub enum Pattern {
    Name(BindingDecl),
    Tuple(Vec<BindingDecl>),
}
```

However, the **implemented V1 frontend subset** should initially require:

- `Pattern::Name`

even if tuple patterns remain reserved in the HIR vocabulary.

That keeps the exact contract future-aware without forcing immediate tuple
projection support into MIR.

### 9.3 `assert`

```rust
pub enum AssertStmt {
    Expr {
        expr: Expr,
        span: Span,
    },
    Relation {
        use_: RelationUse,
        span: Span,
    },
}
```

This is better than smuggling relation assertion through generic call syntax.

### 9.4 `if`

```rust
pub struct IfStmt {
    pub condition: Expr,
    pub then_region: Region,
    pub else_region: Option<Region>,
    pub span: Span,
}
```

### 9.5 `match`

```rust
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: MatchArmBody,
    pub span: Span,
}

pub enum MatchPattern {
    Literal(LiteralValue),
    Wildcard,
}

pub enum MatchArmBody {
    Region(Region),
    Expr(Expr),
}
```

HIR should preserve the source distinction between:

- block arms
- expression arms

because that is still source semantics rather than compiler normalization.

### 9.6 `for`

```rust
pub struct ForStmt {
    pub binding: BindingDecl,
    pub range: RangeExpr,
    pub body: Region,
    pub span: Span,
}

pub struct RangeExpr {
    pub start: Expr,
    pub end: Expr,
    pub span: Span,
}
```

This remains reserved for V3. It should exist in the HIR vocabulary, but the
frontend can reject it under earlier staged subsets.

### 9.7 `emit`

```rust
pub struct EmitStmt {
    pub event: EventId,
    pub args: Vec<Expr>,
    pub span: Span,
}
```

### 9.8 `return`

```rust
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}
```

Keeping `return` source-shaped is better than forcing tuple-return machinery
into HIR.

---

## 10. Exact Expression Model

```rust
pub enum Expr {
    Literal(LiteralExpr),
    Local(LocalRefExpr),
    Context(ContextRefExpr),
    Const(ConstRefExpr),
    TableRead(TableReadExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    CallFunction(CallFunctionExpr),
    CallCapability(CallCapabilityExpr),
    Hash(HashExpr),
    EvalRelation(RelationUse),
    Select(SelectExpr),
    Tuple(TupleExpr),
    List(ListExpr),
}
```

The exact recommendation is:

- do **not** leave generic call syntax unresolved in HIR
- classify it semantically during HIR construction

### 10.1 Why explicit call categories are better

By the time HIR exists, the frontend should already know whether a surface call
refers to:

- an internal helper function
- a capability
- or a blessed builtin hash family

Relations are already explicit through `EvalRelation`.

That means the semantic categories are visible immediately and HIR -> MIR
lowering becomes much simpler.

### 10.2 Exact expression structs

```rust
pub struct LocalRefExpr {
    pub local: LocalRef,
    pub span: Span,
}

pub struct ContextRefExpr {
    pub field: ContextFieldId,
    pub span: Span,
}

pub struct ConstRefExpr {
    pub const_id: ConstId,
    pub span: Span,
}

pub struct TableReadExpr {
    pub table: TableId,
    pub key: Vec<Expr>,
    pub field: FieldId,
    pub span: Span,
}

pub struct CallFunctionExpr {
    pub callee: CallableId,
    pub args: Vec<Expr>,
    pub span: Span,
}

pub struct CallCapabilityExpr {
    pub capability: CapabilityId,
    pub args: Vec<Expr>,
    pub span: Span,
}

pub struct HashExpr {
    pub family: HashFamily,
    pub args: Vec<Expr>,
    pub span: Span,
}

pub struct RelationUse {
    pub relation: RelationId,
    pub args: Vec<Expr>,
    pub span: Span,
}
```

### 10.3 Why `TableRead` remains expression-shaped in HIR

At source level, table reads behave like expressions.

HIR should preserve that.

MIR is where table reads become explicit state-effect ops.

### 10.4 Why `Hash` is explicit in HIR

The seam decision is already final:

- a tiny blessed builtin hash family exists
- everything else is capability

So HIR should reflect that semantic distinction rather than hiding it in
generic call syntax.

---

## 11. ConstExpr Contract

`ConstExpr` should be its own exact subset.

```rust
pub enum ConstExpr {
    Literal(LiteralValue),
    Unary {
        op: UnaryOp,
        expr: Box<ConstExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<ConstExpr>,
        rhs: Box<ConstExpr>,
    },
    Tuple(Vec<ConstExpr>),
    List(Vec<ConstExpr>),
}
```

`ConstExpr` should not allow:

- state reads
- context reads
- generic calls
- capability calls
- event emission
- local references

This makes constant evaluation an explicit frontend responsibility.

---

## 12. Exact HIR Validation Responsibilities

HIR validation should enforce at least the following.

### 12.1 Program structure

- exactly one program header
- at most one `state` block
- at most one `context` block
- no duplicate top-level declaration symbols under the chosen symbol policy

### 12.2 Category legality

- `query` bodies are read-only at the source-semantic level
- `tx` bodies are allowed to mutate state
- invalid `emit` in read-only body kinds is rejected
- invalid relation use mode is rejected
- no assignment to consts or context

### 12.3 Reference resolution

- all top-level references are resolved to typed IDs
- all lexical references resolve to `ParamId` or `BindingId`
- all tables/fields/relations/events/capabilities referenced in bodies exist

### 12.4 Relation definition well-formedness

- `enum`, `range`, `map`, `tuple set`, and `extern` shapes match signatures
- map entries have correct input/output arity
- enum/range forms only appear on compatible relation signatures

### 12.5 Const well-formedness

- const values are valid `ConstExpr`
- const values typecheck against declared types

### 12.6 Frontend subset gating

HIR validation is also the natural place to enforce stage gates such as:

- tuple patterns not yet enabled
- `for` not yet enabled
- spec-layer declarations not yet enabled

This is cleaner than trying to encode rollout staging into the parser alone.

---

## 13. HIR -> MIR Lowering Contract

This is the most important boundary in the frontend.

### 13.1 What survives structurally

The following should survive into MIR almost directly:

- program-level semantic universe:
  - state schema
  - context schema
  - constant pool
  - relation manifest
  - capability manifest
  - event manifest
- callable categories:
  - `Function`
  - `Query`
  - `Tx`
- structured control:
  - `if`
  - `match`
- explicit semantic distinctions:
  - relation evaluation
  - capability calls
  - builtin hash

### 13.2 What changes materially

The following should change at the HIR -> MIR boundary:

- lexical bindings become MIR locals
- local names become local IDs with optional debug symbols
- expression nesting is normalized into explicit MIR ops
- table reads become `ReadState`
- state assignment becomes `WriteState`
- `assert relation` becomes `AssertRelation`
- generic return expressions become MIR `Return { values }`
- effect summaries are inferred and attached to callables

### 13.3 Exact lowering rules

#### Top-level lowering

- `StateDecl` -> MIR `state`
- `ContextDecl` -> MIR `context`
- `ConstDecl` -> MIR `const_pool`
- `RelationDecl` -> MIR `relation_manifest`
- `EventDecl` -> MIR `event_manifest`
- `CallableDecl` -> MIR `Callable`

#### Callable lowering

- `CallableKind` is preserved
- params and returns are preserved structurally
- `CallableSpec` does not disappear, but unsupported clauses may be rejected by
  staged lowering until later phases

#### Statement lowering

- `Let` lowers by:
  - lowering the RHS expression into a MIR value or op sequence
  - binding the final produced value to fresh MIR local(s)
- `StateAssign` lowers to MIR `WriteState`
- `AssertStmt::Expr` lowers to MIR `Assert`
- `AssertStmt::Relation` lowers to MIR `AssertRelation`
- `Emit` lowers to MIR `EmitEvent`
- `Return` lowers to MIR `Return`
- `If` lowers structurally to MIR `If`
- `Match` lowers structurally to MIR `Match`

#### Expression lowering

- `Local` / `Context` / `Const` lower to MIR `ValueRef`
- `TableRead` lowers to MIR `ReadState` plus produced local(s)
- `CallFunction` lowers to MIR `CallFunction`
- `CallCapability` lowers to MIR `CallCapability`
- `Hash` lowers to MIR `BindValue(ValueOp::Hash)`
- `EvalRelation` lowers to MIR `EvalRelation`
- pure unary/binary/select expressions lower to MIR `BindValue`

### 13.4 Lowering helper shape

The lowering helper should conceptually look like:

```rust
struct LoweredExpr {
    ops: Vec<mir::Op>,
    value: mir::ValueRef,
}
```

This lets HIR keep expression nesting while MIR receives:

- explicit op ordering
- explicit temporary locals
- explicit effectful reads/calls

### 13.5 Why HIR should not infer effects

HIR should enforce source-semantic body policy, but it should not own final
effect summary inference.

That belongs in MIR because MIR already has:

- explicit ops
- explicit call sites
- explicit state reads/writes
- explicit capability and relation ops

That is the right layer for summary inference.

---

## 14. Frontend Skeleton

The recommended frontend skeleton is:

### 14.1 `lang::ast`

- parser-oriented syntax tree
- spans
- concrete syntax artifacts

### 14.2 `lang::hir`

- exact HIR data model
- typed IDs and references
- HIR builder
- HIR validation

### 14.3 HIR builder pipeline

The builder should proceed in explicit stages:

1. parse source to AST
2. collect top-level declarations and assign semantic IDs
3. build the root program symbol environment
4. lower top-level declarations into HIR skeleton nodes
5. lower callable bodies while resolving:
   - top-level names
   - lexical bindings
   - generic call targets into semantic call categories
6. run HIR validation

This is the right structure because HIR construction needs:

- a top-level symbol pass before body resolution
- lexical scopes during body lowering
- and semantic classification before MIR exists

### 14.4 Recommended module layout

An initial skeleton could look like:

- `crates/lang/src/ast/...`
- `crates/lang/src/parser/...`
- `crates/lang/src/hir/mod.rs`
- `crates/lang/src/hir/ids.rs`
- `crates/lang/src/hir/item.rs`
- `crates/lang/src/hir/expr.rs`
- `crates/lang/src/hir/stmt.rs`
- `crates/lang/src/hir/builder.rs`
- `crates/lang/src/hir/validate.rs`

The exact file split can vary, but the responsibilities should remain distinct.

### 14.5 Testing skeleton

The first frontend tests should target:

- parser -> AST
- AST -> HIR construction
- HIR validation
- HIR -> MIR boundary for V1 programs

That is more valuable than starting with ad hoc end-to-end tests only.

---

## 15. Recommended Implementation Order

After MIR exact contract is frozen, the next implementation work should be:

1. define exact HIR Rust data structures
2. define typed IDs and reference nodes used by HIR
3. implement top-level symbol collection
4. implement lexical binding resolution inside bodies
5. implement AST -> HIR construction for the V1 subset
6. implement HIR validation
7. implement HIR -> MIR lowering for the V1 subset
8. only then expand HIR coverage to V2/V3 syntax

This sequencing matters.

If HIR construction stays informal while MIR is already fixed, frontend logic
will leak resolution and semantic classification into later passes where it does
not belong.

---

## 16. What This Note Commits To

This note is intended to settle the following.

- HIR should use one `Program` root with explicit `state`, `context`, and
  ordered `items`.
- HIR should keep explicit declaration categories through `Item`.
- HIR should unify `fn`, `query`, and `tx` as `CallableDecl` plus
  `CallableKind`.
- HIR should already use typed semantic IDs and lexical binding IDs.
- HIR should classify generic surface calls into:
  - function
  - capability
  - builtin hash
  rather than leaving them generic.
- HIR should keep structured single-block regions and source-shaped statements.
- HIR validation should own source-semantic legality and staged feature gates.
- HIR -> MIR lowering should normalize expression nesting and state effects, but
  should preserve callable kinds and structured control.
- The frontend should be built as:
  - AST
  - top-level symbol collection
  - HIR construction
  - HIR validation
  - HIR -> MIR lowering

With this note in place, the next natural step is to start translating these
contracts into exact Rust code, beginning with canonical IR and MIR data-model
implementation and then wiring the new frontend skeleton to match them.
