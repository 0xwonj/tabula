# Program HIR Contract and Data Model

> **Status**: Implemented V3 structured-control contract
> **Date**: 2026-03-25
> **Scope**: Freezes the exact HIR contract that the new `tabula-lang`
> frontend builds today, and the exact ownership boundary between HIR and MIR.
> **Related**: [program-hir-design.md](program-hir-design.md),
> [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md)

---

## 1. Role

HIR is the **resolved source-semantic contract** for the new frontend.

It is:

- symbol-based
- region-based
- non-SSA
- source-shaped enough for diagnostics and body-policy checks
- fully resolved enough to feed MIR lowering

It is not:

- parser syntax
- generic unresolved call syntax
- SSA
- effect-summary carrying IR
- canonical/runtime identity carrying IR

The key ownership split is:

- `tabula-lang` owns AST, HIR, symbol collection, HIR building, and HIR
  verification
- `tabula-compiler` owns `VerifiedHIR -> MIR`

---

## 2. Current Code Scope

The implemented exact HIR contract now includes the V2 boundary surface plus
exact core V3 structured control, while still leaving later spec and sugar
forms closed.

Included now:

- `program`
- `use capability`
- `context`
- `state`
- `const`
- `relation`
- `event`
- `fn`
- `query`
- `tx`
- `let`
- state assignment
- `assert`
- `emit`
- `if`
- `match`
- `return`
- bare call heads resolved to function, capability, or blessed builtin hash
- `eval relation`
- `select`

Not implemented in exact code now:

- `requires` (intentionally deferred to a later phase)
- `ensures`
- `for`
- `predicate`
- `invariant`
- tuple patterns
- private context

---

## 3. Frontend API

The new frontend path lives under `tabula_lang`.

Authoritative public API:

```rust
parse_program(source: &str) -> Result<ast::Program, FrontendError>
build_hir(ast: ast::Program, prelude: &FrontendPrelude) -> Result<hir::Program, FrontendError>
verify_hir(program: hir::Program, prelude: &FrontendPrelude) -> Result<hir::VerifiedProgram, FrontendError>
compile_to_hir(source: &str, prelude: &FrontendPrelude) -> Result<hir::VerifiedProgram, FrontendError>
```

`FrontendPrelude` is the external semantic input boundary. It supplies:

- type resolution through the semantic registry
- capability descriptors
- blessed builtin hash classification
- field-scheme admissibility checks shared by the builder and verifier

`verify_hir` is intentionally **semantic-context aware**. That is the frontend
analogue of MLIR verifier/dialect-interface dependence: raw HIR stays
structural, while verifier authority still depends on immutable semantic
context rather than builder accidents.

The frontend does not own canonical/runtime registries directly.

---

## 4. Exact Root Shape

HIR root:

```rust
pub struct Program {
    pub symbol: String,
    pub uses: Vec<UseDecl>,
    pub context: Option<ContextDecl>,
    pub state: Option<StateDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}
```

Important decisions:

- `program_id` is not part of HIR
- `context` is an optional public-only top-level block
- `state` stays structurally separate from ordinary `items`

This keeps HIR source-semantic rather than artifact-identity bearing.

---

## 5. HIR-Local Identity

HIR owns its own semantic IDs.

Current HIR-local IDs:

- `TableId`
- `FieldId`
- `ConstId`
- `RelationId`
- `CallableId`
- `CapabilityRefId`
- `ContextFieldId`
- `EventId`
- `BindingId`
- `ParamId`

These are HIR-layer identities, not canonical/runtime identities.

Compiler lowering is responsible for deterministically minting MIR and canonical
IDs from verified HIR.

---

## 6. Top-Level Structure

### 6.1 Uses

Current exact HIR only implements capability imports.

```rust
pub struct UseDecl {
    pub capability: CapabilityDescriptor,
    pub span: Span,
}
```

Bare call heads may resolve only to:

- local `fn`
- imported capability
- imported capability classified as blessed builtin hash

They may not resolve to:

- `tx`
- `relation`
- `query`

### 6.2 State

```rust
pub struct StateDecl {
    pub tables: Vec<TableDecl>,
    pub span: Span,
}

pub struct TableDecl {
    pub id: TableId,
    pub symbol: String,
    pub keys: Vec<ParamDecl>,
    pub fields: Vec<StateFieldDecl>,
    pub span: Span,
}
```

State keys are preserved in HIR as source-semantic key declarations. Lowering
later maps them to canonical key schemas.

### 6.3 Items

```rust
pub enum Item {
    Const(ConstDecl),
    Relation(RelationDecl),
    Event(EventDecl),
    Callable(CallableDecl),
}
```

There is no exact HIR item today for:

- predicates
- invariants

### 6.4 Relations

```rust
pub struct RelationDecl {
    pub id: RelationId,
    pub symbol: String,
    pub params: Vec<ParamDecl>,
    pub results: Vec<ResultDecl>,
    pub body: RelationBody,
    pub span: Span,
}
```

`results: Vec<ResultDecl>` keeps output names in HIR. MIR may drop result names.

Supported relation bodies in the current exact HIR:

- `Enum`
- `Range`
- `Map`
- `Set`
- `Extern`

`Extern` remains structurally representable in raw HIR for future phases, but
current verification rejects it before MIR lowering. No `VerifiedProgram`
may contain an `Extern` relation today.

### 6.5 Callables

```rust
pub enum CallableKind {
    Function,
    Query,
    Tx,
}

pub struct CallableDecl {
    pub id: CallableId,
    pub symbol: String,
    pub kind: CallableKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub body: Body,
    pub span: Span,
}
```

Important current policy:

