# SP-3 Witness Chip-Kit — Implementation Plan

> Status: draft, awaiting approval
> Date: 2026-04-19
> Branch: `refactor/witness-chip-kit`
> Design: [2026-04-19-sp3-witness-chip-kit-design.md](../specs/2026-04-19-sp3-witness-chip-kit-design.md)
> Umbrella: [2026-04-18-architecture-refactoring-design.md](../specs/2026-04-18-architecture-refactoring-design.md)

Concrete step-by-step plan for landing SP-3. Implements the shape
resolved in the design doc (see §1 Goal), with the §2.1 amendment
(trait in `tabula-stark`, not `tabula-ext`) already applied in S1.

The single goal of this plan is to **make `tabula-witness`
chip-agnostic**: zero `use tabula_chips::<concrete-row>` lines left
after S4, without breaking byte-identical proof determinism on the
`basic` and `membership` end-to-end flows.

---

## 0. Current State (S1 landed)

Commit `dc2690e` on `refactor/witness-chip-kit` added:

- `tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitScratch, KitFinalizeContext}`
- `tabula_ext::ChipWitnessKit` re-export for authoring ergonomics
- `ExecutionBackend::witness_kits()` default-empty method
- Off-mode guardrail test `sp3_witness_chip_import_guardrail_off_mode`
  that scans `crates/witness/src/**/*.rs` and reports ~21 non-allowlisted
  `use tabula_chips::` imports

### S1 review — refinements to fold into S2

1. **`KitFinalizeContext` is a PhantomData stub.** S2 must flesh it out
   with: a downcast-and-take accessor for the kit's scratchpad entry,
   and a read-only borrow of `PreparedRelationProof` (needed for
   `RelationTableKit` in S3). Core lowering output access is deferred
   — the chips that need cross-chip reads land in S3, and the concrete
   signature shape can wait until then.
2. **`KitError::Internal` variant is unused by S1 and cannot be
   constructed by outside callers from a different crate without public
   constructors.** Leave it for now; kits in S2/S3 will populate it via
   `KitError::Internal { chip, message: ... }` literal construction,
   which works because its fields are `pub`.
3. **Kit registry location.** Design doc §3.1 was amended to say "TBD
   in S2." Plan below pins it: **a new `tabula-witness::ChipKitRegistry`
   owned by the witness lowering driver, not by `ChipRegistry`**.
   `ChipRegistry` stays AIR-only (matches its current scope).
4. **Guardrail allowlist.** S1 allowlist is `*Kit` suffix +
   `*_WITNESS_LABEL` suffix + a fixed set of tails. Tighten in S4 once
   migrations complete so `tabula_chips::<chip>::*Row` is explicitly
   forbidden. No change needed mid-flight.

---

## 1. Global Invariants (hold across every stage)

Every commit in this plan must preserve:

- **Green build:** `cargo build --workspace --all-features` succeeds.
- **Green tests:** `cargo test --workspace --all-features` passes.
- **Formatted:** `cargo fmt --all -- --check` passes.
- **Byte-identical proofs:** `basic` and `membership` end-to-end runs
  produce `proof.bin` byte-identical to a reference captured before
  S2 starts. Checked via the recipe in §7.

Stages do not land a commit until all four hold for that commit.

---

## 2. S2 — Pilot Kit Migration: `IrHashChip`

**Goal.** Replace the `ir_hash_calls: Vec<IrHashCall>` typed field in
the witness lowering pipeline with a `ChipWitnessKit`-mediated push
path. Witness stops importing `IrHashCall` anywhere. The end-to-end
proof output is unchanged byte-for-byte.

Chosen as pilot because `IrHashCall` has a single producer
(`ops/hash.rs` line 45: `self.ir_hash_calls.push(call)`, confirmed
append-only) and a single consumer (`execution_store.rs` line 32:
`store.put(IR_HASH_WITNESS_LABEL, ...)`).

### S2.1 — Capture byte-identity reference

