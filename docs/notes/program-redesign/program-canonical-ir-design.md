# Program Canonical IR Design

> **Status**: Proposed architecture note
> **Date**: 2026-03-24
> **Scope**: Defines the intended redesign of Tabula's canonical execution and
> proof IR after the introduction of AST/HIR/MIR layering.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-design.md](program-hir-design.md),
> [program-mir-design.md](program-mir-design.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary),
> [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md),
> [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md),
> [../../research/conditional-branching.md](../../research/conditional-branching.md)

---

## 1. Why This Note Exists

The frontend rewrite only makes sense if it has a stable target.

That target is the canonical IR: the final execution/proof contract that the
compiler emits and the runtime stack consumes.

Tabula's current IR already plays that role informally, but it predates:

- `const`
- `relation`
- `context`
- `query`
- typed `event`
- structured control lowering
- and the explicit AST -> HIR -> MIR -> canonical IR stack

It therefore should not simply be extended opportunistically. It should be
redesigned as the intended long-term canonical layer.

This note records that redesign.

---

## 2. Naming and Scope

### 2.1 Is this "Proof IR"?

Conceptually, yes.

This is the layer that:

- execution consumes,
- runtime proving consumes,
- journals reflect,
- and proof preparation lowers from.

So in architecture discussions, calling it **Proof IR** is reasonable.

### 2.2 Why the canonical noun should still be `IR`

Within the codebase, however, the preferred canonical noun remains simply
`IR`, for the same reason that `HIR` and `MIR` are named relative to it.

The naming model should therefore be:

- `HIR` = high-level source IR
- `MIR` = mid-level compiler IR
- `IR` = canonical execution/proof IR