- `tx` is unit-return only
- `query` has exactly one return type in the source surface
- direct `query` bodies may not contain `StateAssign`
- direct `query` bodies may not contain `Emit`

---

## 7. Body, Region, Terminator

HIR now uses region/terminator structure for both root bodies and exact V3
nested control.

```rust
pub struct Body {
    pub region: Region,
}

pub struct Region {
    pub statements: Vec<Stmt>,
    pub terminator: Terminator,
    pub span: Span,
}

pub enum Terminator {
    Return { values: Vec<Expr>, span: Span },
    Yield { values: Vec<Expr>, span: Span },
}
```

Important decisions:

- `return` is a region terminator, not a statement
- root callable regions terminate with `Return`
- nested `if`/`match` regions terminate with `Yield`
- exact V3 nested regions currently yield zero values only
- explicit `return` inside nested control is rejected in HIR verification

This keeps the HIR/MIR boundary aligned with MLIR-style structured control
while preserving the current exact "root `Return`, nested `Yield`" discipline.

---

## 8. Statements and Expressions

Exact current statements:

```rust
pub enum Stmt {
    Let(LetStmt),
    StateAssign(StateAssignStmt),
    Assert(AssertStmt),
    Emit(EmitStmt),
    If(IfStmt),
    Match(MatchStmt),
    Expr(ExprStmt),
}
```

Exact current expressions:

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
    EvalRelation(EvalRelationExpr),
    Select(SelectExpr),
}
```

Important decisions:

- there is no generic unresolved `Call` in HIR
- `Hash` is explicit in HIR after builder classification
- `CallFunction` and `CallCapability` carry resolved targets
- `Context` is explicit after bare-name resolution
- `TableRead` is explicit source-semantic state access

Expression statements are restricted in verified HIR to:

- `CallFunction`
- `CallCapability`

Pure value expressions may not survive as statement expressions.

---

## 9. Symbol Policy

The exact current symbol policy is:

- one flat unique top-level namespace inside the program
- table field names are table-local
- local bindings and params may not shadow top-level symbols or imported
  capability aliases
- local bindings and params may not shadow context field names
- imported capability aliases must come through `use capability`

This is intentionally stricter than a future more permissive language surface.
It keeps early name resolution and diagnostics simple while the new stack is
being brought up.

---

## 10. Builder and Verifier Responsibilities

`tabula-lang` is split into explicit passes.

### 10.1 Collection

`CollectCx` owns:

- top-level symbol collection
- HIR-local ID assignment
- duplicate-name rejection
- unique `state` block enforcement

### 10.2 Building

`BuildCx` and `BodyBuildCx` own:

- AST to HIR skeleton lowering
- lexical binding resolution
- bare-call classification
- table/field/relation/callable/capability resolution
- resolved `TypeRef` attachment on HIR nodes

### 10.3 Verification

`VerifyCx` owns source-semantic checks such as:

- duplicate and shadowing rejection under the exact current policy
- relation shape checks
- `eval relation` only on functional relations
- call arity and type checks
- tx unit-return policy
- query single-return policy
- direct query rejection of `StateAssign`
- direct query rejection of `Emit`
- event argument arity/type checks
- expression statement restriction
- no unresolved surface forms survive HIR
- remaining stage gates outside the implemented V2/V3 surface

---

## 11. HIR -> MIR Boundary

`tabula-compiler` owns:

```rust
lower_hir_to_mir(
    program: &tabula_lang::hir::VerifiedProgram,
    program_id: tabula_ir::ProgramId,
)
    -> Result<mir::Program, CompilerError>
```

Compiler-owned program identity is minted by the compile pipeline and passed
explicitly into HIR -> MIR lowering. This pass returns raw MIR. The MIR
pipeline remains separate:

1. `mir::verify_program`
2. `mir::analyze_program`
3. `mir::inline_functions`
4. `mir::canonicalize_program`
5. `mir::analyze_program`
6. `mir::lower_to_canonical`

This ownership split is deliberate:

- HIR lowering is a frontend conversion pass
- MIR verification/analysis/normalization stay MIR-owned

### 11.1 Exact lowering rules

Key current lowering rules:

- `let` binds a `BindingId -> mir::ValueRef`
- literal/param/const RHS do not force fake copy ops
- `context` lowers to `ContextSchema` plus `ValueRef::Context`
- state assignment lowers to `WriteState`
- `assert expr` lowers to MIR `Assert`
- `assert relation` lowers to MIR `AssertRelation`
- `emit` lowers to `EmitEvent`
- statement-level `if` lowers to zero-result MIR `If`
- statement-level `match` lowers to zero-result MIR `Match`
- expression-statement function/capability calls lower to MIR call ops with
  dropped results
- table reads lower to `ReadState { dst_value, dst_present }`
- blessed imported hash lowers to `ValueOp::Hash`
- `eval relation` lowers to `EvalRelation`
- `select` lowers to `ValueOp::Select`
- root `return` lowers to MIR `Terminator::Return`
- nested control regions lower to MIR `Terminator::Yield`

The current exact source path does not lower to `DeleteState`.

---

## 12. Implemented Status

The new path is implemented in parallel under:

- `crates/lang/src/program/`
- `crates/compiler/src/hir_lower.rs`

Current exact code and tests cover:

- parsing the V2 boundary surface plus exact V3 statement-level control
- deliberate rejection of deferred `requires`
- building and verifying HIR
- lowering verified HIR to MIR
- lowering nested `if`/`match` regions to MIR structured control
- continuing through MIR verification, analysis, normalization, canonical
  lowering, canonical validation, runtime resolution, and executor entry
  execution

That is the authoritative contract for the current rewrite stage.
