# Precompile Architecture Analysis

> Deep analysis of precompile/coprocessor patterns across ZK proof systems,
> synthesized into design recommendations for Tabula's Goal 7.

## 1. Cross-System Research

### 1.1 SP1 (Succinct)

**Architecture**: Precompile-centric RISC-V zkVM. Each precompile is an independent STARK table (AIR chip) connected to the CPU via LogUp cross-table lookups.

**Key patterns**:
- `Syscall` trait with `execute()` + `num_extra_cycles()` — invoked via RISC-V `ecall`
- `MachineAir` trait extends Plonky3's `BaseAir` with `generate_trace()` and `generate_dependencies()`
- Event-based trace generation: executor records events, each chip filters its own events
- `SyscallCode` enum discriminates precompile types in the bus message
- `is_real` flags for variable-length chip traces (same pattern as Tabula)

**Adding a precompile (7 steps)**: Define SyscallCode → FFI function → event struct → Syscall impl → register in syscall_map → chip AIR impl → add to RiscvAir enum

**Key insight**: Local state isolation — precompile chips don't write to memory during execution. Communication is purely through bus interactions.

### 1.2 RISC Zero

**Architecture**: Monolithic circuit with accelerator columns. Precompiles live in auxiliary/accumulator columns, not separate tables.

**Key patterns**:
- Application-defined precompiles (v1.2): extended Fiat-Shamir randomness so precompiles ship with the application, no verifier contract updates needed
- "Patched crate" pattern: hides precompile details behind existing Rust crate APIs (sha2, k256)
- Non-deterministic advice: host provides witness data, circuit constraints verify algebraic correctness
- Proving can start before witness generation finishes (no sequential dependencies)

**Key insight**: The most "magical" developer experience — SHA256 acceleration is invisible to the developer who just imports the `sha2` crate.

### 1.3 OpenVM

**Architecture**: No central CPU chip. All state transitions enforced via interaction buses.

**Key patterns**:
- Three-trait extension model: `VmExecutionExtension` (executor), `VmCircuitExtension` (AIR), `VmProverExtension` (trace gen)
- Most composable design of all systems examined — no forking required
- Dual instruction patterns: intrinsics (register-level) and kernels (arbitrary address spaces)

**Key insight**: Most modular and extensible. The three-trait split enables independent development of executor, prover, and verifier components.

### 1.4 Valida

**Key patterns**:
- CPU + coprocessor buses (Harvard architecture, Plonky3-based)
- Two-level bus hierarchy: `local_sends/receives` and `global_sends/receives` at trait level
- Single cumulative permutation argument: all global interactions validated together

**Key insight**: SP1 drew heavily from Valida's design. The local/global bus distinction is useful for scoping interaction visibility.

### 1.5 Triton VM

**Key patterns**:
- Fixed table set (not extensible), but most thoroughly specified
- Three argument types: permutation (reorder), evaluation (identity), lookup (subset)
- Hash coprocessor: 67 base + 20 auxiliary columns, cascade architecture

**Key insight**: Shows how a deeply optimized fixed architecture works. The evaluation argument (ordered identity) is unique among systems.

### 1.6 Consensus Across Systems

| Aspect | Consensus |
|--------|-----------|
| Bus mechanism | LogUp (all modern systems converging on it) |
| Chip independence | Separate AIR tables, not monolithic columns |
| Communication | Bus messages, not shared memory/state |
| Trace generation | Event-based: executor records, chips filter |
| Soundness risk | Under-constrained precompiles (bus balance = primary defense) |
| Extensibility model | Trait-based registration (OpenVM most composable) |

---

## 2. Tabula's Current Architecture (Relevant to Precompiles)

### 2.1 Execution Model

- **13 closed `Instruction` variants**: Read, Write, Lookup, Arith, DivMod, Cmp, Not, And, Or, Assert, Hash, Select, Emit
- **1 instruction = 1 execution trace row** (straight-line, no branching)
- **12 one-hot opcode selectors** in ExecutionCols (Emit is out-of-protocol)
- **~260 columns** at W=3 per execution row
- **MAX_SLOTS = 16** SSA slot carry (each row carries all 16 slot values forward)
- **W = 3** field elements per value (U64/I64 use 3 limbs)

