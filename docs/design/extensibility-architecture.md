# Tabula Extensibility Architecture

> **Status**: v1.0
> **Date**: 2026-03-12
> **Scope**: Framework-level extensibility for purpose-built ZK applications
> **Depends on**: sharded-protocol-design.md, prover-pipeline-acceleration.md

---

## 1. Design Philosophy

### 1.1 Core Values

**Near-Optimal for Any ZK Application.** Tabula is a framework for building purpose-built verifiable state machines. The architecture provides composable building blocks — chips, buses, state strategies, and proof infrastructure — that application developers assemble into an optimized, application-specific proving system. A ZK DEX built on Tabula should achieve within 5-10% of a hand-built circuit's performance, while saving months of development time.

**Zero-Modification Principle.** Applications MUST define all customizations — chips, buses, commitment strategies, precompile handlers — purely in their own crate. Tabula is consumed as an immutable Cargo dependency. No forking, no patching, no conditional compilation flags in framework code.

```
┌──────────────────────────────────────────────────────────────┐
│  Tabula (immutable Cargo dependency, never modified by apps) │
│  tabula-core, tabula-ir, tabula-executor, tabula-machine,   │
│  tabula-stark, tabula-gadgets, tabula-chips, tabula-witness  │
└──────────────────────────┬───────────────────────────────────┘
                    Cargo dependency (read-only)
┌──────────────────────────▼───────────────────────────────────┐
│  App Crate (100% of customization lives here)                │
│                                                              │
│  Composition point (the only "wiring" code):                 │
│    TabulaMachine::builder()                                  │
│        .with_core_chips()                                    │
│        .with_extension(MyExtension)                          │
│        .build()                                              │
│                                                              │
│  Pure app definitions (trait impls, no Tabula code changes): │
│    impl ChipSpec + Air<AB> for AppChip  (auto AnyRap)       │
│    impl ChipExtension for MyExtension                        │
│    impl ColumnCommitment for AppCommitment                    │
│    impl PrecompileHandler for AppPrecompiles                 │
│    define_bus! for app buses                                 │
│    .tab files (DSL tx types)                                 │
└──────────────────────────────────────────────────────────────┘
                    uses
┌──────────────────────────────────────────────────────────────┐
│  Plonky3 (re-exported through tabula-machine::prelude)       │
└──────────────────────────────────────────────────────────────┘
```

**Graduated Complexity.** 80% of applications need only the DSL (`.tab` files). 15% add standard precompiles. 5% (like a ZK DEX) write custom AIR chips. Each level inherits everything from the levels below.

**LogUp Buses as Universal Interface.** All inter-chip communication flows through LogUp buses. This is the composability primitive — chips never reference each other directly. A custom orderbook chip and the built-in memory chip compose because they both speak "bus." Bus balance guarantees soundness: if fingerprints don't match, verification fails.

**Open Newtypes, Closed Semantics.** Identifiers (`ChipId(u16)`, `BusId(u16)`, `TracePhase(u32)`, `EncodingWidth(usize)`) are open newtypes that support downstream extension. Core semantics (the 4 value types, 13 instructions) are intentionally closed enums with exhaustive matching for soundness.

### 1.2 Design Goals

1. **Zero-modification**: Apps never fork or modify Tabula's codebase
2. **Near-optimal efficiency**: Custom chips approach purpose-built circuit performance (~5-10% overhead)
3. **Graduated complexity**: Simple apps use core IR unchanged; complex apps go deeper
4. **Composability**: Extensions compose via LogUp buses — no coupling between components
5. **Type safety**: Setup-time validation of chip composition, compile-time bus signatures
6. **Minimal boilerplate**: Macros and traits eliminate repetitive wiring code
7. **Upgrade resilience**: Apps survive Tabula minor version updates without code changes

### 1.3 Non-Goals

- Changing the base field (KoalaBear is fixed)
- General-purpose computation (Tabula is a state machine, not a zkVM)
- Custom type extensibility (closed ValueType + bytes32 escape hatch — see custom-type-extensibility.md)
- Hot-swapping chips at runtime (setup is a one-time configuration step)

---

## 2. Architecture Overview

### 2.1 Three-Tier Proof Structure (Implemented)

Full sharding IS the base architecture. No monolithic code paths remain.

