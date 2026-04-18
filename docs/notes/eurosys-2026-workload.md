# EuroSys 2027 — Evaluation Workload

Working note. **Authoritative for the paper's workload** until superseded
by the evaluation section of the draft itself. Paper target is EuroSys
2027 (deadline 2026-05-14; conference April 2027).

Companion to:
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) — contribution list; C3 ablation taxonomy references this note.
- [`evaluation-harness.md`](evaluation-harness.md) — `tabula-eval` crate design that measures this workload (workload model, SystemAdapter, reuse policy, schema).
- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md) — cross-role stage types the harness consumes.

## Framing

The paper's workload must:
- Exercise all five C2 mechanisms (M1–M5).
- Carry the A3 core-scaling story (per-column shard parallelism).
- Have a real-world analogue reviewers recognize.
- Admit a fair SP1 / RISC0 port for A5 comparison.
- Be implementable in the current Tabula DSL without extensions.

The workload *class* that satisfies all five is **typed-tabular rollup
batches** — the class that production systems like StarkEx (Immutable X,
Sorare, previously dYdX v3) are built around. This workload is
**inspired by that class**, not a literal port of any one production
system. It is implemented **Tabula-native** — optimized for Tabula's
per-column sharding and compile-time sealing, not shaped to match
Cairo's uniform-circuit model or any other prover's constraints.

The paper cites StarkEx as architectural precedent (hand-rolled tabular
proving for rollups at production scale) and frames Tabula's workload
as "the same class of computation, implemented with a compiler–proof
co-designed proving system."

The comparison this note supports is therefore **semantic-native**, not
constraint-equalized: all systems implement the same state-transition
semantics and aligned commitment primitives, but the paper does not
claim that every prover's internal representation is normalized to a
single cross-system circuit shape.

## Why not a literal StarkEx port

StarkEx Spot's production details (Pedersen-ECDSA-on-STARK-curve
signatures, order-tree partial fills, fee routing, quantization math,
validium/rollup split, fact registry) are scaffolding for a product.
None of them carry paper-claim content.

Most critically: signature verification on the STARK curve is **20–30×
the cost of the state transition itself** per transaction (from the
Starkware spec review). Including signatures would cause every
measurement to be dominated by a cryptographic-primitive choice rather
than the tabular-proof architecture that is the paper's claim.

A faithful port is the wrong engineering target. The right target is to
implement the *workload class* — a multi-asset account-based rollup
batch with transfer, conditional transfer, spot trade, and withdrawal
semantics — in a Tabula-native shape.

## State model

Two tables.

### `accounts` — multi-asset portfolio

Key: `account_id: Digest`

Columns:
- `balance_0, balance_1, ..., balance_{N-1}: U64` — per-asset balances
- `nonce: U64` — per-account transaction ordering
- `frozen: Bool` — admin / compliance freeze

`N` is the asset count, swept for A3 core-scaling:
`N ∈ {4, 8, 16, 32}`.

**Why multi-column rather than literal StarkEx flat vaults.** StarkEx
uses `vault_id → {owner, token_id, balance}` because Cairo circuits
don't benefit from per-column structure. Tabula's per-column sharding
makes the natural representation a per-account row with `N` asset
columns. This is itself an instance of the co-design claim — state
representation follows proof architecture. It also respects Tabula's
`NATIVE_MAX_KEY_COMPONENTS = 1` constraint cleanly.

### `withdrawal_queue` — append log of pending L1 withdrawals

Key: `withdrawal_id: U64` (monotonic counter)

Columns:
- `account_id: Digest`
- `asset_idx: U64`
- `amount: U64`
- `l1_dest: Digest`

Exercises NF-1 discipline (each `withdrawal_id` unique per batch) and
multi-table cross-shard bus balance (M3c).

## Transaction types

Four types. All integer arithmetic; no fixed-point; no signature
verification (see *Signature verification elided* below).

### 1. Transfer

Fields: `asset_idx, sender_id, receiver_id, amount, sender_nonce`

Reads: `sender.balance[asset_idx]`, `sender.nonce`, `sender.frozen`,
`receiver.balance[asset_idx]`, `receiver.frozen`.

Checks:
- `sender_id ≠ receiver_id` (NF-4 key-alias)
- `!sender.frozen ∧ !receiver.frozen`
- `sender.balance[asset_idx] ≥ amount`
- `sender.nonce == sender_nonce`

Writes:
- `sender.balance[asset_idx] -= amount`
- `receiver.balance[asset_idx] += amount`
- `sender.nonce += 1`

### 2. ConditionalTransfer

Like Transfer, plus:
- Field: `fact_present: Bool`
- Check: `fact_present == true`

The L1 fact-registry predicate reduces to a single Bool input per the
StarkEx spec ("condition" is a truncated keccak stored in a
`mapping(bytes32 => bool)` on L1).