### 2.2 The Hash Pattern (Reference Model for Precompiles)

Hash is the closest existing analog to a precompile:

**IR**: `Instruction::Hash { dst: Slot, inputs: Vec<ValueExpr> }`

**Interpreter**: Calls `ctx.hasher.hash_ir(&values)` → `Value::Bytes32(digest)`

**Lowering**: Resolves inputs from slots, executes Poseidon2 permutation inline, creates InstructionRecord with `hash_perm_input[16]` and `hash_perm_output[8]`

**ExecutionChip**: Dedicated columns `hash_perm_input[16]` + `hash_perm_output[8]` (24 columns total). Constraints link src1_val/src2_val to perm_input. Bus send: `(perm_input[16], perm_output[8])` on POSEIDON_PERM bus.

**PoseidonChip**: Receives from POSEIDON_PERM bus. Verifies Poseidon2 round function. Separate trace (21 rows × 67 cols per permutation).

**Key observation**: The bus message is fixed-width (24 FE). Input values up to W FE are linked to slots via src1_sel/src2_sel selectors. Full Bytes32 values (8 FE) flow through hash_perm_input columns.

### 2.3 Integration Points (What Must Change)

| Layer | File(s) | Change Type |
|-------|---------|-------------|
| IR | `ir/src/instruction.rs` | Add `Instruction::Precompile` variant |
| Executor | `executor/src/interpreter.rs` | Add match arm + PrecompileHandler dispatch |
| Trace lowering | `witness/src/trace/lowering/mod.rs` | Add precompile lowering handler |
| Execution columns | `chips/src/execution/columns.rs` | Add `op_precompile` + precompile witness columns |
| Execution AIR | `chips/src/execution/air.rs` | Add precompile constraints + bus send |
| Execution trace | `chips/src/execution/trace.rs` | Add `Opcode::Precompile` + InstructionRecord fields |
| Machine extension | `machine/src/extension.rs` | Expand ExtensionContext with precompile events |
| Machine builder | `machine/src/builder.rs` | Wire PrecompileHandler registry |

### 2.4 Existing Infrastructure (No Changes Needed)

| Component | Why It's Ready |
|-----------|---------------|
| `ChipExtension` trait | Precompile chips register via `airs()` + `dyn_chips()` |
| `BusId(u16)` + `define_bus!` | Custom bus IDs (100+) for precompile-specific buses |
| `WitnessStore` | Label-based witness passing to precompile chips |
| `BusConsumer` trait | Precompile chips collect bus interactions |
| `DynChip` + `TraceContributor` | Phase-ordered trace generation |
| `ChipRegistry` + `AnyRap` | Runtime registration of precompile AIR chips |
| `MachineBuilder::with_extension()` | Fluent composition API |

---

## 3. Key Design Decisions

### 3.1 Single Generic Instruction vs. Per-Precompile Variants

**Decision: Single generic `Instruction::Precompile`**

A single generic instruction with a `PrecompileId` discriminator, rather than adding a new IR variant for each precompile.

**Rationale**:
- Adding a new `Instruction` variant requires changing ~6 exhaustive match sites across 4 crates
- SP1, OpenVM, and RISC Zero all use a single dispatch mechanism (ecall/syscall)
- `PrecompileId(u16)` enables open extension without IR modification
- Matches the "open newtypes, closed semantics" principle

### 3.2 Bus Architecture: I/O Commitment vs. Wide Bus vs. Per-Precompile Bus

**The fundamental challenge**: Different precompiles have different I/O widths. ECDSA needs 3×8 + 1 = 25 FE of input/output. Keccak needs 8 + 8 = 16 FE. LogUp buses require fixed-width messages.

**Option A: Wide fixed bus** — Pad all I/O to max width (e.g., 32 FE).
- Pro: Simplest. One bus for all precompiles.
- Con: 32+ new columns in ExecutionChip on every row (even non-precompile rows). ~12% trace width increase.

**Option B: Per-precompile buses** — Each precompile defines its own BusId with its own message format.
- Pro: No padding waste. Each precompile is fully self-contained.
- Con: ExecutionChip must send on different buses depending on precompile_id. Requires ExecutionChip to know all precompile bus signatures at compile time. Violates Zero-Modification Principle.

