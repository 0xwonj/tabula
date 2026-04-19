# SP-3 — Witness-Chips Protocol Abstraction

> Status: design, ready for ultraplan
> Date: 2026-04-19
> Umbrella: [2026-04-18-architecture-refactoring-design.md](2026-04-18-architecture-refactoring-design.md)
> Predecessor: [2026-04-18-sp2-machine-backend-primitive-split-design.md](2026-04-18-sp2-machine-backend-primitive-split-design.md)

Sub-project 3 of the architecture refactoring. Makes `tabula-witness`
chip-agnostic by formalizing the extension-authoring seam: each concrete
chip owns its witness-lowering contribution through a `ChipWitnessKit`
trait hosted in `tabula-ext`, and the `tabula-machine` builder drives a
kit registry so that adding a new chip stops requiring witness-crate
edits.

---

## 1. Goal

After this sub-project:

- `tabula-ext` defines `ChipWitnessKit` (exact name; see §2.2), the
  single trait a chip implements to contribute rows to the execution
  tier's witness store.
- Each concrete chip in `tabula-chips` ships a `ChipWitnessKit`
  implementation alongside its row type, AIR, and metadata.
- `tabula-witness::prepare_execution_store` becomes a chip-agnostic
  dispatcher: it iterates the registered kits and asks each one to
  finalize its contribution into the `WitnessStore`. The hardcoded list
  of eight chip labels in that function is gone.
- `tabula-witness` no longer imports any concrete chip *row type* from
  `tabula_chips`. Label-level identifiers (`Opcode`, `MAX_SLOTS`,
  `EXECUTION_STANDARD_VALUE_WIDTH`, the `*_WITNESS_LABEL` constants,
  and the shared crypto helpers) may remain — they are
  instruction-set / protocol identifiers, not chip-internal rows, and
  witness consumes them as part of its language-level role.
- `tabula-machine`'s builder exposes a symmetrical registration entry
  point for witness kits so an `ExecutionBackend` author registers
  `(airs, dyn_chips, bus_consumers, witness_kit)` in one place.
- Adding a hypothetical new chip touches `tabula-chips` (row + AIR +
  kit) and one line of builder registration in `tabula-machine` (or
  an `ExecutionBackend` impl in `tabula-ext`). **No edits under
  `crates/witness/`.** Chips whose row data originates in the runtime
  (runtime-pre-stuff pattern — see the SP-3 "Landed" amendment) do
  require a single `insert_*` call from `tabula-runtime`; that is an
  expected property of the pattern, not a violation of the goal.
  "Chip-agnostic" is a claim about `tabula-witness`, not about the
  workspace as a whole.

SP-3 is **not** byte-breaking. The on-disk `proof.bin` layout, the
witness-store label set, and the per-chip row encodings are all
preserved. Determinism on the `basic` and `membership` end-to-end
flows is required at *byte* level, not just structural level, and is
the primary landing criterion.

---

## 2. Resolved Open Decisions

### 2.1 Where does `ChipWitnessKit` live?

**Problem.** The umbrella (§SP-3 scope) leaves the home open: `tabula-ext`
vs. a new dedicated crate. The choice sets the shape of the chip ↔
witness dependency edge for the lifetime of the extension seam.

**Options considered:**

1. **`tabula-chips`.** Each chip impls its kit next to its row types.
   Creates a dependency cycle: `tabula-witness → tabula-chips → (kit
   machinery that references witness internals like `LoweringOutput`)
   → tabula-witness`. Non-viable without splitting the kit machinery
   into a third crate anyway.
2. **New `tabula-chip-kit` trait crate** below `witness`/`machine` and
   above `chips`. Cleanly breaks the cycle; explicit "this is the
   chip-authoring protocol layer" identity. Costs: new crate, new
   Cargo entry, new lint config, new workspace member, and a second
   home for contracts that `tabula-ext` already partly owns.
3. **`tabula-ext`.** `tabula-ext` is *already* the chip-authoring
   surface — it owns `ExecutionBackend`, `ColumnProofBackend`,
   `RootBackend`. Adding a sibling `ChipWitnessKit` trait keeps the
   full authoring protocol in one place. Dependency direction
   `chips → ext` is one-way and already compatible with how chips
   reach ext for AIR types today.

**Decision: option 3.** `ChipWitnessKit` lives in `tabula-ext`,
alongside `ExecutionBackend` and the other authoring traits. No new
crate. The trait is declared in a new module
`crates/ext/src/witness_kit.rs` re-exported from `crates/ext/src/lib.rs`,
so the public import path is `tabula_ext::ChipWitnessKit`.