This is already consistent with the current
[verification vocabulary](../../design/architecture.md#verification-vocabulary).

### 2.3 Scope of redesign

This note intentionally assumes a **major redesign** of the current IR surface.

The following are retained as architectural invariants:

- flat execution model
- SSA discipline
- no general CFG
- explicit validation
- executor/runtime friendliness
- proof-facing determinism

The following are **not** assumed to survive unchanged:

- the current instruction enum shape,
- the current `ValueExpr` / `RowExpr` split,
- the current `Lookup` instruction,
- the current capability instruction taxonomy,
- the current event representation,
- and the current state access / nullability encoding.

In other words:

> **the philosophy is retained, but the concrete IR surface is redesigned.**

---

## 3. External Reference Points

This design is not made in a vacuum. It deliberately borrows ideas from other
systems while rejecting those that do not fit Tabula's proof model.

### 3.1 MLIR

Useful lessons:

- operations, blocks, and regions as structural concepts
- symbols and symbol tables
- structured control in `scf` before lower-level conversion
- block arguments as a cleaner alternative to phi nodes in CFG-based IRs

Sources:

- [MLIR Language Reference](https://mlir.llvm.org/docs/LangRef/)
- [MLIR SCF Dialect](https://mlir.llvm.org/docs/Dialects/SCFDialect/)
- [MLIR Symbols and Symbol Tables](https://mlir.llvm.org/docs/SymbolsAndSymbolTables/)
- [MLIR Rationale](https://mlir.llvm.org/docs/Rationale/Rationale/)

What Tabula borrows:

- layered IR design
- region-first thinking above canonical IR
- symbol discipline
- structured control preservation before final lowering

What Tabula does **not** borrow:

- full MLIR dependency
- generic dialect infrastructure as a core requirement
- CFG as the default canonical execution form

### 3.2 Cairo Sierra

Useful lesson:

- keep a safe semantic IR distinct from the final low-level execution form

Source:

- [Cairo Book: Sierra](https://book.cairo-lang.org/appendix-09-sierra.html)

What Tabula borrows:

- the idea of a stable safe IR layer that is not yet the final machine form

What Tabula does differently:

- Tabula's canonical IR remains much closer to execution and proving than
  Sierra's source-to-CASM separation
- and Tabula intentionally remains CFG-free in the final canonical layer

### 3.3 Noir / ACIR

Useful lesson:

- one backend-adaptable IR can sit between source language and proving backend

Source:

- [Noir Documentation](https://noir-lang.org/docs/)

What Tabula borrows:

- backend-agnostic semantic/proof-facing IR as a design goal

What Tabula does differently:

- Tabula's canonical IR is not just an arithmetic circuit IR
- it must also remain an execution contract for the deterministic executor

### 3.4 Circom

Useful lesson:

- constraint-generation shape cannot depend arbitrarily on unknown control
  conditions

Sources:

- [Circom Control Flow](https://docs.circom.io/circom-language/control-flow/)
- [Circom Unknowns](https://docs.circom.io/circom-language/circom-insight/unknowns/)

What Tabula borrows:

- the discipline that proof/constraint shape must not become witness-dependent
  in uncontrolled ways

What Tabula does differently:

- rather than banning unknown-condition control flow at the language boundary,
  Tabula preserves structured control in HIR/MIR and lowers it into a
  predicated canonical IR

---

## 4. Core Thesis

The ideal Tabula canonical IR is:

- **small**
- **flat**
- **SSA-disciplined**
- **CFG-free**
- **executor-friendly**
- **proof-friendly**
- **backend-clean**
- **effect-explicit**

It should be the first layer that is simultaneously:

- a valid execution contract for the executor,
- a valid semantic effect source for journaling,
- and a valid proof input contract for runtime proving.

That implies a stronger claim:

> **The canonical IR should not describe source structure, nor should it
> describe a generic control-flow machine. It should describe one
> deterministic, proof-aware, flat semantic program.**

---

## 5. Design Goals

### 5.1 Preserve semantic categories

The IR should directly encode the categories that matter for proving:

- pure values
- state reads and writes
- state structural queries
- relation uses
- capability calls
- assertions
- event emission

### 5.2 Remove backend leakage

The IR should not expose:

- lookup-table internals as a user-facing semantic category
- machine trace rows
- chip-specific structure
- proof-slot layout
- or runtime host IDs

### 5.3 Remain the semantic execution contract

This is not a pure proof-only IR.

It should still define execution semantics deterministically, but the hot-path
consumer should be a runtime-resolved execution contract rather than raw
portable IR.

### 5.4 Be amenable to journal-first proving

The executor's journal should be derivable naturally from canonical IR effects,
not by reverse-engineering meaning later.

### 5.5 Support future predicated control lowering

The IR should remain CFG-free, but it must still have extension seams for:

- `if`
- `match`
- later bounded loop lowering

This means value merging and guarded effects must be designed in from the
beginning, even if not all of them are used in V1.

---

## 6. Non-Goals

The canonical IR should **not** attempt to be:

- a parser-facing language
- a structured region IR
- a generic compiler optimization IR
- a CFG machine
- an arithmetic circuit IR only
- or a general-purpose VM bytecode

It is intentionally narrower than all of those.

---

## 7. Place in the Stack

The intended stack is:

1. AST
2. HIR
3. MIR
4. canonical IR
5. execution
6. `ExecutionJournal`
7. `ProofJournal`
8. `ProofArtifacts`

This means canonical IR sits exactly at the boundary where:

- compiler structure ends,
- executor semantics begin,
- and proof frontend assumptions must already be satisfied.

---

## 8. Program-Level Object Model

The canonical IR should still model whole-program structure, but only the
minimal structure required for execution and proof.

```rust
pub struct Program {
    pub program_id: ProgramId,
    pub state: StateSchema,
    pub context: ContextSchema, // initial policy: public-only, statement-bound
    pub const_pool: ConstantPool,
    pub relation_manifest: RelationManifest,
    pub event_manifest: EventManifest,
    pub capability_manifest: CapabilityManifest,
    pub entries: Vec<Entry>,
}
```

Notably absent:

- source AST details
- helper functions
- predicates
- invariants

Those must already have been erased, inlined, or lowered by MIR time.

### 8.1 Why `fn` should disappear before canonical IR

Internal helpers are an authoring and compiler concern. The final execution and
proof contract does not benefit from a rich call model if it can instead work
with normalized flat bodies.

Therefore:

- `fn` belongs to HIR and MIR
- but not to canonical IR

### 8.2 Why `query` remains

Queries are not mere helpers. They are part of the program's external semantic
surface. They therefore belong to the program-level canonical contract, even if
their bodies are lowered into the same canonical instruction family as txs.

---

## 9. Entry Definitions

Transactions and queries should lower to the same general body shape, but
retain distinct kinds.

```rust
pub enum EntryKind {
    Query,
    Tx,
}

pub struct Entry {
    pub id: EntryId,
    pub kind: EntryKind,
    pub params: Vec<Param>,
    pub results: Vec<ResultValue>, // empty for tx
    pub body: Body,
}
```

Program-level storage should keep one `Vec<Entry>` and preserve the semantic
distinction through `EntryKind`.

### 9.1 Why `query` and `tx` stay distinct

Their body representation may converge, but their semantics do not:

- `query` is read-only and externally observational
- `tx` is mutating and may emit outputs

The canonical IR must preserve that distinction because validation and runtime
API generation depend on it.

---

## 10. Value Model

The current `ValueExpr` / `RowExpr` split should be replaced.

### 10.1 Why `RowExpr` should disappear

The old row-oriented model is too narrow for:

- composite keys,
- future richer table schemas,
- and a cleaner unification of value references across state, relation, and
  event operations.

The new canonical IR should instead use one shared value-reference model and
structured tuples where needed.

### 10.2 Proposed value reference model

```rust
pub enum ValueRef {
    Literal(PortableValue),
    Param(ParamId),
    Context(ContextFieldId),
    Const(ConstId),
    Local(LocalId),
}
```

This unifies:

- tx/query parameters
- instance-global context values
- constants
- SSA locals

### 10.3 Compound argument model

For table keys, relation arguments, event arguments, and capability inputs, the
IR should use ordered value lists:

```rust
pub type ValueTupleRef = Vec<ValueRef>;
```

This is preferable to special-purpose row-expression types.

---

## 11. Local Model and SSA

Canonical IR should remain SSA-disciplined.

That means:

- locals are compiler-owned IDs
- each local is defined exactly once
- locals are never reassigned
- validation enforces single assignment

However, unlike LLVM-style SSA:

- there is no CFG
- there are no phi nodes
- and there is no dominance-based merge machinery

Future control-flow lowering should merge values via `Select` or one-hot
combines, not via CFG edge semantics.

---

## 12. Operation Taxonomy

The canonical IR should be organized by semantic role.

The most important static distinction is not just pure versus impure.

Canonical IR op design should reflect three axes already prepared by MIR:

- **world interaction**
- **proof-observable semantic interaction**
- **checked or failure behavior**

### 12.1 Total pure value operations

These operations are:

- deterministic
- total
- free of semantic side effects
- and safe to speculatively evaluate under later predication

Examples:

- arithmetic
- comparisons
- boolean ops
- tuple projection or packing if needed
- `Select`
- `Hash` for the small blessed builtin hash family

These are not world effects and not proof-observable semantic effects in the
same sense as state, relation, capability, or event operations.

### 12.2 Checked or partial operations

Some operations are not naturally pure-and-total in the same sense.

Examples:

- integer division and modulus
- other operations that may trap or require domain checks

These should not be treated like ordinary total value ops if future branch
lowering may need to avoid evaluating untaken paths.

The IR should therefore distinguish them explicitly rather than pretending they
are ordinary arithmetic.

This class is important because it aligns with MIR's semantic-failure axis and
with the later guarded-lowering frontier.

### 12.3 Guardable semantic operations

These are operations whose semantic occurrence matters for execution and
proving:

- state reads
- state writes
- state deletions
- structural state-property reads
- assertions
- relation uses
- capability calls
- event emission
- and checked partial operations if needed

These are the operations that need a future guard seam.

Most of them are either:

- world-interacting,
- proof-observable,
- or both.

---

## 13. Proposed Core Operation Set

The following is the intended direction, not final Rust syntax.

```rust
pub enum Op {
    // Total value ops
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
        inputs: Vec<ValueRef>,
    },

    // Checked / guardable ops
    DivMod {
        guard: Option<GuardRef>,
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },

    // State ops
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
        dst_value: LocalId,
        dst_key: LocalId,
        dst_present: LocalId,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },

    // Assertions
    Assert {
        guard: Option<GuardRef>,
        cond: ValueRef,
    },

    // Relation ops
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

    // Capability ops
    CallCapability {
        guard: Option<GuardRef>,
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },

    // Output ops
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

This is intentionally flatter and more semantically explicit than either HIR or
MIR.

---

## 14. State Model

State is one of the two dominant semantic categories in the canonical IR.

### 14.1 Reads and writes must be explicit

By canonical IR time, state access should no longer be encoded as expressions
such as `table[key].field`.

It must already be:

- `ReadState`
- `WriteState`
- `DeleteState`

This makes:

- execution semantics explicit,
- journaling explicit,
- and proof reduction straightforward.

### 14.2 Composite keys are first-class

Table keys should be encoded as ordered tuples of value references rather than a
special `RowExpr`.

This generalizes better and matches the DSL's keyed table design.

### 14.3 Presence should be positive, not null-centric

The current null-flag model should be reconsidered.

The preferred direction is:

- `dst_present: bool`

rather than `dst_is_null`.

Positive presence semantics are easier to read and reduce better across the
rest of the system.

### 14.4 Structural state queries stay

State structural queries such as:

- minimum,
- maximum,
- successor,
- predecessor,
- non-existence range,
- aggregate

are still valuable and should remain explicit canonical ops, but they should be
named as state-property reads rather than generic "property read" where
possible.

This distinguishes them clearly from the external `query` surface.

---

## 15. Relations Replace Lookup

### 15.1 `Lookup` should be deleted

The old `Lookup` instruction should not survive the redesign.

Reason:

- it bakes in a table-shaped, row/column, storage-like model
- it leaks one specific proving realization upward into the semantic IR
- and it obscures the real semantic intent, which is relation membership or
  relation evaluation

This is no longer the right abstraction once `relation` becomes first-class.

### 15.2 Replacement

Canonical IR should instead have:

- `AssertRelation`
- `EvalRelation`

These should operate against:

- `RelationId`
- relation manifest membership
- typed input/output contracts

### 15.3 Why this is better for proving

This aligns much better with the proof architecture:

- the executor journals relation uses directly
- the runtime reduces relation effects into relation-oriented proof inputs
- and backend choice remains open:
  - lookup argument
  - custom AIR
  - arithmetic lowering
  - committed relation witness

In short:

> **relations are semantic; lookup is one backend implementation strategy**

---

## 16. Capabilities

Capabilities are explicit operational semantics, not relations.

### 16.1 Capability calls remain explicit

Canonical IR should use `CallCapability` for:

- typed capability-like calls,
- external operational kernels,
- and other nontrivial semantic computation capabilities.

This is better than:

- generic calls,
- magic instruction IDs in the DSL,
- or backend-specific ad hoc ops.

### 16.1.1 Capability metadata matters

Capability signatures alone are not sufficient.

The descriptor or manifest-level capability contract should eventually record at
least:

- total versus checked or partial,
- query-safe versus tx-only,
- proof-observable versus not journaled,
- and whether the capability belongs to a blessed builtin family such as hash.

This metadata is needed for:

- query legality,
- guarded lowering,
- and canonical validation policy.

### 16.2 Why capability remains separate from relation

Relations express:

- membership
- fixed function-like semantic tables

Capabilities express:

- algorithmic computation
- custom runtime semantics
- nontrivial execution kernels

That is a meaningful semantic split and should survive into canonical IR.

---

## 17. Events

Events should survive into canonical IR as typed output effects.

### 17.1 Explicit event IDs

`EmitEvent` should reference:

- `EventId`
- not a raw string topic

### 17.2 Why events belong in canonical IR

Events are not parser sugar. They are semantically relevant output effects.

If they exist in the language, they should be visible:

- to the executor,
- to the execution journal,
- and potentially to later proof output binding.

### 17.3 Initial proof-binding policy

The initial proof boundary should bind only:

- the ordered event digest

while the runtime and execution journal keep the full typed event stream.

---

## 18. Queries and Transactions

Queries and transactions both lower to entry bodies in canonical IR.

### 18.1 Query semantics

Queries are:

- read-only
- externally callable
- result-bearing

Canonical IR should preserve them as entry definitions with:

- explicit return values
- and validation rules forbidding illegal effects

Queries should not be described as pure in the stronger semantic sense.

They may still legitimately perform:

- state reads,
- state-property reads,
- relation operations,
- and query-safe deterministic capability calls.

They must still forbid:

- state writes,
- state deletions,
- and event emission.

The initial implementation should support:

- runtime query execution,
- canonical validation of read-only query bodies,
- and future compatibility with a separate `prove_query` mode.

It should not require ordinary tx-proof statements to carry query results.

### 18.2 Transaction semantics

Transactions are:

- externally callable
- state-mutating
- generally unit-returning

Events may provide their externally visible outputs instead of tx return values.

### 18.3 Functions disappear

Internal helper functions do not belong in canonical IR. MIR should inline or
otherwise normalize them away.

---

## 19. Control Flow and Guards

### 19.1 Canonical IR remains CFG-free

This note strongly reaffirms:

- no basic blocks
- no branch terminators
- no phi nodes
- no general CFG

in canonical IR.

### 19.2 Why `Select` remains

`Select` is the right canonical value-merge primitive.

It is:

- flat
- SSA-friendly
- proof-friendly
- and already aligned with the current IR philosophy

### 19.3 Why guards are needed

Once `if` and `match` exist in the source language, canonical IR needs a way to
represent:

- effectful work that occurs only on the taken path,
- without introducing CFG,
- and without forcing all effectful work to be duplicated unsafely.

The preferred seam is:

- optional guards on guardable and partial ops

```rust
pub struct GuardRef(pub LocalId); // must be Bool
```

Semantics:

- if guard is absent, the op always applies
- if guard is present and true, the op applies
- if guard is present and false, the op is semantically inactive

This should be read together with the typing/effect model:

- total pure value ops do not need guards,
- world-interacting and proof-observable ops may need guards,
- checked partial ops may need guards because speculative evaluation may be
  unsound.

### 19.3.1 Initial finalized guardable frontier

The initial guardable class is:

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

The initial non-guardable class is:

- arithmetic
- comparisons
- boolean ops
- `Select`
- builtin `Hash`
- other total pure value ops

### 19.4 Why not put guards on everything

Total pure value ops do not need guards in the initial canonical model.

They can be:

- evaluated normally,
- and later merged by `Select`

This keeps canonical IR smaller and separates:

- value predication
- from effect predication

### 19.5 Partial ops are different

Operations like `DivMod` cannot always be speculatively evaluated if they may
trap or require domain conditions.

That is why they belong in the guardable class rather than the pure total
value-op class.

This distinction is important for future control lowering.

It is also the reason canonical IR should preserve the MIR-level classification
between:

- total value operations,
- proof-observable semantic operations,
- and checked operations that may fail.

### 19.5.1 Inactive output semantics

For output-producing guarded ops, the initial canonical policy is:

- a false guard makes the op semantically inactive,
- but output locals still receive typed inactive default values.

Examples:

- `ReadState` may produce `present = false` with a default value
- `DivMod` may produce default quotient and remainder values
- `EvalRelation` and `CallCapability` may produce default output tuples

### 19.6 `match` lowering

The canonical strategy remains:

- `match` in HIR/MIR
- one-hot selector synthesis in lowering
- guarded effect emission plus `Select`-style value merges in canonical IR

This preserves fixed proof shape without canonical CFG.

---

## 20. Validation Invariants

Canonical IR validation should enforce stronger invariants than MIR.

At minimum:

- locals are assigned exactly once
- all local uses are defined
- value references are type-correct
- guard references are boolean locals
- query bodies contain only allowed world and proof-observable effects
- guarded output-producing ops obey inactive-default semantics
- tx bodies contain only allowed returns
- relation references exist in the relation manifest
- capability references exist in the capability manifest
- event references exist in the event manifest
- table/field references exist in the state schema
- result arities match signatures

This is one of the central reasons to keep the canonical IR small and explicit:
its validation should be simple, strong, and fail-closed.

---

## 21. Execution Co-Design

The canonical IR should be designed together with execution, not before it.

### 21.1 Runtime and executor relationship

Canonical IR should lower naturally into runtime-owned resolved contracts:

- `ValidatedProgram`
- `RuntimeProgram { execution, proof }`
- `ResolvedExecutionProgram`
- `ResolvedProofProgram`

The executor should consume `ResolvedExecutionProgram`, not raw portable IR.

That means canonical IR operations must still map naturally onto:

- deterministic interpretation
- typed state access
- typed relation interaction
- typed capability execution
- explicit event recording

### 21.2 Journal relationship

Each semantic effect family in canonical IR should map naturally to journal
families:

- state access
- state property read
- relation use
- capability call
- event emission

Builtin `Hash` should not require a dedicated journal family in the initial
model.

The journal should not have to rediscover semantics from generic instructions.

### 21.3 Proof relationship

Proof visibility filtering and statement assembly should happen after execution,
in runtime-owned reduction:

- executor emits a semantic `ExecutionJournal`
- runtime reduces that into a proof-facing `ProofJournal`
- runtime assembles public context binding and event digest into the eventual
  statement surface

### 21.4 Runtime proving relationship

Runtime proving should reduce journaled canonical effects into:

- `ProofJournal`
- then `ProofArtifacts`

without needing source-language concepts anymore.

This is why canonical IR must already cleanly separate:

- state
- relation
- capability
- output

at the op level.

---

## 22. Proof-System Co-Design

### 22.1 Fixed shape is still the north star

The canonical IR should keep the proving story simple:

- fixed operation sequence
- fixed local namespace
- fixed op semantics
- future control lowering via selectors and guards

This is the right compromise between:

- Circom-style "reject unknown control entirely"
- and full zkVM-style CFG execution traces

### 22.2 Backend cleanliness

Canonical IR should remain proof-backend-clean:

- no lookup-specific op
- no chip-specific op
- no trace-specific op

That keeps the proving backend replaceable.

### 22.3 Why this is more proof-friendly than preserving current `Lookup`

`Lookup` is proof-technique-shaped.

`Relation` is proof-semantic-shaped.

The latter is the stronger abstraction for:

- canonical journaling
- backend flexibility
- and language integrity

The same general principle applies to effect taxonomy:

- world effects should not be collapsed into proof-observable semantic effects,
- and checked behavior should not be hidden inside ordinary arithmetic.

---

## 23. Rewrite Implications

This note implies a major rewrite of `tabula-ir`.

### 23.1 What should be preserved

- the crate's role as canonical IR
- flatness
- SSA discipline
- explicit validation
- executor/runtime alignment

### 23.2 What should be rethought

- value reference model
- state key model
- nullability and presence model
- event representation
- capability taxonomy
- deletion versus null-write semantics
- checked op classification
- explicit relation operations
- guard seam
- removal of `Lookup`

### 23.3 Migration stance

This is not an incremental cleanup. It is a foundational redesign of the
canonical IR surface, while preserving the architectural role of the crate.

---

## 24. Strong Design Commitments

This note is intended to settle the following.

- The canonical IR is conceptually the proof IR.
- The canonical codebase noun remains `IR`.
- The redesign scope is major, not incremental.
- The final IR remains flat, SSA-disciplined, and CFG-free.
- `Lookup` is removed from the canonical IR.
- `Relation` is first-class in the canonical IR.
- Queries and transactions remain distinct entry kinds.
- Internal helper functions disappear before canonical IR.
- State keys become tupled value references, not `RowExpr`.
- Guarded semantic ops are the preferred seam for future control lowering.
- The canonical IR is co-designed with execution and proof journaling, not
  treated as an isolated compiler detail.

---

## 25. Next Step

With this note in place, the next implementation-design step is concrete enough
to begin:

1. define exact Rust data structures for the new canonical IR,
2. define MIR -> canonical IR lowering rules,
3. define canonical validation passes,
4. define executor migration from old IR to new IR,
5. and define relation-aware journaling from canonical ops into
   `ExecutionJournal`.

That is the right sequence if the frontend rewrite is going to stay aligned
with runtime and proving rather than drift away from them.