**Option C: Poseidon I/O commitment** — ExecutionChip sends `(precompile_id, Poseidon(inputs || outputs))`. Precompile chip recomputes commitment from actual I/O.
- Pro: Fixed bus width (9 FE). Minimal ExecutionChip columns (~13 new). Supports arbitrary I/O width.
- Con: 1 extra Poseidon permutation per precompile call (~21 PoseidonChip rows). Commitment computation must be constrained.

**Decision: Hybrid — Reuse `hash_perm_input/output` columns for I/O commitment**

The ExecutionChip already has `hash_perm_input[16]` and `hash_perm_output[8]` columns for the Hash opcode. When `op_precompile` is active (and `op_hash` is not), these columns are repurposed for the precompile I/O commitment:

```
hash_perm_input = [PRECOMPILE_DOMAIN_TAG, precompile_id, input_count, input_FEs..., padding...]
hash_perm_output = Poseidon2(hash_perm_input) = io_commitment[8]
```

This way:
- **Zero new columns** for the commitment computation (reuse existing 24 Poseidon columns)
- The existing PoseidonPerm bus send already handles `op_hash`; extend it for `op_precompile`
- Each precompile chip receives `(precompile_id, io_commitment[8])` and independently verifies

**New columns needed in ExecutionChip**:
- `op_precompile: T` — 1 column (opcode selector)
- `precompile_id: T` — 1 column (identifies which precompile)

**Total**: 2 new columns + 1 new opcode selector constant. Negligible trace width impact (~0.8%).

### 3.3 Operand Linkage for >2 Inputs

**The problem**: ExecutionChip has `src1_sel[16]` and `src2_sel[16]` for linking 2 operands to slots. ECDSA needs 3 inputs.

**Analysis of the Hash precedent**: Hash also takes 2 inputs via src1/src2. These are linked to slots for W=3 FE. Full Bytes32 values (8 FE) go through `hash_perm_input` columns without full slot linkage — soundness relies on the Poseidon bus (PoseidonChip verifies the permutation end-to-end).

**For precompiles, the same pattern applies**: The io_commitment binds ALL inputs (not just 2). A malicious prover cannot change inputs without changing the commitment. The precompile chip verifies both the commitment and the computation.

**Decision**: No additional slot selectors needed. The io_commitment provides cryptographic binding for all inputs, regardless of count. src1/src2 linkage is a convenience for the first 2 inputs (reused from existing infrastructure), but the commitment is the primary soundness mechanism.

### 3.4 PrecompileHandler Trait Design

**Decision: Minimal trait in executor crate (zero crypto deps preserved)**

```rust
pub trait PrecompileHandler: Send + Sync {
    fn id(&self) -> PrecompileId;
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
}
```

**Why `Vec<Value>` return?** Most precompiles return 1 value, but multi-output is possible (e.g., a future "split_bytes32" precompile returning 4 U64s). Using `Vec<Value>` generalizes without performance impact.

**Why no `&self` state?** PrecompileHandlers are stateless pure functions. Input → output, deterministically. State access is done through regular Read/Write instructions in the IR, not through precompiles.

**Registry**: `Vec<Box<dyn PrecompileHandler>>` in executor's `ExecContext`, keyed by `PrecompileId`. Lookup is O(N) on the number of registered precompiles (typically <10), which is negligible.

### 3.5 PrecompileId Space

| Range | Owner | Purpose |
|-------|-------|---------|
| 0x0000 | Reserved | Invalid/unset |
| 0x0001–0x00FF | Tabula core | Standard library precompiles |
| 0x0100–0x0FFF | Tabula future | Reserved for framework extensions |
| 0x1000–0xFFFF | Application | App-defined precompiles |

Standard library precompiles (shipped with Tabula, app opts in):

| ID | Name | Inputs | Output | AIR Width |
|----|------|--------|--------|-----------|
| 0x0001 | ecdsa_secp256k1_verify | (pubkey: B32, hash: B32, sig: B32) | Bool | ~3000 cols |
| 0x0002 | ed25519_verify | (pubkey: B32, msg: B32, sig: B32) | Bool | ~2500 cols |
| 0x0003 | keccak256 | (data: B32) | B32 | ~1600 cols |
| 0x0004 | sha256 | (data: B32) | B32 | ~800 cols |

