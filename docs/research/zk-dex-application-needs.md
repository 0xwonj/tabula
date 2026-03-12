# ZK DEX Application Needs — Framework Requirements Analysis

> Research into what ZK applications (specifically DEXes) need from an underlying proving system,
> based on analysis of Lighter Protocol, StarkEx/dYdX, and ZK framework extensibility patterns
> (SP1, RISC Zero, OpenVM, Halo2, Plonky3).

## 1. Lighter Protocol Architecture

### Overview

Lighter is a ZK DEX (decentralized exchange) offering verifiable order matching and liquidations
with performance comparable to centralized exchanges. It operates as a validium/rollup hybrid
with off-chain execution and on-chain proof verification.

### Proving Stack

- **Circuit layer**: Custom Plonky2 circuits (Rust, ~18 circuit modules)
- **Recursion**: Block proofs aggregated into batch proofs via cyclic recursion
- **On-chain verification**: Plonky2 proofs wrapped in a PLONK/BN254 SNARK via gnark,
  verified by Solidity contracts on Ethereum L1
- **Data availability**: EIP-4844 blobs + validium mode (separate stateRoot and validiumRoot)

### State Model

Lighter manages state as Merkle trees with these key structures:

| Tree | Depth | Purpose |
|------|-------|---------|
| Account tree | 48 levels | All user accounts |
| Market tree | 12 levels | Market configurations |
| Asset tree | 6 levels | Asset definitions |
| Order book tree | 80 levels | Per-market order books |
| Position tree | 8 levels | Per-account positions |
| Account orders | 60 levels | Per-account order history |
| API key tree | 8 levels | Per-account API keys |
| Account delta tree | 48 levels | Batch change tracking |

### Types Required by a DEX

Analysis of Lighter's `circuit/src/types/` reveals 28 type modules:

**Core financial types**:
- `Order`: price_index (32-bit), nonce_index (48-bit), cumulative ask/bid base/quote sums
- `AccountAsset`: balance (96-bit BigUint), locked_balance (96-bit BigUint), margin_mode
- `AccountPosition`: position size (56-bit signed), entry quote, margin, funding rate prefix sums
- `Market`: order book root, nonces, fees, size/quote multipliers, open interest, limits
- `MarketDetails`: margin fractions (16-bit), mark/index/impact prices (32-bit), funding rates (58-bit),
  open interest (56-bit), strategy index

**Risk management types**:
- `RiskParameters`: collateral (96-bit signed), total_account_value (96-bit signed),
  initial/maintenance/close-out margin requirements (BigUint)

**Fixed-point arithmetic** — The system uses multi-limb BigInt/BigUint representations:
- 64-bit, 96-bit, 128-bit, 256-bit limbed integers
- Tick-based scaling: FEE_TICK=1M, MARGIN_TICK=10K, SHARE_TICK=10K, FUNDING_RATE_TICK=1M
- USDC_TO_COLLATERAL_MULTIPLIER for precision conversion

**Beyond U64/I64/Bool/Bytes32, a DEX needs**:
1. **Fixed-point decimals** (various precisions: 6, 8, 16, 20 decimal bits)
2. **BigUint / BigInt** (96-bit minimum for collateral, up to 256-bit for intermediate products)
3. **Signed integers** with explicit sign tracking (position sizes, P&L, funding)
4. **Packed bitfield types** (margin mode, market status, order type flags)
5. **Merkle path witnesses** (variable-depth authentication paths)
6. **Cryptographic key types** (EdDSA pubkeys, ECDSA recovery, Schnorr signatures)

### Operations That Must Be Proven

Lighter's circuit modules reveal the complete operation set:

**Transaction types** (41 total across 3 categories):
- L1 (11): Deposit, Withdraw, CreateMarket, UpdateMarket, CreateOrder, ChangePubKey, etc.
- L2 (22): Transfer, CreateOrder, CancelOrder, ModifyOrder, UpdateLeverage, Stake/Unstake, etc.
- Internal (8): ClaimOrder, Deleverage, Liquidation, ExitPosition, PendingUnlock, etc.

**Core computations proven in circuits**:

1. **Order matching** (`matching_engine.rs`):
   - Taker/maker price crossing validation
   - Order book Merkle path verification
   - Priority queue position calculation
   - Spot vs perpetual routing

2. **Trade application** (`apply_trade.rs`):
   - Balance delta computation with extension multipliers
   - Fee calculation and distribution (taker/maker/liquidation fees)
   - Position size updates with entry quote recalculation
   - Realized P&L computation on position closure
   - Funding rate application to collateral
   - Isolated vs cross margin handling
   - Open interest tracking

