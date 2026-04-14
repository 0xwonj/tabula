# Program DSL Grammar Sketch

> **Status**: Proposed grammar note
> **Date**: 2026-03-24
> **Scope**: Defines the intended top-down grammar sketch for the redesigned
> Tabula program DSL, including V1/V2/V3 staging, semantic restrictions, and
> rationale for the main syntax choices.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary),
> [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md),
> [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md)

---

## 1. Why This Note Exists

The redesign note fixes the language's ontology and compiler layering. This
note narrows that into a grammar sketch that can drive the front-end rewrite.

The grammar here is intentionally:

- more precise than prose,
- less rigid than a final parser implementation,
- and explicit about which parts belong to V1, V2, and V3.

The goal is not to freeze every token today. The goal is to freeze enough of
the source model that:

- the AST/HIR/MIR redesign can proceed coherently,
- the new parser can be built against a stable target,
- and the rollout can stay incremental without re-litigating syntax each week.

When this note and broader redesign prose differ on concrete surface syntax,
this note should win. It is the syntax source of truth for the redesign.

---

## 2. Design Goals

The grammar should satisfy the following constraints.

### 2.1 Program-first, not contract-first

The root unit is a **program**, not an open-world contract. The syntax should
feel like a closed-world state machine definition.

### 2.2 State-transition-first

The language should make it obvious that:

- persistent state exists,
- transactions mutate it,
- helper functions support that logic,
- and proof-native semantic categories such as relations and constants are
  first-class.

### 2.3 Proof-aware, backend-clean

The syntax should expose semantic concepts such as:

- `const`,
- `relation`,
- `query`,
- `event`,

without leaking backend terms such as:

- lookup table internals,
- trace rows,
- chips,
- proof slots,
- or numeric capability transcript IDs.

### 2.4 Staged rollout without architectural churn

The target grammar should already show the intended V3 language, but the
compiler should be able to enable it in stages:

- V1: core state machine subset
- V2: external boundary surfaces
- V3: structured control and spec/sugar layer

---

## 3. Non-Goals

The grammar does **not** currently aim to support:

- open-world cross-program calls,
- inheritance,
- modifiers,
- fallback or receive-style handlers,
- unrestricted loops,
- ambient dynamic dispatch,
- raw lookup syntax as a user-facing primitive,
- or Solidity-style visibility-heavy function taxonomy.

---

## 4. Program Model

The grammar assumes:

- one `program` per source file,
- one sealed semantic universe per file,
- at most one `state` block,
- at most one `context` block,
- any number of `const`, `relation`, `event`, `fn`, `query`, and `tx`
  declarations.

Later multi-file composition may exist, but it should enter through imports and
artifact composition rather than by weakening the one-program-per-file source
model too early.

---

## 5. Feature Staging

### 5.1 V1 core

V1 should implement:

- `program`
- `state`
- `table`
- `const`
- `relation`
- `fn`
- `tx`
- `let`
- assignment
- `assert`
- `return`
- capability calls
- explicit `eval relation`
- straight-line bodies only

### 5.2 V2 boundary surfaces

V2 should add:

- `context`
- `query`
- `event`
- `emit`
- optional `requires`

### 5.3 V3 structured control and spec layer

V3 should add:

- `if`
- `match`
- bounded `for`
- `predicate`
- optional `ensures`
- restricted `invariant`
- sugar such as `x in R` and `F(x)` for functional relations

---

## 6. High-Level File Shape

The intended source shape is:

```tabula
use capability poseidon_hash

program Registry

state {
  table users(key id: UserId) {
    active: bool @ssmc;
    tier: u8 @ssmc;
  }
}

const MAX_TIER: u8 = 3;

relation AllowedTier(tier: u8) = enum { 0, 1, 2, 3 };

event UserRegistered(id: UserId, tier: u8);

fn validate_tier(tier: u8) {
  assert relation AllowedTier(tier);
}

query is_active(id: UserId) -> bool {
  return users[id].active;
}

tx register(id: UserId, tier: u8) {
  validate_tier(tier);
  users[id].active = true;
  users[id].tier = tier;
  emit UserRegistered(id, tier);
}
```

This form is intentionally:

