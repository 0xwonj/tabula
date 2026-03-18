# Tabula: Typed State Transitions as the Unit of Zero-Knowledge Proving

> Status: Draft
> Scope: Full draft

## Outline

1. Why Tabula Exists
2. Why zkVMs Are the Wrong Default Abstraction for Stateful Applications
3. The Core Shift: Typed Table State Transitions as the Unit of Proving
4. The State Model: Tables, Columns, Rows, Values, and Batches
5. The Programming Model: Syntax, Semantic Operations, and Expressive Envelope
6. From Programming Model to Normalized IR: Compiler Discipline and Trust
7. Execution Semantics: Overlay, Rollback, and Consistency
8. From Execution to Proof: Witnesses, Traces, and the C+2 Architecture
9. Column Sharding and Commitment-Aware State
10. Extensibility and App-Specific Proving Kernels
11. Symbolic AIR Compilation as the Next Step
12. Where Tabula Is Strong, Where It Is Weak, and What It Proposes

## 1. Why Tabula Exists

Tabula starts from a narrow but important claim: many applications people want
to prove are not naturally described as arbitrary programs running on a generic
machine. They are better described as systems that read and write persistent
state with explicit structure: balances, positions, ledgers, inventories,
configuration tables, settlement records, and other keyed application data.
When that is the real object of interest, the proving system does not have to
begin from an ISA and reconstruct application semantics later. It can begin
from the state transition itself.

This observation places Tabula between two existing approaches. Custom circuits
can be very efficient, but every new application logic change becomes a proving
engineering task. zkVMs offer a much better software stack, but they commit to
a lower abstraction boundary: they prove machine execution first and recover
application meaning only indirectly. Tabula explores a third point in the
design space. It keeps a reusable compiler-runtime-prover stack, but makes
typed state transition, rather than hardware-like execution, the primary unit
of representation.

That choice affects the entire system. It changes how state is modeled, what
the source language is allowed to express, what the IR is allowed to contain,
what the compiler is responsible for normalizing, and how proofs can be
sharded. It also changes what kinds of specialization become natural. Once
tables, columns, value types, and state accesses are first-class semantic
objects, commitment schemes, execution traces, and future compiled AIRs can be
organized around them directly.

This article is an attempt to explain that design from the codebase outward. It
is not a polished paper and it is not a marketing overview. The goal is to make
the system legible as a research program: what the core abstraction is, how the
current implementation reflects it, and why that abstraction may matter for a
class of stateful zero-knowledge applications.

## 2. Why zkVMs Are the Wrong Default Abstraction for Stateful Applications

zkVMs solved a real and important problem. They let developers target a general
software environment, reuse ordinary toolchains, and avoid building a new
circuit for every application. Any serious alternative has to begin by
acknowledging that advantage. The issue is not that zkVMs are useless or
incorrect. The issue is that the abstraction boundary they choose is often
lower than a stateful application actually needs.

A zkVM proves that a machine executed correctly. For general-purpose
computation, that is exactly the right statement. For many stateful workloads,
it is an indirect one. An application-level operation such as reading two
balances, checking a constraint, and updating two cells is semantically a small
typed transition over persistent state. At the zkVM layer, that same operation
is expressed as register motion, stack discipline, memory traffic,
serialization, dispatch, and runtime bookkeeping. The proof system is therefore
asked to establish a large amount of machine behavior that is not itself the
application claim.

This is not only a performance issue, though it is certainly that. It is also a
semantic one. Once a program is lowered to an ISA, much of the structure that
matters to the application disappears. A balance is no longer a typed cell in a
known column; it is a word reconstructed from memory operations. A state update
is no longer visibly a write to a particular semantic location; it is a
sequence of machine events that eventually produce a new memory image. The
compiler may still optimize the program, but those are optimizations for
execution on a machine model. They do not generally preserve the semantic
structure a prover would most like to exploit. LLVM can help produce faster
programs. It does not, by itself, turn a generic machine proof into a
state-aware proof.