**Amendment (2026-04-19, S1).** The as-shipped trait lives in
`tabula-stark` (`crates/stark/src/witness_kit.rs`), not `tabula-ext`.
Reason: `tabula-ext` is a public package surface above
`tabula-machine` / `tabula-witness` in the dependency graph (see
`docs/design/architecture.md`), so neither machine nor witness can
reference a trait declared there. `tabula-stark` already owns the
sibling `ChipId` / `ChipSpec` / `WitnessStore` identifiers, making it
the natural home for the chip-authoring protocol seam too. The
public import path for extension authors remains
`tabula_ext::ChipWitnessKit` via a re-export, so option 3's authoring
ergonomics are preserved.

Rationale:

- `tabula-ext`'s stated purpose (per its crate README and lib.rs
  header) is "the chip-authoring surface". Witness-lowering is a
  natural sibling to AIR/DynChip authoring; separating them would
  fragment the protocol story.
- Dependency graph stays acyclic: `chips → ext`, `witness → ext`,
  `machine → ext`, `ext → stark/types`. Kit definitions never force
  witness to import chip internals.
- Option 2 (new crate) earns no expressive power option 3 lacks;
  it only adds workspace plumbing.

### 2.2 Trait shape — when do kits run?

**Problem.** A kit needs to (a) identify its chip, (b) declare which
witness-store label it owns, (c) accumulate its rows during opcode
lowering (the only point where the typed row data exists), and
(d) hand its accumulated rows to the execution store at finalize
time. The open question is whether there is a separate per-tx
callback on the kit, or whether rows are pushed *into* the kit
inline from the opcode handlers.

**Options considered:**

1. Kit gets a per-tx `lower_tx(ctx)` callback that reads from an
   already-populated `TxLoweringOutput`. Requires a neutral row
   representation that witness produces first and kits consume — at
   which point witness still knows what each chip's row looks like,
   and the decoupling fails.
2. Kit is written to inline by opcode handlers during lowering; no
   per-tx callback. Kit surfaces only `chip_id`,
   `witness_store_label`, and `finalize`. Row types stay fully
   private to the chip crate; witness imports kit types, never row
   types.

**Decision: option 2.** The trait collapses to:

```rust
pub trait ChipWitnessKit: Send + Sync {
    /// Stable identifier this kit populates rows for. Must match the
    /// `ChipId` of the AIR this backend registers.
    fn chip_id(&self) -> ChipId;

    /// Canonical witness-store label under which this kit's rows live
    /// in the execution-tier `WitnessStore`. Matches the string the
    /// chip's AIR reads from.
    fn witness_store_label(&self) -> &'static str;

    /// Finalize: drain the kit's accumulated rows from the scratchpad
    /// and write them into the execution `WitnessStore` under
    /// `witness_store_label()`.
    fn finalize(
        &self,
        ctx: &mut KitFinalizeContext<'_>,
        store: &mut WitnessStore,
    ) -> Result<(), KitError>;
}
```

Opcode handlers talk to the kit through *typed helpers the kit
exposes on its own interface* (e.g. `IrHashKit::push_call(ctx,
call)`), not through a generic `ctx.push_kit_row::<T>()`. The row
type stays private to the chip crate; witness imports the kit type
only. This is the shape resolved at design time; see §2.3 for how
`ctx` surfaces the scratchpad.

`KitFinalizeContext` is an opaque borrow from `tabula-witness` that
gives the kit access to:

- the merged `LoweringOutput::core` (read-only — most kits don't
  need it; state-touching chips may read `instruction_records` for
  cross-chip joins),
- the prepared relation proof (for chips that consume relation
  table rows),
- the kit's own `Any`-typed scratchpad entry, keyed by its
  `ChipId`, for downcasting back to its private row Vec.

The scratchpad is how kits avoid leaking their row type into
witness: witness holds `BTreeMap<ChipId, Box<dyn Any + Send>>`,
each kit downcasts its entry to its own private row Vec. No
concrete chip row type appears in any `crates/witness/src/**`
signature.

### 2.3 How does `LoweringOutput` shed concrete row types?

**Problem.** `LoweringOutput` (in `crates/witness/src/stark/lowering/driver.rs`)
currently holds named typed fields: `instruction_records: Vec<InstructionRecord>`,
`ir_hash_calls: Vec<IrHashCall>`, `relation_transcript_calls:
Vec<RelationTranscriptCall>`, and so on. Each of those types is a
concrete import from `tabula_chips::<module>::<row>`. As long as those
fields exist, witness must import the row types to own them.

