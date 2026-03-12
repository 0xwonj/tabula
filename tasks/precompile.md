# Goal 7: Precompile Framework

> Status: 🔵 Ready (Goal 6 complete — BusId, ChipExtension, MachineBuilder all available)
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §3 (Computation Extension)
> Research: [docs/research/precompile-architecture-analysis.md](../docs/research/precompile-architecture-analysis.md)
> Depends: Goal 6 (Extensibility API) ✅
> Unblocks: Goal 11 (DSL improvements)

**Sharding context**: Precompile chips live in Tier 1 (execution proof) — bus-linked to ExecutionChip via PRECOMPILE bus. Orthogonal to column sharding.

## Goal

App developers can add custom instructions (precompiles) without modifying Tabula. Follows the proven pattern: Hash → PoseidonChip, Lookup → StaticTableChip.

## Architecture Summary

**Key design decision**: I/O commitment via Poseidon. The ExecutionChip reuses its existing `hash_perm_input[16]` / `hash_perm_output[8]` columns to compute `io_commitment = Poseidon(domain_tag, precompile_id, n_inputs, inputs..., outputs...)`. This commitment goes through the PRECOMPILE bus (fixed 9 FE width). Each precompile chip receives the commitment and independently verifies both the computation and commitment consistency.

**Trace width impact**: +2 columns (op_precompile, precompile_id). Poseidon columns reused — 0.8% total increase.

**ID space**: 0x0001–0x00FF (Tabula standard library), 0x1000–0xFFFF (app-defined).

## Phase 1: IR + Executor (~120 LOC)

> No dependencies. Can start immediately.

### P1-1: PrecompileId type in IR

- [ ] Define `PrecompileId(pub u16)` newtype in `ir/src/instruction.rs`
  - Derives: Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Borsh
- [ ] Add `Instruction::Precompile` variant:
  ```rust
  Precompile {
      id: PrecompileId,
      dst_slots: Vec<Slot>,
      inputs: Vec<ValueExpr>,
  }
  ```
- [ ] Update `Instruction::map_slots()` — map dst_slots + input slots
- [ ] Update `Instruction::dst_slots()` — return dst_slots
- [ ] Update IR pass exhaustive matches: `pass/canonicalize/`, `pass/typecheck.rs`
- [ ] Re-export `PrecompileId` from `ir/src/lib.rs`

### P1-2: PrecompileHandler trait in executor

- [ ] New file `executor/src/precompile.rs`:
  ```rust
  pub trait PrecompileHandler: Send + Sync {
      fn id(&self) -> PrecompileId;
      fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
  }

  pub struct PrecompileRegistry { handlers: Vec<Box<dyn PrecompileHandler>> }
  impl PrecompileRegistry {
      pub fn new() -> Self;
      pub fn register(&mut self, handler: impl PrecompileHandler + 'static);
      pub fn get(&self, id: PrecompileId) -> Result<&dyn PrecompileHandler, TabulaError>;
  }
  ```
- [ ] Add `precompiles: &'a PrecompileRegistry` to `ExecContext`
- [ ] Update `ExecContext` construction in `batch.rs`
- [ ] Add Precompile dispatch in `interpreter.rs`:
  - Resolve inputs from slots/params/literals
  - Call `handler.execute(&args)`
  - Validate result count matches dst_slots count
  - Write results to dst_slots
- [ ] Re-export from `executor/src/lib.rs`

### P1-3: Executor tests

- [ ] Identity precompile: `f(x) = x` — round-trip test
- [ ] Multi-input precompile: `f(a, b) = a + b` via precompile
- [ ] Multi-output precompile: `f(a) = (a, a+1)` → two dst_slots
- [ ] Error: unknown precompile ID → `TabulaError::InvalidIr`
- [ ] Error: wrong result count → `TabulaError::InvalidIr`

## Phase 2: Witness + ExecutionChip (~200 LOC)

> Blocked on: Phase 1

### P2-1: Trace lowering