Before any code changes:

1. `cargo build -p tabula-cli --features prove`
2. Generate both examples: `target/debug/tabula-cli example basic --dir /tmp/sp3-ref-basic` and `example membership --dir /tmp/sp3-ref-membership`
3. Execute + prove both, saving `proof.bin` as the reference.
4. Store the two `proof.bin` files at `/tmp/sp3-ref/{basic,membership}-proof.bin` so subsequent stages can `diff` against them.
5. **Pre-flight check:** verify which of `basic` / `membership` actually
   exercises `Op::Hash`. If `basic` emits zero IR-hash calls, the
   pre-S2 run produces `store.put(IR_HASH_WITNESS_LABEL, vec![])` (empty
   vec) while the post-S2 run with `IrHashKit` also produces an empty
   vec via `ctx.take_scratch::<Vec<IrHashCall>>` default — so the
   absent-label-vs-empty-vec delta does not reach the AIR. But confirm
   the empty-vec path in `execution_store.rs` still emits the label
   before S2 (it does: `store.put(IR_HASH_WITNESS_LABEL, lowering.ir_hash_calls.clone())`
   is unconditional). If S2 changes this to skip absent labels, byte
   identity can drift. Guard: **the `IrHashKit::finalize` must always
   emit the label, even when the scratch entry is absent.** Already
   encoded in S2.3 code (default Vec).

No commit. This is a local anchor only.

### S2.2 — Flesh out `KitFinalizeContext`

In `crates/stark/src/witness_kit.rs`:

1. Remove the `PhantomData` stub. Replace with:
   ```rust
   pub struct KitFinalizeContext<'a> {
       scratch: &'a mut KitScratch,
   }
   impl<'a> KitFinalizeContext<'a> {
       pub(crate) fn new(scratch: &'a mut KitScratch) -> Self { Self { scratch } }
       /// Remove the kit's scratchpad entry and downcast to `T`. Returns
       /// a default `T` if the entry is absent (kit was registered but
       /// no rows emitted).
       pub fn take_scratch<T: Any + Default + Send>(&mut self, chip_id: ChipId) -> Result<T, KitError> {
           match self.scratch.remove(&chip_id) {
               None => Ok(T::default()),
               Some(boxed) => boxed.downcast::<T>()
                   .map(|b| *b)
                   .map_err(|_| KitError::DowncastFailed(chip_id)),
           }
       }
   }
   ```
2. Expose `new` as `pub(crate)` behind a shim — the witness driver is
   the only legitimate constructor. Since `pub(crate)` on a trait-level
   type doesn't cross crate boundaries, expose as `#[doc(hidden)] pub
   fn new(...)` with a doc warning that the witness driver is the sole
   intended caller.
3. Add a unit test in `crates/stark/src/witness_kit.rs`:
   - kit with no scratch entry → `take_scratch` returns `Vec::new()`
   - wrong downcast type → `DowncastFailed(chip_id)`
   - happy path → round-trips the stored vector

### S2.3 — Define `IrHashKit`

New file `crates/chips/src/ir_hash/kit.rs`:

```rust
use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext, KitScratch};

use super::{IR_HASH_WITNESS_LABEL, IrHashCall};

/// Chip id for IrHashChip. Matches `IrHashChip::CHIP_ID`.
pub struct IrHashKit;

impl IrHashKit {
    // reuse the CHIP_ID constant defined on IrHashChip
    const CHIP_ID: ChipId = crate::ir_hash::chip::IR_HASH_CHIP_ID;

    /// Typed push helper used by witness ops files.
    pub fn push_call(scratch: &mut KitScratch, call: IrHashCall) {
        let entry = scratch
            .entry(Self::CHIP_ID)
            .or_insert_with(|| Box::<Vec<IrHashCall>>::default());
        entry
            .downcast_mut::<Vec<IrHashCall>>()
            .expect("IrHashKit scratch type mismatch")
            .push(call);
    }
}