- less Solidity-like than `contract { ... }`,
- more structured than the current tx-only DSL,
- and still compact enough to feel like a state machine definition rather than
  a heavyweight module system.

---

## 7. Canonical Grammar Sketch

The following grammar is the **target language**. V1/V2/V3 staging determines
which productions are enabled first.

The notation is EBNF-like rather than parser-generator-specific.

### 7.1 Lexical building blocks

```ebnf
Ident         ::= /[A-Za-z_][A-Za-z0-9_]*/
Path          ::= Ident ("::" Ident)*
Integer       ::= /0|[1-9][0-9_]*/
Hex32         ::= /0x[0-9A-Fa-f]{64}/
String        ::= /"..."/
```

Type syntax is intentionally abstract in this note. The exact type grammar is
not the focus of this redesign and may evolve independently.

```ebnf
Type          ::= Path TypeArgs?
TypeArgs      ::= "<" Type ("," Type)* ">"
```

### 7.2 File grammar

```ebnf
File          ::= UseDecl* ProgramDecl EOF

ProgramDecl   ::= "program" Ident TopDecl*
```

`program Name` is a header, not a brace-delimited block. The entire file body
belongs to the program until EOF.

This is deliberate:

- it preserves the one-program-per-file model,
- reduces contract-like visual weight,
- and makes the file itself feel like the sealed program boundary.

### 7.3 Top-level declarations

```ebnf
TopDecl       ::= StateDecl
                | ContextDecl
                | ConstDecl
                | RelationDecl
                | EventDecl
                | PredicateDecl
                | InvariantDecl
                | FnDecl
                | QueryDecl
                | TxDecl

UseDecl       ::= "use" UseKind Path ";"
UseKind       ::= "capability"
                | "relation"
                | "type"
```

### 7.4 State declarations

```ebnf
StateDecl     ::= "state" "{" TableDecl* "}"

TableDecl     ::= "table" Ident "(" "key" KeyDecl ("," KeyDecl)* ")" "{"
                  FieldDecl*
                  "}"

KeyDecl       ::= Ident ":" Type
FieldDecl     ::= Ident ":" Type SchemeAnn? ";"
SchemeAnn     ::= "@" Ident
```

Example:

```tabula
state {
  table accounts(key id: AccountId) {
    balance: u64 @ssmc;
    tier: u8 @ssmc;
  }

  table allowances(key owner: Address, spender: Address) {
    amount: u64 @smt;
  }
}
```

The `key` keyword is explicit on purpose. This avoids the ambiguity of
declaration syntax such as `table accounts[id: AccountId]`, which looks too
similar to later read syntax like `accounts[id].balance`.

### 7.5 Context declarations

```ebnf
ContextDecl   ::= "context" "{" ContextField* "}"
ContextField  ::= Ident ":" Type ";"
```

Initial implementation policy:

- all context fields are public,
- all context fields are statement-bound,
- and visibility syntax is intentionally omitted from the initial grammar.

Private context remains a reserved future extension at the architectural level.

### 7.6 Constant declarations

```ebnf
ConstDecl     ::= "const" Ident ":" Type "=" ConstExpr ";"
```

`ConstExpr` is a restricted pure expression subset evaluated at compile time.
It should eventually support:

- literals,
- tuples,
- arrays or vectors if the type system permits them,
- named constant references,
- and simple pure operators.

For grammar purposes:

```ebnf
ConstExpr     ::= Literal
                | Ident
                | "(" ConstExpr ")"
                | UnaryConstExpr
                | BinaryConstExpr
                | TupleConstExpr
                | ListConstExpr
```

The exact constant-expression evaluator is a type-system and compiler concern
rather than a parser concern.

### 7.7 Relation declarations

