# Introduction

## 1. The Expressibility Problem in Zero-Knowledge Proofs

Zero-knowledge proofs allow a prover to convince a verifier that a
computation was performed correctly, without revealing the computation's
inputs. The practical deployment of this primitive — in rollups,
verifiable databases, and privacy-preserving protocols — hinges on a
deceptively simple question: *how does one specify what to prove?*

The answer, historically, has been arithmetic circuits. A computation is
encoded as a system of polynomial constraints over a finite field, and
the prover demonstrates that a satisfying assignment exists. Circuit
construction languages — Circom, Noir, Halo2's PLONKish API, and others
— give developers varying levels of abstraction over the underlying
constraint system. But the abstraction is thin. Writing correct,
efficient circuits requires intimate knowledge of the proving system:
field arithmetic, constraint degree, witness generation, soundness
pitfalls. The difficulty is severe enough that "ZK circuit engineer" has
emerged as a distinct job title, and security audits of circuits
routinely uncover critical vulnerabilities — under-constrained witnesses,
missing range checks, arithmetic overflows that silently wrap in the
field.

The expressibility problem has two faces. The first is **developer
experience**: writing circuits is difficult, error-prone, and requires
cryptographic expertise that most application developers do not possess.
The second is **auditability**: each new circuit is a new attack surface
that must be independently verified for soundness. A DeFi protocol that
changes its state transition logic must re-audit its circuits, at
substantial cost in time and money.

## 2. The zkVM Solution and Its Cost

The zero-knowledge virtual machine (zkVM) is the field's dominant
response to the expressibility problem. The idea is elegant: instead of
writing a new circuit for each application, write one circuit — the
circuit that proves correct execution of a general-purpose instruction
set architecture (ISA), typically RISC-V. Application developers then
write their logic in Rust, C, or any language that compiles to the
target ISA, and the zkVM proves that the compiled program executed
correctly.

This resolves both faces of the expressibility problem. Developer
experience improves dramatically: application logic is written in
familiar languages with standard tooling. Auditability concentrates at a
single point: if the zkVM circuit is sound, then *any* program it
executes is proven correctly. Projects like SP1, RISC Zero, and Jolt
have demonstrated the viability of this approach, and zkVMs have become
the default choice for new verifiable computation deployments.

But generality has a cost. A zkVM proves *machine execution*, and the
machine is indifferent to the application semantics of the code it runs.
Every instruction — whether it implements application logic or runtime
infrastructure — occupies one row in the execution trace and is
constrained identically in the Algebraic Intermediate Representation
(AIR). For stateful applications, this indifference creates systematic
overhead across four dimensions.

**ISA intermediation.** A single logical operation — "read the balance of
account X" — expands into a sequence of ISA instructions: load an
address into a register, issue a memory-read syscall, decode the result
bytes, store the value to the stack. Each instruction is a row in the
execution trace, each row is fully constrained. The application-level
operation is conceptually one cell access; the proof treats it as a
dozen fetch-decode-execute cycles.

**Undifferentiated memory consistency.** The sorted-memory argument —
the standard technique for proving that "every read returns the
last-written value" — must cover *all* memory: stack frames, heap
allocations, function arguments, compiler temporaries, and application
state. The cost scales with total memory operations, the vast majority
of which are infrastructure with no application-level significance. A
transaction that touches 8 state cells may generate hundreds of
memory-table rows for stack management alone.

**Untyped encoding.** The prover sees a flat byte array. It cannot
exploit the fact that a column contains only `u64` values, or that a
table has 50 rows, or that a field is a boolean. Type information — which
could drive encoding width, constraint specialization, and commitment
strategy — is erased at the ISA boundary. Every value is encoded at the
machine word width regardless of its actual range.

**Application-level state commitment.** zkVMs prove execution; they do
not manage state. When a rollup needs a cryptographic commitment to its
post-state (a Merkle root, typically), the application must compute it:
hashing leaves, building paths, updating the tree — all as ISA
instructions, all proven through the same execution trace. For a batch
of 100 account updates in a depth-20 Merkle tree, this means on the
order of 2,000 hash invocations, each proven as a sequence of RISC-V
instructions (or, with precompiles, as a precompile call that still
crosses the ISA boundary for argument marshalling).

The net effect is a cost structure where application semantics — the
actual reads, comparisons, and writes that define the state transition —
constitute a small fraction of the total constraint count. For a batch
of 100 balance transfers, the business logic is five operations per
transaction (read, read, compare, subtract-add, write, write). The
proving cost is dominated by ISA overhead, memory consistency over the
full address space, and state root computation. Empirically, the
overhead ranges from 10x to 100x relative to a hand-written circuit for
the same logic.

