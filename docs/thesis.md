# Tabula Thesis

Tabula moves the abstraction boundary from ISA-level execution to
schema-typed state transitions. Memory consistency scales with state
accesses, not total computation.

## 1. The Problem

General-purpose zkVMs (SP1, RISC Zero, etc.) prove machine execution:
a RISC-V program runs, and the prover generates a STARK that the
execution trace is correct. This is general and powerful, but
structurally wasteful for stateful applications.

**Every state access goes through the full ISA pipeline.** A database read
— conceptually, "look up a value by key" — becomes: load address into
register, call memory-read syscall, decode result bytes, store to stack.
Each step is a RISC-V instruction, each instruction is a row in the
execution trace, each row is constrained in the AIR.

**Memory consistency covers everything.** The sorted-memory argument that
ensures "every read returns the last-written value" must cover *all*
memory accesses: stack frames, heap allocations, function arguments,
temporaries — not just application state. The proving cost scales with
total memory operations, most of which are infrastructure.

**Memory is untyped and flat.** The prover sees a byte array. It cannot
exploit the fact that a column contains only u64 values, or that a table
has 50 rows. Type information is lost at the ISA boundary, and with it,
any opportunity for type-aware optimization.

**State root computation is application-level.** zkVMs do not manage
state commitment — they prove execution. When a rollup needs a state
root, the application computes it: hashing leaves, building Merkle
paths, updating the tree — all as RISC-V instructions (or precompiles),
all proven through the ISA. Updating 100 accounts in a depth-20 Merkle
tree requires on the order of 2000 hash invocations, all proven as ISA
execution.

For a batch of 100 balance transfers, the business logic is trivial:
read, compare, subtract, add, write. The proving cost is ISA overhead,
memory consistency over stack+heap+state, and ~2000 hashes for the
state root. The actual application semantics are a small fraction of
the total constraint count.

## 2. The Approach

Computation in ZK has two categories with fundamentally different cost
structures:

- **Stateful** — reads and writes to persistent storage. Expensive because
  of infrastructure: ISA overhead, memory consistency, state root hashing.
- **Stateless** — pure computation: arithmetic, comparisons, hashing.
  Localized: a fixed number of constraints per operation, no memory
  consistency overhead. Cost is predictable and scales with op count
  alone.

General-purpose zkVMs do not distinguish between the two. Both go through
the same ISA trace, paying the same per-instruction overhead. The ISA
abstraction layer artificially equalizes the marginal proving cost of an
arithmetic operation and a state access — a balance comparison and a
database read both become sequences of RISC-V instructions, each
constrained identically in the AIR.

Tabula separates them:

- **State lives in tables** — typed, column-partitioned, key-addressed.
  The IR, commitment scheme, and AIR constraints are co-designed around
  this structure. State root computation is a native protocol concern
  handled by purpose-built AIR chips, not application code proven
  through an ISA.
- **Computation lives in chips** — each operation type (arithmetic,
  hashing, comparison) is constrained by its own AIR chip, connected
  via a LogUp bus protocol. No ISA intermediary.

The rest of this document argues why tables are the right structure for
state, and how the co-design around them enables optimizations that are
impossible in a flat memory model.

## 3. Tables for State

Tables — typed, column-partitioned, key-addressed state — are not an
arbitrary choice. They emerge from the requirements of efficient ZK
proving.

### 3.1 Per-Type Chip Specialization

Each column has a known type from the schema. This lets the constraint
system specialize per type: a Bool column uses 1-FE-wide AIR chips, a
U64 column uses 3-FE-wide chips with limb decomposition, a Bytes32
column uses 8-FE-wide native digest chips. Type information directly
reduces trace width, constraint degree, and lookup table size — each
width class has its own optimized constraint set with no branching and
no dynamic dispatch.

An untyped memory model must either use the widest encoding for all
values, or pay constraint overhead to dispatch on type at runtime.

### 3.2 Per-Column Commitment

Not all state is equal. A 50-row config table and a 10M-row ledger need
different commitment strategies:

- **Small columns:** Sorted Sparse Map Commitment (SSMC) — a sorted list
  with streaming Poseidon. O(n) commitment, no tree overhead.
- **Large columns:** Sparse Merkle Tree (SMT) — O(log n) per access,
  amortized over the batch.

Columns are declared in the schema, so the prover pre-selects the
optimal strategy at compile time. Multiple transactions in a batch that
touch the same column share commitment costs — the sorted-memory
argument is partitioned by (table, column), and untouched columns
require zero proof work.

In a flat memory model, commitment granularity is fixed at the VM
boundary — one strategy for all of memory. In Tabula, commitment
granularity is schema-level: each column independently selects the
strategy that matches its access pattern.

### 3.3 Compile-Time Access Analysis

Table accesses use `(table, column, row)` addresses, where table and
column are statically known. Only the row key is dynamic. This enables
**normal-form rules** — compile-time structural invariants:

- **NF-1:** At most one Read per (t, c, r) per tx.
- **NF-2:** At most one Write per (t, c, r) per tx.
- **NF-3:** No Read after Write to the same cell.
- **NF-4:** Row expressions must be provably equal or provably distinct.
  ("Provably" here means decidable from the IR's restricted row-expression
  language — literal, parameter, or slot — by compile-time inspection of
  the public program.)