impl ChipWitnessKit for IrHashKit {
    fn chip_id(&self) -> ChipId { Self::CHIP_ID }
    fn witness_store_label(&self) -> &'static str { IR_HASH_WITNESS_LABEL }
    fn finalize(&self, ctx: &mut KitFinalizeContext<'_>, store: &mut WitnessStore) -> Result<(), KitError> {
        let calls: Vec<IrHashCall> = ctx.take_scratch(Self::CHIP_ID)?;
        store.put(IR_HASH_WITNESS_LABEL, calls);
        Ok(())
    }
}
```

Wire:
- `crates/chips/src/ir_hash/mod.rs`: `pub mod kit; pub use kit::IrHashKit;`
- If `IR_HASH_CHIP_ID` isn't already a named `pub const`, extract it from the `ChipSpec` impl into a module constant so the kit and the chip share one source of truth. Do this regardless — it's a minor readability improvement.

### S2.4 — Wire `IrHashExecutionBackend::witness_kits`

In `crates/ext/src/backend/execution.rs`:

```rust
fn witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>> {
    vec![Box::new(IrHashKit)]
}
```

Add the matching `use tabula_chips::ir_hash::IrHashKit;` import.

### S2.5 — Introduce `ChipKitRegistry` in witness

New file `crates/witness/src/stark/kit_registry.rs`:

```rust
pub struct ChipKitRegistry {
    kits: Vec<Box<dyn ChipWitnessKit>>,
}
impl ChipKitRegistry {
    pub fn new() -> Self { Self { kits: Vec::new() } }
    pub fn register(&mut self, kit: Box<dyn ChipWitnessKit>) { self.kits.push(kit); }
    pub fn register_all(&mut self, kits: impl IntoIterator<Item = Box<dyn ChipWitnessKit>>) { self.kits.extend(kits); }
    pub fn iter(&self) -> impl Iterator<Item = &dyn ChipWitnessKit> { self.kits.iter().map(|k| k.as_ref()) }
}
```

Re-export from `crates/witness/src/stark/mod.rs`.

### S2.6 — Route runtime bootstrap to populate the registry

The runtime engine calls `prepare_execution_store(&lowered, &relation_proof)`.
Extend this to take a registry:

1. `engine.rs` — collect a `ChipKitRegistry` during proving setup by
   walking the same three backend choices used in
   `build_registered_program_machine` (`PublicStatementTranscriptExecutionBackend`,
   `IrHashExecutionBackend`, `RelationExecutionBackend`). Move the
   backend selection logic into a new
   `bootstrap/program.rs::execution_backends_for_shape(shape)` helper
   that returns `Vec<Arc<dyn ExecutionBackend>>`, and drive both machine
   build and kit registry from it. This fixes a latent duplication.
2. Call `registry.register_all(backend.witness_kits())` for each
   backend.
3. `prepare_execution_store(&lowered, &relation_proof, &registry)`.

### S2.7 — Thread scratchpad through lowering

1. `LoweringCx` — replace `pub(crate) ir_hash_calls: Vec<IrHashCall>`
   with `pub(crate) kit_scratch: KitScratch`. Initialize empty.
2. `TxLoweringOutput` — drop `ir_hash_calls: Vec<IrHashCall>`; add
   `kit_scratch: KitScratch`.
3. `LoweringOutput` — drop `ir_hash_calls: Vec<IrHashCall>`; add
   `kit_scratch: KitScratch`.
4. `merge_lowering_outputs` — merge per-tx scratchpads by moving each
   per-chip boxed buffer into the merged map, concatenating if the key
   already exists. The concatenation requires a per-kit helper since
   the merged map only holds `Box<dyn Any>`; introduce a
   `ChipWitnessKit::merge_scratch(left: Box<dyn Any + Send>, right:
   Box<dyn Any + Send>) -> Result<Box<dyn Any + Send>, KitError>`
   method on the trait, default impl that returns the right one and
   panics if left is non-empty. Override in `IrHashKit` to concat the
   `Vec<IrHashCall>` vectors. **Reconsider:** this adds trait surface
   for a pass-through concern. Alternative: merging happens per-kit
   inside a `KitScratchMergeContext` that each kit's typed `push_*`
   helper can cooperate with. Simpler alternative: **witness lowering
   pushes directly into a single `LoweringOutput`-level scratchpad via
   the typed helper, and per-tx scratchpads disappear entirely**.
   Adopt this — see revision below.

**Revised approach for S2.7.** Eliminate per-tx scratchpads. Witness
carries one `KitScratch` at the `LoweringOutput` level and every opcode
pushes into it directly.

Concretely:

1. `LoweringCx` is per-tx, but the shared scratchpad reference is
   threaded by the driver:
   ```rust
   pub(crate) struct LoweringCx<'a, const W: usize> {
       // ...
       pub(crate) kit_scratch: &'a mut KitScratch,
   }
   ```
2. `lower_successful_tx` now takes `&mut KitScratch` alongside its
   `LowerSuccessfulTxInput`. The driver in `driver.rs` owns the
   scratchpad across all tx calls, accumulating rows in emission
   order.
3. `TxLoweringOutput` no longer carries kit rows; only core rows.
4. `LoweringOutput` owns `kit_scratch: KitScratch`.
5. `merge_lowering_outputs` stays unchanged for core rows.

This preserves byte-identity trivially: `ir_hash_calls` was already
merged by concatenation across txs in emission order; pushing into a
shared scratchpad gives the same order without a merge step.

### S2.8 — Update `ops/hash.rs`

```rust
// drop: use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::ir_hash::IrHashKit;
// ...
IrHashKit::push_call(&mut self.kit_scratch, call);
```

### S2.9 — Update `prepare_execution_store`

```rust
pub fn prepare_execution_store(
    lowering: &mut LoweringOutput,  // now &mut: we drain kit_scratch
    relation_proof: &PreparedRelationProof,
    registry: &ChipKitRegistry,
) -> Result<WitnessStore, TabulaError> {
    let mut store = WitnessStore::new();
    // core rows (unchanged)
    store.put(witness_labels::EXECUTION_RECORDS, lowering.instruction_records.clone());
    store.put(witness_labels::STATIC_TABLE_ROWS, lowering.static_table_rows.clone());
    // transcript labels + relation labels still hardcoded for now — S3 migrates them.
    // ...
    // drive kits
    let mut ctx = KitFinalizeContext::new(&mut lowering.kit_scratch);
    for kit in registry.iter() {
        kit.finalize(&mut ctx, &mut store).map_err(|e| ...)?;
    }
    Ok(store)
}
```

Drop the `IR_HASH_WITNESS_LABEL` and `IrHashCall` imports and the
`store.put(IR_HASH_WITNESS_LABEL, ...)` call — the kit now owns that
put.

### S2.10 — Adjust tests

- `crates/witness/tests/stark_lowering.rs` exercises
  `prepare_execution_store` directly. Update the call site to pass a
  `ChipKitRegistry` populated with `IrHashKit`. Keep coverage.

### S2.11 — Verification for S2

1. `grep -rn 'IrHashCall' crates/witness/src/` → **zero matches.**
2. `grep -rn 'ir_hash_calls' crates/witness/src/` → **zero matches.**
3. `cargo fmt --all -- --check` / `cargo test --workspace --all-features` green.
4. **Byte-identity check:**
   - Regenerate `basic` / `membership` examples and proofs under a
     fresh `/tmp/sp3-s2-{basic,membership}`.
   - `diff /tmp/sp3-ref-basic/proof.bin /tmp/sp3-s2-basic/proof.bin` → empty.
   - Same for `membership`.
5. Guardrail off-mode test shows 2 fewer reported imports (driver.rs
   and ops/hash.rs `IrHashCall` entries gone).

### S2.12 — Commit

```
SP-3 S2: migrate IrHashChip to ChipWitnessKit

