# PropertyRead Design Analysis

> Deep analysis of PropertyRead/PropertyOpening architecture.
> Evaluates the current design, identifies architectural issues,
> and proposes the optimal implementation for Tabula's sharded proof structure.

## 1. What PropertyRead Does

PropertyRead enables **declarative structural queries** on committed column state:

```
let best_price = property_read orders[market_id].prices.minimum()
let next_order = property_read orders[market_id].prices.successor(current_key)
assert no_gap = property_read orders[market_id].prices.non_existence_range(a, b)
let total_volume = property_read orders[market_id].quantities.aggregate(sum)
```

These queries are impossible with the basic `Read(table, col, row_key)` instruction, which only retrieves a specific key. PropertyRead answers questions about the *structure* of the data: ordering, extrema, gaps, aggregates.

**Why this matters**: Without PropertyRead, a DEX cannot prove "this is the best available price" — a critical soundness property for fair order matching. The prover could skip better-priced orders.

## 2. Current Design (Goal 6 E8)

### 2.1 PropertyOpening Trait

```rust
pub trait PropertyOpening: Send + Sync {
    fn name(&self) -> &str;
    fn compatible_scheme_tag(&self) -> u16;        // Links to ColumnScheme
    fn supported_queries(&self) -> &[PropertyQueryKind];
    fn prove(
        &self,
        commitment_digest: &[BabyBear],            // Column commitment
        query: &PropertyQuery,
        state: &[(RowKey, &[BabyBear], bool)],     // Full column data
    ) -> Result<Box<dyn PropertyWitness>, PropertyError>;
    fn verifier_extension(&self) -> Option<Box<dyn ChipExtension>>; // Verifier chips
}
```

### 2.2 PropertyQuery Enum

```rust
enum PropertyQuery {
    Minimum,                                    // Row with smallest key
    Maximum,                                    // Row with largest key
    Successor { key: RowKey },                  // Next row after key
    Predecessor { key: RowKey },                // Previous row before key
    NonExistenceRange { lower: RowKey, upper: RowKey }, // No keys in range
    Aggregate { kind: AggregateKind },          // Sum or Count
}
```

### 2.3 PropertyWitness (Opaque)

```rust
pub trait PropertyWitness: Send + Sync {
    fn value(&self) -> &[BabyBear];     // Result as field elements
    fn key(&self) -> Option<RowKey>;    // The key satisfying the property
    fn is_null(&self) -> bool;           // No matching key?
    fn as_any(&self) -> &dyn Any;        // Downcast for chip processing
}
```

### 2.4 Current Integration Path

```
MachineBuilder::with_property_opening(opening)
    → opening.verifier_extension() registered in EXECUTION TIER
    → opening stored in TabulaMachine::property_openings
```

## 3. Architectural Problem: Tier Mismatch

**The current design places verifier chips in the wrong tier.**

Tabula's three-tier proof structure:
```
Tier 1 (Execution):  ExecutionChip, StaticTable, Poseidon, RangeCheck
Tier 2 (Column):     MemoryShard, StateShard, MetaShard, Poseidon, RangeCheck
Tier 3 (Root):       SmtColPath, SmtTablePath, Poseidon, RangeCheck
```

The current design registers `verifier_extension()` chips in **Tier 1 (execution)**.
But verification requires access to **column commitment state**, which lives in **Tier 2**.

### 3.1 The Problem in Detail

PropertyOpening verification requires:
1. The column commitment digest (com_old or com_new)
2. The column state data (sorted entries, Merkle paths, etc.)
3. Proof that the query result is consistent with the commitment

All three are available in **Tier 2** (column proofs), where:
- `StateShardChip` processes sorted hash chain entries (com_old → com_new)
- `MetaShardChip` holds commitment metadata (com_old, com_new, scheme_tag)
- `MemoryShardChip` processes read/write access events

If the verifier is in Tier 1, it needs the column commitment as input. But:
- Tier 1 and Tier 2 are **independent proofs** with separate Fiat-Shamir
- They can only communicate through **external buses** (cross-tier LogUp)
- There's no existing mechanism for Tier 1 to receive commitment digests

### 3.2 How Read/Write Already Solve This

The existing `Read`/`Write` instructions demonstrate the correct cross-tier pattern:

```
Tier 1: ExecutionChip SENDS on READ_ACCESS bus
        → (table_id, col_id, row_key, tx_index, value[W], is_null)
Tier 2: MemoryShardChip RECEIVES from READ_ACCESS bus
        → Verifies against column state

Root proof: Verifies READ_ACCESS bus balance across tiers
```