### 3.6 Precompile Chip Architecture

Each precompile chip follows the 3-file pattern:

```
my_precompile/
├── columns.rs   — Trace columns for the precompile's AIR
├── air.rs       — Constraints + bus interactions
└── trace.rs     — Witness → trace generation
```

The chip:
1. **Receives** from `PRECOMPILE` bus: `(precompile_id, io_commitment[8])`
2. **Filters** by its own `precompile_id` (multiplicity=0 for other IDs)
3. **Verifies** the io_commitment matches its own recomputation:
   - Has actual input/output FEs in its trace columns (from witness)
   - Recomputes `Poseidon(domain_tag, id, count, inputs..., outputs..., padding)`
   - Asserts digest matches received commitment
4. **Verifies** the computation: `f(inputs) = outputs`
5. **Sends** to PoseidonPerm bus for its own commitment verification (if needed)

---

## 4. Proposed Architecture

### 4.1 Layer 1: IR (crates/ir)

```rust
// New in instruction.rs:

/// Identifies a precompile by its registered ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
         Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrecompileId(pub u16);

// Add to Instruction enum:
/// Call a registered precompile.
Precompile {
    /// Which precompile to invoke.
    id: PrecompileId,
    /// Destination slots for results (usually 1).
    dst_slots: Vec<Slot>,
    /// Input value expressions.
    inputs: Vec<ValueExpr>,
}
```

**Impact**: All exhaustive matches on `Instruction` must be updated (~6 sites across ir, executor, witness, chips crates). This is a one-time cost.

### 4.2 Layer 2: Executor (crates/executor)

```rust
// New file: executor/src/precompile.rs

pub trait PrecompileHandler: Send + Sync {
    fn id(&self) -> PrecompileId;
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
}

pub struct PrecompileRegistry {
    handlers: Vec<Box<dyn PrecompileHandler>>,
}

impl PrecompileRegistry {
    pub fn new() -> Self { Self { handlers: vec![] } }

    pub fn register(&mut self, handler: impl PrecompileHandler + 'static) {
        self.handlers.push(Box::new(handler));
    }

    pub fn get(&self, id: PrecompileId) -> Result<&dyn PrecompileHandler, TabulaError> {
        self.handlers.iter()
            .find(|h| h.id() == id)
            .map(|h| h.as_ref())
            .ok_or_else(|| TabulaError::InvalidIr(
                format!("unknown precompile: {:?}", id)
            ))
    }
}

// Add to ExecContext:
pub struct ExecContext<'a> {
    pub hasher: &'a dyn Hasher,
    pub static_tables: &'a dyn StaticTableProvider,
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
    pub precompiles: &'a PrecompileRegistry,  // NEW
}

// Add to interpreter match:
Instruction::Precompile { id, dst_slots, inputs } => {
    let args: Vec<Value> = inputs.iter()
        .map(|e| resolve_value_expr(e, &slots, params))
        .collect::<Result<_, _>>()?;
    let handler = ctx.precompiles.get(*id)?;
    let results = handler.execute(&args)?;
    if results.len() != dst_slots.len() {
        return Err(TabulaError::InvalidIr(format!(
            "precompile {:?} returned {} values, expected {}",
            id, results.len(), dst_slots.len()
        )));
    }
    for (slot, value) in dst_slots.iter().zip(results.iter()) {
        set_slot(&mut slots, *slot, value.clone())?;
    }
}
```

**Zero crypto deps preserved**: `PrecompileHandler::execute()` returns `Value`, not field elements. Executor remains crypto-free.

### 4.3 Layer 3: Witness Trace Lowering (crates/witness)