Pilot migration of the chip-authoring protocol. IrHashKit lives in
tabula-chips::ir_hash; IrHashExecutionBackend::witness_kits() publishes
it; witness lowering pushes rows through the kit's typed helper into a
shared KitScratch, and prepare_execution_store drives kit finalize
instead of hard-coding IR_HASH_WITNESS_LABEL.

- Eliminates ir_hash_calls typed field from TxLoweringOutput /
  LoweringOutput / LoweringCx.
- Byte-identical basic/membership proofs vs pre-S2 reference.
- Off-mode guardrail report shrinks by 2 entries.
```

---

## 3. S3 — Remaining Chip Migrations

S3 ports the remaining 19 forbidden imports by applying the S2 recipe
per chip. Each bullet below is its own commit. Order chosen so the
simplest / most independent chips land first and harder cross-chip
reads land last.

### S3.1 — `RelationTranscriptChip`

`RelationTranscriptCall` lives in
`ops/relation.rs`, `lowering/driver.rs`, `lowering/context.rs`, and
`execution_store.rs`. Append-only producer; mirror S2.

Exit: `grep 'RelationTranscriptCall' crates/witness/src/` empty.

### S3.2 — `RelationTableChip`

`RelationTableWitnessRow` in `execution_store.rs` — read from
`relation_proof.table_rows()`. Kit's `finalize` must read the prepared
relation proof. This is the first kit that needs the
`PreparedRelationProof` borrow; add it to `KitFinalizeContext`:

```rust
pub struct KitFinalizeContext<'a> {
    scratch: &'a mut KitScratch,
    relation_proof: Option<&'a PreparedRelationProof>,
}
```

`PreparedRelationProof` lives in `tabula-witness`. To avoid pulling
witness-crate types into stark (which would be a layering inversion),
do this instead: the kit reads from the scratchpad under a well-known
key that the runtime pre-populates with the table rows. That pushes
relation proof knowledge out of the kit and keeps stark dependency-free.

Concretely:
- `prepare_execution_store` (or its caller) stuffs
  `relation_proof.table_rows()` into `lowering.kit_scratch` under
  `RelationTableKit::CHIP_ID` before driving finalize.
- `RelationTableKit::finalize` downcasts that entry and emits the
  witness row.

No `KitFinalizeContext` schema change. Layering clean.

Exit: `grep 'RelationTableWitnessRow' crates/witness/src/` empty.

### S3.3 — Transcript family (capability, public-context, tx-batch, event)

**Kit production pattern differs from `IrHashKit`.** `IrHashKit` uses
*inline-push*: opcode handlers call `IrHashKit::push_call(&mut
scratch, call)` directly as they execute. Transcript kits and
`RelationTableKit` use *runtime-pre-stuff*: the runtime computes the
full row vector (e.g. `tx_batch_transcript_items`) and writes it into
`kit_scratch` under the kit's `CHIP_ID` before `prepare_execution_store`
drives finalize. The kit's `finalize` body is identical in both cases
— it just `take_scratch`es and `store.put`s. This asymmetry is
intentional: inline-push suits opcode-local rows (IR hash), pre-stuff
suits aggregate rows computed at batch level (transcripts,
relation-table).


Four chips, one kit each:
- `PublicContextTranscriptChip`
- `TxBatchTranscriptChip`
- `EventTranscriptChip`
- `CapabilityTranscriptChip` (if it emits rows via the current pipeline
  — confirm during S3 scoping)

Each kit's finalize stores the pre-merged `Vec<[KoalaBear; 8]>` under
its `*_WITNESS_LABEL`. These labels remain in the allowlist (§2.5).

`LoweringOutput` loses `public_context_transcript_items`,
`tx_batch_transcript_items`, `event_transcript_items` — they move to
the kit scratchpad, pushed by the respective executor hooks in
`engine.rs` directly into scratchpad entries.

### S3.4 — Column / root-tier chips

Densest code paths:
- `SmtPathWitness` / `SmtTablePathWitness` (`roots/paths.rs`)
- `SsmcWitness`, `SsmcColumnWitness`, `SharedColumnWitness`
  (`schemes/ssmc.rs`, `schemes/smt.rs`, `memory/mod.rs`)
- `StateShardRow`, `MemoryShardRow`, `MetaShardRow`
  (`memory/*.rs`)
- `PropertyReadRecord` (`schemes/ssmc.rs`)
- `EntrySource` (allowlisted — this is a helper, not a row)

These feed column/root tier stores, not the execution witness store.
SP-3 §6 says "does not touch column or root tiers structurally" but
extension chips at those tiers may adopt the kit pattern. **Decision:
migrate the chips that are in the execution-tier flow only.** Column
and root stores (`prepare_column_store`, `prepare_smt_root_store`) are
out of SP-3 scope; the imports there stay but the §2.5 guardrail only
enforces on the execution-tier files.

Re-scope the guardrail to allow these specific column/root files to
keep their current imports, with a comment explaining the deferred
work. List them explicitly in the test source.

Affected files that REMAIN chip-aware after S3 (with allowlist
rationale):
- `crates/witness/src/stark/roots/paths.rs` — root tier
- `crates/witness/src/stark/schemes/ssmc.rs` — column scheme
- `crates/witness/src/stark/schemes/smt.rs` — column scheme
- `crates/witness/src/stark/memory/*.rs` — column memory shards

**Add a design-doc footnote** reflecting this narrower S3 landing.

### S3.5 — Verification for S3

After every sub-commit:
1. Byte-identity against `/tmp/sp3-ref` reference.
2. Off-mode guardrail report shrinks as expected.

After S3 total:
- `grep -rn 'use tabula_chips::' crates/witness/src/stark/lowering/` →
  only allowlisted paths (Opcode/MAX_SLOTS/labels/kits).
- `grep -rn 'use tabula_chips::' crates/witness/src/stark/execution_store.rs` →
  only label constants.

---

## 4. S4 — Flip the Guardrail Assertive

1. Rename `sp3_witness_chip_import_guardrail_off_mode` →
   `sp3_witness_chip_import_guardrail`. Remove the off-mode `eprintln!`
   report path; replace with `assert!` on the forbidden list being
   empty, printing the offending files in the panic message.
2. Scope: **execution-tier files only**. Explicitly skip
   `crates/witness/src/stark/{roots,schemes,memory}/` per S3.4
   decision. Encode the skip-list inside the test as a documented
   constant.
3. Confirm the guardrail now fails if a regression re-introduces a
   chip row import under the execution-tier scope.
4. No other production changes — S4 is a test change.

### S4 Verification

- `cargo test --workspace --all-features` green.
- Temporarily introduce a bogus `use tabula_chips::ir_hash::IrHashCall;`
  in `crates/witness/src/stark/lowering/ops/hash.rs` locally to
  confirm the guardrail fails loudly. Revert before commit.

---

## 5. S5 — Documentation

1. `crates/ext/README.md` — new section "Chip witness kits": shape of
   `ChipWitnessKit`, `ChipId`/label alignment rule, the push-helper
   convention, what to do about column/root-tier rows (out of SP-3
   scope, still imported directly), pointer to the §2.5 guardrail.
2. `crates/witness/README.md` — describe `ChipKitRegistry`, the
   shared `KitScratch`, and the deferred column/root-tier migration.
3. `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md`
   — update the SP-3 row to "shipped" with a pointer to the S1–S4
   commits.
4. `docs/superpowers/specs/2026-04-19-sp3-witness-chip-kit-design.md`
   — append a final "Landed" amendment section summarizing what
   deviated from the spec (trait location, scratchpad model, S3 scope
   narrowing).

---

## 6. Merge / PR

After S5:

1. `cargo test --workspace --all-features` + `cargo fmt --all -- --check` green.
2. Run `basic` / `membership` reproduction one more time, `diff`
   against the reference.
3. Merge `refactor/witness-chip-kit` → `main`.
4. Delete the branch.

---

## 7. Byte-identity Reproduction Recipe

From the repo root:

```sh
# once, before S2
cargo build -p tabula-cli --features prove
rm -rf /tmp/sp3-ref /tmp/sp3-ref-basic /tmp/sp3-ref-membership
mkdir /tmp/sp3-ref
target/debug/tabula-cli example basic --dir /tmp/sp3-ref-basic
target/debug/tabula-cli example membership --dir /tmp/sp3-ref-membership
for ex in basic membership; do
  target/debug/tabula-cli execute \
    --program /tmp/sp3-ref-$ex/program.tab \
    --state /tmp/sp3-ref-$ex/state.json \
    --batch /tmp/sp3-ref-$ex/batch.json \
    --context /tmp/sp3-ref-$ex/context.json \
    --receipt-out /tmp/sp3-ref-$ex/receipt.bin
  target/debug/tabula-cli prove \
    --program /tmp/sp3-ref-$ex/program.tab \
    --receipt /tmp/sp3-ref-$ex/receipt.bin \
    --proof-out /tmp/sp3-ref-$ex/proof.bin \
    --public-statement-out /tmp/sp3-ref-$ex/public_statement.json \
    --summary-out /tmp/sp3-ref-$ex/summary.json
  cp /tmp/sp3-ref-$ex/proof.bin /tmp/sp3-ref/$ex-proof.bin
done

# at the end of each stage
STAGE=s2  # s2, s3a, s3b, ...
for ex in basic membership; do
  OUT=/tmp/sp3-$STAGE-$ex
  rm -rf "$OUT"
  target/debug/tabula-cli example $ex --dir "$OUT"
  target/debug/tabula-cli execute --program "$OUT/program.tab" --state "$OUT/state.json" --batch "$OUT/batch.json" --context "$OUT/context.json" --receipt-out "$OUT/receipt.bin"
  target/debug/tabula-cli prove --program "$OUT/program.tab" --receipt "$OUT/receipt.bin" --proof-out "$OUT/proof.bin" --public-statement-out "$OUT/public_statement.json" --summary-out "$OUT/summary.json"
  diff "$OUT/proof.bin" /tmp/sp3-ref/$ex-proof.bin || { echo "BYTE DRIFT in $STAGE/$ex"; exit 1; }
done
```

---

## 8. Open Risks

- **Deterministic Box<dyn Any> merge across txs.** Resolved in S2.7
  revision by eliminating per-tx scratchpads entirely; one shared
  scratchpad avoids any merge-order ambiguity.
- **Kit-registry / backend ordering.** Kits must drive finalize in
  the same order the backends were registered today, otherwise
  store-order changes may perturb downstream trace generation even if
  each label's contents are unchanged. `ChipKitRegistry` preserves
  insertion order (`Vec` not `BTreeMap`) and we walk backends in the
  same order as the existing machine builder.
- **RelationTableKit scratchpad pre-population.** S3.2 hack: the
  runtime stuffs `relation_proof.table_rows()` into the scratchpad
  before finalize. This puts runtime-layer logic into the kit bridge,
  which isn't ideal. If the approach feels brittle during S3.2, revisit
  by adding a narrow read-only relation-proof borrow to
  `KitFinalizeContext` — accepting the tabula-stark → (relation-proof
  type) dependency. Record the decision as an amendment at that time.
- **Column/root tier narrowing.** S3.4 defers column-tier and root-tier
  chip migrations. Record this explicitly in the S5 docs so it isn't
  an invisible gap.

---

## 9. Non-Goals

- Column and root proof stores (`prepare_column_store`,
  `prepare_smt_root_store`) stay chip-aware. Future SP extends the
  kit pattern there.
- `ExecutionBackend` is not otherwise redesigned. Adds one method,
  nothing else.
- No new wire types (`ArtifactId`, etc.) — still parked.