- [ ] New file `witness/src/trace/lowering/precompile.rs`:
  - `lower_precompile<const W: usize>(ctx, id, dst_slots, inputs)`
  - Resolve all input values + encode to FE
  - Build Poseidon perm_input: `[PRECOMPILE_DOMAIN_TAG, id, n_inputs, input_FEs..., output_FEs..., padding]`
  - Execute Poseidon2 permutation → io_commitment
  - Update slot state for dst_slots
  - Create InstructionRecord with `opcode: Opcode::Precompile`, reuse `hash_perm_input/output` fields
  - Resolve src1/src2 slot indices for first 2 inputs
- [ ] Add dispatch in `lower_tx_body()` match
- [ ] Add `Opcode::Precompile` variant in `chips/src/execution/trace.rs`
- [ ] Add `precompile_id: Option<PrecompileId>` to `InstructionRecord`
- [ ] Update `max_dst_slot()` and `collect_param_operands()` for Precompile variant

### P2-2: ExecutionChip columns + constraints

- [ ] Add `op_precompile: T` to `ExecutionCols` (opcode selector, after `op_lookup`)
- [ ] Add `precompile_id: T` to `ExecutionCols` (opcode-specific witness)
- [ ] Update opcode one-hot constraint: sum of 13 selectors = 1 when is_real
- [ ] New file `chips/src/execution/ops/precompile.rs`:
  - `constrain_precompile()`:
    - Domain tag: `perm_input[0] = PRECOMPILE_DOMAIN_TAG (0x30)`
    - ID consistency: `perm_input[1] = precompile_id`
    - Input count: `perm_input[2] = n_inputs` (constrained per precompile_id range)
    - First input: `perm_input[3..3+W] = src1_val[0..W]`
    - Second input: `perm_input[3+W..3+2W] = src2_val[0..W]`
    - Result binding: `slots[written_slot][i] = output_val[i]` (output packed in perm_input at correct offset)
    - Slot null clear: `slot_is_null[written_slot] = 0`
- [ ] Update `air.rs` eval: call `constrain_precompile()` + bus sends

### P2-3: Bus interactions

- [ ] Define `PRECOMPILE: BusId = BusId(17)` in `stark/src/air/interaction.rs` (core_buses)
- [ ] `send_precompile()` in `chips/src/execution/buses.rs`:
  - Values: `(precompile_id, hash_perm_output[0..8])` = 9 FE
  - Multiplicity: `is_real * op_precompile`
  - Bus: `core_buses::PRECOMPILE`
- [ ] Extend `send_poseidon_perm()` multiplicity: include `op_precompile`
  - `mult = is_real * (op_hash + op_precompile)`
- [ ] Update `linkage.rs` — extend `needs_src1` / `needs_src2` to include `op_precompile`

### P2-4: Trace generation

- [ ] Handle `Opcode::Precompile` in trace generation (`chips/src/execution/trace.rs`):
  - Set `op_precompile = 1`
  - Set `precompile_id` from record
  - Set `hash_perm_input` from record (reuse existing path for Hash)
  - Set `hash_perm_output` from record (reuse existing path for Hash)
  - Standard slot carry + operand linkage (same as other opcodes)

### P2-5: Witness pipeline integration

- [ ] Expand `ExtensionContext` with `precompile_records: Vec<PrecompileRecord>`
- [ ] Define `PrecompileRecord { id, inputs: Vec<Value>, outputs: Vec<Value>, io_commitment: [BabyBear; 8] }`
- [ ] Populate `ExtensionContext` during witness build (extract from InstructionRecords)
- [ ] Call `extension.populate_witness(store, &ctx)` in witness pipeline

## Phase 3: Machine Builder + Integration (~100 LOC)

> Blocked on: Phase 2

### P3-1: Builder API

- [ ] Add `with_precompile(handler)` to `MachineBuilder`:
  - Stores `PrecompileHandler` for executor dispatch
  - Validates: no duplicate PrecompileId
- [ ] Wire `PrecompileRegistry` into executor context during `build_traces()`
- [ ] Ensure `PrecompileHandler` is accessible at execution time

### P3-2: Integration tests

- [ ] Test: register identity precompile → execute batch → build traces → prove → verify
- [ ] Test: precompile bus balance (ExecutionChip send = PrecompileChip receive)
- [ ] Test: mock precompile chip (receives from PRECOMPILE bus, verifies io_commitment)
- [ ] Test: multiple precompile types in one batch
- [ ] Test: precompile with multi-output (2 dst_slots)
- [ ] Test: unknown precompile ID → error at execution time