**Options considered:**

1. Erase `LoweringOutput` into `BTreeMap<ChipId, Box<dyn Any + Send>>`
   outright. Kits downcast to their own row type on both emit and
   finalize. Witness holds no typed chip rows. Cost: the per-opcode
   lowering files under `crates/witness/src/stark/lowering/ops/*`
   currently push directly into the typed fields — they would all
   need to go through a downcasted accessor.
2. Keep typed fields for rows that are *intrinsic* to the execution
   model (`InstructionRecord`, `StaticTableRow`), move *extension-style*
   rows (`IrHashCall`, `RelationTranscriptCall`, each transcript chip's
   row) into per-kit scratchpads. This matches the semantic split
   between "the execution IR's canonical trace" and "extensions that
   hang off the execution bus".
3. Split `LoweringOutput` into a `CoreLoweringOutput` (intrinsic rows,
   typed) + `Map<ChipId, Box<dyn Any>>` (extension kits). Each kit
   sees the core output read-only in its callback and appends to its
   own scratchpad.

**Decision: option 3.** The split between "execution IR core" and
"extension chips" already lives in the codebase informally — the
former has AIRs that witness depends on for the proof's semantic
skeleton; the latter are pluggable. Formalize it:

- `CoreLoweringOutput { instruction_records, static_table_rows }` —
  typed, stays in witness, never moves.
- `KitScratch = BTreeMap<ChipId, Box<dyn Any + Send>>` — per-kit
  opaque buffers. Each kit owns and downcasts its own entry.
- `LoweringOutput = { core: CoreLoweringOutput, kits: KitScratch }` —
  what `merge_lowering_outputs` produces.

Concrete row types witness currently imports that will move under the
kit scratchpad: `IrHashCall`, `RelationTranscriptCall`,
`RelationTableWitnessRow`, `PropertyReadRecord`, `StateShardRow`,
`MemoryShardRow`, `MetaShardRow`, `SsmcWitness`, `SsmcColumnWitness`,
`SharedColumnWitness`, `SmtPathWitness`, `SmtTablePathWitness`, the
three transcript-call row types for public-context / tx-batch / event.

Concrete types that stay in core: `InstructionRecord`, `StaticTableRow`.

Rationale:

- Forces a semantic split we want anyway: execution IR rows are
  load-bearing for the trace model itself; extension rows are hot-swap.
- Contains the blast radius of the refactor. `crates/witness/src/stark/lowering/ops/*`
  that writes to `instruction_records` stays unchanged. Only op files
  that currently write extension-chip rows (e.g. hash ops, relation
  ops) route through the kit scratchpad API.
- Chip authors get a single opaque box to use however they want.
  Tabula's built-in chips keep their existing row types, moved into
  their own crate module — witness never names them.

### 2.4 How does `MachineBuilder` register kits?

**Problem.** Kits must line up with AIRs: every AIR registered at
machine-build time needs its kit registered with witness, otherwise
the trace will be incomplete. A separate registration surface is a
footgun.

**Decision.** Extend the existing `ExecutionBackend` trait (in
`tabula-ext`) with a `witness_kits()` method:

```rust
pub trait ExecutionBackend {
    // ... existing: name, airs, dyn_chips, bus_consumers ...

    /// Witness kits whose rows feed the execution-tier trace.
    /// Must be aligned with `airs()`: each AIR that reads from the
    /// execution `WitnessStore` at a chip-specific label should have
    /// a matching kit here.
    fn witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>> {
        Vec::new()
    }
}
```

Default-empty so the change is source-compatible with existing
backends that don't yet emit extension rows (purely AIR-only chips
like `PoseidonChip` / `RangeCheckChip`, which derive their witness
from the trace itself and have no separate row buffer).

`MachineBuilder::with_backend_execution_extension` collects kits into
a registry alongside AIRs. Core chips (`ExecutionChip`,
`StaticTableChip`) bypass kits — their rows live in `CoreLoweringOutput`.

A machine-build-time invariant enforces that every `ChipId` with a
witness-store label present in `WitnessStore` is either
(a) served by a registered kit, or (b) a core chip. Missing-kit
mismatches become a `SetupError` caught before trace generation.

### 2.5 Guardrail test

**Problem.** The "witness imports no concrete chip row types" invariant
is easy to regress through a single `use` line.