### 3. SpotTrade

Fields: `sell_idx, buy_idx, a_id, b_id, amount_a_sells,
amount_b_sells, a_nonce, b_nonce, ratio_num, ratio_den,
fee_collector_id, fee_rate`

Reads: for both `a` and `b`: `balance[sell_idx]`, `balance[buy_idx]`,
`nonce`, `frozen` (8); plus `fee_collector.balance[buy_idx]` (1) —
**9 column reads total**.

Checks:
- `a_id ≠ b_id` (NF-4)
- `sell_idx ≠ buy_idx`
- Neither party frozen
- Sufficient balances on both sides
- Nonces match
- Price consistency: `amount_a_sells × ratio_den == amount_b_sells × ratio_num`
- Fee derivation: `fee_amount = amount_a_sells × fee_rate / FEE_SCALE`
  (integer division; `FEE_SCALE` is a sealed constant, e.g. `10_000`
  for basis points)

Writes (**7 total**):
- `a.balance[sell_idx] -= amount_a_sells`; `a.balance[buy_idx] += amount_b_sells`
- `b.balance[buy_idx] -= amount_b_sells`;
  `b.balance[sell_idx] += (amount_a_sells - fee_amount)`
- `fee_collector.balance[sell_idx] += fee_amount`
- both nonces `+= 1`

**Simplification from StarkEx:** no partial fills, no order tree, no
order-flow fee routing (rebate / referral / maker-taker routing trees).
Every matched trade is a full fill. Removes the third Merkle structure
StarkEx maintains for order accounting without losing any load-bearing
mechanism for the paper. We **retain semantic fee collection** into a
designated `fee_collector` row — this exercises NF-4's hard aliasing
case when `fee_collector_id == a_id` or `== b_id` (the fee row aliases
a trading-party row, forcing same-row coalescing of two distinct
writes on the same `(table, column)` coordinate). §5.4(c) uses the
resulting empirical regime as the basis for the heavy-aliasing
losing-threshold measurement.

**Integer width:** `amount × ratio` can exceed U64. Clamp at
fixture-generation time (enforce `amount × ratio < 2^63`) rather than
upgrading to U128 in the DSL.

### 4. Withdraw

Fields: `account_id, asset_idx, amount, l1_dest, nonce`

Reads: `account.balance[asset_idx]`, `account.nonce`, `account.frozen`.

Checks: not frozen; sufficient balance; nonce matches.

Writes:
- `account.balance[asset_idx] -= amount`
- `account.nonce += 1`
- **append** new row to `withdrawal_queue` with a fresh `withdrawal_id`.

## Static relations (M5)

- `asset_idx ∈ {0, 1, ..., N-1}` — static enum, resolves to a sealed lookup table.
- `frozen ∈ {0, 1}` — Bool domain.
- `amount ∈ [0, 2^63)` — range check, resolves to a sealed range table.
- `nonce ∈ [0, 2^32)` — range check.
- Optional (toggle): sanctions set membership on `account_id` — static set relation.

## Signature verification elided

The paper methodology explicitly elides signature verification.

1. **Orthogonal to the claim.** The paper claims compiler–proof
   co-design for tabular state transitions. Signature verification is
   a cryptographic-primitive choice; it sits under the state-transition
   proof layer, not inside it.
2. **Cost domination.** Per Starkware's own arithmetization, per-tx
   Pedersen + STARK-curve ECDSA is 20–30× the state-transition cost.
   Including signatures would make every measurement dominated by that
   factor and precompile availability rather than by tabular-proof
   architecture.
3. **Symmetric across the comparison.** Tabula, SP1, and RISC0 would
   each pay roughly the same additive signature cost if included.
   Eliding preserves the *relative* comparison.

The paper must disclose this elision explicitly in the methodology
section and reference separate lines of work for signature-proof
integration.

## Mechanism coverage

| Mechanism | Exercised by | Strength |
|-----------|--------------|----------|
| **M1 (NF sealing)** | NF-1 (withdrawal_queue append), NF-3 (read-then-write balance), NF-4 (src≠dst, a≠b, and fee_collector aliasing hard case in SpotTrade), True SSA on witness vars | all four + SSA |
| **M2 (width specialization)** | Digest (account_id, l1_dest), U64 (balance, nonce, amount), Bool (frozen, fact_present) | full width mix |
| **M3 (per-column sharding)** | N balance columns + nonce + frozen + 4 withdrawal_queue columns; cross-shard bus balance across two tables | **primary A3 vehicle** |
| **M4 (statement-first)** | `PublicStatement = (old_root, new_root, public_context_digest, applied_tx_digest, event_digest)` checked through `BoundStatement` against the sealed artifact | standard |
| **M5 (relation resolution)** | asset_idx enum, frozen Bool, range checks on amount/nonce, price-ratio cross-multiplication | strong |