## Phase 4: DSL Syntax (~200 LOC)

> Blocked on: Phase 3

### P4-1: Parser

- [ ] Add `@` prefix syntax for precompile calls in `lang/src/parser/expr.rs`
  - `@name(args...)` → `ExprKind::PrecompileCall { name, args }`
- [ ] Precompile name → PrecompileId mapping (resolved at lowering time)

### P4-2: Lowering

- [ ] Lower `ExprKind::PrecompileCall` → `Instruction::Precompile` in `lang/src/lower/expr.rs`
- [ ] Precompile declaration: `precompile name(param_types) -> return_type = id`
- [ ] Type checking: validate argument types match declared parameter types

### P4-3: E2E tests

- [ ] DSL source → compile → execute → verify for `@identity(x)` precompile
- [ ] DSL source with multiple precompile calls in one tx
- [ ] Error: undefined precompile name
- [ ] Error: wrong argument count

## Phase 5: PropertyRead (~420 LOC)

> Cross-tier structural queries on committed column state.
> Best paired with Precompile since exhaustive matches are already updated.
> Design: [docs/research/property-read-design-analysis.md](../docs/research/property-read-design-analysis.md)

**Architecture**: PropertyRead is architecturally distinct from Precompile. ExecutionChip SENDS on PROPERTY_READ external bus (BusId 18). PropertyVerifierChip in **Tier 2** RECEIVES and verifies against column commitment (com_old). Root proof validates bus balance across tiers.

**State semantics**: Queries pre-batch committed state (com_old) — snapshot isolation. In-flight overlay has no commitment and cannot be verified in ZK.

### P5-1: IR + Executor ✅

- [x] `Instruction::PropertyRead` variant with `dst_val`, `dst_key`, `dst_is_null` slots
- [x] `PropertyQueryKind` enum in IR (Minimum, Maximum, Successor, Predecessor, NonExistenceRange, Aggregate)
- [x] `Instruction::map_slots()`, `dst_slots()` updated
- [x] IR pass exhaustive matches updated (canonicalize, typecheck)
- [x] `PropertyReadFn` callback trait in executor context (replaces heavy CommittedStateProvider + PropertyOpeningRegistry)
- [x] Interpreter dispatch: call property_read_fn, decode result, write to 3 dst slots
- [x] DSL `property_read()` function in `lang/src/lower/stmt.rs`
- [x] Tests: IR instruction, executor dispatch, DSL lowering

### P5-2: ExecutionChip ✅

- [x] `op_property_read`, `property_query_type`, `property_result_val[W]`, `property_result_key[W]`, `property_result_is_null` columns
- [x] `property_val_sel[MAX_SLOTS]`, `property_key_sel[MAX_SLOTS]` one-hot selectors
- [x] `Opcode::PropertyRead` variant, opcode one-hot constraint (15 selectors total)
- [x] `constrain_property_read()` in `ops/property_read.rs`: val/key/null slot binding, boolean, one-hot, non-overlap
- [x] `PROPERTY_READ: BusId(18)` defined, `PropertyReadAirBuilder` macro
- [x] `send_property_read()`: tuple `(t, c, query_type, result_val[W], result_key[W], is_null)`
- [x] Trace generation: 3-slot write (val=dst_val, key=dst2_val, null=[is_null,0,0])

### P5-3: Column Tier Verifier ✅

- [x] `PropertyVerifierChip<W>` in `chips/src/shards/property/` (air, columns, trace modules)
- [x] AIR: receive from PROPERTY_READ bus, boolean (is_real, is_null), is_real prefix, constant identity (table_id, col_id)
- [x] Trace generation: `generate_property_verifier_trace()`, power-of-2 padding
- [x] `TraceContributor` impl: phase=MEMORY, reads from `PROPERTY_READ_WITNESS_LABEL`
- [x] Wired into `SsmcScheme::create_chips()` as 4th shard chip (always included, zero rows = minimal overhead)
- [x] Note: Full soundness (linking result to com_old via hash chain) is deferred — current chip verifies format and bus balance only