These guarantee a **unique, deterministic execution trace**. Concretely,
normal form eliminates the entire intra-transaction memory consistency
argument:

- No per-transaction sorted-memory table (NF-1, NF-2 make access
  sequences trivially consistent).
- No intra-transaction last-write tracking or write coalescing (NF-2
  gives syntactic uniqueness).
- No runtime address equality checking (NF-4 resolves key identity at
  compile time).
- No read-after-write forwarding logic (NF-3 prevents the case entirely).

The only memory consistency argument that remains is **inter-transaction**
— across the batch, not within a single transaction. This is handled by
the SortedMemChip, partitioned by (table, column).

Normal form is a structural property of the public program IR, not a
runtime assertion. Anyone — including the verifier — can check it by
inspecting the program. The proof system does not re-prove normal form
inside the STARK; it proves that execution of a valid program produced
the claimed state transition.

In a general-purpose zkVM, memory addresses are computed at runtime —
you cannot know at compile time whether two accesses touch the same
cell, so the full sorted-memory argument (intra- and inter-transaction)
is unavoidable.

## 4. SSA: No Stack, No Heap

Tabula's IR uses strict SSA: each local variable is a slot assigned
exactly once. SSA slots are not memory — they are **wires in the
execution trace**. A slot is a column that carries a value from its
definition row to its use rows. No memory table entry, no sorted-memory
row, no consistency argument. Only persistent state accesses (Read/Write
to table cells) enter the sorted-memory argument.

A transaction with 5 Reads, 100 arithmetic operations, and 3 Writes
produces 8 entries in the sorted-memory table. In a zkVM, the same
logic generates memory entries for every stack push, local variable,
function call frame, and intermediate result — easily 100+ additional
entries, each adding a row to the sorted-memory proof.

This transforms memory consistency from **O(total instructions)** to
**O(state accesses)**. For workloads where computation is large but
the number of persistent state touches is comparatively small, the
reduction is substantial.

## 5. Co-Design

Tabula's architecture co-designs the IR, commitment scheme, and AIR
constraints as a single system:

```
  IR instruction          →  purpose-built AIR chip
  ─────────────              ──────────────────────
  Read(t, c, r)           →  ExecutionChip + SortedMemChip
  Write(t, c, r, v)       →  ExecutionChip + SortedMemChip + MergeChip
  Hash(inputs)            →  ExecutionChip + PoseidonChip
  Assert(cond)            →  ExecutionChip (inline constraint)
  Arith(dst, op, a, b)    →  ExecutionChip (inline constraint)

  Column commitment        →  SSMCChip + ColumnMetaChip
  State root transition    →  ColumnMetaChip (SMT inclusion proofs)
```

Each IR instruction maps directly to AIR chips. A `Read` is one row in
the execution trace and one entry in the sorted-memory table — not
dozens of fetch-decode-execute cycles. State root computation is
constrained by SSMCChip and ColumnMetaChip, not hashed through an ISA.

The LogUp bus protocol connects chips: ExecutionChip sends memory access
tuples, SortedMemChip validates them, SSMCChip computes commitments,
ColumnMetaChip wires everything to the state root. Each chip is
independently constrained; cross-chip consistency is a multiset argument.

**Extension.** The same bus protocol that connects built-in chips can
connect new ones. PoseidonChip already works this way: ExecutionChip
sends hash requests, PoseidonChip receives and constrains them. An
ECDSA chip or application-specific chip follows the same pattern — define
columns, define constraints, plug into the bus.

**Precompiles optimize operations; co-design optimizes the abstraction
boundary.** General-purpose zkVMs can add precompiled circuits for
specific operations (hashing, signature verification). But a precompile
still crosses the ISA call boundary — arguments are marshalled through
registers and memory, results decoded back. It cannot specialize its
encoding per column type, cannot eliminate the intra-transaction memory
argument, and cannot enforce structural invariants like normal form.

## 6. Summary

| | General-purpose zkVM | Tabula |
|---|---|---|
| State model | Flat byte memory | Typed, column-partitioned tables |
| Instruction set | RISC-V / WASM | Tabula IR (TIR) |
| State commitment | Application-level (ISA/precompile) | Native AIR chips |
| Value encoding | Word-granularity, untyped | Type-aware, per-column width class |
| Access analysis | Runtime only | Compile-time (NF rules) |
| Memory consistency | All memory (stack + heap + state) | State accesses only |
| Memory argument scope | Whole VM (intra + inter tx) | Inter-tx only, per (table, column) |
| Local variables | Memory (stack/heap) | Trace columns (SSA) |
| Extension | Precompiles (ISA boundary) | Custom AIR chips (LogUp bus) |

General-purpose zkVMs prove arbitrary computation — that is their
strength. Tabula targets a narrower domain: stateful applications where
persistent state transitions dominate the workload. For this domain,
co-designing the IR, commitment, and constraint system around typed
tabular state eliminates the ISA layer and enables structural
optimizations — per-type chips, per-column commitment, compile-time
access analysis, state-only memory consistency — that a flat memory
model cannot express.