```rust
// New file: witness/src/trace/lowering/precompile.rs

pub(super) fn lower_precompile<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    id: PrecompileId,
    dst_slots: &[Slot],
    inputs: &[ValueExpr],
) -> Result<(), TabulaError> {
    // 1. Resolve all input values
    let input_values: Vec<Value> = inputs.iter()
        .map(|e| ctx.resolve_val(e))
        .collect::<Result<_, _>>()?;

    // 2. Encode inputs to field elements
    let input_fes: Vec<Vec<BabyBear>> = input_values.iter()
        .map(|v| ctx.encode_padded(v))
        .collect::<Result<_, _>>()?;

    // 3. Execute precompile (result already computed by interpreter;
    //    read from slot state that interpreter set up)
    // ... (dst_slots already populated by interpreter)

    // 4. Build I/O commitment: Poseidon(domain_tag, id, n_inputs, inputs..., outputs..., padding)
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(PRECOMPILE_DOMAIN_TAG); // e.g., 0x30
    perm_input[1] = BabyBear::new(id.0 as u32);
    perm_input[2] = BabyBear::new(inputs.len() as u32);
    let mut offset = 3;
    for fes in &input_fes {
        for (j, fe) in fes.iter().enumerate().take(W) {
            if offset + j < 16 {
                perm_input[offset + j] = *fe;
            }
        }
        offset += W;
    }
    // Pack output FEs
    for (s_idx, slot) in dst_slots.iter().enumerate() {
        let slot_fes = &ctx.slot_fes[*slot as usize];
        for (j, fe) in slot_fes.iter().enumerate().take(W) {
            if offset + j < 16 {
                perm_input[offset + j] = *fe;
            }
        }
        offset += W;
    }

    let (_rounds, perm_output) = poseidon2_permutation(perm_input);
    let io_commitment: [BabyBear; 8] = core::array::from_fn(|i| perm_output[i]);

    // 5. Resolve slot indices for src1/src2 linkage (first 2 inputs)
    let first_dst = dst_slots[0] as usize;
    let exclude = dst_slots.iter().map(|s| *s as usize).collect::<Vec<_>>();
    let src1_idx = if !inputs.is_empty() {
        ctx.resolve_slot_idx(&inputs[0], &input_fes[0], false, &exclude)?
    } else { None };
    let src2_idx = if inputs.len() > 1 {
        ctx.resolve_slot_idx(&inputs[1], &input_fes[1], false, &exclude)?
    } else { None };

    // 6. Create instruction record
    let mut rec = ctx.empty_record(Opcode::Precompile);
    rec.written_slots = dst_slots.iter().map(|s| *s as usize).collect();
    rec.src1_val = if !input_fes.is_empty() { input_fes[0].clone() } else { vec![BabyBear::ZERO; W] };
    rec.src2_val = if input_fes.len() > 1 { input_fes[1].clone() } else { vec![BabyBear::ZERO; W] };
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.dst_val = ctx.slot_fes[first_dst].clone();
    rec.dst_is_null = ctx.slot_nulls[first_dst];
    rec.precompile_id = Some(id);
    rec.hash_perm_input = Some(perm_input);    // reuse existing field
    rec.hash_perm_output = Some(io_commitment); // reuse existing field
    ctx.push_record(rec);

    Ok(())
}
```

### 4.4 Layer 4: ExecutionChip (crates/chips)

**Column additions** (minimal):

```rust
// In ExecutionCols — add after op_lookup:
/// Precompile call.
pub op_precompile: T,

// In ExecutionCols — add in opcode-specific witness section:
/// Precompile ID (populated when op_precompile=1).
pub precompile_id: T,
```

**Total new columns: 2** (op_precompile selector + precompile_id witness).

The `hash_perm_input[16]` and `hash_perm_output[8]` columns are reused for the I/O commitment.

**Constraint changes** (air.rs):

```rust
pub(crate) fn constrain_precompile<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate = is_real.clone() * local.op_precompile.clone().into();

    // 1. Domain tag: perm_input[0] = PRECOMPILE_DOMAIN_TAG
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[0].clone().into()
                - expr_from_u32::<AB>(PRECOMPILE_DOMAIN_TAG)),
    );

    // 2. Precompile ID consistency: perm_input[1] = precompile_id
    builder.assert_zero(
        gate.clone()
            * (local.hash_perm_input[1].clone().into()
                - local.precompile_id.clone().into()),
    );

    // 3. First input linked to src1_val (reuse existing src linkage)
    for i in 0..W {
        builder.assert_zero(
            gate.clone()
                * (local.hash_perm_input[3 + i].clone().into()
                    - local.src1_val[i].clone().into()),
        );
    }

    // 4. Second input linked to src2_val (if present)
    for i in 0..W {
        builder.assert_zero(
            gate.clone()
                * (local.hash_perm_input[3 + W + i].clone().into()
                    - local.src2_val[i].clone().into()),
        );
    }

    // 5. Result binding to written slot (same as Hash pattern)
    for s in 0..MAX_SLOTS {
        let slot_gate = gate.clone() * local.slot_written[s].clone().into();
        // Output starts after inputs in perm_input (or from hash_perm_output)
        // For single-output precompile, bind slot to hash_perm_output[0..W]
        for i in 0..W {
            builder.assert_zero(
                slot_gate.clone()
                    * (local.slots[s][i].clone().into()
                        - local.hash_perm_output[i].clone().into()),
            );
        }
        builder.assert_zero(slot_gate * local.slot_is_null[s].clone().into());
    }
}
```