```
Tier 1: Execution Proof (1, global)
  Chips: ExecutionChip<W>, StaticTableChip<W>, PoseidonLocal, RangeCheckLocal
  Proves: instruction correctness, control flow, slot SSA

Tier 2: Column Proofs (C, parallel)
  Per-column (table_id, col_id) with encoding width W
  Chips: MemoryShardChip<W>, StateShardChip<W>, MetaShardChip, PoseidonLocal, RangeCheckLocal
  Proves: memory consistency, state transitions, commitment correctness

Tier 3: Root Proof (1, lightweight)
  Chips: SmtColPathChip, SmtTablePathChip, PoseidonLocal, RangeCheckLocal
  Proves: SMT root integrity, cross-column commitment balance
  Pluggable via RootProof trait
```

**Key flow**: `partition_by_tier()` → runtime-owned per-tier `build_all_traces()` → `ProofInstance` per tier → shared Fiat-Shamir → independent proofs

### 2.2 Current Extension Points (Implemented)

| Component | Location | Status |
|-----------|----------|--------|
| `ChipRegistry` + `AnyRap` | `machine/src/registry.rs` | ✅ Runtime chip registration via `Box<dyn AnyRap>` |
| `BusId(u16)` + `define_bus!` | `stark/src/air/interaction.rs` | ✅ Open bus IDs, core 0-99, app 100+ |
| `TraceContributor` + `WitnessStore` | `stark/src/trace/contributor.rs` | ✅ Phase-ordered trace generation |
| `DynChip` + `BusConsumer` | `stark/src/trace/dyn_chip.rs` | ✅ Object-safe chip + bus consumption |
| `RootProof` trait | `machine/src/composition.rs` | ✅ Pluggable root proof (default: SMT) |
| Per-tier setup | `machine/src/setup.rs` | ✅ `TierSetup` with registry + keys + dyn_chips |
| `EncodingWidth` + `ColumnPlan` | `stark/src/trace/column_commitment.rs` | ✅ Per-column width polymorphism |

### 2.3 Extension Points to Build (Goal 6)

| Component | Mechanism | Priority |
|-----------|-----------|----------|
| `ChipExtension` trait | Package chips + witness + buses as distributable unit | High |
| `TabulaMachine::builder()` | Fluent API for composition | High |
| `tabula-machine::prelude` | Stable re-export of p3 types | High |
| `ColumnCommitment` impls | Extract SSMC/SMT into trait; enable custom strategies | High |
| `PropertyOpening` trait | Structural queries on committed state | Medium |
| `Precompile` IR variant + handler | Custom computation dispatch | High (Goal 7) |
| `TemplateChip` trait | Optimized tx-specific execution | Medium (Goal 8) |
| `ProofAggregator` trait | Sub-proof aggregation / recursion | Low (future) |

---

## 3. Seven Extension Axes

Tabula's extensibility decomposes into seven orthogonal axes, each independently extensible. Extensions compose via LogUp buses — the "product composition" property.

```
Axis 1: Computation ──────── what operations are available (precompiles)
Axis 2: Chip Composition ─── what AIR components prove correctness
Axis 3: Trace Pipeline ───── how witnesses flow to chips
Axis 4: State Commitment ─── how column state is committed (ColumnCommitment)
Axis 5: State Opening ────── structural queries on committed state (PropertyOpening)
Axis 6: Execution Strategy ─ how tx bodies are proven (interpreter/template)
Axis 7: Proof Composition ── how sub-proofs aggregate (recursion/IVC)
```

### 3.1 Axis 1: Computation Extension (Precompile Pattern)

**Problem**: The 13 core instructions are computationally complete but not efficient for specialized operations (ECDSA, Keccak, bitwise ops).

**Mechanism**: A single generic `Precompile` IR instruction dispatches to app-defined chips via a shared `PrecompileBus`. The ExecutionChip sends; each precompile chip receives and proves its computation.

```rust
// IR — one-time addition to Instruction enum
Precompile {
    id: PrecompileId,
    dst_slots: Vec<Slot>,
    inputs: Vec<ValueExpr>,
}

// Executor — app implements this trait
pub trait PrecompileHandler: Send + Sync {
    fn id(&self) -> PrecompileId;
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
}

// AIR — app defines chip, registers via ChipExtension
struct EcdsaVerifyChip;
impl Air<AB> for EcdsaVerifyChip { /* receive from PrecompileBus, constrain ECDSA */ }
```

**Bus**: All precompiles share `BusId::PRECOMPILE` with `precompile_id` field for discrimination:

```rust
define_bus!(PrecompileAirBuilder(BusId::PRECOMPILE, ...) {
    precompile_id: expr,  // prevents cross-precompile collision
    nonce: expr,          // unique per invocation
    inputs: var_slice,
    outputs: var_slice,
})
```

**Standard library precompiles** (shipped with Tabula, app opts in):

| ID | Name | Use Case |
|----|------|----------|
| 0x0001 | ecdsa_secp256k1_verify | User authentication |
| 0x0002 | ed25519_verify | Oracle attestation |
| 0x0003 | keccak256 | EVM compatibility |
| 0x0004 | sha256 | Bitcoin compatibility |

**App-defined precompiles** (0x10000+): Implemented entirely in app crate.

**Status**: Designed. Implementation in Goal 7.

### 3.2 Axis 2: Chip Composition (Implemented)

**Problem**: Adding a chip should not require modifying Tabula's source.

**Mechanism**: `ChipRegistry` + `AnyRap` blanket impl. Any type implementing `ChipSpec + Air<AB>` automatically satisfies `AnyRap` and can be registered at runtime.

```rust
// AnyRap — blanket impl, zero boilerplate for app developers
pub trait AnyRap: BaseAir<KoalaBear> + Air<...all AB bounds...> + Send + Sync {
    fn chip_id(&self) -> ChipId;
    fn chip_name(&self) -> &str;
    fn has_interactions(&self) -> bool;
    // ...
}

// ChipRegistry — runtime registration
pub struct ChipRegistry {
    chips: Vec<RegisteredChip>,  // Box<dyn AnyRap>
    buses: BTreeSet<BusId>,
}
```

**ChipExtension** — packages chips + witness logic as distributable unit:

```rust
pub trait ChipExtension: Send + Sync {
    /// Register all chips this extension provides.
    fn register_chips(&self, registry: &mut ChipRegistry);

    /// Populate witness store with extension-specific data.
    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext);

    /// Human-readable name for diagnostics.
    fn name(&self) -> &str;
}
```

**App composition**:

```rust
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_extension(LighterDexExtension)
    .with_config(production_config())
    .build()?;
```

**Status**: ChipRegistry + AnyRap + BusId ✅ implemented. ChipExtension + builder API: Goal 6.

### 3.3 Axis 3: Trace Pipeline (Implemented)

**Problem**: Hardcoded per-chip wiring in orchestration.rs prevents adding chips without source modification.

**Mechanism**: `TraceContributor` trait + `WitnessStore` typed key-value store. Chips declare their phase and data dependencies. The framework orchestrates trace generation in phase order.

```rust
pub trait TraceContributor: ChipSpec {
    fn phase(&self) -> TracePhase;  // INDEPENDENT(0), MEMORY(100), DEPENDENT(200)
    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError>;
}

pub struct WitnessStore {
    entries: HashMap<WitnessKey, Box<dyn Any + Send + Sync>>,
}
```

**Status**: ✅ Implemented. TracePhase, TraceContributor, WitnessStore, DynChip, BusConsumer all in place.

### 3.4 Axis 4: State Commitment Extension

**Problem**: Column state commitment is hardcoded to SSMC (sorted hash chain) and SMT (sparse Merkle tree). Applications with specialized data structures (e.g., sorted orderbook tree) need custom strategies.

**Mechanism**: `ColumnCommitment` trait (already defined in `stark/src/trace/column_commitment.rs`). Each column's state is committed via a pluggable strategy.

```rust
/// Pluggable column commitment scheme (batch API).
/// Already implemented in stark/src/trace/column_commitment.rs.
pub trait ColumnCommitment: Send + Sync {
    /// Human-readable name (e.g., "ssmc", "smt").
    fn name(&self) -> &str;

    /// All chip IDs this scheme produces.
    fn chip_ids(&self) -> Vec<ChipId>;

    /// Build traces for all columns of this scheme.
    fn build_traces(
        &self,
        cols: &[ColumnPlan],
        store: &WitnessStore,
    ) -> Result<Vec<(ChipId, TraceEntry)>, TabulaError>;

    /// Buses this scheme sends on.
    fn output_buses(&self) -> Vec<BusId>;
}
```

Also already implemented: `BusConsumer` trait, `ColumnPlan`, `ProofPlan`, `EncodingWidth`.

Custom commitment chips integrate through **existing buses** — no new bus definitions needed:

| Bus | Direction | Purpose |
|-----|-----------|---------|
| COMMITMENT_VERIF | receive | Bind computed digest to column metadata |
| BASE_STATE_ENTRY | receive | Consume init rows (base state values) |
| COALESCED_WRITE | receive | Consume write operations |
| POSEIDON_PERM | send | Internal hashing (if Poseidon-based) |
| RANGE_CHECK | send | Range-check limbs (if needed) |

**Built-in implementations** (Goal 6): `SsmcCommitment` (small columns), `SmtCommitment` (large columns).

**Status**: Trait ✅ implemented. Concrete impls (SSMC/SMT wrapping): Goal 6.

### 3.5 Axis 5: State Opening Extension

**Problem**: Tabula's state model is key-value: `Read(t, c, r)` returns the value at key `r`. Applications like DEXs need structural properties: "minimum key," "successor of k," "no key in range [a, b]." Without this, a malicious prover could skip better-priced orders.

**Mechanism**: `PropertyOpening` trait. Works with any compatible `ColumnScheme`. Verifier chips run in **Tier 2 (column proof)**, not Tier 1, because column state (sorted chains, Merkle paths) lives in Tier 2.

```rust
pub trait PropertyOpening: Send + Sync {
    fn name(&self) -> &str;
    fn compatible_scheme_tag(&self) -> u16;  // Links to ColumnScheme
    fn supported_queries(&self) -> &[PropertyQueryKind];
    fn prove(
        &self,
        commitment_digest: &[KoalaBear],
        query: &PropertyQuery,
        state: &[(RowKey, &[KoalaBear], bool)],
    ) -> Result<Box<dyn PropertyWitness>, PropertyError>;
    fn column_verifier(&self) -> Option<Box<dyn ChipExtension>>;  // Tier 2 chips
}

pub trait PropertyWitness: Send + Sync {
    fn value(&self) -> &[KoalaBear];
    fn key(&self) -> Option<RowKey>;    // The key satisfying the property
    fn is_null(&self) -> bool;
    fn as_any(&self) -> &dyn Any;
}

pub enum PropertyQuery {
    Minimum,
    Maximum,
    Successor { key: RowKey },
    Predecessor { key: RowKey },
    NonExistenceRange { lower: RowKey, upper: RowKey },
    Aggregate { kind: AggregateKind },
}
```

Surfaces as a new IR instruction:

```rust
Instruction::PropertyRead {
    dst_val: Slot,
    dst_key: Slot,       // Key at the result position
    dst_is_null: Slot,
    table: TableId,
    col: ColId,
    query: PropertyQuery,
}
```

**Cross-tier verification via PROPERTY_READ bus**:

```
Tier 1 (Execution): ExecutionChip SENDS on PROPERTY_READ bus
    → (table_id, col_id, query_type, query_arg0, query_arg1, result_key, result_val[W], is_null)

Tier 2 (Column): scheme-owned property chip (for SSMC: SsmcPropertyChip) RECEIVES from PROPERTY_READ bus
    → Verifies result against the column's old committed state anchors

Tier 3 (Root): Verifies PROPERTY_READ bus balance across tiers
    → Handled automatically by existing unbalanced_buses() mechanism
```

**State semantics**: PropertyRead queries **pre-batch committed state (com_old)**, providing snapshot isolation. The in-flight overlay has no commitment and cannot be verified in ZK.

**Scheme compatibility**: SSMC columns support O(1) min/max/successor/predecessor queries (sorted hash chain). SMT columns are unordered by key hash — structural queries require full scan or an indexed variant (future).

**Multiple queries**: Multiple PropertyRead calls on the same column in one batch are supported. The scheme-owned property chip receives all bus messages and verifies each against the same `com_old` state.

**Status**: Trait implemented in `machine/src/property.rs`. Cross-tier integration: Goal 7 Phase 5.

### 3.6 Axis 6: Execution Strategy Extension

**Problem**: The ExecutionChip is monolithic (278 columns at W=3). For applications where 90% of transactions follow a few patterns, most columns are wasted per instruction.

**Mechanism**: Template chips — execution chips specialized for specific tx patterns. Fewer columns, tighter constraints, same bus interactions.

```rust
pub trait TemplateChip: ChipSpec {
    fn template_id(&self) -> TemplateId;
    fn matches(&self, def: &TxTypeDef, info: &BodyTypeInfo) -> bool;
    fn max_instructions(&self) -> usize;
}
```

**Soundness invariant**: A template chip MUST emit identical LogUp bus messages as the generic ExecutionChip for the same transaction. The framework provides an equivalence test harness to verify this.

**Status**: Designed. Implementation in Goal 8.

