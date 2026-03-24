# Program HIR Design

> **Status**: Proposed architecture note
> **Date**: 2026-03-24
> **Scope**: Defines the intended role, structure, invariants, and MLIR-inspired
> design choices for Tabula's new HIR layer.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md),
> [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md),
> [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md)

---

## 1. Why This Note Exists

The redesign now assumes a real compiler pipeline:

- AST
- HIR
- MIR
- canonical IR

That layering is no longer optional. It is required if Tabula is going to
support:

- `program`-scoped declarations,
- constants and relations,
- external query and output surfaces,
- and later structured control such as `if` and `match`

without polluting the canonical proof IR.

This note fixes the intended HIR design before the rewrite begins.

The purpose of HIR is not merely "another tree". Its role is:

> **the first semantic program representation that preserves the language's
> major source-level categories while already being structured enough for real
> compiler reasoning.**

---

## 2. Position in the Compiler Stack

The intended stack is:

1. **AST**
   - parsed surface syntax
2. **HIR**
   - semantic source IR
3. **MIR**
   - normalized compiler IR
4. **canonical IR**
   - flat proof/execution contract

### 2.1 AST versus HIR

AST is syntax-oriented.

HIR is semantics-oriented.

AST answers:

- what tokens and source forms appeared,
- how the parser grouped them,
- where they occurred in source.

HIR answers:

- what declarations exist,
- what kind of declaration each one is,
- what semantic scopes they belong to,
- and what structured program each body denotes.

### 2.2 HIR versus MIR

HIR is still language-shaped.

MIR is compiler-shaped.

HIR preserves:

- top-level source categories,
- structured control,
- lexical binding structure,
- and the difference between `tx`, `query`, `fn`, `relation`, `event`, and
  `const`.

MIR is where:

- sugar disappears,
- names resolve to stable semantic IDs,
- effects become explicit,
- helpers are inlined or normalized,
- control regions are normalized,
- and lowering feasibility is checked.

### 2.3 HIR versus canonical IR

HIR is **not** proof IR.

HIR must not be forced to:

- become SSA,
- become flat,
- carry guarded effects,
- encode `select`-level normalization,
- or own canonical proof-facing effect ordering.

That belongs later.

---

## 3. Core Thesis

The ideal HIR for Tabula is:

- **structured**
- **symbol-based**
- **region-based**
- **semantic-category-preserving**
- **typed enough for semantic checking**
- **but not yet SSA, CFG, or proof-shaped**

This is intentionally close to the good parts of MLIR's worldview:

- operations,
- regions,
- blocks,
- values,
- symbols,
- and structure-preserving lowering

without importing MLIR wholesale.

---

## 4. What HIR Is Responsible For

HIR should own the following responsibilities.

### 4.1 Program-level semantic shape

HIR should represent the whole source file as one semantic program containing:

- state declarations,
- context declarations,
- constants,
- relations,
- events,
- predicates,
- invariants,
- functions,
- queries,
- transactions,
- imports or uses.

### 4.2 Declaration categorization

HIR should make declaration kind explicit.

This is one of its most important jobs. The language should no longer flow
through the compiler as a generic bag of named nodes.

`ConstDecl`, `RelationDecl`, `EventDecl`, `FnDecl`, `QueryDecl`, and `TxDecl`
should remain distinct HIR nodes.

### 4.3 Structured bodies

HIR should preserve source-shaped bodies:

- statement blocks,
- lexical scopes,
- `if` regions,
- `match` arms,
- and later bounded loop regions.

This preserves semantic structure for diagnostics and for later MIR
normalization.

### 4.4 Lexical binding and local structure

HIR should still model:

- local bindings by name,
- lexical visibility,
- destructuring patterns if supported,
- and source-like expression nesting.

SSA conversion belongs later.

### 4.5 Source-level semantic validation

HIR is the natural place to validate source semantics such as:

- declaration uniqueness,
- relation-definition shape,
- illegal use of `query` from the wrong context,
- misuse of relation evaluation mode,
- illegal event emission in a read-only body,
- or the difference between state and const references.

HIR should therefore already preserve enough callable-category information to
enforce:

- `fn` as internal helper logic,
- `query` as externally callable read-only logic,
- `tx` as externally callable mutating logic.

This is not all of typechecking, but it is more than parsing.

---

## 5. What HIR Is Not Responsible For

HIR should **not** do the following.

### 5.1 Not canonical lowering

HIR should not encode:

- guarded effects,
- selector synthesis,
- `select` insertion,
- one-hot match lowering,
- or flat slot-level execution order.