3. **Liquidation** (`liquidation.rs`):
   - Zero-price computation (where margin = 0)
   - Account health assessment
   - Forced position closure with penalty fees
   - Insurance fund interaction

4. **Block processing** (`block.rs`, `block_constraints.rs`, `block_pre_execution.rs`):
   - State root chain verification (old_root -> new_root)
   - Timestamp ordering
   - Priority operation hash continuity
   - Oracle price updates, premium calculation, funding rate computation

5. **Signature verification**:
   - EdDSA (L2 transactions)
   - ECDSA with address recovery (L1 transactions)
   - Schnorr signatures

6. **Batch recursion** (`recursion/batch.rs`):
   - Block proof aggregation into batch proofs
   - State root chain validation across blocks
   - On-chain operation hash accumulation via Keccak

### Performance Profile

**Circuit configuration**:
- 136 wires, 80 routed wires per gate
- 100-bit security level
- FRI: 28 query rounds, rate_bits=3, cap_height=4, 16-bit proof-of-work
- Two-layer proof composition (inner circuit + outer wrapper)

**Capacity constants**:
- 3 accounts per transaction
- 2 assets per transaction
- 8 strategies maximum
- 1000 orders per account maximum

**Operational requirements** (inferred from design):
- Block-level batching of transactions (non-empty blocks required)
- Sequential state root chaining within blocks
- Recursive aggregation across blocks into batches
- Final SNARK wrapping for on-chain verification (~200-300ms on Ethereum)

## 2. Comparison with Other ZK DEXes

### StarkEx (powers dYdX v3)

- **Proving system**: STARK proofs generated by SHARP (shared prover service)
- **Execution model**: Cairo VM programs define state transitions
- **State**: Merkle tree with vault leaves
- **Operations**: Deposits, withdrawals, trades, conditional transfers, forced actions
- **Data availability**: ZK-Rollup, Validium, or Volition (per-transaction choice)
- **Key difference**: General-purpose Cairo VM vs Lighter's custom circuits

### Architectural Patterns Across ZK DEXes

| Aspect | Lighter | StarkEx/dYdX | General Pattern |
|--------|---------|-------------|-----------------|
| Proof system | Plonky2 + PLONK wrapper | STARK via SHARP | Custom circuits or zkVM |
| State model | Sparse Merkle trees | Merkle trees (vaults) | Always Merkle-committed |
| Batching | Block -> Batch recursion | Transaction batches | Always batched, never single-tx |
| On-chain | SNARK verification | STARK verification | Minimal on-chain footprint |
| Escape hatch | Desert mode | Forced actions | Always needed for trust |

## 3. What ZK Frameworks Offer (Extensibility Patterns)

### SP1 (Succinct)

- **Model**: zkVM — compile Rust to RISC-V, prove execution
- **Extensibility**: "Precompiles" — additional STARK tables alongside CPU
  - Built-in: SHA256, Keccak, secp256k1, BN254, BLS12-381
  - Custom precompiles addable as separate proving tables
- **Developer experience**: "Write ZK programs in standard Rust"
- **Latest**: Hypercube (multilinear STARKs, Jagged PCS, LogUp GKR)
- **Performance**: "Real-time proving with 16 GPUs", up to 5x for compute-heavy workloads

### RISC Zero

- **Model**: zkVM — Rust compiled to RISC-V ELF, proven via STARK
- **Developer flow**: Guest program (proven code) + Host program (orchestration)
- **Extensibility**: Feature flags for GPU acceleration, less custom-circuit extensibility
- **Minimum**: 16GB RAM for local proving

### OpenVM (formerly Axiom)

- **Model**: "Modular no-CPU architecture" — no central processor, just parallel chips
- **ISA extension model**: Three-component pattern per extension:
  1. `circuit/` — STARK constraints for the operation
  2. `guest/` — Rust intrinsics for calling the operation
  3. `transpiler/` — Compilation from high-level to VM instructions
- **Built-in extensions**: RV32IM, bigint, algebra, ECC, pairing, keccak256, sha256, native field
- **Key insight**: Custom chips addable without forking; Rust intrinsics provide ergonomic access
- **On-chain**: Every VM instance includes Ethereum verification support

### Halo2

- **Model**: Circuit SDK — developers build custom PLONKish circuits
- **Chip pattern**: Hierarchical composition of chips sharing columns
  - Top-level chip defines column layout
  - Sub-chips operate on subsets of columns
  - Communication via shared columns and lookup tables