```ebnf
RelationDecl  ::= "relation" Ident "(" ParamList? ")" RelationResult?
                  "=" RelationBody ";"

RelationResult ::= "->" ResultList
ResultList    ::= ResultParam ("," ResultParam)*
ResultParam   ::= Ident ":" Type

RelationBody  ::= EnumRelation
                | RangeRelation
                | MapRelation
                | SetRelation
                | ExternRelation

EnumRelation  ::= "enum" "{" ExprList? "}"
RangeRelation ::= "range" "(" Expr "," Expr ")"
MapRelation   ::= "map" "{" MapItemList? "}"
SetRelation   ::= "set" "{" TupleItemList? "}"
ExternRelation ::= "extern"

MapItemList   ::= MapItem ("," MapItem)* ","?
MapItem       ::= TupleLike "=>" TupleLike

TupleItemList ::= TupleLike ("," TupleLike)* ","?
TupleLike     ::= Expr | "(" ExprList? ")"
```

Examples:

```tabula
relation AllowedTier(tier: u8) = enum { 0, 1, 2, 3 };

relation SmallRange(x: u64) = range(0, 16);

relation FeeForTier(tier: u8) -> fee: u64 = map {
  0 => 1,
  1 => 5,
  2 => 12,
  3 => 20,
};

relation AllowedTransition(old: u8, op: u8, new: u8) = set {
  (0, 1, 2),
  (2, 1, 3),
};

relation CountryCode(code: u16) = extern;
```

This design intentionally separates:

- **relation definition**, which states what the relation is,
- from **relation use**, which states whether it is used in `assert` or `eval`
  mode.

### 7.8 Event declarations

```ebnf
EventDecl     ::= "event" Ident "(" ParamList? ")" ";"
```

Example:

```tabula
event TransferApplied(from: AccountId, to: AccountId, amount: u64, fee: u64);
```

### 7.9 Predicate and invariant declarations

```ebnf
PredicateDecl ::= "predicate" Ident "(" ParamList? ")" Block

InvariantDecl ::= "invariant" Ident? "(" ParamList? ")"? Block
```

These are part of the target grammar, but they are not early rollout
requirements.

### 7.10 Functions, queries, and transactions

```ebnf
FnDecl        ::= "fn" Ident "(" ParamList? ")" ReturnType? Block
QueryDecl     ::= "query" Ident "(" ParamList? ")" ReturnType QueryClause* Block
TxDecl        ::= "tx" Ident "(" ParamList? ")" TxClause* Block

ReturnType    ::= "->" Type
QueryClause   ::= "requires" Expr
TxClause      ::= "requires" Expr
                | "ensures" Expr

ParamList     ::= Param ("," Param)*
Param         ::= Ident ":" Type
```

Recommended semantic split:

- `fn`: internal helper, callable from program bodies
- `query`: external read-only surface
- `tx`: external mutating surface

The grammar allows clauses such as `requires` and `ensures`, but these should
be staged carefully in implementation.

---

## 8. Statement Grammar

```ebnf
Block         ::= "{" Stmt* "}"

Stmt          ::= LetStmt
                | AssignStmt
                | AssertStmt
                | IfStmt
                | MatchStmt
                | ForStmt
                | EmitStmt
                | ReturnStmt
                | ExprStmt

LetStmt       ::= "let" Pattern "=" Expr ";"
Pattern       ::= Ident
                | "(" Ident ("," Ident)+ ")"

AssignStmt    ::= LValue "=" Expr ";"
LValue        ::= TableAccess "." Ident

AssertStmt    ::= "assert" AssertTarget ";"
AssertTarget  ::= Expr
                | RelationAssert

RelationAssert ::= "relation" Ident "(" ExprList? ")"

EmitStmt      ::= "emit" Ident "(" ExprList? ")" ";"

ReturnStmt    ::= "return" Expr? ";"

ExprStmt      ::= Expr ";"
```

### 8.1 Control-flow statements

```ebnf
IfStmt        ::= "if" Expr Block ElsePart?
ElsePart      ::= "else" Block
                | "else" IfStmt

MatchStmt     ::= "match" Expr "{" MatchArm+ "}"
MatchArm      ::= MatchPattern "=>" Block
                | MatchPattern "=>" Expr ","

MatchPattern  ::= "_"
                | Literal
                | Path
                | "(" ExprList? ")"

ForStmt       ::= "for" Ident "in" RangeExpr Block
RangeExpr     ::= Expr ".." Expr
```

`IfStmt`, `MatchStmt`, and `ForStmt` are target-language productions but belong
to V3 in the rollout plan.