### 3.7 Axis 7: Proof Composition Extension

**Problem**: Linear proof size/time with batch size. High-throughput apps need aggregation.

**Mechanism**: `ProofAggregator` trait (future).

```rust
pub trait ProofAggregator: Send + Sync {
    fn aggregate(&self, proofs: &[TabulaProof]) -> AggregatedProof;
    fn verify(&self, proof: &AggregatedProof) -> Result<(), VerificationError>;
    fn fan_in(&self) -> usize;
}
```

Strategies: layered STARK aggregation, recursive SNARK wrapper (Groth16/FFLONK for L1), IVC.

**Status**: Designed. Implementation deferred (future).

---

## 4. Framework Prerequisites

One-time changes in Tabula that enable the Zero-Modification Principle. After these, all app development requires zero Tabula code changes.

| # | Change | Status | Scope |
|---|--------|--------|-------|
| F1 | `BusId(u16)` newtype replacing closed enum | ✅ Done | ~50 LOC |
| F2 | `ChipExtension` trait | **Goal 6** | ~150 LOC |
| F3 | `TraceContributor` + `DynChip` | ✅ Done | Phase 1 |
| F4 | `WitnessStore` typed key-value store | ✅ Done | ~100 LOC |
| F5 | `ColumnCommitment` trait | ✅ Done (trait defined) | stark/src/trace/column_commitment.rs |
| F5b | `ColumnCommitment` impls (SSMC/SMT) | **Goal 6** | Extract existing logic into trait impls |
| F6 | `PropertyOpening` trait | **Goal 6** | ~100 LOC |
| F9 | `Precompile` IR variant | **Goal 7** | ~50 LOC |
| F10 | `PrecompileHandler` trait | **Goal 7** | ~50 LOC |
| F11 | `TemplateChip` trait | **Goal 8** | ~200 LOC |
| F12 | `tabula-machine::prelude` re-exports | **Goal 6** | ~50 LOC |
| F13 | `op_precompile` + `PrecompileBus` | **Goal 7** | ~100 LOC |

**Dependency chain**:

```
✅ Phase 1 (AnyRap + ChipRegistry)
✅ F1 (BusId) → F2 (ChipExtension) → F5b (ColumnCommitment impls)
✅ F3 (TraceContributor)                 → F6 (PropertyOpening)
✅ F4 (WitnessStore)                     → F11 (TemplateChip)
✅ F5 (ColumnCommitment trait)
F9 (Precompile IR) → F10 (PrecompileHandler) → F13 (PrecompileBus)
F12 (prelude) — independent
```

**Goal 6 scope**: F2, F5b, F6, F12 + builder API + extract SSMC/SMT into ColumnCommitment impls.

---

## 5. Case Study: ZK DEX (Lighter Protocol)

Lighter Protocol is a ZK order-book DEX built on custom Plonky2 circuits (~18 circuit modules). This case study validates Tabula's extensibility by mapping Lighter's architecture onto the framework.

### 5.1 Lighter's Architecture

- **Off-chain**: Custom matching engine + prover generates Plonky2 STARK proofs
- **State**: 8 concurrent Merkle trees at depths 6-80 (account, market, asset, orderbook, position, account-orders, API keys, account-delta)
- **Transactions**: 41 types across L1 (11), L2 (22), internal (8)
- **Key objects**: `AccountAsset` (96-bit balance), `Order` (price index, cumulative sums), `Position` (signed size, margin)
- **Proof pipeline**: Block proofs → recursive aggregation → PLONK/BN254 SNARK wrapper → Solidity verifier
- **Operations proven**: order matching, balance updates, ECDSA/EdDSA/Schnorr signatures, Merkle path verification, liquidation, funding rate, oracle prices

### 5.2 What Lighter Built Manually

| Component | Lighter's Approach | LOC (estimated) |
|-----------|-------------------|------|
| ECDSA verification circuit | Custom Plonky2 gadgets | ~2,000 |
| Orderbook tree (80-level Merkle) | Custom circuit per tree operation | ~3,000 |
| Order matching logic | Monolithic circuit with all 41 tx types | ~5,000 |
| State root computation | Custom SMT circuit | ~1,500 |
| Block proof aggregation | Cyclic recursion circuits | ~2,000 |
| Fixed-point arithmetic | Custom gadgets for 96-bit ops | ~1,000 |
| Plonky2 → SNARK wrapper | gnark (Go) BN254 wrapping | ~1,500 |
| **Total custom circuit code** | | **~16,000** |