For stateful applications, that distinction matters a great deal. Many such
systems do not need the proving backend to emulate hardware and then prove the
emulation. They need it to prove that a constrained set of typed state
transitions happened correctly. Once framed that way, a different design space
opens up. The proving system can preserve column identity, value type, static
lookup boundaries, and state-access structure all the way down to witness and
AIR design. Tabula is an exploration of that alternative.

## 3. The Core Shift: Typed Table State Transitions as the Unit of Proving

Tabula's central move is simple to state: it treats a program as a collection
of schema-typed state transitions over tables. The primary semantic object is
not an instruction stream over a universal machine, but a transaction-shaped
relation between one committed state and the next. State is organized into
tables. Tables have columns. Columns have declared value types. Concrete cells
are identified by `(table, column, row)`. Programs operate over that structure
directly.

That shift changes what remains visible throughout compilation and proving. In a
machine-oriented system, a memory address is usually just a runtime location. In
Tabula, the table and column of a state access are part of the program's
meaning. A read is not merely "load from memory"; it is "read this cell from
this table and column." A write is not a mutation somewhere in a flat address
space; it is an explicit update to a semantic location. This lets the compiler
and prover keep track of facts that a zkVM would normally erase very early.

The result is a deliberately specialized system. Tabula is not trying to hide
that specialization under the language of general machine proving. It is making
a stronger claim instead: for a broad class of stateful workloads, the right
thing to preserve is not machine behavior but application state structure.
That is why Tabula is better understood as an alternative point in the design
space between hand-written circuits and fully general zkVMs. It gives up some
universality in exchange for a representation that is much closer to the object
the application actually wants to prove.

Once this choice is made, the rest of the architecture follows. The state model
must expose typed cells rather than opaque bytes. The source language must make
state access explicit. The IR must preserve state semantics instead of
flattening them into generic machine operations. The compiler must do more than
translation; it must normalize programs into a form the proving backend can
exploit safely. The next sections unpack those consequences in order.

## 4. The State Model: Tables, Columns, Rows, Values, and Batches

Tabula's state model begins with tables rather than memory. Each table has a
schema. Each schema declares columns. Each column has a value type. A concrete
cell is addressed by a triple `(table, column, row)`, represented in the core
types as `CellKey`. This sounds simple, but it is a major design commitment.
The protocol does not treat semantic location as something to recover from
machine execution after the fact. It makes location explicit from the start.

The value domain is also intentionally closed. At the proof-facing layer,
Tabula works with `U64`, `I64`, `Bool`, and `Bytes32`. That restriction keeps
encoding rules and constraint choices explicit. A boolean column is known to be
a boolean column. A digest column is known to hold 32-byte values. The system
does not have to pretend that all application data are interchangeable machine
words and then recover discipline later through convention.

This typed structure is already doing proof work. Once a value is known to live
in a specific column with a specific type, the commitment layer can choose an
appropriate encoding, the witness layer can attach the access to a particular
state shard, and the prover can reason about the state as a collection of typed
locations rather than opaque bytes. In other words, the state model is not only
an application data model. It is part of the proving model.

On top of this state sit transactions and batches. A transaction is a named
state transition with typed inputs and associated execution metadata. A batch is
an ordered list of transactions whose combined effect carries the global state
from one commitment to another. That ordering is part of the semantics, not an
implementation detail. Tabula is not proving isolated function calls. It is
proving an ordered change to persistent state.

## 5. The Programming Model: Syntax, Semantic Operations, and Expressive Envelope

Tabula's programming model is best understood as a transaction language over
typed persistent state. A program declares tables and transactions. Tables
define the shape of persistent state. Transactions define the allowed state
transitions over that state. The surface syntax is intentionally small, but it
already exposes most of the semantic actions the runtime and prover care about.