- **Regions**: Isolated cell groups with relative references
- **Floor planner**: Automatic row assignment optimizing circuit layout
- **Key insight**: Lookup tables with tag columns for multi-table dispatch

### Plonky3

- **Model**: Low-level toolkit — polynomial commitment schemes, fields, hash functions
- **Approach**: ~50 composable crates (fields, hash, FRI, AIR, etc.)
- **Developer use**: Select and combine primitives for custom proof systems
- **Key insight**: Not an application framework — a proving-system construction kit

## 4. What a Near-Optimal ZK App Framework Needs

### From the Lighter Case Study

Lighter spent enormous engineering effort building custom Plonky2 circuits for every
operation. Their `circuit/` crate contains 18 subdirectories and 25+ source files of
hand-written constraint logic. This reveals the gap between proving system toolkits
and application needs.

### Required Framework Capabilities

**A. Type System Extensibility**

Applications need types far beyond scalar fields:
- Fixed-point decimals with configurable precision (6-20 fractional bits)
- Multi-precision integers (96-bit, 128-bit, 256-bit)
- Signed arithmetic with explicit overflow handling
- Packed structs with bitfield access
- Domain-specific types (Price, Balance, Position) with invariant enforcement

**B. State Management Primitives**

Every ZK app manages authenticated state:
- Sparse Merkle tree operations (insert, update, delete, membership proof)
- State root chaining across sequential transitions
- Batch delta tracking (what changed in this block/batch)
- Multiple concurrent state trees at different depths
- State partitioning for parallel proving

**C. Operation Composition**

Applications need to compose primitive operations into complex transactions:
- Conditional execution (if spot market vs perpetual market, different logic)
- Multi-party state updates (taker + maker + fee account in one transaction)
- Transaction batching with sequential consistency
- Operation-level error handling (cancel trade vs fail block)
- Constraint routing based on runtime type tags

**D. Cryptographic Primitives**

Standard set needed by virtually all applications:
- Hash functions (Poseidon for in-circuit, Keccak/SHA for interop)
- Signature verification (EdDSA, ECDSA with recovery)
- Merkle proof generation and verification
- Commitment schemes

**E. Recursive Proof Composition**

Production systems always need proof aggregation:
- Transaction proofs -> block proofs -> batch proofs
- Cross-proof state root chaining
- SNARK wrapping for on-chain verification
- Configurable recursion depth

**F. Performance Requirements**

Based on exchange-grade applications:
- Block production: sub-second transaction inclusion
- Proof generation: seconds per block (with GPU), minutes per batch
- Verification: milliseconds (on-chain gas cost matters)
- Proof size: <1KB compressed for on-chain submission
- Throughput: 100-1000+ transactions per proof batch

**G. Developer Experience**

The SP1/OpenVM trend is clear — developers want to write Rust, not circuits:
- Domain-specific abstractions over constraint systems
- Automatic witness generation from execution traces
- Debugging tools (constraint violation reporting, trace inspection)
- Testing framework (unit test individual operations, integration test batches)
- Type-safe APIs that prevent constraint bugs at compile time

### Framework Architecture Spectrum

```
Plonky3          Halo2           Tabula          OpenVM/SP1
(toolkit)     (circuit SDK)   (app framework)     (zkVM)
   ↓               ↓               ↓                ↓
raw primitives   chip pattern   state+types+ops    write Rust
max flexibility  good balance   domain-optimized   max ergonomics
max effort       moderate        moderate           least effort
best perf        good perf      good perf          worst perf
```

Tabula sits in the "app framework" zone — more opinionated than Halo2 (provides state
management, type system, execution model) but more domain-efficient than zkVMs (custom
chips for critical operations rather than general-purpose CPU emulation).

### Key Differentiators for Tabula

What Lighter built manually that Tabula could provide as framework primitives:

1. **Authenticated state management** — Tabula's SMT chips, column shards, and state
   commitment system already handle this. Framework should expose this as a first-class
   API for application state.

2. **Type-safe multi-precision arithmetic** — The TypeTag system could be extended with
   FixedPoint, BigUint, SignedBigInt types that compile to efficient field element
   representations with automatic overflow checking.

3. **Transaction composition model** — Tabula's execution model (programs operating on
   typed columnar state) maps naturally to DEX-style multi-party state transitions.

4. **Batch proving with state chaining** — The sharded proof architecture already
   supports this. Framework should make block/batch composition declarative.

5. **Pluggable signature verification** — Via precompile/chip extension mechanism.

6. **Recursive aggregation** — Already in the roadmap. Critical for production throughput.