Development time: months of ZK-specialized engineering.

### 5.3 Mapping to Tabula

| Lighter Component | Tabula Axis | Mechanism | App Code |
|---|---|---|---|
| ECDSA verification | Axis 1 (Precompile) | `EcdsaVerifyChip` (precompile 0x0001) | ~0 (standard library) |
| Orderbook tree state | Axis 4 (State Commitment) | `OrderbookTreeCommitment` (custom ColumnCommitment) | ~500 LOC |
| Best price query | Axis 5 (State Opening) | `PropertyQuery::Minimum` | ~300 LOC |
| Fill order execution | Axis 6 (Execution Strategy) | `FillOrderTemplate` (template chip) | ~300 LOC |
| Order placement | Core IR | `.tab` file with hash, read, write, assert | ~30 LOC |
| Risk/margin checks | Core IR | Arith + Cmp + Assert + DivMod | ~20 LOC |
| Fixed-point arithmetic | Core IR | Mul + DivMod with precision constants | ~0 (DSL) |
| State root | Core | Built-in SMT root proof | ~0 |
| Proof aggregation | Axis 7 | `ProofAggregator` (future) | ~0 (framework) |
| Signatures for oracle | Axis 1 | Same EcdsaVerifyChip | ~0 |
| **Total app-specific code** | | | **~1,150 LOC** |

### 5.4 DSL Example: Order Placement

```
tx place_order(
    sig: Bytes32,
    pubkey: Bytes32,
    market_id: U64,
    side: U64,
    price: U64,
    quantity: U64,
    nonce: U64,
) {
    // 1. Verify ECDSA signature (precompile — zero custom circuit code)
    let order_hash = hash(market_id, side, price, quantity, nonce);
    let valid = @ecdsa_verify(pubkey, order_hash, sig);
    assert(valid);

    // 2. Check margin
    let balance = read accounts[pubkey].balance;
    let required_margin = price * quantity / 1000000;
    assert(balance >= required_margin);

    // 3. Write order to orderbook
    let index = price * 1048576 + nonce;  // price-time priority
    write orders[market_id].prices[index] = price;
    write orders[market_id].quantities[index] = quantity;
    write orders[market_id].owners[index] = pubkey;

    // 4. Lock margin
    let locked = read accounts[pubkey].locked;
    write accounts[pubkey].balance = balance - required_margin;
    write accounts[pubkey].locked = locked + required_margin;

    emit("order_placed", market_id, side, price, quantity);
}
```

### 5.5 DSL Example: Fill Order

```
tx fill_order(
    taker: Bytes32,
    maker: Bytes32,
    market_id: U64,
    maker_index: U64,
    fill_qty: U64,
    fill_price: U64,
) {
    // 1. Verify maker order exists
    let maker_qty = read orders[market_id].quantities[maker_index];
    let maker_price = read orders[market_id].prices[maker_index];
    assert(maker_qty >= fill_qty);
    assert(maker_price == fill_price);

    // 2. Verify best price (PropertyOpening — custom VC chip proves this)
    let best_price = property_read orders[market_id].prices.minimum();
    assert(fill_price <= best_price);

    // 3. Update orderbook
    write orders[market_id].quantities[maker_index] = maker_qty - fill_qty;

    // 4. Settlement
    let cost = fill_qty * fill_price / 1000000;
    let taker_bal = read accounts[taker].balance;
    let maker_bal = read accounts[maker].balance;
    write accounts[taker].balance = taker_bal - cost;
    write accounts[maker].balance = maker_bal + cost;

    emit("fill", market_id, taker, maker, fill_qty, fill_price);
}
```

### 5.6 App Composition

```rust
// lighter-dex/src/main.rs
use tabula_machine::prelude::*;

fn main() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_extension(LighterDexExtension)
        .build()
        .expect("setup failed");

    let proof = machine.prove(&traces, &identities, &statement);
    machine.verify(&proof).expect("verification failed");
}

// lighter-dex/src/extension.rs
struct LighterDexExtension;

impl ChipExtension for LighterDexExtension {
    fn name(&self) -> &str { "lighter-dex" }

    fn register_chips(&self, reg: &mut ChipRegistry) {
        // Standard precompile (from tabula-std)
        reg.register(EcdsaVerifyChip::default());
        // Custom VC chip for orderbook
        reg.register(OrderbookTreeChip::<24>::default());
        // Custom property opening chip
        reg.register(OrderbookMinChip::default());
    }

    fn populate_witness(&self, store: &mut WitnessStore, ctx: &ExtensionContext) {
        let ecdsa_events = ctx.precompile_events(ECDSA_VERIFY_ID);
        store.put("ecdsa_events", ecdsa_events);
        let tree_witnesses = ctx.commitment_witnesses("orderbook_tree");
        store.put("orderbook_witnesses", tree_witnesses);
    }
}
```