At the schema level, the language names tables, columns, value types, and,
when useful, proving-relevant attributes such as commitment strategy. At the
statement level, it supports local bindings with `let`, destructuring
assignment through `divmod`, assignments back into persistent state, `assert`,
`emit`, `@precompile(...)`, and structural property queries. At the expression
level, it supports literals, identifiers, dynamic cell reads such as
`balances[from].balance`, static reference-table reads, arithmetic, comparison, boolean
operators, hashing, unary operators, and `select` as a value-level
conditional.

The following example gives the flavor:

```tabula
table balances {
    balance: u64,
    pubkey: bytes32,
}

table orders {
    price: u64 @commitment(ssmc),
    owner: u64,
}

tx withdraw(user: u64, amount: u64, sig: bytes32) {
    let bal = balances[user].balance
    let pubkey = balances[user].pubkey
    @precompile(verify_ecdsa, [sig_ok], pubkey, amount, sig)
    assert sig_ok
    assert bal >= amount
    balances[user].balance = bal - amount
    emit "withdraw" (user, amount)
}

tx match_best(max_price: u64) {
    let (best_order, best_price, best_is_null) = @property(maximum, orders.price)
    assert !best_is_null
    assert best_price <= max_price
    let maker = orders[best_order].owner
    emit "match" (best_order, maker, best_price)
}
```

This example already shows most of the semantic vocabulary that matters. The
reads from `balances[...]` are ordinary state reads. The
`@precompile(verify_ecdsa, ...)` call denotes an application-specific primitive
whose semantics are explicit to both execution and proving; it is not just an
opaque library call. The property query is also explicit: it asks for a
structural fact about committed column state rather than manually reconstructing
that logic in application code. The assertions are explicit validity
conditions. The assignments are state writes. The `emit` records an event
alongside the state transition. A commitment annotation appears only where it is
semantically interesting, on `orders.price`, to signal that the way a column is
committed can affect what kinds of queries and proofs are natural over it.

More generally, the programming model can be described in terms of semantic
operations. A dynamic cell read corresponds to `Read`. A static-table read
corresponds to `Lookup`, a separate operation for immutable reference data.
Arithmetic expressions become explicit arithmetic
operations, with `divmod` as a dedicated form. Comparisons and boolean
connectives become explicit predicates. `select` becomes a value-level
`Select`, not a control-flow branch. Assignments to committed state correspond
to `Write`, `assert` to `Assert`, `emit` to `Emit`, and `@precompile(...)` to
`Precompile`, and structural queries to `PropertyRead`. Schema-level commitment
annotations feed into the proof plan and determine how individual columns are
routed through the commitment and proving stack.

This makes the expressive envelope fairly clear. Tabula is not a
general-purpose language for arbitrary software. It is a language for programs
that read typed state, derive local values from those reads, consult static
reference data, assert validity conditions, update persistent state, emit
events, and invoke application-specific extension points. That is enough for a
large class of ledger-like and database-like state transitions. It is
intentionally not a language for unconstrained control flow, arbitrary mutable
environments, or opaque machine-style computation.

What the language excludes is therefore as important as what it includes. There
are no general loops, no general control-flow graph, no shadowing-driven local
semantics, and no attempt to hide state manipulation behind generic memory
operations. The programming model is transaction-shaped and mostly linear on
purpose. It gives the programmer constructs that already look like the state
transition they want to express. The compiler's next job is to preserve that
structure while turning it into a stricter internal form.

## 6. From Programming Model to Normalized IR: Compiler Discipline and Trust

Once the programming model is expressed in this semantic vocabulary, the
compiler's job is not to rediscover meaning. It is to freeze that meaning into a
more rigid and more easily provable internal form. Lowering therefore produces a
flat instruction stream over explicit state and value operations rather than a
machine-like IR. Most source constructs map directly to semantic instructions,
while schema-level proving annotations are collected into capability manifests
and proof-planning metadata attached to the compiled artifact.