## 3. The Design Space Between Circuits and zkVMs

The circuit DSL and the zkVM represent two extremes of a design
continuum. At one end, custom circuits achieve minimal constraint counts
but demand cryptographic expertise for every application change. At the
other, zkVMs accept arbitrary programs but prove far more than the
application requires. The question is whether there exists an
intermediate point that preserves most of the developer-experience
benefits of zkVMs while recovering a substantial fraction of the
efficiency lost to ISA generality.

Tabula is a proposal for such an intermediate point. The key observation
is that the overhead described above is not inherent to verifiable
computation — it is an artifact of the ISA abstraction layer. A zkVM
cannot distinguish between application state and runtime infrastructure
because the ISA treats all memory uniformly. It cannot specialize
constraint encoding per value type because the ISA erases types. It
cannot eliminate redundant consistency arguments because the ISA makes
memory access patterns opaque at compile time. These are properties of
the *abstraction boundary*, not of the *computation*.

Tabula moves the abstraction boundary. Instead of proving that a
RISC-V program executed correctly, Tabula proves that a
**schema-typed state transition** was applied correctly. The
intermediate representation, the commitment scheme, and the constraint
system are co-designed around a single structural primitive: the typed,
column-partitioned, key-addressed table.

## 4. Thesis

Tabula's central claim is:

> For stateful applications, the natural proof boundary is the state
> transition, not the machine execution. By co-designing the IR,
> commitment scheme, and constraint system around typed tabular state,
> memory consistency cost scales with persistent state accesses — not
> total computation.

This claim rests on a structural observation about computation in
zero-knowledge systems. ZK computation decomposes into two categories
with fundamentally different cost structures:

- **Stateful operations** — reads and writes to persistent storage.
  Expensive in a zkVM because of ISA intermediation, memory
  consistency over the full address space, and application-level state
  commitment.
- **Stateless operations** — arithmetic, comparisons, hashing. Locally
  constrained: a fixed number of AIR rows per operation, with no memory
  consistency overhead. Cost scales predictably with operation count.

General-purpose zkVMs do not distinguish between the two. Both pass
through the same ISA trace, paying the same per-instruction overhead.
The ISA abstraction artificially equalizes the marginal proving cost of
an addition and a database read — both become sequences of RISC-V
instructions, constrained identically.

Tabula separates them. State lives in tables, with commitment and
consistency handled by purpose-built AIR chips. Computation lives in an
execution chip that constrains each operation type directly — no ISA
intermediary. The connection between chips is a LogUp bus protocol: a
multiset equality argument that ensures cross-chip consistency without
requiring a single monolithic constraint system.

## 5. Architecture Overview

### 5.1 Typed Tabular State

A state cell in Tabula is addressed by `(TableId, ColId, RowKey)`.
Tables and columns are declared in a schema with explicit types (`Bool`,
`U64`, `I64`, `Bytes32`). This structure is not cosmetic — it drives
three proof-level optimizations that are impossible in a flat memory
model.

**Per-type chip specialization.** Each column type has a known encoding
width: `Bool` occupies 1 field element, `U64` and `I64` occupy 3 (using
a 30+30+4 bit limb decomposition over BabyBear), and `Bytes32` occupies
8 (as native Poseidon2 digest elements). Constraint chips are
parameterized by width class, with per-type constraint sets — no runtime
type dispatch, no worst-case-width padding.

**Per-column commitment.** Different columns have different access
patterns and sizes. A 50-row configuration table and a 10-million-row
ledger need different commitment strategies. Tabula uses a hybrid
scheme: Sorted Sparse Map Commitment (SSMC) — a sorted list with
streaming Poseidon hashing, O(n) commitment with no tree overhead — for
small columns, and Sparse Merkle Tree (SMT) — O(log n) per access,
amortized over the batch — for large columns. The strategy is selected
per-column at compile time based on the schema. Untouched columns
require zero proof work.

**Per-(table, column) partitioning.** The memory consistency argument
is partitioned by `(t, c)`. Multiple transactions in a batch that
touch the same column share consistency costs. The partitioning is
natural because `(t, c)` are statically known in the IR — only the
row key is dynamic.

### 5.2 Normal Form: Compile-Time Access Analysis

Table accesses in Tabula's IR use `(table, column, row)` addresses where
table and column are static constants. This restricted address structure
enables **normal-form rules** — compile-time structural invariants
enforced by the compiler during program registration:

- **NF-1 (Unique-Read):** At most one `Read` per `(t, c, r)` per
  transaction.