### 5.7 Efficiency Analysis

| Component | Purpose-Built (Lighter) | Tabula Framework | Overhead |
|---|---|---|---|
| ECDSA chip | Custom Plonky2 gadgets | Precompile chip (same AIR) | ~0% |
| Orderbook tree | Custom Merkle circuit | Custom ColumnCommitment chip | ~5% (bus fingerprints) |
| Fill execution | Monolithic 41-tx circuit | Template chip (~60 cols) vs Interpreter (278 cols) | ~10% (bus overhead) |
| State root | Custom SMT circuit | Built-in SMT root proof | ~0% |
| Proof aggregation | Custom recursion | Framework aggregator | TBD |
| **Overall** | **Baseline** | | **~5-10% overhead** |

**The 5-10% overhead** is the composability tax: LogUp bus fingerprint computation that enables modular composition. In exchange:

- **~16,000 LOC** of custom circuit code → **~1,150 LOC** of app code
- Months of ZK-specialized development → weeks
- Custom proving infrastructure → battle-tested framework
- Upgrade burden on every protocol change → framework handles it

### 5.8 What Bytes32 Covers

Lighter needs types beyond U64/I64/Bool — 96-bit balances, signed positions, packed bitfields, Merkle paths. All of these map to Tabula's existing types:

| Lighter Type | Tabula Encoding | How |
|---|---|---|
| 96-bit balance | 2x U64 (hi/lo split) | `balance_hi * 2^64 + balance_lo` |
| Signed position size | I64 | Direct |
| Price (64-bit) | U64 | Direct |
| Merkle path node | Bytes32 | Direct (8 KoalaBear field elements) |
| EdDSA pubkey | Bytes32 | Direct |
| Order flags (packed bits) | U64 | Bit masking with existing logic ops |
| Fixed-point decimal | U64 | Integer with implicit denominator (e.g., /10^6) |

No custom type extensibility needed. The closed `ValueType` enum handles all cases.

---

## 6. Developer Experience

### 6.1 Complexity Tiers

| Tier | Who | What They Write | Effort |
|---|---|---|---|
| **DSL only** | App developer | `.tab` files (DSL tx types) | Trivial |
| **Standard precompiles** | App developer | Import and configure | Trivial |
| **Custom precompile** | App developer (ZK) | `PrecompileChip` + `ChipExtension` (~300-500 LOC) | Medium |
| **Custom chip** | App developer (ZK) | `ChipSpec` + `Air<AB>` + register | Medium |
| **Custom commitment** | App developer (ZK) | `ColumnCommitment` + AIR chip (~500-1000 LOC) | High |
| **Custom property opening** | App developer (ZK) | `PropertyOpening` + AIR chip (~500 LOC) | High |
| **Template chip** | Framework contributor | `TemplateChip` + equivalence tests (~300 LOC) | Medium |

### 6.2 Chip Definition Pattern (3-File Pattern)

App developers define custom chips using the same pattern as core chips:

1. **`columns.rs`**: `#[repr(C)]` trace columns parameterized by `T`
2. **`air.rs`**: `impl Air<AB> for MyChip where AB: InteractionAirBuilder`
3. **`trace.rs`**: `impl TraceContributor for MyChip`

The `AnyRap` blanket impl automatically applies — zero additional boilerplate.

### 6.3 Plonky3 Re-export Strategy

Apps building custom chips need p3 types. Rather than direct p3 dependency (diamond conflicts), Tabula re-exports through a stable prelude:

```rust
// tabula-machine/src/prelude.rs
pub use p3_air::{Air, AirBuilder, BaseAir};
pub use p3_koala_bear::KoalaBear;
pub use p3_field::{Field, PrimeField32, PrimeCharacteristicRing};
pub use p3_matrix::dense::RowMajorMatrix;

// Tabula-specific
pub use crate::{ChipSpec, AnyRap, ChipRegistry, ChipExtension};
pub use tabula_stark::{BusId, InteractionAirBuilder, TraceContributor, WitnessStore};
pub use tabula_stark::trace::{ColumnCommitment, ColumnPlan, EncodingWidth};
```