This is where Tabula's compiler becomes more than a front-end. Its job is not
only to turn source syntax into instructions. Its real job is to shrink the
space of behaviors the prover must account for. SSA removes ambiguity in local
value flow. Normal form removes large classes of aliasing and access ambiguity.
Typechecking fixes the interpretation of values and state accesses before
proving begins. In effect, the compiler does cheap semantic cleanup so that the
proof system does not have to perform the same reasoning inside a much more
expensive algebraic setting.

The registration pipeline makes these guarantees concrete through
`canonicalize -> typecheck -> validate`. Duplicate reads are canonicalized away,
duplicate writes are rejected, read-after-write ambiguity is ruled out, and
write-relevant alias ambiguity must be eliminated or guarded explicitly. By the
time the prover sees a program, it is not looking at an arbitrary lowered
artifact. It is looking at a checked and normalized state-transition IR.

The result of that process is not merely an IR blob. Tabula keeps using a
`CompiledProgram` because the proving pipeline depends on more than raw
instructions. The sealed artifact includes the normalized program, capability
information, proof-planning metadata, and contract-facing metadata needed to
prepare execution and verification. This is the stable object the runtime,
witness generator, and verifier-oriented setup are built around.

That structure clarifies the trust boundary. The proof is about correct
execution with respect to a prepared, normalized program artifact. It does not,
today, prove that the source text was lowered correctly into that artifact.
Source-to-artifact compilation is therefore outside the ZK statement. Program
identity is then completed at the wrapper-verification layer, which checks the
execution statement's program and metadata hashes against the sealed artifact
used to prepare verification. This still allows trust in the compiler to be
amortized into a one-time step: produce or receive a sealed artifact, validate
it and its compiler-derived shape, and then reuse it as the program identity
for many executions and many proofs.

That statement should also be understood with one current limitation in mind.
Not every contract-facing field is bound in AIR yet. The state roots are
bound, but some fields such as `program_root`, `applied_tx_digest`,
`static_table_root`, and `budgets` remain explicitly deferred in the current
binding registry. The system therefore already has a real and useful artifact
boundary, but it is not yet the strongest end-state version of that boundary.

This is one of Tabula's central compiler-prover co-design ideas. Some semantic
work is intentionally moved out of the proof and into a much cheaper validation
layer. The trust boundary shifts slightly toward the compiler, but in exchange
the proving problem becomes narrower, more regular, and more specialized. That
trade is not hidden; it is part of the design. The next step is to see how the
runtime and proving stack exploit the resulting artifact.

## 7. Execution Semantics: Overlay, Rollback, and Consistency

Once a program has been compiled into a normalized artifact, Tabula executes it
against a snapshot of state through a local overlay rather than mutating the
base state directly. This overlay is the concrete execution model for a batch.
All reads and writes go through it. Its semantics are simple but important:
reads see prior writes from the same batch, repeated snapshot reads are cached,
and only the final write to a key survives into the outgoing write set. In
other words, the execution layer already behaves like a transactional state
transition system before proving ever begins.

This matters because batch execution is not just "run the interpreter many
times." Each transaction runs with a checkpointed overlay. If execution
succeeds, the checkpoint is discarded and its writes remain live for later
transactions in the same batch. If execution fails, the overlay rolls back to
the checkpoint and the batch continues. The failed transaction therefore leaves
behind a failure record and any partial trace that should remain visible, but it
does not mutate the final state. This gives Tabula a clear notion of
partially-applied batch execution: transactions are attempted in order, but only
successful ones contribute to the committed state transition.

The batch executor also fixes a number of runtime responsibilities around that
core loop. It resolves transaction types, validates parameter schemas, checks
signatures and nonces, and invokes the reference interpreter over the
transaction body. Successful transactions produce emitted events, access traces,
and any auxiliary outputs such as precompile I/O or property-query results.
Failed transactions record a reason and the instruction at which failure
occurred. By the end of execution, the system has a precise record of which
effects are semantically live, which were rolled back, and what the final
read-set and write-set actually are.