**Decision.** Extend `crates/runtime/tests/architecture_dependencies.rs`
with a test that scans `crates/witness/src/**/*.rs` for
`use tabula_chips::...` lines and asserts each imported path segment is
on an allowlist:

- Protocol-level identifiers: `Opcode`, `CmpOp`, `MAX_SLOTS`,
  `EXECUTION_STANDARD_VALUE_WIDTH`.
- Witness-store labels: any `*_WITNESS_LABEL` constant.
- Crypto helpers: `native_key_payload_prefix3`, `poseidon2_permutation`.
- Core row types: `InstructionRecord`, `StaticTableRow`.
- Shared helpers: `EntrySource`.

Any new path (e.g. `tabula_chips::ir_hash::IrHashCall`) fails the test
with an explicit message pointing the author at `ChipWitnessKit`.

Kit types (e.g. `tabula_chips::ir_hash::IrHashKit`) are explicitly
permitted: witness ops files call the kit's typed helpers (§3.4) and
must import the kit type to do so. Chip *row* types remain forbidden.
The allowlist rule is: path tail is a `*Kit` type, or is on the
explicit protocol/label/helper list above.

---

## 3. Shape Of The Change

### 3.1 New types and traits

- `crates/ext/src/witness_kit.rs`:
  - `ChipWitnessKit` trait (as in §2.2).
  - `KitLoweringContext<'a>` / `KitFinalizeContext<'a>` opaque
    borrow types.
  - `KitError` error enum.
  - `KitScratch` type alias (`BTreeMap<ChipId, Box<dyn Any + Send>>`).
- `crates/ext/src/lib.rs`:
  - `pub mod witness_kit;`
  - `pub use witness_kit::{ChipWitnessKit, KitLoweringContext,
    KitFinalizeContext, KitError};`
- `crates/ext/src/backend/execution.rs`:
  - Add `witness_kits(&self) -> Vec<Box<dyn ChipWitnessKit>>` with a
    default-empty impl.
- Kit registry (location TBD in S2): kits collected from each
  `ExecutionBackend::witness_kits()` into a registry owned by the
  witness-lowering driver, not by `ChipRegistry` (which holds AIRs
  only). S1 leaves this unwired; S2 introduces it once the pilot
  kit lands.
- `crates/witness/src/stark/lowering/driver.rs`:
  - `LoweringOutput` splits into `{ core: CoreLoweringOutput, kits:
    KitScratch }`.
  - `merge_lowering_outputs` updated accordingly.
- `crates/witness/src/stark/execution_store.rs`:
  - `prepare_execution_store` becomes chip-agnostic: walks the kit
    registry, calls `kit.finalize(...)` for each.

### 3.2 Removed or relocated types

- Chip-specific row types that witness currently re-uses move into
  their owning chip modules under `tabula-chips` (most are already
  there — the move is primarily about dropping the `use` lines in
  witness and giving each kit responsibility for the row).
- The hardcoded 8-label list inside `prepare_execution_store` is
  deleted. Labels come from each kit's `witness_store_label()`.

### 3.3 ExecutionBackend migration

Each built-in `ExecutionBackend` in `crates/ext/src/backend/execution.rs`
grows a `witness_kits()` impl:

- `IrHashExecutionBackend` — returns `vec![Box::new(IrHashKit)]`.
- `RelationExecutionBackend` — returns kits for both
  `RelationTranscriptChip` and `RelationTableChip`.
- `CapabilityTranscriptExecutionBackend` — returns kits for the
  three transcript chips (public-context, tx-batch, event).
- `PublicStatementTranscriptExecutionBackend` — returns its
  transcript kit.

Each `*Kit` type lives next to its chip in `tabula-chips`, e.g.
`tabula_chips::ir_hash::IrHashKit`. The chip module is the only
place that names the chip's row type.

### 3.4 Witness-side ops refactor

The per-opcode lowering files under
`crates/witness/src/stark/lowering/ops/*` currently call into
chip-specific helpers and push onto typed fields of
`TxLoweringOutput`. They are rewritten to:

1. Keep writing to `CoreLoweringOutput` for instruction/static rows.
2. Route extension rows through **kit-typed helpers** exposed on each
   kit's own interface — e.g. `IrHashKit::push_call(ctx, call)`,
   `RelationTranscriptKit::push_transcript(ctx, row)`. Witness ops
   files import the *kit type*; the row type stays fully private to
   the chip crate and never appears in any `crates/witness/src/**`
   signature or `use` line.

   The rejected alternative — a generic `ctx.push_kit_row::<T>(chip_id,
   row)` helper where `T` is the chip's row type — would force witness
   ops files to import the row type as a generic parameter, leaking
   it back into witness and defeating the §2.5 guardrail. Kit-typed
   helpers are the form.