### P5-4: Aggregate Support (deferred)

- [ ] Running accumulator column in PropertyVerifier (or satellite chip)
- [ ] Sum/Count verification: final accumulator = result value
- Not blocked by anything; add when aggregate queries are needed

### P5-5: Witness + Machine Integration ✅

- [x] `extract_property_read_records()` in `witness/src/trace/builder.rs`: groups PropertyRead instruction records by (table, col)
- [x] Global WitnessStore stores `BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>` under `PROPERTY_READ_ALL_LABEL`
- [x] `partition_by_tier()` distributes per-column PropertyRead records to column stores under `PROPERTY_READ_WITNESS_LABEL`
- [x] PropertyVerifierChip reads from column store in `TraceContributor::contribute()`
- [x] `PropertyOpening` trait in `machine/src/property.rs` for extensible verification strategies
- [x] `MachineBuilder::with_property_opening()` registration

### P5-6: Tests ✅

- [x] PropertyVerifier AIR constraint tests (20 tests): valid queries, null results, multi-query, empty, invalid boolean/prefix/identity
- [x] C18 PropertyRead cross-tier bus balance tests (3 tests): single, multiple, null — Execution→PropertyVerifier
- [x] Test builders: `make_property_read()`, `InstructionBuilder::property_read()/dst2_fe()`
- [x] Regression tests updated: execution width 311, column tier 6 chips
- [ ] Full E2E: PropertyRead minimum → prove → verify (blocked on full integration test harness)
- [ ] PropertyRead + Write in same batch (blocked on E2E harness)

## App Developer Experience

```rust
// App crate — precompile handler (executor-side, zero crypto deps)
struct Sha256Handler;
impl PrecompileHandler for Sha256Handler {
    fn id(&self) -> PrecompileId { PrecompileId(0x0004) }
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
        let data = inputs[0].as_bytes32()?;
        let digest = sha256::digest(data);
        Ok(vec![Value::Bytes32(digest)])
    }
}

// App crate — precompile chip (prover-side, AIR constraints)
struct Sha256Chip;
impl Air<AB> for Sha256Chip { /* receive PRECOMPILE bus, verify SHA256 rounds */ }

// App crate — extension packages both
struct Sha256Extension;
impl ChipExtension for Sha256Extension {
    fn name(&self) -> &str { "sha256" }
    fn airs(&self) -> Vec<Box<dyn AnyRap>> { vec![Box::new(Sha256Chip)] }
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> { vec![Box::new(Sha256Chip)] }
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> { vec![Box::new(Sha256Chip)] }
    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext) {
        let records: Vec<_> = ctx.precompile_records.iter()
            .filter(|r| r.id == PrecompileId(0x0004))
            .cloned().collect();
        store.put("sha256_records", records);
    }
}

// Composition
let machine = TabulaMachine::builder()
    .with_columns(col_configs)
    .with_precompile(Sha256Handler)
    .with_extension(Sha256Extension)
    .build()?;
```

## DSL Example

```
precompile ecdsa_verify(pubkey: bytes32, hash: bytes32, sig: bytes32) -> bool = 0x0001

tx transfer(sig: bytes32, pubkey: bytes32, to: u64, amount: u64) {
    let msg_hash = hash(to, amount)
    let valid = @ecdsa_verify(pubkey, msg_hash, sig)
    assert valid, "invalid signature"

    let balance = accounts[pubkey].balance ?? 0
    assert balance >= amount, "insufficient balance"
    accounts[pubkey].balance = balance - amount
    accounts[to].balance += amount

    emit "transfer" (pubkey, to, amount)
}
```

## Estimated LOC by Phase

| Phase | New LOC | Modified LOC | Total |
|-------|---------|-------------|-------|
| P1: IR + Executor | ~80 | ~40 | ~120 |
| P2: Witness + Chip | ~150 | ~50 | ~200 |
| P3: Builder + Tests | ~60 | ~40 | ~100 |
| P4: DSL syntax | ~150 | ~50 | ~200 |
| P5: PropertyRead | ~350 | ~70 | ~420 |
| **Total** | **~790** | **~250** | **~1,040** |

## Verification

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
```
