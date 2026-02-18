# Tabula Semantics Spec (v0.3.0)

## Program Semantics, Core IR Contract, and Canonical State Normal Form

### Scope

This document defines:

* **Runtime Core IR contract** — the normative specification of Tabula's intermediate representation (types, operations, well-formedness rules, budgets)
* **Canonical State Normal Form** — the protocol-level **structural IR invariants** that make proving tractable (unique-read, unique-write, no-read-after-write), enforced by the compiler and `Program::register()`
* **Execution semantics** — how transactions read, compute, and write state
* **Lowering rules** — how the surface language maps to Core IR under the normal form

This document does NOT define:

* AIR constraints, LogUp arguments, trace layout, or STARK integration — see [proof-spec.md](./proof-spec.md)
* Cryptographic building blocks (Poseidon, SMT, SSMC) — see [proof-spec.md](./proof-spec.md) §4
* Surface language grammar — see [dsl-philosophy.md](../dsl-philosophy.md)

**Relationship to proof-spec.** The proof system ([proof-spec.md](./proof-spec.md)) assumes all programs satisfy the invariants defined here. In particular, the canonical state normal form (§2) is what eliminates the need for an intra-tx RAM-consistency argument — both for state cells (unique-read/write) and for local variables (true SSA).

---

## 1. Runtime Core IR Contract (Normative)

### 1.1 Entities

| Entity | Description |
|--------|-------------|
| **Program** | A set of named transaction types, each with a parameter schema and instruction body |
| **TxTypeDef** | `(id, name, param_schema, body: Vec<Instruction>)` — currently single-block; future: CFG of BasicBlocks |
| **Instruction** | A single operation (see §1.5) |
| **Slot** | `u16` index identifying a single-assignment local value (SSA wire) |

### 1.2 True SSA (Single Static Assignment)

**Invariant.** In a well-formed Tabula IR transaction body, each destination slot index (`dst`) appears **at most once** across all instructions in the body. Each slot is assigned exactly once; all subsequent uses reference the assigned slot by index (wire semantics).

**Multi-result operations.** `Read` assigns two slots `(dst_val, dst_is_null)` and `DivMod` assigns two slots `(dst_q, dst_r)`. All count as definitions; none may appear as a destination elsewhere. The two destination slots of a multi-result operation MUST be distinct (`dst_val ≠ dst_is_null`, `dst_q ≠ dst_r`).

**Def-before-use.** Every `Slot(s)` operand must reference a slot defined by an earlier instruction in the body. Using an undefined slot is ill-formed.

**Enforcement.** `Program::register()` MUST validate these invariants and reject programs that violate them.

### 1.3 Types

#### Value Types (v1)

| Type | Rust enum | Width `w(T)` | Description |
|------|-----------|--------------|-------------|
| `Bool` | `Value::Bool(b)` | 1 | Boolean `{true, false}` |
| `U64` | `Value::U64(x)` | 3 | Unsigned 64-bit integer |
| `I64` | `Value::I64(x)` | 3 | Signed 64-bit integer (offset-encoded for proof: `x + 2^63`) |
| `Digest` | `Value::Bytes32(h)` | 8 | Hash output — see §1.3.2 |