PropertyRead should follow the same pattern.

## 4. Proposed Architecture: PropertyRead as Cross-Tier External Bus

### 4.1 Overview

```
Tier 1 (Execution):
  ExecutionChip adds op_property_read selector
  SENDS on PROPERTY_READ bus: (table_id, col_id, query_type, result_key[W], result_val[W], is_null)

Tier 2 (Column):
  PropertyVerifierChip RECEIVES from PROPERTY_READ bus
  Verifies result key and value against column commitment (com_old)
  Has access to sorted state data from WitnessStore

Tier 3 (Root):
  Verifies PROPERTY_READ bus balance across tiers
  (Automatically handled by existing unbalanced_buses() mechanism)
```

### 4.2 Why This Is Optimal

1. **Verifier has direct access to column state** — no commitment forwarding needed
2. **Follows existing cross-tier pattern** — same as READ_ACCESS/WRITE_ACCESS
3. **Root proof handles bus balance** — existing mechanism, no new infrastructure
4. **Column-parallel verification** — each column's PropertyVerifier runs independently
5. **Minimal ExecutionChip changes** — just one more opcode selector + bus send

### 4.3 What State Does PropertyRead Query?

**Critical design decision**: PropertyRead queries **pre-batch committed state (com_old)**, not the in-flight overlay state.

**Rationale**:
- `com_old` is known at batch start and is verifiable
- The overlay state (mid-batch) has no commitment — can't be verified in ZK
- This matches **snapshot isolation** semantics from databases
- For a DEX: "best price as of the last proven batch" is the correct guarantee

**Implication**: PropertyRead sees the state BEFORE the current batch's modifications. If a transaction writes to a column and then does PropertyRead on the same column, it sees the pre-write state.

This is correct because:
- The batch prover knows the pre-batch state (from the previous proof)
- Each PropertyRead can be independently verified against com_old
- The verifier in Tier 2 checks against the SSMC/SMT commitment that is proven correct

## 5. Query Verification by Scheme

### 5.1 SSMC (Sorted Hash Chain)

SSMC already maintains entries in sorted key order. The `StateShardChip` proves the sorted chain from com_old to com_new. This makes structural queries **nearly free**:

| Query | Verification Cost | How |
|-------|------------------|-----|
| Minimum | O(1) | First entry in sorted chain. Assert result_key = chain[0].key |
| Maximum | O(1) | Last entry in sorted chain. Assert result_key = chain[N-1].key |
| Successor(k) | O(1) | Find adjacent entries where prev.key ≤ k < next.key. Return next |
| Predecessor(k) | O(1) | Find adjacent entries where prev.key < k ≤ next.key. Return prev |
| NonExistenceRange(a,b) | O(1) | Show adjacent entries with keys < a and ≥ b. Gap proves no keys in [a,b) |
| Aggregate(Sum) | O(N) | Running accumulator column in StateShardChip |
| Aggregate(Count) | O(N) | Running counter column in StateShardChip |

**Key insight**: For SSMC, the PropertyVerifierChip can be integrated INTO the existing StateShardChip or run as a lightweight satellite chip that reads the same sorted data. No new Merkle paths or complex proofs needed.

**Verification pattern for Minimum**:
```
PropertyVerifierChip (Tier 2) for SSMC Minimum:
1. Receive from PROPERTY_READ bus: (t, c, QueryType::Minimum, result_key[W], result_val[W], is_null)
2. Read sorted state from WitnessStore (same data as StateShardChip)
3. Assert: result_key = state[0].key (first entry's key in sorted order)
4. Assert: result_val = state[0].value (first entry's value in sorted order)
5. Assert: no entry with smaller key exists (guaranteed by sorted order)
6. Recompute hash chain commitment from sorted entries
7. Assert: commitment matches com_old (received from MetaShard or embedded as witness)
```

### 5.2 SMT (Sparse Merkle Tree)

SMTs are inherently unordered — they organize by key hash, not key value. Structural queries on SMTs are significantly harder:

| Query | Feasibility | Cost |
|-------|-------------|------|
| Minimum | Requires full tree scan | O(N) |
| Maximum | Requires full tree scan | O(N) |
| Successor(k) | Requires indexed variant | O(log N) with index |
| NonExistence(k) | Native SMT proof | O(log N) |
| Aggregate | Requires full tree scan | O(N) |