### 5.2 Not SSA

HIR should not force:

- unique assignment names,
- explicit value numbering,
- phi-like merge modeling,
- or dominance-driven structure.

### 5.3 Not generic CFG

HIR should not introduce:

- arbitrary basic blocks,
- branch terminators,
- explicit jump graphs,
- or block arguments as a general discipline.

Those may appear internally later if needed, but they should not be HIR's
semantic foundation.

### 5.4 Not proof backend concerns

HIR should not know about:

- proof slots,
- trace shapes,
- witness stores,
- chips,
- machine backends,
- or journal reduction order.

---

## 6. MLIR Concepts to Absorb

The HIR should deliberately absorb several structural ideas from MLIR.

### 6.1 Operations, values, regions, and blocks

MLIR's core observation is that:

- operations define values,
- operations live inside blocks,
- blocks live inside regions,
- and operations may themselves own regions.

This is a very good fit for Tabula's HIR world because:

- top-level declarations behave like named semantic operations,
- `fn` / `query` / `tx` own executable bodies,
- `if` and `match` naturally own nested regions,
- and later `for` can do the same.

Tabula does not need to copy MLIR's exact infrastructure, but it should copy
this structural mindset.

### 6.2 Symbols and symbol tables

MLIR's symbol design is also worth absorbing.

A program is naturally a symbol table containing named declarations:

- tables,
- constants,
- relations,
- events,
- predicates,
- functions,
- queries,
- transactions,
- imported capabilities.

Top-level references should therefore behave more like symbolic references than
like local SSA value references.

That is particularly useful for:

- `const` references,
- relation references,
- event names in `emit`,
- and capability references in call-like syntax.

### 6.3 Structured control operations

MLIR's `scf.if` and `scf.index_switch` are good models for what Tabula should
preserve in HIR and likely also in MIR:

- structured branches,
- region-owned bodies,
- explicit yielded structure when needed,
- but no commitment to canonical CFG as the end state.

This is a strong fit for Tabula because the final canonical IR remains
predicated and flat, while the higher layers still need structured control.

### 6.4 Traits and interfaces as design thinking

Tabula does not need MLIR traits or interfaces literally, but it should borrow
the habit of classifying nodes by semantic behavior.

Examples of HIR-level semantic traits include:

- callable-like declarations,
- read-only bodies,
- mutating bodies,
- value-producing operations,
- declaration-like symbols,
- and effectful statements.

This classification discipline matters even in Rust enum form.

---

## 7. MLIR Concepts to Exclude

MLIR is a powerful framework, but not every part of it fits Tabula's goals.

### 7.1 No MLIR dependency

Tabula should not make HIR depend on:

- MLIR tooling,
- LLVM build infrastructure,
- tablegen,
- or generic MLIR textual syntax.

The language remains a Rust-native compiler.

### 7.2 No generic operation soup

Tabula HIR should not become a generic "everything is an op with attributes"
system just because MLIR can do that.

The language benefits from typed Rust node families such as:

- `RelationDecl`
- `FnDecl`
- `TxDecl`
- `IfStmt`
- `MatchStmt`

The objective is not maximal genericity. The objective is semantic clarity.

### 7.3 No arbitrary CFG in HIR

MLIR can represent CFGs and multi-block regions. That is not a good default for
Tabula HIR.

HIR should remain:

- structured,
- lexical,
- source-shaped.

General CFG belongs neither in the source model nor in the canonical proof
model.

### 7.4 No dominance-driven semantics

HIR should not use SSA dominance as the organizing principle for source
semantics. That belongs to later lowering stages if needed.

---

## 8. HIR Design Principles

### 8.1 Preserve source categories

If the source distinguishes:

- `tx`,
- `query`,
- `fn`,
- `relation`,
- `const`,
- `event`,

HIR should preserve that distinction.

### 8.2 Be richer than the parser tree

HIR should not merely mirror surface tokens.

It should normalize and attach semantics where useful:

- keyword-driven declaration category,
- normalized relation body shape,
- explicit symbol identity,
- and canonical parameter or field representation.

### 8.3 Stay structured as long as possible

Flattening too early is a mistake.

HIR should keep:

- nested regions,
- statement nesting,
- and source control constructs

because that is where diagnostics, legality checks, and many later
transformations are easiest.

### 8.4 Delay proof-shaping

HIR should prepare for canonical lowering, but should not itself be shaped by
final proof constraints. Otherwise the source language will be bent around
backend details too early.

---