When Tabula upgrades p3 (e.g., 0.4 → 0.5), the prelude adapts internally. Apps see no breakage.

---

## 7. API Stability

### 7.1 Stability Tiers

| Tier | Guarantee | Examples |
|------|-----------|---------|
| **S (Stable)** | Breaking changes only on major versions | `Value`, `ValueType`, `CellKey`, `TableId`, `ColId`, `Transaction`, `Batch`, `TabulaError`, `Hasher`, `SigVerifier` |
| **A (Extension)** | May evolve across minor versions, with migration path | `ChipSpec`, `AnyRap`, `ChipExtension`, `TabulaMachine`, `ColumnCommitment`, `PropertyOpening`, `PrecompileHandler`, `BusId`, `WitnessStore`, `define_bus!` |
| **I (Internal)** | No stability guarantee | Individual chip implementations, column layouts, gadget internals, constraint details |

**Rule**: An app using only S + A APIs survives all minor version upgrades.

### 7.2 Bus Signature Stability

Bus signatures (LogUp fingerprint field layouts) are the primary interoperability contract:

- **Core buses** (BusId 0-99): Tier A — stable within minor versions
- **App buses** (BusId 100+): App-controlled — no Tabula stability guarantee

---

## 8. Implementation Plan (Goal 6)

### Phase 1: Builder API + ChipExtension (F2, F12)

| Task | Scope | Details |
|------|-------|---------|
| `TabulaMachine::builder()` | ~200 LOC | Fluent API: `.with_core_chips()`, `.with_chip()`, `.with_extension()`, `.build()` |
| `ChipExtension` trait | ~150 LOC | `register_chips()`, `populate_witness()`, `name()` |
| `tabula-machine::prelude` | ~50 LOC | Re-export p3 types + Tabula extension traits |
| Migrate `TabulaMachine::new()` | ~refactor | Internal: delegate to builder |

### Phase 2: ColumnCommitment impls (F5b)

| Task | Scope | Details |
|------|-------|---------|
| `SsmcCommitment` impl | ~refactor | Wrap existing SSMC shard logic into `ColumnCommitment` trait impl |
| `SmtCommitment` impl | ~refactor | Wrap existing SMT logic into `ColumnCommitment` trait impl |
| Per-column commitment selection | ~50 LOC | `ProofConfig.set_column_commitment(table, col, name)` |
| Wire into witness pipeline | ~refactor | `ColumnPlan.scheme_name` → dispatch to registered `ColumnCommitment` |

### Phase 3: PropertyOpening trait (F6)

| Task | Scope | Details |
|------|-------|---------|
| `PropertyOpening` trait + `PropertyQuery` | ~100 LOC | Trait + query enum |
| `PropertyRead` IR variant | ~50 LOC | One-time instruction addition |
| Executor dispatch for property reads | ~50 LOC | Route to PropertyOpening.prove() |
| Wire into trace pipeline | ~50 LOC | PropertyWitness → WitnessStore → chip trace |

### Estimated Total: ~800 LOC framework changes + refactoring

After Goal 6, Goals 7 (Precompile) and 8 (Templates) become unblocked.

---

## 9. Completeness Checklist

Requirements for supporting arbitrary ZK applications:

| Requirement | Axis | Mechanism | Status |
|---|---|---|---|
| Custom computations (ECDSA, Keccak) | 1 | Precompile pattern | Designed (Goal 7) |
| App-defined chips | 2 | ChipRegistry + AnyRap | ✅ Implemented |
| App-defined buses | 2 | BusId + define_bus! | ✅ Implemented |
| Automatic trace routing | 3 | TraceContributor + WitnessStore | ✅ Implemented |
| Custom state commitment | 4 | ColumnCommitment trait + impls | Trait ✅, impls **Goal 6** |
| Ordered data queries | 5 | PropertyOpening trait | **Goal 6** |
| Optimized tx execution | 6 | TemplateChip trait | Designed (Goal 8) |
| Proof aggregation | 7 | ProofAggregator trait | Designed (future) |
| Pluggable root proof | — | RootProof trait | ✅ Implemented |
| Builder composition API | — | TabulaMachine::builder() | **Goal 6** |
| Stable p3 re-exports | — | tabula-machine::prelude | **Goal 6** |
| Cross-tx invariants | Core | Continuation token pattern | ✅ Already possible |
| Oracle integration | 1 | SigVerify precompile | Designed (Goal 7) |