**Recommendation**: For columns needing structural queries, use SSMC scheme (not SMT). The `compatible_scheme_tag()` method enforces this — a PropertyOpening for Minimum would declare `compatible_scheme_tag() = scheme_tags::SSMC`.

**Future**: An Indexed Merkle Tree variant (Aztec's approach) could support O(log N) successor/predecessor queries on SMT-based columns. This would be a new ColumnScheme, not a modification to existing SMT.

### 5.3 Custom Schemes

The `PropertyOpening` trait is designed for extensibility. A custom ColumnScheme (e.g., B+ tree for an orderbook) would provide its own PropertyOpening with scheme-specific verification:

```rust
struct OrderbookBTreeOpening;
impl PropertyOpening for OrderbookBTreeOpening {
    fn compatible_scheme_tag(&self) -> u16 { BTREE_SCHEME_TAG }
    fn prove(...) -> Result<Box<dyn PropertyWitness>, PropertyError> {
        // B+ tree path proof for min/max/successor
    }
    fn verifier_extension(&self) -> Option<Box<dyn ChipExtension>> {
        // B+ tree path verification AIR
    }
}
```

## 6. Revised Trait Design

### 6.1 PropertyOpening (Minor Changes)

```rust
pub trait PropertyOpening: Send + Sync {
    fn name(&self) -> &str;
    fn compatible_scheme_tag(&self) -> u16;
    fn supported_queries(&self) -> &[PropertyQueryKind];

    /// Prove a structural property about the committed column state.
    ///
    /// `commitment_digest` is com_old (pre-batch commitment).
    /// `state` is the pre-batch column data.
    fn prove(
        &self,
        commitment_digest: &[BabyBear],
        query: &PropertyQuery,
        state: &[(RowKey, &[BabyBear], bool)],
    ) -> Result<Box<dyn PropertyWitness>, PropertyError>;

    /// Verifier chips for Tier 2 (column proof).
    ///
    /// CHANGED: Returns chips for the COLUMN tier, not the execution tier.
    /// These chips receive from PROPERTY_READ bus and verify the query result
    /// against the column commitment.
    fn column_verifier(&self) -> Option<Box<dyn ChipExtension>> {
        None
    }
}
```

**Key change**: `verifier_extension()` → `column_verifier()`. The name clarifies that verifier chips belong in the column tier, not the execution tier.

### 6.2 PropertyQuery (No Changes)

The current enum is complete for v1. All six query types cover the core use cases:
- **DEX**: Minimum (best price), Successor (next order), NonExistenceRange (market gap)
- **Auction**: Maximum (highest bid)
- **Token registry**: NonExistenceRange (unique token ID), Count (total supply)
- **Rate limiter**: Sum (total volume in window), Count (number of operations)

### 6.3 Future Extension: Range Aggregates

```rust
// Future addition (v2):
PropertyQuery::RangeAggregate {
    lower: RowKey,
    upper: RowKey,
    kind: AggregateKind,
}
```

This computes Sum/Count over a subset of keys `[lower, upper)`. Currently deferrable — full-column aggregates cover the common cases.

## 7. IR Instruction Design

### 7.1 Instruction Variant

```rust
/// Query a structural property of committed column state.
///
/// The result is the value at the key satisfying the property
/// (e.g., the value at the minimum key). For aggregate queries,
/// the result is the aggregate value itself.
///
/// Queries operate on pre-batch committed state (com_old),
/// providing snapshot isolation semantics.
PropertyRead {
    /// Destination slot for the result value.
    dst_val: Slot,
    /// Destination slot for the key at the result position.
    dst_key: Slot,
    /// Destination slot for the null flag (true if no matching key).
    dst_is_null: Slot,
    /// Table to query.
    table: TableId,
    /// Column to query.
    col: ColId,
    /// The structural query to execute.
    query: PropertyQuery,
}
```

**Design note**: `PropertyRead` uses three destination slots: value, key, and is_null. The key slot receives the row key satisfying the property (e.g., the key of the minimum-valued row). For aggregate queries (Sum, Count), the key is meaningless and set to null. The is_null flag covers the empty-column case (e.g., Minimum on an empty column returns null).

### 7.2 Interpreter Dispatch

```rust
Instruction::PropertyRead { dst_val, dst_key, dst_is_null, table, col, query } => {
    let opening = ctx.property_openings.find(*table, *col, query.kind())?;
    let col_state = ctx.committed_state.get_column(*table, *col)?;
    let commitment = ctx.committed_state.get_commitment(*table, *col)?;

    let state_tuples: Vec<_> = col_state.entries()
        .map(|(key, fes, is_null)| (key, fes.as_slice(), is_null))
        .collect();

    let witness = opening.prove(&commitment, query, &state_tuples)?;

    let value = decode_value_from_fes(witness.value(), col_type);
    set_slot(&mut slots, *dst_val, value)?;
    set_slot(&mut slots, *dst_key, witness.key().map(|k| Value::U64(k.0)).unwrap_or(Value::U64(0)))?;
    set_slot(&mut slots, *dst_is_null, Value::Bool(witness.is_null()))?;
}
```

**Note**: The interpreter needs access to committed (pre-batch) column state, not the overlay. This requires a new field in `ExecContext`:

```rust
pub struct ExecContext<'a> {
    pub hasher: &'a dyn Hasher,
    pub static_tables: &'a dyn StaticTableProvider,
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
    pub precompiles: &'a PrecompileRegistry,
    pub committed_state: &'a dyn CommittedStateProvider,  // NEW
    pub property_openings: &'a PropertyOpeningRegistry,   // NEW
}
```

## 8. ExecutionChip Integration

### 8.1 New Columns (Minimal)

```rust
// In ExecutionCols:
pub op_property_read: T,            // 1 col (opcode selector)
pub property_query_type: T,         // 1 col (query kind discriminator)
pub property_result_key: [T; W],    // W cols (result key, encoded like RowKey in READ_ACCESS)
```

Total: **2 + W new columns** (5 at W=3). PropertyRead reuses the existing access log columns for (table_id, col_id) and slot write columns for the result value. The result key needs dedicated columns because the existing `access_key` columns may be used differently (PropertyRead is not a regular state access).

### 8.2 Constraints

```rust
fn constrain_property_read<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate = is_real.clone() * local.op_property_read.clone().into();

    // 1. PropertyRead does NOT increment the access clock
    //    (it's not a regular state access — it queries committed state)

    // 2. Result binding to dst_val slot (same pattern as Read)
    for s in 0..MAX_SLOTS {
        let slot_gate = gate.clone() * local.slot_written[s].clone().into();
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone()
                    * (local.slots[s][i].clone().into()
                        - local.access_val[i].clone().into()),
            );
        }
    }
}
```

### 8.3 Bus Interaction

```rust
fn send_property_read<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.op_property_read.clone().into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(3 + W + W + 1);
    values.push(local.access_t.clone().into());          // table_id
    values.push(local.access_c.clone().into());          // col_id
    values.push(local.property_query_type.clone().into()); // query type
    for i in 0..W {
        values.push(local.property_result_key[i].clone().into()); // result key
    }
    for i in 0..W {
        values.push(local.access_val[i].clone().into()); // result value
    }
    values.push(local.access_is_null.clone().into());    // is_null

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::PROPERTY_READ,
    });
}
```

**PROPERTY_READ bus width**: 3 + W + W + 1 = 10 FE (at W=3). External bus (cross-tier). The key uses W field elements for consistency with how RowKey is encoded in the READ_ACCESS bus.

### 8.4 No Clock Increment

PropertyRead does NOT increment the access clock (`clk`). It doesn't access the overlay — it queries committed state. This means:
- `is_access = 0` for PropertyRead
- No READ_ACCESS/WRITE_ACCESS bus interaction
- No MemoryShard processing needed

## 9. Column Tier Verification

### 9.1 SSMC PropertyVerifierChip

For the common case (SSMC columns with min/max/successor queries), the verifier is a lightweight chip in Tier 2:

```rust
pub struct SsmcPropertyVerifierChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for SsmcPropertyVerifierChip<W> {
    fn eval(&self, builder: &mut AB) {
        // 1. Receive from PROPERTY_READ bus
        builder.receive(AirInteraction {
            values: [table_id, col_id, query_type, result_key[W], result_val[W], is_null],
            multiplicity: is_real,
            bus: core_buses::PROPERTY_READ,
        });

        // 2. For Minimum: assert result equals first entry in sorted state
        //    (sorted entries come from witness, commitment verified by hash chain)

        // 3. Verify commitment consistency with StateShardChip
        //    (via shared PoseidonPerm bus — same hash chain)

        // 4. For NonExistenceRange: assert gap between adjacent entries
        //    covers the queried range
    }
}
```

### 9.2 Placement in Column Tier Setup

```rust
fn column_tier_setup_with_scheme(
    config: &ColumnSetupConfig,
    scheme: &dyn ColumnScheme,
    property_openings: &[Box<dyn PropertyOpening>],  // NEW parameter
    alloc: &mut ChipIdAllocator,
) -> Result<TierSetup, SetupError> {
    let mut chip_set = scheme.create_chips(config, alloc)?;

    // Add property verifier chips for this column
    for opening in property_openings {
        if opening.compatible_scheme_tag() == config.scheme_tag {
            if let Some(verifier) = opening.column_verifier() {
                chip_set.airs.extend(verifier.airs());
                chip_set.dyn_chips.extend(verifier.dyn_chips());
            }
        }
    }

    // ... build TierSetup from chip_set
}
```

## 10. Comparison: PropertyRead vs Precompile

| Aspect | PropertyRead | Precompile |
|--------|-------------|------------|
| **What it does** | Queries committed state structure | Computes a pure function |
| **Verification tier** | Tier 2 (column) | Tier 1 (execution) |
| **Bus type** | External (cross-tier) | Internal (within Tier 1) |
| **State access** | Pre-batch committed state | None (pure computation) |
| **Clock increment** | No | No |
| **Column dependency** | Yes (specific table + col) | No |
| **Verifier placement** | Column tier | Execution tier |
| **Commitment binding** | Direct (verifier has access) | Indirect (io_commitment hash) |

**Conclusion**: PropertyRead and Precompile are architecturally distinct. They should be separate IR instructions with separate buses.

## 11. Soundness Requirements

### 11.1 What Must Be Proven

For `property_read T[C].minimum()` returning value V:

1. **Existence**: Key K exists in column (T, C) with value V in committed state
2. **Extremality**: No key K' < K exists in column (T, C) in committed state
3. **Commitment binding**: The committed state is the one committed to by com_old
4. **Cross-tier consistency**: The commitment used by PropertyVerifier matches the actual column commitment in Tier 2

### 11.2 Attack Scenarios

**Attack 1: Forged minimum** — Prover claims min_key=100 when min_key=50.
- Defense: PropertyVerifier checks sorted chain. Entry at key=50 exists → min_key=100 is not the minimum → constraint failure.

**Attack 2: Omitted entry** — Prover removes key=50 from the sorted chain to make key=100 the minimum.
- Defense: Sorted chain commitment (hash chain) changes if entries are omitted. StateShardChip verifies com_old matches the actual chain → commitment mismatch → constraint failure.

**Attack 3: Wrong commitment** — Prover uses a different commitment that omits key=50.
- Defense: Cross-tier bus balance. The commitment used by PropertyVerifier must match MetaShard's com_old. Root proof verifies all bus sums balance → mismatch detected.

**Attack 4: PropertyRead result forged in ExecutionChip** — Prover puts arbitrary value in result slot.
- Defense: PROPERTY_READ bus carries the result. PropertyVerifier receives and checks. Bus sum must balance. If ExecutionChip sends a forged result, PropertyVerifier doesn't receive a matching message → bus imbalance → verification failure.

## 12. Implementation Phases

### Phase 1: IR + Executor (~80 LOC)

- [ ] Add `PropertyQuery` serialization (Borsh + Serde) for IR storage
- [ ] Add `Instruction::PropertyRead` variant
- [ ] Update exhaustive matches (combine with Precompile variant — Phase P1)
- [ ] Add `CommittedStateProvider` trait to core
- [ ] Add `PropertyOpeningRegistry` to executor
- [ ] Interpreter dispatch for PropertyRead
- [ ] Tests: minimum on simple state, null on empty column

### Phase 2: ExecutionChip (~60 LOC)

- [ ] Add `op_property_read` and `property_query_type` columns
- [ ] Add `Opcode::PropertyRead` variant
- [ ] Constraints: result binding to slot, no clock increment
- [ ] Define `PROPERTY_READ: BusId = BusId(18)` (next core bus)
- [ ] Bus send on PROPERTY_READ
- [ ] Update opcode one-hot sum (14 selectors)

### Phase 3: Column Tier Verifier (~150 LOC)

- [ ] `SsmcPropertyVerifier<W>` chip in `chips/src/shards/`
- [ ] AIR: receive from PROPERTY_READ bus, verify against sorted state
- [ ] Trace generation: read sorted entries from WitnessStore
- [ ] Verification for Minimum, Maximum, Successor, Predecessor
- [ ] Verification for NonExistenceRange
- [ ] Wire into `column_tier_setup_with_scheme()`
- [ ] Change `verifier_extension()` → `column_verifier()` in PropertyOpening trait

### Phase 4: Aggregate Support (~80 LOC)

- [ ] Running accumulator columns in StateShardChip (or satellite chip)
- [ ] Sum verification: final accumulator = result value
- [ ] Count verification: final counter = result value
- [ ] Tests: sum of all values, count of non-null entries

### Phase 5: Integration Tests (~50 LOC)

- [ ] E2E: PropertyRead minimum → prove → verify (across all 3 tiers)
- [ ] E2E: PropertyRead on empty column → null result
- [ ] Cross-tier bus balance verification
- [ ] PropertyRead + Write in same batch (PropertyRead sees pre-batch state)
- [ ] Multiple PropertyRead queries on same column

## 13. Key Changes from Current Design

| Aspect | Current (E8) | Proposed |
|--------|-------------|----------|
| Verifier tier | Tier 1 (execution) | **Tier 2 (column)** |
| Verifier method | `verifier_extension()` | **`column_verifier()`** |
| Bus | None defined | **PROPERTY_READ (external, cross-tier)** |
| State queried | Not specified | **Pre-batch committed (com_old)** |
| Clock behavior | Not specified | **No increment (not a state access)** |
| Query enum | ✅ No changes | ✅ No changes |
| PropertyWitness | value + is_null + as_any | **+ key() method** |

## 14. What the Current Design Gets Right

1. **PropertyOpening trait abstraction** — Clean separation of prover (prove()) and verifier (extension)
2. **Scheme tag matching** — Ensures compatibility between openings and column schemes
3. **Opaque PropertyWitness** — Decouples witness format from trait interface; `as_any()` enables downcasting
4. **PropertyQuery enum** — Complete for v1 use cases
5. **Builder validation** — Rejects openings for unregistered scheme tags at build time
6. **ChipId consistency validation** — Prevents AIR/DynChip mismatches in verifier extension

## 15. Design Decisions and Open Questions

### 15.1 Multiple PropertyReads per Column per Batch — Decided: Supported

If a batch has 5 PropertyRead(Minimum) queries on the same column, the PROPERTY_READ bus receives 5 messages. The PropertyVerifier handles all 5 by receiving each bus message and verifying it against the same com_old state. Since all queries target the same committed state, identical queries return identical results, and the verifier checks each independently. No special batching logic is needed — the bus receive loop naturally handles arbitrary multiplicity.

### 15.2 PropertyRead Key vs Value Queries — Decided: Key Return Included

PropertyRead returns both the key and the value at the result position. The PropertyWitness trait includes a `key()` method:

```rust
pub trait PropertyWitness: Send + Sync {
    fn value(&self) -> &[BabyBear];
    fn key(&self) -> Option<RowKey>;    // The key satisfying the property
    fn is_null(&self) -> bool;
    fn as_any(&self) -> &dyn Any;
}
```

The IR instruction includes a `dst_key` slot (see section 7.1). The PROPERTY_READ bus carries the key alongside the value (see section 8.3).

**Rationale**: Use cases like DEX order matching need the key (e.g., "which order has the best price?") as much as the value. Including the key from the start avoids a breaking bus width change later.

### 15.3 PropertyRead on New State (com_new) — Deferred

Some use cases might want to query post-batch state. E.g., "after all fills, what's the new best price?"

This is harder because:
- com_new isn't known until the batch completes
- PropertyRead during execution can't know com_new

**Solution**: Post-batch PropertyRead as a separate verification step (not during tx execution). The root proof could include post-batch property assertions. Deferred to future work.

**Decision**: Post-batch queries are explicitly not planned for v1. The pre-batch (com_old) snapshot isolation semantics cover all current use cases.

### 15.4 Interaction with Overlay Semantics

PropertyRead sees pre-batch state, but later instructions in the same tx might Read/Write the same column. The overlay handles Read/Write correctly (overlay semantics). PropertyRead is independent — it bypasses the overlay entirely.

This is semantically correct: "what is the committed minimum?" is a different question from "what did I just write?". Programs that need both can use both:

```
let committed_min = property_read orders[m].prices.minimum()  // com_old
let current_val = orders[m].prices[key]                        // overlay
```