The loop form is intentionally narrow:

- only bounded range iteration is intended,
- and even then only after compiler lowering and proof cost policy are ready.

---

## 9. Expression Grammar

The expression grammar below shows the intended semantic surface. Exact parser
precedence rules can be encoded later in Pratt or precedence-climbing form.

```ebnf
Expr          ::= OrExpr

OrExpr        ::= AndExpr ("||" AndExpr)*
AndExpr       ::= EqExpr ("&&" EqExpr)*
EqExpr        ::= CmpExpr (("==" | "!=") CmpExpr)*
CmpExpr       ::= AddExpr (("<" | "<=" | ">" | ">=") AddExpr)*
AddExpr       ::= MulExpr (("+" | "-") MulExpr)*
MulExpr       ::= UnaryExpr (("*" | "/" | "%") UnaryExpr)*

UnaryExpr     ::= ("!" | "-") UnaryExpr
                | PostfixExpr

PostfixExpr   ::= PrimaryExpr PostfixOp*
PostfixOp     ::= "(" ExprList? ")"      // call
                | "[" ExprList? "]"      // table key selection
                | "." Ident              // field selection

PrimaryExpr   ::= Literal
                | Ident
                | TupleExpr
                | ListExpr
                | "(" Expr ")"
                | EvalRelationExpr
                | SelectExpr

EvalRelationExpr ::= "eval" "relation" Ident "(" ExprList? ")"
SelectExpr    ::= "select" "(" Expr "," Expr "," Expr ")"

TupleExpr     ::= "(" ExprList? ")"
ListExpr      ::= "[" ExprList? "]"
ExprList      ::= Expr ("," Expr)*
```

### 9.1 Table reads

Example:

```tabula
users[id].active
allowances[owner, spender].amount
```

These are parsed via postfix structure:

- `users[id]`
- then `.active`

### 9.2 Calls

Bare calls such as:

```tabula
hash(a, b)
helper(x)
poseidon_hash(x, y)
```

should resolve only to:

- local `fn`,
- imported capabilities,
- and potentially future pure builtins.

They should **not** resolve to:

- `query`,
- `tx`,
- or relation evaluation.

Relations keep dedicated syntax in the core language to preserve semantic
clarity.

---

## 10. Grammar by Stage

This section maps the target grammar to implementation stages.

### 10.1 V1-enabled productions

Top level:

- `ProgramDecl`
- `StateDecl`
- `ConstDecl`
- `RelationDecl`
- `FnDecl`
- `TxDecl`
- `UseDecl` for capabilities if needed

Statements:

- `LetStmt`
- `AssignStmt`
- `AssertStmt`
- `ReturnStmt`
- `ExprStmt`

Expressions:

- literals
- identifiers
- table reads
- arithmetic and comparison
- calls
- `eval relation`
- `select`

V1 relation constructors should include:

- `enum`
- `range`
- `map`
- `set`
- `extern`

V1 bodies should still be straight-line despite `select`.

### 10.2 V2-enabled productions

Adds:

- `ContextDecl`
- `EventDecl`
- `QueryDecl`
- `EmitStmt`
- optional `requires`

This is the stage where the language gains an explicit external read/output and
instance-input surface.

### 10.3 V3-enabled productions

Adds:

- `PredicateDecl`
- `InvariantDecl`
- `IfStmt`
- `MatchStmt`
- `ForStmt`
- optional `ensures`

This is the stage where structured control and higher-level spec affordances
enter the surface language.

---

## 11. Semantic Restrictions Outside the Grammar

Several crucial rules are semantic, not grammatical.

### 11.1 `query` is external and read-only

Queries should:

- not mutate state,
- not emit events,
- and not call `tx`.

They may share expression and local-binding syntax with `fn`, but their effect
discipline is different.

More precisely, a query should be treated as:

- externally callable,
- result-bearing,
- read-only in the state/world sense,
- but not necessarily pure in the stronger semantic sense.

In particular, queries may still legitimately:

- read state,
- read state properties,
- use relations,
- and potentially call query-safe deterministic capabilities.

### 11.2 `tx` is mutating

Transactions are the only external mutating entrypoints.