## 9. Top-Level HIR Object Model

The exact Rust types may change, but the semantic structure should look roughly
like this.

```rust
pub struct Program {
    pub name: ProgramName,
    pub uses: Vec<UseDecl>,
    pub top_level: Vec<TopLevelDecl>,
    pub span: Span,
}

pub enum TopLevelDecl {
    State(StateDecl),
    Context(ContextDecl),
    Const(ConstDecl),
    Relation(RelationDecl),
    Event(EventDecl),
    Predicate(PredicateDecl),
    Invariant(InvariantDecl),
    Function(FunctionDecl),
    Query(QueryDecl),
    Transaction(TxDecl),
}
```

This preserves the semantic top-level structure explicitly.

It is acceptable for later compiler passes to build symbol tables or indexed
maps over this representation, but the HIR itself should remain declaration
structured.

---

## 10. Symbol Model

HIR should be symbol-aware from the start.

### 10.1 Program as the root symbol table

The program defines the root symbol table.

Top-level declarations are named semantic entities. The compiler should not
have to rediscover their category from surrounding syntax later.

### 10.2 Symbol categories

HIR should distinguish at least the following symbol categories:

- table
- const
- relation
- event
- predicate
- function
- query
- transaction
- imported capability

Type names may live in a separate type namespace.

### 10.3 Stable symbol identity

Name resolution should produce stable symbolic identity, even if the HIR still
prints or stores source names for diagnostics.

That identity may be represented as:

- symbol IDs,
- interned names with declaration anchors,
- or other compiler-owned handles.

The exact mechanism matters less than the rule:

> HIR references to top-level declarations should be symbol-like, not stringly
> typed.

### 10.4 Local bindings are different from symbols

Top-level declarations are symbols.

Local `let`-bound names are lexical local bindings, not symbols in the same
sense. They remain block-local and source-like in HIR.

This is an important distinction borrowed from MLIR's separation between
symbols and SSA values.

---

## 11. HIR Regions and Blocks

HIR should use region structure, but in a controlled way.

### 11.1 Region-owning declarations

The following HIR nodes should own body regions:

- `FunctionDecl`
- `QueryDecl`
- `TxDecl`
- `PredicateDecl`
- `InvariantDecl`

### 11.2 Region-owning statements

The following HIR statements should own nested regions:

- `IfStmt`
- `MatchStmt`
- later `ForStmt`

### 11.3 Single-block structured regions

HIR regions should initially be **single-block structured regions**.

That means:

- bodies are statement lists,
- not general CFGs,
- and not arbitrary collections of basic blocks.

This captures the good part of MLIR's region model without importing its more
general control-flow complexity.

### 11.4 Lexical capture in HIR

HIR regions should follow source-like lexical capture rules:

- they can refer to in-scope parameters,
- surrounding local bindings,
- and top-level symbol references.

This is another reason to avoid SSA/block-argument discipline in HIR.

---

## 12. Declaration Nodes

### 12.1 State and tables

```rust
pub struct StateDecl {
    pub tables: Vec<TableDecl>,
    pub span: Span,
}

pub struct TableDecl {
    pub name: SymbolName,
    pub keys: Vec<KeyFieldDecl>,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

pub struct KeyFieldDecl {
    pub name: FieldName,
    pub ty: TypeRef,
    pub span: Span,
}

pub struct FieldDecl {
    pub name: FieldName,
    pub ty: TypeRef,
    pub scheme: Option<SchemeAnn>,
    pub span: Span,
}
```

The important thing here is semantic separation:

- key fields are not ordinary value fields,
- state declaration remains explicit,
- scheme annotations remain attached at source semantic level.

### 12.2 Context

```rust
pub struct ContextDecl {
    pub fields: Vec<ContextFieldDecl>,
    pub span: Span,
}
```

Even if V1 does not expose `context`, HIR should be ready for it.

### 12.3 Constants

```rust
pub struct ConstDecl {
    pub name: SymbolName,
    pub ty: TypeRef,
    pub value: ConstExpr,
    pub span: Span,
}
```

HIR should preserve the difference between:

- a constant declaration,
- and a local `let` expression.

### 12.4 Relations

```rust
pub struct RelationDecl {
    pub name: SymbolName,
    pub params: Vec<ParamDecl>,
    pub results: Vec<ParamDecl>,
    pub body: RelationBody,
    pub span: Span,
}

pub enum RelationBody {
    Enum { values: Vec<Expr> },
    Range { lower: Expr, upper: Expr },
    Map { items: Vec<MapRelationItem> },
    Set { items: Vec<TupleExpr> },
    Extern,
}
```