## Scaling and experiment plan

### Sweep axes

- **N** (asset / column count): `{4, 8, 16, 32}` — A3 core-scaling vehicle.
- **M** (batch size, txs per batch): `{1 000, 5 000, 10 000}` — linear-scaling sanity check; pinned comparison point `M = 5 000`.
- **S** (state size, number of accounts): `{10 000, 100 000, 1 000 000}`.
- **Tx distribution:** 60% Transfer / 10% ConditionalTransfer / 25% SpotTrade / 5% Withdraw. Informed by StarkEx-production reported traffic (dYdX v3 + Immutable X; L2Beat snapshots).
- **Asset mix within batch:** 80/20 — top-2 assets account for 80% of tx volume, remaining `N-2` assets share the 20% tail. Matches concentration of real trading pairs.

### Experiments

| Experiment | Fixed | Varied | Systems |
|------------|-------|--------|---------|
| **A5 baseline comparison** | N=16, M=5k, S=100k | — | Tabula, SP1, RISC0 |
| **A3 core-scaling**         | M=5k, S=100k       | N ∈ {4,8,16,32} | Tabula only |
| **A1 NF toggle**            | N=16, M=5k, S=100k | NF on vs off | Tabula only |
| **A2 uniform-width**        | N=16, M=5k, S=100k | typed vs Digest-uniform | Tabula only |
| **Batch-size scan**         | N=16, S=100k       | M ∈ {1k,5k,10k} | Tabula primary; SP1 secondary if time |

## SP1 / RISC0 port design

Rust guest program with identical semantics to the Tabula DSL program:

```rust
struct Portfolio {
    balances: [u64; N],
    nonce: u64,
    frozen: bool,
}
type State = HashMap<[u8; 32], Portfolio>;
```

State commitment: **Poseidon2 sparse Merkle tree**. Not SHA256 or
Keccak — otherwise the hashing-cost axis is unfair against Tabula's
Poseidon2-based SSMC.

**Poseidon2 precompile availability** in SP1 / RISC0 is a prerequisite.
Verify before locking. If absent, the port implements Poseidon2 as a
guest function and the paper reports this choice transparently.

Semantic equivalence: identical transaction types, identical validation
rules, identical state commitment scheme, and the same externally
supplied statement-first verification boundary. The only difference is
how each system *proves* the resulting state transition.

The Rust port's source is committed to the evaluation artifact.

## Measurement Outputs Feeding The Paper

The final in-paper figure/table inventory is controlled by the locked
section-outline note. This workload note defines the measured outputs
that feed that inventory:

1. Headline end-to-end metrics at `(N=16, M=5k, S=100k)` for Tabula,
   SP1, and RISC0.
2. Compile/setup latency and amortization scenarios for schema-evolution
   events.
3. Internal ablation deltas for A1, A2, and A3-seal.
4. Supplementary scaling sweeps over `N ∈ {4, 8, 16, 32}` and core
   count.
5. Transfer-of-representation checks and signature-elision
   extrapolation.

## Tabula DSL implementation notes

Source will be templated over `N`. Expected size: ~450 LoC for
N-agnostic scaffolding plus generated per-N specialization.

Compile-time code generation handles the `N` balance columns. Spike a
small `N = 4` implementation end-to-end before committing to the
`N = 32` point, to verify the DSL handles wide schemas without
compile-time blowup.

## Open risks

1. **Poseidon2 precompile in SP1 / RISC0.** If missing, baselines
   implement Poseidon2 in guest code — costs them, not us, but report
   transparently.
2. **N = 32 column compile-time.** Spike `N = 4` before committing.
3. **SpotTrade overflow.** Clamp at fixture-generation time so
   `amount × ratio < 2^63`.
4. **Reviewer objection to signature elision.** Pre-empt with an
   explicit methodology-section disclosure.
5. **Fixture realism.** Tx distribution and asset-mix parameters must
   be defensible. Cite the L2Beat + StarkEx traffic sources.
6. **Statistical rigour.** ≥ 5 runs per data point; report median plus
   95% CI. Pin the measurement machine spec (CPU, RAM, cores).

## References

External:
- `starkware-libs/starkex-for-spot-trading` — Cairo program for StarkEx Spot; primary architectural reference.
- `docs.starkware.co/starkex/spot` — gateway, signature, fact-registry, and trade-flow docs.
- `l2beat.com` — production traffic snapshots for Immutable X, Sorare, dYdX v3.

Internal:
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) — paper contribution list.
- [`evaluation-harness.md`](evaluation-harness.md) — harness crate design that measures this workload.
- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md) — cross-role stage types the harness consumes.
- [`distributed-proving.md`](distributed-proving.md) — Def 1 vs Def 2 analysis.