Tabula then runs an explicit consistency check over the successful execution
events. This is not the proof yet. It is a cheap verifier for the intended
last-write semantics of the execution trace. Reads must match either the most
recent prior write to the same key or the initial value from the deduplicated
snapshot read-set. This check plays the same general role as other compiler- and
runtime-side validations in the system: it keeps the proving target honest and
well-formed before the expensive proving layer is asked to certify it.

The runtime pipeline wraps all of this into a canonical sequence. It normalizes
the input state artifact, materializes an in-memory snapshot, executes the
batch, runs the consistency check, and only then merges the final write-set into
the post-state artifact. This separation is worth emphasizing. Execution comes
first, proving comes later, and the proving layer is fed not by raw source code
but by a deterministic state-transition result that has already been reduced to
the semantic objects that matter: transaction outcomes, read-set, write-set,
and ordered execution effects.

## 8. From Execution to Proof: Witnesses, Traces, and the C+2 Architecture

The proving pipeline begins where execution ends. Tabula does not prove the
interpreter directly. Instead, it turns the executed batch into a structured
witness that is already organized around the state model and proof architecture.
The witness generator starts from three things: the execution result, the table
schemas, and the pre-batch column states. From them it derives the set of
touched columns, constructs per-column init rows from the deduplicated snapshot
read-set, constructs per-column access rows from the successful execution trace,
groups final writes by column, applies those writes to old column states, and
computes both old and new global state roots. The output is not a flat trace. It
is a `BatchWitness` organized by column, with explicit column metadata and root
information.

That witness still is not the final proving input. The next step is trace
building. The witness layer lowers the IR body and the execution result into
instruction records, static-table rows, property-query records, and path
witnesses. These are collected into a generic witness store that is then split
by semantic tier rather than by source syntax. This split is the bridge between
the execution model and Tabula's proof decomposition. The execution tier gets
instruction records and static-table rows. Each touched column gets its own
column-tier witness store. The root tier gets the data needed to bind per-column
commitments into the old-root to new-root transition.

This is what the C+2 architecture means in practice: one execution proof, one
proof per relevant column, and one root proof. The execution tier carries the
generic transaction semantics, including explicit opcodes such as `Read`,
`Write`, `Lookup`, `Precompile`, and `PropertyRead`. The column tiers carry the
actual state-transition evidence for particular `(table, col)` shards, using the
commitment scheme chosen for that column. The root tier proves that the old and
new column commitments are correctly aggregated into the global state-root
transition. The machine setup mirrors this decomposition directly by preparing
separate chip registries, keys, and trace builders for the execution, column,
and root tiers.

One consequence of this design is that the proof pipeline is not merely
sharding for parallelism. It is a semantic decomposition. Execution semantics,
column-local state evolution, and root aggregation are treated as different
proof obligations because they really are different proof obligations. This lets
Tabula keep the execution tier relatively stable while allowing column-tier
proofs to vary with commitment scheme and query support. It also means untouched
columns need not look like active execution state; they are represented through
metadata and root-binding logic rather than full per-column execution evidence.

Seen end to end, the pipeline is therefore: execute the batch, generate a
column-aware witness, build per-tier traces, and then prove those traces with a
machine whose structure already matches the semantics of the state transition.
This is the practical meaning of Tabula's main claim. Once typed state
transition is taken as the unit of proving, execution, witness generation, and
proof construction can all be organized around the same object instead of being
recovered indirectly from a general machine trace.

## 9. Column Sharding and Commitment-Aware State

Tabula organizes committed state by column, and the proving architecture follows
the same boundary. A batch usually interacts with a small subset of the full
schema rather than with every table and every column at once. Once state access
is tracked at the level of `(table, col, row)`, the proof system can preserve
that sparsity instead of flattening it away.