### 3.5 Contract boundaries that don't change

- `proof.bin` wire layout — unchanged.
- `PublicStatement`, `BoundStatement`, `ProofEnvelope` — unchanged.
- Execution-tier `WitnessStore` label set — unchanged.
- Per-chip row encoding and AIR shape — unchanged.
- Fiat-Shamir transcript absorption — unchanged.
- `tabula-runtime` public API — unchanged.

---

## 4. Stages

SP-3 lands in five stages, each mergeable independently with green
tests and byte-identical proof determinism.

### S1 — Trait, registry, guardrail infrastructure

- Add `ChipWitnessKit` trait + context types to `tabula-ext`.
- Extend `ExecutionBackend` with default-empty `witness_kits()`.
- Extend `ChipRegistry` with kit storage and `register_kit` helper
  (not yet populated from backends).
- Add the guardrail test (§2.5) in *off* mode: it runs, prints the
  current list of forbidden imports, but does not fail. This is a
  visibility step so subsequent stages can see progress.
- No behavior change; no existing test breaks.

### S2 — Pilot kit migration: `IrHashChip`

Chosen because it is a self-contained extension chip with a single
row type (`IrHashCall`) and a narrow set of witness-crate touch
points.

- Define `IrHashKit` in `tabula-chips::ir_hash::kit`, impl
  `ChipWitnessKit`.
- Wire `IrHashExecutionBackend::witness_kits()` to return the kit.
- Route the witness hash-ops file through the kit's `push_call` API
  instead of the typed `ir_hash_calls` field.
- Drop `ir_hash_calls: Vec<IrHashCall>` from `TxLoweringOutput` /
  `LoweringOutput`; move to kit scratchpad.
- `prepare_execution_store` consults the kit for
  `IR_HASH_WITNESS_LABEL` instead of the typed field.
- All other chips stay on the old typed-field path.
- Determinism test on `basic` / `membership`: byte-identical proofs.

Exit criterion: `grep 'IrHashCall' crates/witness/src/` returns
zero lines.

### S3 — Remaining extension chip migrations

One kit per chip, each in its own small commit, following the
S2 recipe:

- `RelationTranscriptChip` + `RelationTableChip`
  (`RelationExecutionBackend::witness_kits`).
- `PublicContextTranscriptChip`, `TxBatchTranscriptChip`,
  `EventTranscriptChip` (`CapabilityTranscriptExecutionBackend`).
- Column / root-tier chips that currently cross the boundary
  (`SmtPathWitness`, `SsmcWitness`, shard chips) — scoped last
  because their code paths are the densest.

### S4 — Flip the guardrail

- Turn the guardrail test (§2.5) assertive.
- Any remaining concrete-row imports in `crates/witness/src/**` that
  are not on the allowlist fail CI.

### S5 — Documentation

- Update `crates/witness/README.md` and `crates/ext/README.md` to
  describe the kit contract: how to author a new chip kit, the
  `ChipId`/label alignment rule, and the §2.5 guardrail.
- Update the umbrella design doc's SP-3 entry to point at this
  spec as the shipped shape.

---

## 5. Verification

Acceptance is gated on:

1. `cargo test --workspace --all-features` passes.
2. `cargo fmt --all -- --check` passes.
3. `grep -r 'use tabula_chips::' crates/witness/` contains only
   allowlisted paths (§2.5).
4. `basic` and `membership` end-to-end flows produce byte-identical
   `proof.bin` across two clean runs, and byte-identical to a
   reference captured before S1. Every stage merge point preserves
   this byte identity against the pre-S1 reference — the inline-push
   model keeps row emission order, so no stage is permitted to shift
   intermediate proof bytes.
5. The architecture guardrail test in
   `crates/runtime/tests/architecture_dependencies.rs` passes with
   the new kit-import allowlist.
6. A dry-run "hypothetical new chip" check: add a trivial
   `NoopChip` + `NoopKit` to a test fixture. Confirm it registers
   through `ExecutionBackend::witness_kits` and participates in
   trace generation without any edits under `crates/witness/src/`.
   This test lives in `crates/machine/tests/` and is gated behind a
   `test-fixtures` feature so production builds don't carry it.

---

## 6. What This Design Does Not Do