- **NF-2 (Unique-Write):** At most one `Write` per `(t, c, r)` per
  transaction.
- **NF-3 (No-Read-After-Write):** A cell cannot be read after it has
  been written in the same transaction.
- **NF-4 (Key-Alias Resolvability):** Row expressions must be provably
  equal or provably distinct by compile-time inspection of the program's
  restricted row-expression language (literals, parameters, or SSA
  slots).

These rules are not runtime assertions — they are structural properties
of the public program IR, checkable by anyone including the verifier.
Their consequence for the proof system is dramatic: **the entire
intra-transaction memory consistency argument is eliminated.**

In a general-purpose zkVM, the sorted-memory argument must operate
within every transaction (because the VM cannot know at compile time
whether two memory accesses touch the same address) and across
transactions. In Tabula, NF-1 and NF-2 make per-cell access sequences
within a transaction trivially consistent. NF-3 prevents read-after-write
forwarding. NF-4 resolves key identity at compile time. The only memory
consistency argument that remains is **inter-transaction** — across the
batch — handled by a single SortedMem chip partitioned by `(t, c)`.

This is where the compiler does work that the proof system would
otherwise have to do. The normal-form rules shift complexity from
runtime proving (expensive, per-execution) to compile-time validation
(cheap, once per program). The result is a structural guarantee that
every valid program produces a unique, deterministic execution trace.

### 5.3 True SSA: No Stack, No Heap

Tabula's IR uses strict Static Single Assignment: each local variable is
a slot assigned exactly once. SSA slots are not memory — they are
**columns in the execution trace** that carry values from their
definition row to their use rows. No memory-table entry, no
sorted-memory row, no consistency argument.

A transaction with 5 reads, 100 arithmetic operations, and 3 writes
produces exactly 8 entries in the sorted-memory table — one per state
access. In a zkVM executing the same logic, every stack push, local
variable, function call frame, and intermediate result generates a
memory access, easily adding 100+ rows to the sorted-memory proof.

This transforms memory consistency from O(total instructions) to
O(state accesses). For workloads where the ratio of computation to
state touches is high — which characterizes most stateful applications —
the reduction is substantial.

### 5.4 Co-Designed Multi-Chip Architecture

Tabula's proof system decomposes into purpose-built AIR chips, each
responsible for a single semantic role, connected by a LogUp bus
protocol:

| IR construct | AIR chips |
|---|---|
| `Read(t, c, r)` | ExecutionChip + InterTxOrderChip + StateColumnChip |
| `Write(t, c, r, v)` | ExecutionChip + InterTxOrderChip + StateColumnChip |
| `Arith(dst, op, a, b)` | ExecutionChip (inline) |
| `Hash(inputs)` | ExecutionChip + PoseidonChip |
| `Assert(cond)` | ExecutionChip (inline) |
| Column commitment | StateColumnChip (SSMC hash chains) |
| State root transition | ColumnMetaChip + SmtPathChip |

Each IR instruction maps directly to chip rows. A `Read` is one row in
the execution trace and one entry in the inter-transaction ordering chip
— not dozens of fetch-decode-execute cycles. State root computation is
constrained by ColumnMetaChip and SmtPathChip, not hashed through an
ISA.

The system currently comprises 9 chips connected by 11 LogUp buses,
with a total column budget of approximately 670 columns (the largest
being ExecutionChip at 278 columns for the standard `U64` width class).
Chips are independently constrained; cross-chip consistency is enforced
by the LogUp multiset equality argument over BabyBear^4 (~124-bit
security).

This architecture is extensible. The same bus protocol that connects
built-in chips can connect new ones. The PoseidonChip already works
this way: the ExecutionChip sends hash requests via the
PoseidonPermutation bus, the PoseidonChip receives and constrains them.
An ECDSA verification chip or an application-specific chip follows the
same pattern — define columns, define constraints, plug into the bus.

### 5.5 The DSL: Developer-Facing Surface

Tabula includes a domain-specific language that compiles to the IR.
The design philosophy is transparency: the DSL is a "skin over the IR,
not a replacement for it." Every DSL construct maps to known IR
instructions, and the compilation model is predictable — a developer
reading Tabula source can mentally trace the emitted instructions.

```
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance;
    let recv_bal   = balances[to].balance;
    assert sender_bal >= amount;
    let new_sender = sender_bal - amount;
    let new_recv   = recv_bal + amount;
    balances[from].balance = new_sender;
    balances[to].balance   = new_recv;
}
```

