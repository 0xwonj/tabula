# tabula-ir

Intermediate representation for the Tabula kernel.

## Role

Defines the IR that sits between the DSL compiler and the executor:
`Instruction` enum, `TxTypeDef`, `Program` (with SSA validation and
normal-form enforcement), and compiler passes.

Depends only on `tabula-core`.

## Key Invariants

**True SSA.** Each destination slot is assigned at most once per tx body.
There is no reassignment, no phi nodes, no control flow. A flat
instruction sequence with single-assignment slots — this is what makes
the IR directly provable.

**Normal form (NF).** Four structural rules enforced at registration time:

- **NF-1 (Unique-Read):** At most one `Read` per `(t, c, r)` per tx.
- **NF-2 (Unique-Write):** At most one `Write` per `(t, c, r)` per tx.
- **NF-3 (No-Read-After-Write):** Cannot read a cell after writing to it.
- **NF-4 (Key-Alias Resolvability):** Row expressions targeting the same
  `(t, c)` must be provably equal or provably distinct — ambiguous pairs
  are rejected.

These rules are not runtime checks — they are compile-time structural
guarantees enforced by `Program::register()` (canonicalize → typecheck → validate).

**Two-slot Read/Write.** `Read` produces `(dst_val, dst_is_null)`;
`Write` takes `(src_val, src_is_null)`. Null is a boolean flag, not a
value type.