- **Does not redesign `tabula-ext`.** The crate is mildly extended
  (one new module, one new trait method). Any larger reshape of the
  ext authoring surface is deferred — SP-3 wants to add one seam, not
  reopen the whole authoring story.
- **Does not touch column or root tiers structurally.** Extension
  chips at those tiers can adopt the same kit pattern as part of S3,
  but the tier wrappers (`ColumnProofBackend`, `RootBackend`)
  themselves are not re-cut here.
- **Does not introduce an `ArtifactId`** or any new wire type. SP-2
  explicitly parked that; SP-3 inherits the same parking decision.
- **Does not promote the per-opcode witness lowering into chip
  crates.** The per-opcode driver keeps living in witness. Only the
  chip-specific *row* concerns move; the instruction-set lowering is
  witness's own thing.
- **Does not change the executor journal shape** (`TxCall`,
  `RelationEffect`, `StateEffectKind`, etc., all in `tabula-types`
  post-SP-2). Those types are still produced by executor and consumed
  by witness lowering unchanged.

---

## Landed (2026-04-19)

SP-3 landed on `refactor/witness-chip-kit` in five commits: `dc2690e`
(S1 trait infra), `7ddf374` (S2 IrHashKit pilot), `8066d3f` / `6360ade`
/ `218f6af` (S3.1–S3.3 remaining execution-tier chips), `6c216e8`
(S4 guardrail assertive). Deviations from the original spec shape:

- **Trait home.** `ChipWitnessKit` lives in
  `tabula-stark::witness_kit` rather than a new `tabula-witness-core`
  or `tabula-ext` crate. Stark already owns `WitnessStore` and the
  chip-independent proving contracts, so the kit protocol sits
  naturally next to them without adding a crate.
- **Scratchpad model.** Kits share one
  `KitScratch = BTreeMap<ChipId, Box<dyn Any + Send>>` carried on
  `LoweringOutput`. Each kit owns its entry; `finalize` drains it
  into the witness store under the kit's canonical label. Two
  authoring patterns emerged:
  - *inline-push* (`IrHashKit`, `RelationTranscriptKit`) — opcode
    handlers call kit helpers during lowering.
  - *runtime-pre-stuff* (`RelationTableKit` + context/tx-batch/event
    transcripts) — the runtime installs a full row buffer into the
    scratchpad before `prepare_execution_store` runs.
- **S3 scope narrowing.** Only execution-tier chips were migrated.
  Column- and root-tier chips
  (`crates/witness/src/stark/{memory,roots,schemes}`) remain on
  direct row imports; the `sp3_witness_chip_import_guardrail` test
  skips those subtrees explicitly. Migrating them is deferred per
  §9.
- **`prepare_execution_store` signature.** Dropped the
  `relation_proof` parameter; runtime now pre-stuffs the relation
  table scratchpad. The function publishes `EXECUTION_RECORDS` +
  `STATIC_TABLE_ROWS` and drives the registry — nothing else.
- **Guardrail scope.** Runtime guardrail
  `runtime_relation_proof_prep_stays_witness_owned` was relaxed to
  allow `RelationTableWitnessRow` in `engine.rs`. Chips cannot
  depend on witness's `PreparedRelationProof`, so the row
  projection's natural site is the runtime boundary.

Byte-identical proofs on `basic` and `membership` end-to-end flows
held at every stage (S2, S3.1, S3.2, S3.3).

**SP-4 boundary left by SP-3.** SP-3 splits what can be prepared
once from what must be re-assembled per batch:

- *Prepared-once, SP-4-hoistable.* Backend selection,
  `ChipKitRegistry` construction, and AIR wiring. The registry
  builds from each configured `ExecutionBackend`'s `witness_kits()`
  and does not depend on batch inputs, so it can move onto a
  `PreparedProver` handle without witness-crate changes.
- *Still eager per-batch.* `KitScratch` allocation, runtime
  pre-stuff of relation-table and transcript rows, and
  `prepare_execution_store` itself. These depend on the batch's
  execution trace and relation-proof output and cannot be
  preallocated. SP-4's `PreparedProver` must therefore surface the
  prepared registry/backends but still thread a fresh `KitScratch`
  per prove call.

**Follow-ups landed (2026-04-19).** Four cleanup commits on top of
S4: registry dup-check (chip-id + label), multi-line-aware
`sp3_witness_chip_import_guardrail`, removal of the permanently
empty `LoweringOutput::static_table_rows` field (execution store
now publishes an empty `STATIC_TABLE_ROWS` buffer directly until a
static-table kit takes ownership), and removal of the unused
`KitError::Internal` variant.