`w(T)` is the **encoding width** in field elements, a compile-time constant per column (determined by the column's schema type).

**`Null` is not a value type.** Key absence is represented by a separate `val_is_null: Bool` flag — see §1.3.1.

#### 1.3.1 Null Semantics

Key absence is expressed as a **separate boolean flag** (`val_is_null`), not as a value variant:

* `Read` produces two results: `dst_val` (typed `T`, the column's schema type) and `dst_is_null` (`Bool`). When `val_is_null = true`, `dst_val` MUST equal the canonical zero of type `T`.
* `Write` takes two source expressions: `src_val` (typed `T`) and `src_is_null` (`Bool`). When `src_is_null = true`, the write is a **delete** — removes the key from the database. `src_val` MUST be the canonical zero of type `T`.
* Arithmetic and hash operations accept only typed value slots, never `is_null` flags as value operands (they are `Bool` flags usable in predicates).
* To check key existence after a Read: `Cmp(dst_eq, Eq, Slot(dst_is_null), Literal(Bool(false)))` then `Assert(Slot(dst_eq))`.

**Canonical zero.** When `val_is_null = true`, the value slot MUST contain the canonical zero of the column's schema type. This is a protocol-level MUST (not merely a convention) — it ensures deterministic trace encoding and prevents the prover from smuggling information in "don't-care" slots.

**Enforcement.** For `Read`, the executor enforces this by producing `zero_value(T)` when the key is absent. For `Write` with `src_is_null = true`, the compiler MUST emit `src_val = zero_value(T)`. The executor does NOT validate this at runtime (it ignores `src_val` when `is_null = true`). The AIR constraint layer enforces `val_is_null = 1 ⇒ val = 0^{w(T)}` (proof-spec §8.2), providing the definitive check.

> **Rust IR.** The implementation matches this normative form: `Read { dst_val, dst_is_null, table, col, row }`. `Value::Null` has been removed; the state layer uses `Option<Value>` and the overlay produces events with `val_is_null: bool`.

#### 1.3.2 Digest Representation

`Digest` has a dual representation:

* **Runtime (Rust):** `Value::Bytes32([u8; 32])` — 32 bytes for IO, serialization, and executor logic.
* **Proof (BabyBear):** 8 native BabyBear field elements (Poseidon2 squeeze output). NOT byte-decomposed.

The `ValueCodec` trait maps between these representations. The proof-spec (§10.3) defines the encoding as `w(Digest) = 8` native field elements.

### 1.4 DB Path Model

A cell key is `k := (t, c, r)`:

* `t`: table identifier (`TableId`)
* `c`: column identifier (`ColId`)
* `r`: row key (`RowKey`, sparse u64)

**Static Coordinates Invariant (MUST).** `(t, c)` are **compile-time constants** — they appear directly as instruction fields and MUST NOT be computed at runtime. Only `r` may be dynamic (`RowExpr::Slot` / `RowExpr::Param`).

> **Lemma.** For every instruction in a well-formed program, `(t,c)` are compile-time constants. This enables per-`(t,c)` sharding of memory arguments without runtime dispatch.

### 1.5 Operations

#### Pure Operations (no state effects)

**Arithmetic:**

| Op | Signature | Notes |
|----|-----------|-------|
| `Arith` | `(dst, op: ArithOp, lhs: ValueExpr, rhs: ValueExpr)` | `op ∈ {Add, Sub, Mul}`. Same-type numeric; overflow/underflow → abort |
| `DivMod` | `(dst_q, dst_r, lhs: ValueExpr, rhs: ValueExpr)` | `rhs=0` → abort; two results |

> **Rust IR.** `Instruction::Arith { dst, op: ArithOp, lhs, rhs }` — a single enum variant with `ArithOp ∈ {Add, Sub, Mul}`. In proof-spec's ExecutionChip, each `ArithOp` maps to a distinct opcode one-hot flag.

**Comparison and logical:**

| Op | Signature | Notes |
|----|-----------|-------|
| `Cmp` | `(dst, op: CmpOp, lhs: ValueExpr, rhs: ValueExpr)` | `op ∈ {Eq, Ne, Lt, Lte, Gt, Gte}`. Same-type operands; result is `Bool` — see §1.5.7 |
| `Not` | `(dst, src: ValueExpr(Bool))` | Logical negation; `Bool → Bool` |
| `And` | `(dst, lhs: ValueExpr(Bool), rhs: ValueExpr(Bool))` | Logical AND; `Bool × Bool → Bool` |
| `Or` | `(dst, lhs: ValueExpr(Bool), rhs: ValueExpr(Bool))` | Logical OR; `Bool × Bool → Bool` |

**Control and hashing:**

| Op | Signature | Notes |
|----|-----------|-------|
| `Assert` | `(cond: ValueExpr(Bool))` | `cond = false` → abort tx. No destination slot |
| `Hash` | `(dst, inputs: Vec<ValueExpr>)` | Output: Digest (8 FE). Uses canonical `hash_id` — see §1.5.5 |
| `Select` | `(dst, cond: ValueExpr(Bool), if_true: ValueExpr(T), if_false: ValueExpr(T))` | Typed conditional — see §1.5.6 |

#### Effect Operations (state access)

| Op | Signature | Notes |
|----|-----------|-------|
| `Read` | `(dst_val, dst_is_null, t, c, r)` | Opens base state; two results — see §1.5.1 |
| `Write` | `(t, c, r, src_val, src_is_null)` | Stores value to state — see §1.5.2 |

#### Non-protocol Operations

| Op | Signature | Notes |
|----|-----------|-------|
| `Lookup` | `(dst, static_table, c, r)` | Static table read — see §1.5.4 |
| `Emit` | `(topic, values)` | Output event — see §1.5.3 |

#### 1.5.1 Read Semantics

`Read(dst_val, dst_is_null, t, c, r)` opens the base state at key `(t, c, r)` and produces two SSA slots:

```
Read(t, c, r) → (dst_val: T, dst_is_null: Bool)
```

where `T` is the schema type of column `(t, c)`. In the proof trace, `dst_val` is encoded into `w(T)` field elements via `ValueCodec`.

* `dst_is_null = false` (key present): `dst_val` is the committed base-state value.
* `dst_is_null = true` (key absent): non-membership. `dst_val` MUST equal the canonical zero of type `T`.

Semantically, each `Read` queries the committed base state. In the proof system, base-state openings are amortized: one opening per unique `(t,c,r)` per **batch** via init rows (proof-spec §8). NF-1 (Unique-Read) guarantees at most one `Read` per `(t,c,r)` per tx, so within a single tx the correspondence is 1:1.

> **Rust IR.** `Read { dst_val, dst_is_null, table, col, row }` — matches the normative two-slot form.

#### 1.5.2 Write Semantics

`Write(t, c, r, src_val, src_is_null)` stores a value to state:

* `src_is_null = false`: stores `src_val` as the new value. Key becomes present.
* `src_is_null = true`: **delete** — removes the key. `src_val` MUST be the canonical zero of type `T`. In proof-spec's merge trace, represented by source encoding `(s1,s0) = (1,1)`.

Each non-predicated `Write` corresponds to exactly one entry in `WriteSet` (zero if predicated with `pred = 0` — see ok-gating below).

> **Rust IR.** `Write { table, col, row, src_val, src_is_null }` — matches the normative two-source form.

**Predicated writes (ok-gating, future):**

```
Write(t, c, r, src_val, src_is_null, pred)
```

* `pred = 0` → no persistent effect (not in WriteSet). The write is "cancelled."
* `pred = 1` → normal write semantics.
* This enables fixed-format traces where failed txs execute but produce no state change.

#### 1.5.3 Emit (Out-of-Protocol)

`Emit` produces output events (receipts). These are **not committed** in `AppliedTxDigest` and **not verified** by the proof system. `Emit` has no AIR constraint.

> **Rationale.** Keeping events out-of-protocol simplifies the proof system. If events become protocol-relevant (e.g., committed in a receipt trie), `Emit` will acquire constraints. This is an explicit future extension point.

#### 1.5.4 Lookup (Pure, Separate Domain)

`Lookup(dst, static_table, c, r)` reads from a static (immutable) table — not from committed state. It does not produce access rows in the execution trace. In the proof system, static lookups are handled by LogUp/Lasso lookup arguments, separate from the state memory consistency argument.

Static tables are **total functions** over their declared key domain: every valid `(static_table, c, r)` has a value. Therefore `Lookup` has a single `dst` slot (no `dst_is_null`), unlike `Read` which may encounter absent keys. An out-of-domain key is a program error (analogous to an out-of-bounds access), not a null result.

> **Coordinate convention.** `Lookup(dst, static_table, c, r)` uses the same `(table, col, row)` coordinate order as `Read(dst_val, dst_is_null, t, c, r)` — the table identifier comes first, then column, then row.

Static tables referenced by `Lookup` are committed via `StaticTableRoot` as a public input (proof-spec §5.1). Lookup values are verified via LogUp/Lasso arguments against this root, separate from the state memory consistency argument.

#### 1.5.5 Hash Input Encoding (Normative)

`Hash(dst, inputs)` computes `dst = Poseidon(encoded_inputs)` where the input encoding is defined as follows:

```
Hash(inputs: [x_0, ..., x_{n-1}]) → Poseidon_sponge(
    domain_tag_hash || n || ComEnc(x_0) || ComEnc(x_1) || ... || ComEnc(x_{n-1})
)
```

* `domain_tag_hash`: a fixed domain separator for the IR `Hash` instruction (distinct from SSMC/SMT/leaf tags).
* `n`: the number of inputs, encoded as a single field element. This prevents length-extension ambiguity.
* `ComEnc(x_i)`: Tier 1 commitment encoding of each input — `w(T_i)` field elements determined by the operand's type. `Bool` → 1 FE, `U64`/`I64` → 3 FE, `Digest` → 8 FE.

**Rationale.** Without a normative input encoding, different frontends (Rust DSL, Python EDSL, MLIR) could produce different hash values for the same logical inputs. The length prefix prevents `Hash([a, b])` and `Hash([a_lo, a_hi, b])` from colliding when types differ. The type-aware `ComEnc` encoding ensures each input occupies a fixed, type-determined number of field elements.

> **Encoding divergence.** The executor's `Hasher::hash_ir()` uses a **byte-level** encoding: `domain_tag:u8 || n:u32_le || (type_tag:u8 || canonical_bytes(v))...`. The normative encoding above uses **field-element-level** `ComEnc`. These produce different results for the same inputs. The byte-level encoding is used with the mock Blake3 hasher for testing; the field-element encoding is used in the proof system with Poseidon. The witness generator MUST use the field-element encoding. Programs that depend on specific hash outputs are bound to the canonical hasher (§1.7).

#### 1.5.6 Select (Typed Conditional)

`Select(dst, cond, if_true, if_false)` is a pure operation that evaluates to `if_true` when `cond = true` and `if_false` when `cond = false`:

```
Select(dst: Slot, cond: ValueExpr(Bool), if_true: ValueExpr(T), if_false: ValueExpr(T))
```

* `cond` MUST be a `Bool`-typed expression.
* `if_true` and `if_false` MUST have the same type `T`.
* `dst` receives a value of type `T`.
* No abort conditions — `Select` is total.

**In the proof trace**, `Select` maps to a single constraint: `dst = cond · if_true + (1 - cond) · if_false` (per field element of `w(T)`).

**Why this is needed.** `Select` is the minimal primitive for:
* **Conditional delete**: `val' = Select(is_null, canonical_zero, val)` ensures the canonical-zero MUST (§1.3.1) is satisfiable when `is_null` is a runtime value.
* **Ok-gating** (§1.5.2): predicated writes require conditional value selection.
* **Future `if/else` lowering** (§4.5): both branches are evaluated, `Select` picks the result.

Without `Select`, the IR cannot express runtime-conditional values while maintaining the canonical normal form.

> **Rust IR.** `Instruction::Select { dst, cond, if_true, if_false }` is implemented.

#### 1.5.7 Comparison and Logical Operations

**`Cmp(dst, op, lhs, rhs)`** compares two same-type values and produces a `Bool`:

* `op ∈ {Eq, Ne, Lt, Lte, Gt, Gte}`.
* `lhs` and `rhs` MUST have the same type. `Eq`/`Ne` work on all types including `Bool`. `Lt`/`Lte`/`Gt`/`Gte` require ordered types (`U64`, `I64`, `Bool`); `Digest` comparison is not defined.
* `dst` receives `Bool`.

**In the proof trace**, `Cmp` maps to integer comparison gadgets (proof-spec §4.2.R) for ordered types, or field equality for `Eq`/`Ne`.

> **Rust IR.** `Instruction::Cmp { dst, op: CmpOp, lhs, rhs }`.

**`Not(dst, src)`**, **`And(dst, lhs, rhs)`**, **`Or(dst, lhs, rhs)`** are boolean combinators:

* All operands MUST be `Bool`-typed. Result is `Bool`.
* `Not`: `dst = ¬src`. `And`: `dst = lhs ∧ rhs`. `Or`: `dst = lhs ∨ rhs`.
* These are total — no abort conditions.

**In the proof trace**: `Not` → `dst = 1 - src`. `And` → `dst = lhs · rhs`. `Or` → `dst = lhs + rhs - lhs · rhs`. All single-constraint per field element.

> **Rust IR.** `Instruction::Not { dst, src }`, `Instruction::And { dst, lhs, rhs }`, `Instruction::Or { dst, lhs, rhs }`.

### 1.6 Failure Semantics

A transaction **fails** (aborts) when any of the following occurs:

| Condition | Source |
|-----------|--------|
| `Assert` condition is `false` | `Assert` |
| Arithmetic overflow/underflow | `Arith(Add)`, `Arith(Sub)`, `Arith(Mul)` |
| Division by zero | `DivMod` |

> **Note.** Runtime type mismatches cannot occur in well-formed programs: `Program::register()` (§1.9) statically validates that operand types match instruction requirements. Ill-formed programs are rejected at registration time; their execution is undefined.

**Behavior on failure:**

1. Execution **short-circuits**: subsequent instructions are not evaluated.
2. All state effects from the failed tx are **discarded** (no writes persist).
3. **Default policy (v0.9):** failed txs are **excluded from the proof trace** entirely. Only applied (successful) transactions appear in the execution trace and in `AppliedTxDigest`.
4. **Optional upgrade (Ok-gating):** each tx executes fully with an `ok` flag; all persistent writes gated by `pred = ok`. Failed txs execute but produce no state change. See §2.7.

> **Witness generation note.** The executor captures partial events up to the failure point for debugging purposes. These partial events are NOT part of the proof trace.

### 1.7 Canonical Hash Semantics

All protocol-relevant hashing uses the hash function specified by `hash_id`:

* **v0.9 canonical**: Poseidon2 over BabyBear
* The `Hash` IR instruction's result is determined by `hash_id`. Programs that depend on specific hash values (preimage checks, etc.) are bound to the canonical hasher.
* A Blake3-based hasher exists only as a **non-canonical debugging mode** — no proof/statement compatibility is claimed for such runs.

### 1.8 Program Budgets (Planned — Not Yet Implemented)

To prevent trace width/length blowups (DoS risk), each program will carry explicit budgets:

| Budget | Description |
|--------|-------------|
| `max_ops` | Maximum instruction rows |
| `max_slots` | Maximum SSA slot columns. Slot indices MUST satisfy `max(dst) < max_slots`. Compilers SHOULD allocate slots densely from `0..N-1`. |
| `max_accesses` | Maximum number of Read/Write access events |
| Per-`(t,c)` bounds (optional) | `max_accesses_per_group`, `max_unique_rows_per_group` |

**Enforcement (target).** `Program::register()` will validate budgets at registration time. The prover and verifier will also check budgets against the program header / public statement. Programs exceeding budgets will be **invalid**.

> **Status.** Budget enforcement is not yet implemented in the codebase. `ProgramBudgets` struct and the corresponding `register()` checks are planned for a future milestone. Current programs have no upper bound on instruction count or access count beyond what fits in memory.

### 1.9 Program Validation (`verify()`)

A program is valid iff:

1. **Structural**: True SSA (§1.2), def-before-use, no out-of-range slot/param indices
2. **Typing**: Operand types match instruction requirements; schema types match Write destinations. Ill-formed programs are rejected; their execution is undefined.
3. **Budget** *(planned)*: Instruction count ≤ `max_ops`, slot count ≤ `max_slots`, access count ≤ `max_accesses`. See §1.8 for status.
4. **Canonical NF**: Satisfies the state normal form rules (§2)

> **Rust IR.** Pipeline: `canonicalize` → `typecheck` → `validate`. Canonicalization auto-fixes NF-1 (read deduplication + slot renumbering). Typecheck enforces SSA, def-before-use, param bounds, and schema type matching. Validate checks NF-2, NF-3, NF-4.

---

## 2. Canonical State Normal Form (Minimal Protocol)

This section defines the **core protocol invariants** that make Tabula's proof system tractable. These are **mandatory structural rules on the IR itself** — not runtime caching behaviors, not optimizations. They are enforced at compile time by the lowering pipeline and at registration time by `Program::register()` / `verify()`.

### 2.1 State Semantics as a Pure Function

Transaction semantics are a pure function:

```
execute(BaseState, tx) → WriteSet
```

* **Reads** only consult `BaseState` (the committed snapshot at `oldRoot`). In the proof system, base-state openings are amortized per unique `(t,c,r)` per batch (proof-spec §8); within a single tx, NF-1 guarantees at most one Read per key.
* **Writes** only contribute to `WriteSet` (the delta applied to produce `newRoot`). Each `Write` instruction corresponds to at most one entry in the write set (zero if predicated with `pred = 0`).
* There is no intermediate "overlay" or "cache" in the protocol semantics. The function takes immutable base state and produces an immutable write set.

> **Implementation note.** An executor MAY use an overlay data structure (read-cache + write-buffer map) for convenience. This is an engineering choice, not a protocol semantic. Correctness does not depend on caching — it depends on the IR satisfying the normal form rules below.

### 2.2 Batch Semantics

A batch is an ordered sequence of transactions `[tx_0, ..., tx_{N-1}]` executed against an initial committed state `S_0`:

```
S_0 = BaseState (committed snapshot at oldRoot)
for i in 0..N:
    WriteSet_i = execute(S_i, tx_i)
    S_{i+1} = apply(S_i, WriteSet_i)
newRoot = commit(S_N)
```

* `apply(S, ws)` produces a new state where each key `k ∈ ws` is updated (or deleted if `val_is_null = true`), and all other keys are unchanged.
* Transaction `tx_i`'s `BaseState` is `S_i` — it observes all prior transactions' writes.
* The final `WriteSet_batch = S_N \ S_0` (the diff between initial and final state) is the aggregate effect of the batch.

**Intra-tx vs inter-tx.** The NF rules (§2.3) govern **intra-tx** access patterns and eliminate the need for intra-tx RAM-consistency arguments. **Inter-tx** read-after-write consistency (tx `i+1` reading tx `i`'s writes) is proven by the per-`(t,c)` memory argument in proof-spec §8. These are complementary, non-overlapping concerns.

> **Implementation note.** The executor uses an overlay (write-buffer + read-cache) to maintain `S_i` across transactions within a batch. This is an implementation necessity for inter-tx semantics — unlike the intra-tx case where the overlay is merely a convenience.

### 2.3 Mandatory Normal-Form Rules (Per Key)

These rules are **syntactic/structural IR invariants** on each cell key `(t, c, r)` within a single transaction body:

| ID | Rule | Invariant | Enforcement |
|----|------|-----------|-------------|
| **NF-1** | **Unique-Read** | At most one `Read(…, t, c, r)` instruction per key per tx. | IR NF validation (`Program::register()`) + lowering (§4.1). |
| **NF-2** | **Unique-Write** | At most one `Write(t, c, r, …)` instruction per key per tx. | IR NF validation (`Program::register()`) + lowering (§4.2). |
| **NF-3** | **No-Read-After-Write** | If `Write(t, c, r, …)` appears at instruction index `i`, then no `Read(…, t, c, r)` may appear at any index `j > i`. | IR NF validation (`Program::register()`) + lowering (§4.3). |
| **NF-4** | **Key-Alias Resolvability** | For any two state-access instructions targeting the same `(t, c)`, the row expressions must be provably equal or provably distinct (§2.5–2.6). | IR NF validation (`Program::register()`) + lowering (§4.4). |
| — | **Read-your-writes** | After writing a value to key `k`, any subsequent use of that value references the SSA slot holding the written value — not a re-Read. | Expressed naturally by SSA wire semantics. No additional enforcement needed beyond NF-3. |

**Consequence.** For each key `(t,c,r)`, the access pattern is one of:

| Pattern | Base openings | WriteSet entries |
|---------|---------------|-----------------|
| No access | 0 | 0 |
| Read-only | 1 | 0 |
| Write-only | 0 | 1 |
| Read-then-write | 1 | 1 |

No other pattern is possible in a well-formed program under this normal form.

> **Why these are IR-level rules, not runtime behaviors.**
>
> If the IR could contain repeated `Read(k)` instructions and "cache hit" were a runtime behavior, the proof would need to verify that two reads returned the same value — reintroducing an intra-tx key→value consistency obligation (RAM-like argument). By making unique-read a structural IR invariant, this obligation is eliminated entirely. The same reasoning applies to unique-write: if multiple writes were allowed, the proof would need a "last-write-wins" coalescing mechanism. By forbidding duplicate writes at the IR level, the write set is trivially well-formed.

### 2.4 Read/Write Op Semantics (Summary)

See §1.5.1 (`Read`) and §1.5.2 (`Write`) for full definitions. The NF-relevant properties: at most one Read per key per tx (NF-1), at most one Write per key per tx (NF-2), canonical zero when absent/deleted (§1.3.1).

### 2.5 RowExpr Equality

A `RowExpr` (the `r` component of a cell key) takes one of three forms:

| Form | Description |
|------|-------------|
| `Lit(n)` | Literal constant (`n: u64`) |
| `Param(p)` | Transaction parameter (`p: u16` index) |
| `Slot(s)` | SSA slot reference (`s: u16` index) |

**Provably equal.** Two `RowExpr` values are provably equal iff they are **structurally identical**:
* `Lit(a) = Lit(b)` iff `a == b`
* `Param(p) = Param(q)` iff `p == q`
* `Slot(s) = Slot(t)` iff `s == t`

**Provably distinct.** Two `RowExpr` values are provably distinct only if both are literals with different values: `Lit(a) ≠ Lit(b)` when `a ≠ b`.

**Ambiguous.** All other pairs (`Slot` vs `Param`, different `Slot` indices, different `Param` indices, `Slot`/`Param` vs `Lit`). These cannot be statically resolved.

> **Rationale.** This conservative definition enables the NF-1/2/3 rules to be verified by a simple structural pass over the IR — no abstract interpretation or alias analysis needed. `Program::register()` performs this check in O(n) time per `(t,c)` group.

### 2.6 Key Alias Policy

Two row expressions for the same `(t, c)` may alias at runtime (evaluate to the same `r`). The NF rules (§2.3) require at most one Read and one Write per key, so aliasing must be resolvable at compile time using the equality definition from §2.5.

**Policy (v1: conservative).** For any two state-access instructions targeting the same `(t, c)`:

* **Definitely equal**: provably equal per §2.5 (same `Slot(s)`, same `Param(p)`, or same `Lit(n)`).
* **Definitely distinct**: provably distinct per §2.5 (both `Lit` with different values).
* **Ambiguous**: everything else (§2.5).

If two accesses to the same `(t, c)` have **ambiguous** aliasing, the program is **rejected** at registration time.

**Guidance for developers:**

* Store the row key once in a slot; reuse that slot for all accesses to that key.
* Use `Read` to open a key once; reference the result slot for subsequent computations.
* Never access the same `(t, c)` with different row expressions that might alias.

**Future relaxation.** An `open`/`cell` handle model could allow multiple field accesses through a single opened key, making the aliasing constraint transparent to developers:

```
let cell = open(accounts, id);    // single base opening
let bal = cell.balance;           // field access (column), no re-open
let name = cell.name;             // field access (column), no re-open
cell.balance = bal + amount;      // write through handle
```

### 2.7 Failed Transaction Policy

**Default (v0.9).** Failed transactions are excluded from the proof trace entirely. Only applied (successful) transactions appear in the execution trace. `AppliedTxDigest` commits to the ordered list of applied transactions as a public input. See §1.6 for failure conditions.

**Optional upgrade (Ok-gating canonical form).** Each tx executes with an `ok` flag; all persistent writes are gated by `pred = ok`. Failed txs execute but produce no state change. `AppliedTxDigest` commits to `(tx_id, ok)` pairs or a success bitmask.

### 2.8 Bridge to Proof-Spec

The proof system (proof-spec.md) assumes all programs satisfy the invariants in this section. Specifically:

* **NF-1 Unique-Read** → proof needs at most one base opening per `(t,c,r)` per tx. In batch mode, openings are further amortized via init rows (one per unique `(t,c,r)` per batch). No intra-tx read-consistency argument.
* **NF-2 Unique-Write** → **intra-tx** write-coalescing is unnecessary (at most one Write per key per tx). **Inter-tx** coalescing (extracting the last write across the batch) is still required and handled by `GlobalSortedMem` in Layer C (proof-spec §8).
* **NF-3 No-Read-After-Write** → no intra-tx RAM-consistency argument needed for state cells.
* **NF-4 Key-Alias Resolvability** → key identity is compile-time deterministic; no runtime disambiguation needed.
* **True SSA (§1.2)** → no intra-tx RAM-consistency argument needed for local variables. SSA def-use equality is enforced by trace layout wiring (Layout A: carry constraints, Layout B: LogUp def-use).
* **Static Coordinates (§1.4)** → per-`(t,c)` sharding of memory arguments.
* **Budgets (§1.8, planned)** → bounded trace width/length.

---

## 3. Surface Language (Reference)

The Tabula DSL is defined in [dsl-philosophy.md](../dsl-philosophy.md). Key properties relevant to this spec:

* **Linear execution**: source order = IR instruction order. No branching, no loops, no function calls.
* **One binding, one slot**: each `let` creates an immutable SSA slot.
* **Explicit state mutation**: reads are `let` bindings; writes are assignments to cells.
* **Assert-only control**: the only conditional mechanism. `false` → abort entire tx.
* **Deterministic compilation**: same source → same IR, always.

### 3.1 Minimal Grammar (Informative)

```
program    := table_decl* tx_decl*
table_decl := "table" NAME "(" col_def ("," col_def)* ")"
col_def    := NAME ":" TYPE
tx_decl    := "tx" NAME "(" param_def* ")" "{" stmt* "}"
param_def  := NAME ":" TYPE
stmt       := let_stmt | write_stmt | assert_stmt | emit_stmt
let_stmt   := "let" PATTERN "=" expr ";"
write_stmt := cell_ref "=" expr ";"
assert_stmt:= "assert" expr ";"
emit_stmt  := "emit" STRING "(" expr_list ")" ";"
cell_ref   := TABLE "[" expr "]" "." COLUMN
expr       := cell_ref | literal | NAME | unop | binop | hash_call | select_expr
unop       := "!" expr
binop      := expr OP expr
select_expr:= "if" expr "then" expr "else" expr
```

> See `tabula-lang` crate for the full parser implementation.

---

## 4. Lowering / Normalization (Normative)

The compiler lowers surface language to Core IR while ensuring the canonical state normal form (§2). Every lowering rule targets a specific NF invariant.

### 4.1 Read Deduplication (→ NF-1 Unique-Read)

If the source reads the same key `(t, c, r)` multiple times:

* The first read produces two slots `(dst_val, dst_is_null)`.
* Subsequent reads are rewritten to reference the existing slots (CSE / SSA reuse).
* Only one `Read` instruction is emitted per unique `(t, c, r)`.

The lowering pipeline tracks which keys have been opened and reuses existing slots.

### 4.2 Write Uniqueness (→ NF-2 Unique-Write)

**Rule.** For each key `(t, c, r)`, at most one `Write` instruction is emitted in the IR body.

If the source writes to the same key multiple times, the program is **rejected** at registration time (`NfUniqueWrite` error). Unlike NF-1 read deduplication, there is no automatic write folding — the developer must structure the logic so that only the final value is written.

> **Rationale.** Write folding (coalescing multiple writes into one) would require analyzing the computation graph between writes. This is complex and the use case is rare — most programs naturally write each key once. Keeping the rule as a hard reject is simpler and aligns with the "IR itself has at most one Write per key" invariant.

### 4.3 Read-Before-Write Ordering (→ NF-3 No-Read-After-Write)

If the source reads a key after writing it:

* The compiler does NOT emit a `Read`. Instead, it references the SSA slot holding the value that was written — SSA wire semantics.
* This is "read-your-writes" expressed as SSA reuse, not as a state re-opening.

### 4.4 Key Handle Model (→ NF-4 Key-Alias Resolvability)

To avoid aliasing issues (§2.6), the lowering pipeline should:

1. Track which `(t, c, r)` keys have been opened (by SSA slot identity of `r`, or by `Param(p)` identity, or by literal constant identity).
2. Reuse the existing slots for subsequent reads of the same key.
3. Reject programs where key aliasing cannot be statically resolved.

### 4.5 Branch Lowering (Future — Not Yet Implemented)

When `if/else` is added to the surface language, the planned lowering is:

* Both branches are fully evaluated; `Select` picks the result values.
* Conditional writes lower to `Select` + predicated `Write` (ok-gating via `pred` parameter).
* This preserves the linear execution model.

> **Status.** Neither `if/else` in the surface language nor predicated `Write` (the `pred` field) exist in the current codebase. The `Write` instruction has no `pred` parameter today. This section describes the intended future design. See also §1.5.2 for the predicated write specification.

### 4.6 Compilation Errors

The compiler rejects programs that violate:

* True SSA (duplicate dst slot)
* Def-before-use (undefined slot reference)
* Type mismatches (schema vs actual, including Bool requirements for `Assert`/`Not`/`And`/`Or`/`Select` condition)
* Key alias ambiguity (§2.5, NF-4)
* Static coordinate violation (computed `t` or `c`)
* NF-2 Unique-Write violations (no auto-folding — §4.2)
* NF-3 Read-after-write violations

NF-1 Unique-Read violations are auto-fixed by canonicalization (§4.1), not rejected.

---

## 5. Compatibility Notes (Non-normative)

### 5.1 Frontend Independence

The Core IR contract (§1) and canonical state normal form (§2) are **frontend-independent**. Any frontend that produces IR satisfying these invariants is valid:

* **Rust DSL** (current `tabula-lang` crate)
* **Python EDSL** (planned)
* **JSON-authored programs** (current CLI input)
* **MLIR frontend** (future): `tabula.hl` dialect → `tabula.core` dialect → IR exporter

All frontends MUST produce programs that pass `Program::register()` validation (§1.9).

### 5.2 STARK Trace Coupling

The proof system ([proof-spec.md](./proof-spec.md)) defines how Core IR programs map to STARK traces:

* **SSA values** → slot columns (Layout A) or ValueTable (Layout B)
* **State accesses** → access rows in execution trace + entries in GlobalSortedMem
* **Budgets** → trace dimensions

The semantics defined here are the **input contract** for the proof system. Changes to this spec may require corresponding changes to proof-spec.md.

### 5.3 Extension Path: STARK Splitting

The current proof system uses a single STARK for the entire execution. A future extension may split into:

* **Exec-STARK**: instruction correctness, SSA wiring, clock
* **State-STARK**: memory consistency, state commitment transitions

Both STARKs would share the same access log (committed as a public digest). The Core IR contract and normal form defined here remain unchanged — only the proof-spec trace layout changes.