This leads to selective touched-column proving. The execution tier remains
global, because transaction semantics are global, and the root tier remains
global, because the state root transition is global. Between those two layers,
however, committed state is handled column by column. Only columns that are
actually touched by the batch receive full transition proofs. Untouched columns
still contribute to the global root, but they do not need full per-column proof
work for that batch. They are carried through metadata and root-binding
structure instead.

That decomposition gives Tabula a cost model that matches the semantic footprint
of the batch more closely than a single proof over the entire state. A useful
mental model is not "prove the whole database again," but "one execution proof,
one proof for each touched column, and one root proof that ties them together."
When the touched set is sparse, proving work and proof size can shrink with it.
The same structure also makes parallel proof generation natural, since
touched-column proofs can be produced independently once the shared execution
and root obligations are fixed.

The column boundary is also where commitment policy becomes part of the proof
architecture. Tabula does not require every column to use the same commitment
scheme. Different columns can choose different schemes, and those choices can
lead to different local proof shapes and different supported queries. An
SSMC-style column can support richer structural queries directly in its local
proof. An SMT-style column pushes more of that work toward root aggregation and
offers a different tradeoff surface. As a result, the column is not just a unit
of storage. It is the unit at which state organization, commitment strategy,
query capability, and proof specialization meet.

## 10. Extensibility and App-Specific Proving Kernels

Extensibility in Tabula is not an afterthought. It follows from the same design
choice that shapes the rest of the system: proving is organized around
application semantics rather than around a fixed virtual machine. Once the unit
of proving is a typed state transition, it becomes natural to let applications
specialize the proving kernel at the same semantic boundaries. Different
applications may want different cryptographic checks, different state
commitment schemes, different structural queries, and different root-binding
logic. Tabula is built to expose those choices directly.

Precompiles are the clearest example. A precompile is a named semantic
operation, such as signature verification or hashing, that the application
wants to treat as a primitive rather than as a long sequence of ordinary IR
steps. In Tabula, that choice reaches both execution and proving. The runtime
needs an implementation that can produce the right outputs during execution, and
the proving system needs a corresponding constraint story that makes the same
operation verifiable. Property queries follow the same pattern. An application
may want to ask for a structural fact about a committed column, and that query
needs both a runtime meaning and a proof-level opening strategy.

The same idea extends below the transaction surface. Column schemes let
different parts of the state use different commitment strategies. Root-proof
logic can be specialized to the way an application wants to aggregate column
commitments into a global state commitment. Machine-level extensions can add AIR
constraints that are invisible at the source level but still part of the final
proving system. The result is not a single universal zk machine with a few
pluggable opcodes. It is a proving kernel whose shape can be adapted to the
structure of an application.

What keeps that flexibility coherent is that it is mediated by the compiled
artifact rather than by ad hoc runtime wiring. The program declares which
precompiles, property-query kinds, and scheme tags it depends on, and
preparation checks that the runtime and verifier have matching implementations.
That is why Tabula is best understood as an app-specific proving kernel. The
application does not merely execute on top of a proving system. It helps define
which proving system is instantiated for it.

## 11. Symbolic AIR Compilation as the Next Step

The current execution tier is still a universal machine. It is already much
more state-aware than a generic zkVM, but it remains instruction-oriented: one
IR instruction per row, one large execution chip shape, and one set of witness
columns that must be carried even when a particular transaction uses only a
small fraction of them. The execution-chip design notes make this visible. The
current chip is 278 columns wide, and a simple transaction still pays for slot
carry, operand selection, and opcode-specific witness structure that only exist
because the chip must be prepared to handle many different instructions.