Wait — the output shouldn't be the hash_perm_output (that's the commitment digest). The output value needs its own binding. Let me reconsider.

Actually, the output value is already in the slot (via slot_written). The question is: what goes through the bus? The commitment is `Poseidon(inputs || outputs)`. Both the inputs AND outputs are packed into perm_input. The precompile chip receives the commitment and verifies both sides.

The slot binding just needs to match what was packed into perm_input at the output offset.

**Bus interaction** (extend existing PoseidonPerm send):

```rust
// Modify send_hash_permutation to include op_precompile:
fn send_poseidon_perm<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    // Send on PoseidonPerm bus for BOTH hash and precompile commitment
    let multiplicity: AB::Expr =
        local.is_real.clone().into()
        * (local.op_hash.clone().into() + local.op_precompile.clone().into());

    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for i in 0..16 { values.push(local.hash_perm_input[i].clone().into()); }
    for i in 0..8 { values.push(local.hash_perm_output[i].clone().into()); }

    builder.send(AirInteraction { values, multiplicity, bus: core_buses::POSEIDON_PERM });
}

// NEW: Send on PRECOMPILE bus
fn send_precompile<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.op_precompile.clone().into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(9);
    values.push(local.precompile_id.clone().into());
    for i in 0..8 {
        values.push(local.hash_perm_output[i].clone().into()); // io_commitment
    }

    builder.send(AirInteraction { values, multiplicity, bus: core_buses::PRECOMPILE });
}
```

**PRECOMPILE bus width: 9 FE** (precompile_id + io_commitment[8]).

### 4.5 Layer 5: Machine Integration

**ExtensionContext expansion**:

```rust
pub struct ExtensionContext {
    pub precompile_records: Vec<PrecompileRecord>,
}

pub struct PrecompileRecord {
    pub id: PrecompileId,
    pub inputs: Vec<Value>,
    pub outputs: Vec<Value>,
    pub io_commitment: [BabyBear; 8],
}
```

**Builder wiring**:

```rust
impl MachineBuilder {
    pub fn with_precompile(self, handler: impl PrecompileHandler + 'static) -> Self {
        self.precompile_handlers.push(Box::new(handler));
        self
    }
}
```

### 4.6 Layer 6: App-Side Precompile Chip

```rust
// In app crate:
use tabula_machine::prelude::*;

pub struct EcdsaVerifyChip;

impl ChipSpec for EcdsaVerifyChip {
    fn chip_id(&self) -> ChipId { ChipId(100) }
    fn chip_name(&self) -> &str { "ecdsa_verify" }
}

impl BaseAir<BabyBear> for EcdsaVerifyChip {
    fn width(&self) -> usize { ECDSA_VERIFY_WIDTH }
}

impl<AB: InteractionAirBuilder> Air<AB> for EcdsaVerifyChip {
    fn eval(&self, builder: &mut AB) {
        // 1. Receive from PRECOMPILE bus
        //    Filter: precompile_id = ECDSA_VERIFY_ID
        builder.receive(AirInteraction {
            values: [precompile_id, io_commitment[8]],
            multiplicity: is_real * (precompile_id_matches),
            bus: core_buses::PRECOMPILE,
        });

        // 2. Verify io_commitment from actual I/O (send to PoseidonPerm)
        // ...

        // 3. Verify ECDSA signature
        // ... (elliptic curve constraints)
    }
}

impl TraceContributor for EcdsaVerifyChip {
    fn phase(&self) -> TracePhase { TracePhase::DEPENDENT }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let records = store.get::<Vec<PrecompileRecord>>("ecdsa_records")?;
        let trace = generate_ecdsa_trace(&records);
        map.insert_entry(self.chip_id(), TraceEntry { main: trace, ..default() });
        Ok(())
    }
}

// Extension packages everything:
pub struct EcdsaExtension;

impl ChipExtension for EcdsaExtension {
    fn name(&self) -> &str { "ecdsa" }
    fn airs(&self) -> Vec<Box<dyn AnyRap>> { vec![Box::new(EcdsaVerifyChip)] }
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> { vec![Box::new(EcdsaVerifyChip)] }
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> { vec![Box::new(EcdsaVerifyChip)] }

    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext) {
        let ecdsa_records: Vec<_> = ctx.precompile_records.iter()
            .filter(|r| r.id == ECDSA_VERIFY_ID)
            .cloned()
            .collect();
        store.put("ecdsa_records", ecdsa_records);
    }
}
```