Early versions should treat tx bodies as unit-returning. Bare `return;` may be
allowed as control flow, but typed tx return values are not currently part of
the intended core design.

### 11.3 `fn` is internal

Functions are helper logic, not entrypoints. They are callable from other
program bodies and are the natural location for reusable computations.

The language does not need source-level effect annotations initially, but the
compiler should still infer effect summaries for `fn` internally.

### 11.4 `relation` use is mode-specific

`relation` definitions do not declare `assert` versus `eval`.

Instead:

- `assert relation R(...)` is membership use,
- `eval relation F(...)` is functional use.

Non-functional relations must not be used in `eval` mode.

### 11.5 `const` is not state

Constants are compile-time sealed values. They cannot be assigned to and do not
participate in mutable state semantics.

### 11.6 `for` must be bounded and analyzable

Even if the syntax appears in V3, the accepted semantics should remain narrow.

The intended direction is:

- finite range loops only,
- compiler-known or statically bounded iteration,
- and lowering through unrolling or guarded normalization.

### 11.7 Capability legality depends on descriptor metadata

Bare capability calls should not be validated by name alone.

Their legality depends on capability descriptor metadata such as:

- total versus checked or partial,
- query-safe versus tx-only,
- and proof-observable versus not journaled.

This matters for:

- `query` legality,
- future guarded lowering,
- and canonical IR classification.

### 11.8 Failure behavior is part of the static model

The language should treat failure or checked behavior as semantically
significant.

Examples:

- `assert`
- checked arithmetic such as `divmod`
- partial relation evaluation if ever added
- checked capabilities

This is important because later `if` / `match` lowering must distinguish
operations that are safe to speculate from operations that must be guarded.

### 11.9 `invariant` is global

An invariant is not a reusable local assertion template.

If implemented, it should mean a global semantic law of the program, not merely
"a named assertion you can call".

---

## 12. Syntax Choices and Rationale

### 12.1 `program Name` without braces

This choice is deliberate.

It keeps:

- the root explicit,
- the file visually light,
- and the language less contract-shaped.

It also matches the closed-world, one-program-per-file model better than a
large brace-wrapped root block.

### 12.2 Explicit `state`

Source authors think in terms of state, not schemas. The DSL therefore uses
`state`, while the compiler/runtime remain free to use `StateSchema` as the
internal noun.

### 12.3 Explicit `key`

`table accounts(key id: AccountId)` is chosen over `table accounts[id:
AccountId]` because:

- it reads more like a declaration,
- it avoids confusion with later access syntax,
- and it generalizes cleanly to composite keys.

### 12.4 Explicit `relation` use

`assert relation R(...)` and `eval relation F(...)` are intentionally explicit
in the core language.

This keeps:

- the semantic category visible,
- the distinction from ordinary function calls clear,
- and the lowering path to relation IR explicit.

Sugar such as `x in R` or `F(x)` may come later.

### 12.5 Separate `fn`, `query`, and `tx`

This is one of the most important design choices.

- `fn` = internal helper
- `query` = external read interface
- `tx` = external mutating interface

This preserves semantic clarity better than collapsing everything into one
function category plus visibility flags.

---

## 13. Implementation Guidance

The grammar should be implemented against the **full target model**, even if
many productions are initially disabled.

That means the rewrite should plan for:

- AST forms for the full declaration space,
- HIR support for the full program shape,
- MIR support for region-capable bodies,
- and canonical IR lowering that can later accept structured control lowering
  without redesigning the entire frontend again.

In other words:

- syntax rollout is staged,
- but grammar architecture is full-target from the start.

---

## 14. What This Note Fixes

This note is intended to settle the following design choices unless later
research overturns them with a stronger argument:

- `program` is the preferred top-level source noun.
- one-program-per-file is the default model.
- declarations live in the program scope, not as scattered top-level globals.
- `state`, `const`, and `relation` are core categories.
- `query` and `event` belong in the language model even if implemented later.
- `fn`, `query`, and `tx` remain separate categories.
- `relation` definitions are separate from relation usage modes.
- the target grammar should be defined once, then rolled out in stages.

That should be enough to let the frontend rewrite move from architecture
discussion into concrete AST/HIR/MIR design.