Symbolic AIR compilation asks whether Tabula can take the next step and compile
away that remaining universality. The key observation is that a registered
Tabula program is fixed, linear, and already normalized. The compiler knows the
instruction sequence, the SSA data flow, the slot types, and the state-access
pattern before proving begins. That makes it possible to symbolically execute a
transaction body ahead of time and derive direct algebraic relations between
transaction inputs, state reads, asserts, and final writes. In that view, the
transaction is no longer "a sequence of instructions to simulate." It is "a set
of relations to satisfy."

For simple transaction types, that could collapse the execution proof all the
way down to one row per transaction. Instead of carrying intermediate slots
through multiple instruction rows, the compiled AIR could express the final
write values directly as functions of read values and parameters, and express
asserts directly as algebraic constraints. More complex transaction types may
still want more than one row, but the important shift is the same: the row
structure would be chosen for the program, not inherited from a universal
instruction machine.

That compilation step is not just inlining everything blindly. Some values are
opaque at proof time and must still appear as witness columns: state reads,
transaction parameters, gadget witnesses, precompile outputs, and other values
whose semantics are not reducible to a small algebraic expression. Other values
can remain symbolic and be used inline. The main compiler problem becomes
materialization: deciding which intermediate expressions should become trace
columns and which should stay as derived expressions inside constraints and bus
interactions. Degree management matters here. A compiled chip is only useful if
it stays narrow without letting constraint degree grow out of control.

This is also why symbolic AIR compilation is different from ordinary constraint
optimization. Constraint CSE makes a given AIR cheaper to evaluate. Symbolic AIR
compilation changes which AIR exists in the first place. It moves from a
universal execution circuit toward per-transaction-type compiled chips. Those
compiled chips could live alongside the generic execution chip rather than
replace it all at once: common transaction types could use compiled chips, while
rare or irregular ones fall back to the generic path when that is the better
tradeoff.

The main research challenge is soundness, not just speed. A compiled execution
chip has to mean the same thing as the generic execution chip. At the system
boundary, that means it must emit the same bus messages for reads, writes,
asserts, and delegated gadgets, so that the rest of the machine can remain
unchanged. If that equivalence story holds, symbolic AIR compilation would be
more than a prover optimization. It would complete the trajectory that Tabula
already points toward: treating a transaction as a statically known
state-transition relation and compiling that relation directly into the proving
system.

## 12. Where Tabula Is Strong, Where It Is Weak, and What It Proposes

Where Tabula is strongest is in applications whose state already has clear
schema, locality, and repeated transaction shapes. Financial state machines,
matching engines, order books, credit ledgers, account systems, and other
stateful applications with structured reads and writes fit this model
naturally. These workloads tend to have sparse touched sets relative to total
state, stable transaction families, and strong interest in application-specific
queries or cryptographic checks. That is exactly where compiler normalization,
column sharding, and app-specific proving kernels become most valuable.

The model is weaker when the application wants the proof system to behave like a
general-purpose computer. Highly dynamic control flow, irregular state access,
many rare transaction shapes, or workloads that touch nearly all state on every
batch all reduce the benefits of specialization. In those settings, the cost of
maintaining richer semantic structure may not buy enough in return. Tabula is
not trying to win every proving workload. Its claim is narrower and more
deliberate: when the computation is naturally a structured state transition,
that structure should remain visible all the way through proving.

Tabula starts from a simple disagreement with the default zkVM story. For many
stateful applications, the right thing to prove is not a hardware-style
execution trace but a typed transition over structured state. Once that
decision is made, many other choices begin to line up: a transaction-oriented
language, a normalized IR, a compiler that removes proving work ahead of time,
column-aware witness generation, touched-column proving, heterogeneous
commitment schemes, and application-shaped extension points.

Seen that way, Tabula is not just a collection of optimizations. It is a
proposal about where the abstraction boundary of zero-knowledge systems should
sit. The project is still evolving, and some of its most ambitious pieces are
still ahead. But the direction is already clear. Instead of proving that a
general machine happened to produce the right state transition, Tabula asks what
happens when the state transition itself becomes the thing the proving system is
built to understand.