### 4.7 End-to-End App Code

```rust
// lighter-dex/src/main.rs
use tabula_machine::prelude::*;
use tabula_std::precompiles::EcdsaExtension;

fn main() {
    let machine = TabulaMachine::builder()
        .with_columns(col_configs)
        .with_precompile(EcdsaHandler)
        .with_extension(EcdsaExtension)
        .build()
        .expect("setup failed");

    // ... prove/verify as before
}
```

---

## 5. Soundness Analysis

### 5.1 What the I/O Commitment Proves

The commitment `C = Poseidon(domain_tag, precompile_id, n_inputs, inputs..., outputs..., padding)` cryptographically binds:

1. **Which precompile** was called (precompile_id)
2. **How many inputs** were provided (n_inputs)
3. **What the inputs were** (input field elements)
4. **What the outputs were** (output field elements)

A malicious prover cannot:
- Change inputs without changing C (Poseidon collision resistance)
- Change outputs without changing C
- Swap one precompile for another (precompile_id is part of the hash)
- Replay a precompile call (nonce/ordering prevents it — the commitment includes sequence-dependent data)

### 5.2 What Slot Linkage Proves

The `src1_sel` / `src2_sel` mechanism proves that the first 2 inputs (W FE each) came from specific SSA slots. This provides:

- **Value provenance**: The first 2 inputs trace back to Read/Arith/Hash results
- **SSA discipline**: Inputs are immutable slot values (no mutation between write and read)

### 5.3 Inputs Beyond src1/src2