This compiles to 8 IR instructions: 2 `Read`, 1 `Cmp`, 1 `Assert`,
2 `Arith`, 2 `Write`. The same program in a zkVM would compile to
hundreds of RISC-V instructions, each a row in the execution trace.

The language is intentionally restricted. There is no `if/else` (the
only control mechanism is `assert`-or-abort), no loops, no function
calls, no modules. These omissions are deliberate: they are the
structural properties that enable the normal-form rules and, by
extension, the proof optimizations described above. Un-provable programs
are un-writeable.

This differs from the zkVM approach in a subtle but important way. A
zkVM makes *all* programs provable by constraining the ISA — but at the
cost of proving far more than the application requires. Tabula makes
*fewer* programs expressible but ensures that every expressible program
is efficiently provable. The question of whether this restricted
expressibility is sufficient for practical use cases is the central
research question of this work.

## 6. What the Proof Guarantees

For a batch of transactions `[tx_0, ..., tx_{N-1}]`, Tabula proves the
state transition `oldRoot → newRoot` with the following public inputs
visible to the verifier:

- `oldRoot`, `newRoot`: cryptographic state root digests
- `AppliedTxDigest`: commitment to the applied transaction list
- `ProgramRoot`: commitment to the registered transaction type
  definitions
- `StaticTableRoot`: commitment to static lookup tables
- `budgets`: resource limits (max operations, slots, accesses)

The proof guarantees four properties:

1. Each transaction executed correctly: instruction semantics (arithmetic,
   comparisons, assertions) were respected.
2. Each Read returned the correct value: read values are consistent with
   the committed pre-state or with writes from earlier transactions in
   the batch.
3. Each Write produced a correct state update: write values were
   correctly merged into the post-state.
4. The new state root reflects all writes correctly: the commitment
   transition from `oldRoot` to `newRoot` is cryptographically valid.

This is a *semantic correctness guarantee for state transition* — not
merely evidence that a machine executed a program. The verifier learns
exactly what matters: the state changed correctly.

## 7. Positioning and Scope

Tabula does not claim to replace general-purpose zkVMs. zkVMs prove
arbitrary computation, and that generality is essential for many use
cases — complex business logic, arbitrary smart contracts, general
verifiable computation. Tabula targets a narrower domain: **stateful
applications where persistent state transitions dominate the proving
workload.** Rollup state transitions, verifiable databases, on-chain
settlement layers — applications where the pattern is
read-compute-write over typed, structured state.

For this domain, the co-design of IR, commitment scheme, and constraint
system around typed tabular state eliminates the ISA layer and enables
structural optimizations — per-type encoding, per-column commitment,
compile-time access analysis, state-only memory consistency — that a
flat memory model fundamentally cannot express. Preliminary benchmarks
indicate proving-time improvements exceeding two orders of magnitude
relative to general-purpose zkVMs on representative state-transition
workloads. This is expected: the proof is doing structurally less work,
because the abstraction boundary eliminates the ISA overhead that
dominates zkVM proving cost.

The research contribution is not a faster implementation of the same
abstraction, but a **different abstraction boundary** for a
well-defined application class — with a concrete implementation
demonstrating that the boundary shift is tractable and the resulting
system is practical.

| | Circuit DSL | General-purpose zkVM | Tabula |
|---|---|---|---|
| Developer experience | Low (crypto expertise required) | High (familiar languages) | Medium (domain-specific, but no crypto knowledge) |
| Per-application audit | Required | Not required | Not required |
| State model | Application-defined | Flat byte memory | Typed tables |
| Instruction set | Constraint-specific | RISC-V / WASM | Tabula IR |
| State commitment | Application-defined | Application-level | Native AIR chips |
| Memory consistency | Application-defined | All memory (stack + heap + state) | State accesses only |
| Intra-tx consistency | Application-defined | Full sorted-memory | None (eliminated by NF rules) |
| Local variables | Wires (manual) | Memory (stack/heap) | Trace columns (SSA, automatic) |
| Proving efficiency | Optimal (hand-tuned) | 10-100x overhead | Near-optimal for stateful workloads |
| Expressibility | Arbitrary (but manual) | Arbitrary | Restricted (state transitions) |
| Extension mechanism | N/A | Precompiles (ISA boundary) | Custom AIR chips (LogUp bus) |

The question this work addresses is whether the restricted expressibility
of a typed state-transition model is sufficient to cover the workloads
that currently drive zkVM adoption — and whether the structural
efficiency gains from co-designing around that model justify the
narrower scope. Tabula's answer is that for the class of applications
defined by read-compute-write over persistent state, the answer is yes:
the abstraction boundary *should* match the application semantics, not
the machine architecture.