HIR should normalize relation definitions into a small number of semantic body
forms. It should not preserve incidental syntax if multiple surface spellings
map to the same relation meaning.

### 12.5 Events

```rust
pub struct EventDecl {
    pub name: SymbolName,
    pub params: Vec<ParamDecl>,
    pub span: Span,
}
```

### 12.6 Callable-like declarations

```rust
pub struct FunctionDecl {
    pub name: SymbolName,
    pub params: Vec<ParamDecl>,
    pub result: Option<TypeRef>,
    pub body: BodyRegion,
    pub span: Span,
}

pub struct QueryDecl {
    pub name: SymbolName,
    pub params: Vec<ParamDecl>,
    pub result: TypeRef,
    pub requires: Vec<Expr>,
    pub body: BodyRegion,
    pub span: Span,
}

pub struct TxDecl {
    pub name: SymbolName,
    pub params: Vec<ParamDecl>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub body: BodyRegion,
    pub span: Span,
}
```

The exact clause set may be staged, but the callable categories should remain
distinct.

### 12.7 Body policy should be explicit

Even if HIR does not yet carry full inferred effect summaries, it should still
preserve the body-policy distinction that later effect checking depends on.

Conceptually, HIR should already know whether a body belongs to:

- an internal helper,
- a read-only external query,
- a mutating external transaction,
- a predicate-like logical body,
- or an invariant body.

That policy may be represented implicitly by declaration kind or explicitly via
a small enum such as:

```rust
pub enum BodyKind {
    Function,
    Query,
    Tx,
    Predicate,
    Invariant,
}
```

The important point is not the exact Rust type. The important point is that HIR
must preserve enough information for later effect discipline.

---

## 13. Body Model

HIR bodies should remain statement-oriented and lexical.

```rust
pub struct BodyRegion {
    pub block: Block,
    pub span: Span,
}

pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}
```

This is deliberately simpler than a general CFG.

---

## 14. Statement Nodes

HIR statements should preserve source-level structure.

```rust
pub enum Stmt {
    Let(LetStmt),
    Assign(AssignStmt),
    Assert(AssertStmt),
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    Emit(EmitStmt),
    Return(ReturnStmt),
    Expr(ExprStmt),
}
```

### 14.1 `let` and patterns

```rust
pub struct LetStmt {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}
```

Pattern structure should survive in HIR because:

- it improves diagnostics,
- and lowering destructuring into MIR temporaries is a later concern.

### 14.2 Assignment

```rust
pub struct AssignStmt {
    pub target: LValue,
    pub value: Expr,
    pub span: Span,
}
```

State assignment should remain distinct from local rebinding.

### 14.3 Assertion

```rust
pub struct AssertStmt {
    pub target: AssertTarget,
    pub span: Span,
}

pub enum AssertTarget {
    Expr(Expr),
    Relation(RelationUse),
}
```

This keeps relation membership assertion visible in HIR rather than forcing it
through a generic call-like expression too early.

### 14.4 Structured control

```rust
pub struct IfStmt {
    pub condition: Expr,
    pub then_region: BodyRegion,
    pub else_region: Option<BodyRegion>,
    pub span: Span,
}

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
```

This is one of the strongest places to adopt MLIR-style region thinking without
adopting generic CFG machinery.

### 14.5 Event emission

```rust
pub struct EmitStmt {
    pub event: EventRef,
    pub args: Vec<Expr>,
    pub span: Span,
}
```

Emit should refer to an event symbol, not a topic string.

---

## 15. Expression Nodes

HIR expressions should preserve source semantics while making key semantic
categories explicit.

```rust
pub enum Expr {
    Literal(LiteralExpr),
    Local(LocalRef),
    Const(ConstRef),
    TableRead(TableReadExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Call(CallExpr),
    EvalRelation(RelationUse),
    Select(SelectExpr),
    Tuple(TupleExpr),
    List(ListExpr),
}
```

### 15.1 Why `Const` is explicit

Constants should already be distinct in HIR. Treating them as generic name
lookups would blur one of the central semantic distinctions of the new
language.

### 15.2 Why relation evaluation is explicit

`eval relation` should remain explicit in HIR as a distinct expression kind.

That ensures:

- relation semantics remain visible,
- misuse diagnostics stay strong,
- and MIR does not have to rediscover relation meaning from generic call forms.

### 15.3 Calls remain generic only for non-relation callables

Bare calls in HIR should mean:

- helper function calls,
- capability calls,
- future pure builtins.

They should not be overloaded to mean relation evaluation.

---

## 16. HIR Type Information

HIR should already carry enough type information to be semantically useful.

That does not necessarily mean fully inferred backend-ready types everywhere,
but it does mean:

- declaration signatures are typed,
- table fields are typed,
- relation signatures are typed,
- event signatures are typed,
- body-local expressions can be typechecked against those declarations.

The precise degree of annotation can vary, but HIR should not remain "mostly
untyped parser output".

---

## 17. HIR Validation Rules

HIR is a natural place to verify structural source-level invariants.

Examples include:

- exactly one `program` header per file,
- at most one `state` block,
- at most one `context` block,
- unique top-level symbol names under the chosen symbol policy,
- relation definitions consistent with declared signatures,
- `query` and `tx` category misuse,
- no assignment to constants,
- no `emit` from invalid body kinds,
- no state mutation from read-only body kinds,
- illegal references to undeclared tables, relations, or events,
- and early shape checks on `match` arm structure.

HIR should also enforce the source-side version of the key static distinctions:

- `query` is read-only, not necessarily pure,
- `relation` use remains explicit and mode-checked,
- and capability use may already be constrained by declaration metadata where
  available.

These checks belong here because they are semantic source invariants, not yet
canonical IR invariants.

---

## 18. HIR to MIR Boundary

HIR should cross into MIR only after:

- declaration classes are fixed,
- symbol identities are resolved,
- source scoping is validated,
- relation and const semantics are explicit,
- and structured control is made explicit.

The HIR -> MIR boundary is where the compiler should begin:

- desugaring,
- local-binding normalization,
- effect classification,
- inlining or helper expansion,
- and control-region normalization.

HIR should therefore be designed to make that lowering easy, but not to perform
it prematurely.

---

## 19. Example

Given:

```tabula
program Registry

state {
  table users(key id: UserId) {
    active: bool @ssmc;
    tier: u8 @ssmc;
  }
}

relation AllowedTier(tier: u8) = enum { 0, 1, 2, 3 };

tx register(id: UserId, tier: u8) {
  assert relation AllowedTier(tier);
  users[id].active = true;
  users[id].tier = tier;
}
```

The HIR should preserve this roughly as:

```text
Program(name = Registry)
  StateDecl
    TableDecl(name = users)
      Key(id: UserId)
      Field(active: bool @ssmc)
      Field(tier: u8 @ssmc)

  RelationDecl(name = AllowedTier)
    params = [tier: u8]
    body = Enum { 0, 1, 2, 3 }

  TxDecl(name = register)
    params = [id: UserId, tier: u8]
    body:
      Assert(RelationUse(AllowedTier, [Local(tier)]))
      Assign(TableReadWrite(users, [Local(id)], active), Literal(true))
      Assign(TableReadWrite(users, [Local(id)], tier), Local(tier))
```

This is already semantic, but not yet SSA, not yet flattened, and not yet
proof-shaped.

---

## 20. Implementation Guidance

### 20.1 Ownership

The exact crate boundary may still be decided later, but conceptually HIR
belongs to the **frontend semantic boundary**, not to runtime and not to the
canonical proof IR.

Whether it physically lives in:

- `tabula-lang`,
- `tabula-compiler`,
- or a dedicated frontend crate

is less important than keeping its role clear.

### 20.2 Use typed Rust nodes, not maximal genericity

HIR should be implemented as explicit Rust data structures, not as a generic
"operation plus attribute bag" system.

It should be MLIR-inspired in structure, not MLIR-cloned in infrastructure.

### 20.3 Keep HIR stable enough to document

HIR should be stable enough that:

- parser tests can target it,
- name resolution tests can target it,
- and MIR lowering tests can treat it as the semantic source contract.

That means its node set should be explicit and documented, not left as
incidental compiler glue.

---

## 21. What This Note Commits To

This note is intended to settle the following.

- HIR is a required new layer.
- HIR is semantic and source-shaped, not parser-shaped.
- HIR is structured, symbol-based, and region-based.
- MLIR's region and symbol ideas are good fits and should be absorbed.
- MLIR's generic infrastructure, CFG bias, and operation soup should not be
  copied.
- HIR should preserve declaration categories and structured control.
- HIR should preserve callable body policy strongly enough for later effect
  checking.
- HIR should not become SSA or canonical proof IR.

If these points hold, the next step is the exact HIR contract and data model,
followed by the exact frontend skeleton that builds and validates it.