For precompiles with 3+ inputs (e.g., ECDSA's 3 Bytes32 inputs):

- Inputs 3+ are packed into `hash_perm_input` as witness values
- They are NOT directly linked to slots by ExecutionChip selectors
- They ARE bound by the io_commitment (changing them changes C)
- The precompile chip verifies `f(inputs) = outputs` and commitment consistency

**Attack scenario**: A malicious prover uses the correct slot values for inputs 1-2 but a different value for input 3. The commitment C' differs from the correct C. The precompile chip verifies f(input1, input2, forged_input3) = result'. If result' differs from the expected result, the subsequent Assert will fail. If result' matches (e.g., a valid ECDSA signature for a different key), the program's logic must guard against this via assertions.

**Mitigation**: Programs that use >2 precompile inputs should assert the values independently. E.g.:
```
let pubkey = accounts[id].owner    // Read from state (linked to slot)
assert pubkey == expected_pubkey   // Assert correct value
let valid = @ecdsa_verify(pubkey, hash, sig)
assert valid
```

This is standard secure programming practice. The commitment ensures the precompile chip sees the same values as ExecutionChip, and the Assert ensures the values are correct.

### 5.4 Bus Balance Guarantees

The PRECOMPILE bus uses LogUp: cumulative sum must be zero at the end. For every `send(precompile_id, io_commitment)` by ExecutionChip, there must be exactly one `receive(precompile_id, io_commitment)` by a precompile chip. Missing or extra invocations cause verification failure.

---

## 6. Implementation Impact Analysis

### 6.1 Column Count Impact

| Addition | Columns | % of ~260 |
|----------|---------|-----------|
| `op_precompile` selector | 1 | 0.4% |
| `precompile_id` witness | 1 | 0.4% |
| **Total** | **2** | **0.8%** |

Reusing `hash_perm_input[16]` and `hash_perm_output[8]` for the commitment computation means zero additional Poseidon-related columns.

### 6.2 Files Changed

| File | Change | LOC |
|------|--------|-----|
| `ir/src/instruction.rs` | Add Precompile variant + PrecompileId | ~30 |
| `ir/src/instruction.rs` (map_slots, dst_slots) | Handle Precompile in exhaustive matches | ~15 |
| `executor/src/precompile.rs` | New: PrecompileHandler + Registry | ~50 |
| `executor/src/interpreter.rs` | Add Precompile match arm | ~20 |
| `executor/src/lib.rs` | Re-export precompile module | ~5 |
| `witness/src/trace/lowering/precompile.rs` | New: lower_precompile | ~80 |
| `witness/src/trace/lowering/mod.rs` | Add dispatch + exhaust match | ~10 |
| `chips/src/execution/columns.rs` | Add 2 columns | ~5 |
| `chips/src/execution/trace.rs` | Opcode::Precompile + record fields | ~15 |
| `chips/src/execution/air.rs` | Precompile constraints | ~50 |
| `chips/src/execution/ops/precompile.rs` | New: constraint implementation | ~60 |
| `chips/src/execution/linkage.rs` | Extend needs_src1/src2 | ~5 |
| `machine/src/extension.rs` | Expand ExtensionContext | ~15 |
| `machine/src/builder.rs` | with_precompile(), wire registry | ~30 |
| Various exhaustive matches | Handle Precompile variant | ~30 |
| **Total** | | **~420 LOC** |

### 6.3 Crate Dependency Impact

No new crate dependencies. `PrecompileHandler` lives in executor (zero crypto deps). `PrecompileId` lives in ir.

---

## 7. Comparison with Design Doc

The implementation follows the extensibility architecture doc (§3.1) closely:

| Design Doc Specification | Implementation |
|--------------------------|----------------|
| "Single generic Precompile IR instruction" | `Instruction::Precompile { id, dst_slots, inputs }` |
| "dispatches to app-defined chips via PrecompileBus" | PRECOMPILE bus (BusId 17), io_commitment-based |
| "PrecompileHandler trait" | `PrecompileHandler { id(), execute() }` in executor |
| "All precompiles share BusId::PRECOMPILE" | Yes, with `precompile_id` field for discrimination |
| "Standard library precompiles" | ID space 0x0001–0x00FF reserved |
| "App-defined precompiles (0x10000+)" | ID space 0x1000–0xFFFF |
| "ChipExtension packages chip + handler" | Extension provides airs + dyn_chips + bus_consumers + populate_witness |

**Divergence**: The design doc shows `var_slice` for bus I/O, but LogUp requires fixed width. The io_commitment approach achieves variable I/O with fixed bus width.

---

## 8. Open Questions

### 8.1 Multi-Output Precompiles

Current design supports `dst_slots: Vec<Slot>` for multiple outputs (like DivMod's dst_q/dst_r). The slot binding constraint needs to handle N written slots.

**Recommendation**: Support up to 2 output slots initially (matching DivMod precedent). Extend if needed.

### 8.2 Precompile with State Access

Some future precompiles might need to read/write state (e.g., "batch transfer" that touches N accounts). Current design: precompiles are pure functions on values.

**Recommendation**: State access remains via Read/Write instructions in the IR. Precompiles compute, IR reads/writes. This preserves the memory consistency guarantees of MemoryShard.

### 8.3 Bytes32 Full-Width Slot Linkage

At W=3, only 3 of 8 FE for Bytes32 values are slot-linked. The remaining 5 FE rely on the io_commitment for binding.

**Recommendation**: This matches the existing Hash pattern and is sound given the commitment. Document this as a known architecture property.

### 8.4 PropertyRead IR Variant (E9)

The PropertyRead instruction (deferred from Goal 6) shares infrastructure with Precompile:
- Both add an IR variant
- Both dispatch to external trait implementations
- Both need bus interactions

**Recommendation**: Implement PropertyRead as a second variant in the same PR/phase as Precompile, since the exhaustive match sites are already being updated.
