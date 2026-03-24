# Witness Partition And Batch Proof Plan Architecture

> **Status**: Design note, with Stages 2-3 implemented
> **Date**: 2026-03-24
> **Scope**: Defines the intended ownership, naming, and layering for witness
> partitioning, batch-local proof planning, and machine handoff.
> **Related**: [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md),
> [execution-proof-redesign-workplan.md](execution-proof-redesign-workplan.md),
> [runtime-machine-proof-backend-roadmap.md](runtime-machine-proof-backend-roadmap.md),
> [stage6-proof-topology-generalization-deferred.md](stage6-proof-topology-generalization-deferred.md),
> [../design/architecture.md](../design/architecture.md)

---

## 1. Why This Note Exists

This note was written to pin down one structural problem that remained after
the initial machine boundary cleanup:

- runtime prepares one shared machine-facing witness store,
- machine still partitions that store internally,
- and the partition rule is currently tied to SMT-specific root labels.

As of Stage 1 hardening, that temporary shape was at least explicit and
fail-closed. Stage 2 then removed the shared-store partition problem itself:

- runtime owns an explicit batch-local `BatchProofPlan`,
- runtime materializes tier-partitioned `PreparedMachineInput`,
- machine no longer repartitions a shared store or interprets root-routing
  conventions.

Stage 3 has now completed the remaining root-authority cleanup:

- runtime proving is configured through a real `RootBackendBundle`,
- `BatchProofPlan.root` carries the selected root backend family,
- runtime no longer hardcodes SMT root preparation in artifact assembly,
- `RootWitnessContract` no longer exists in the active architecture.

That means the central witness-partitioning problem and the root-authority
cleanup are both solved in the intended direction.

The real issue is not "how to move three labels around." The issue is that the
codebase still needs one missing planning layer between:

- the runtime-owned semantic proof input, and
- the machine-owned prepared backend payload.

This note records the intended answer so the architecture does not drift back
toward ad hoc partitioning logic.

---

## 2. Design Claim

The intended architecture is:

1. `ProofPlan` remains the runtime-owned static slot-order contract.
2. `ProofJournal` remains the runtime-owned batch-local backend-neutral proof input.
3. runtime derives a batch-local backend-aware `BatchProofPlan`.
4. runtime materializes a fully partitioned `PreparedMachineInput`.
5. machine consumes that prepared input without reinterpreting or repartitioning it.

The key claim is:

> **Witness partitioning is runtime-owned batch-local proof planning, not
> machine-owned trace assembly policy.**

This means the correct home for the missing logic is not:

- inside `tabula-machine` public API,
- inside `ProofPlan`,
- inside `ProofJournal`,
- or inside `tabula-witness` as a whole-batch orchestrator.

It belongs in the runtime proving pipeline as one explicit planning stage.

---

## 3. The Planning Hierarchy

The codebase already has multiple objects named "plan." That is acceptable, but
only if their authority is clearly separated.

### 3.1 `ProofPlan`

`ProofPlan` is the runtime-owned static proof-slot contract.

It answers questions like:

- which column proof slots exist,
- in what deterministic order,
- which precompile proof slots exist,
- which installed backend owns each slot.

It is resolved from:

- sealed program semantics,
- installed runtime capabilities,
- materialized per-slot backend contracts.

It is **not** batch-local and it is **not** a witness partition object.

### 3.2 `ProofJournal`

`ProofJournal` is the runtime-owned batch-local backend-neutral proof input.

It answers questions like:

- what lowering output was produced,
- what reduced per-slot column inputs exist,
- what reduced precompile calls exist,
- what proof-relevant execution facts survived reduction.

It is the final semantic reduction boundary before backend-specific preparation.

It is **not** the right place to encode tier routing, root witness partition
rules, or machine topology details.

### 3.3 `BatchProofPlan`

`BatchProofPlan` is the missing runtime-owned batch-local backend-aware plan.

It should answer questions like:

- which prepared inputs belong to the execution tier,
- which prepared inputs belong to the root tier,
- how column-local prepared inputs map to machine proof units,
- what grouping or amortization policy applies for this batch,
- what backend-specific packaging shape will be emitted.

This is the correct place for witness partitioning decisions.

### 3.4 `PreparedMachineInput`

`PreparedMachineInput` is payload, not planning state.

It should contain only the prepared machine-facing input needed for proving:

- fully prepared tier inputs,
- ordered column inputs,
- the public statement,
- the bound statement digest.

It should not contain planning rules that machine still needs to interpret.

Stage 2 reshaped `PreparedMachineInput` into the intended tier-partitioned
payload. Stage 3 then made the remaining root-tier preparation step explicit
through a runtime-owned `RootBackendBundle`. Stage 5 then consolidated the
surrounding preparation path so runtime proves through one private
`PreparedProofRequest` and treats `PreparedMachineInput` as the true handoff
center rather than one more intermediate carrier.

---

## 4. Why A Separate Batch Plan Is Correct

The question is not whether a new object is aesthetically pleasant. The
question is whether the lifecycle and authority are distinct enough to justify
one.

They are.

### 4.1 Why it should not be merged into `ProofPlan`

`ProofPlan` is static and program-resolved.

It should remain stable across batches for the same:

- sealed program,
- installed backend bundles,
- runtime configuration.

Witness partitioning is not like that. It depends on batch-local proving shape
and eventually may depend on:

- touched-column routing,
- grouping decisions,
- root backend selection details,
- future FRI profile or amortization policy.

That makes it the wrong layer for `ProofPlan`.

### 4.2 Why it should not be merged into `ProofJournal`

`ProofJournal` is meant to be backend-neutral and semantic.

If it absorbs machine-tier routing or backend grouping structure, it stops being
the canonical reduction boundary and starts becoming an accidental machine
artifact carrier.

That would collapse two responsibilities that should remain separate:

- reducing semantic execution facts,
- planning backend proof packaging.

### 4.3 Why it should not be owned by `tabula-machine`

The architecture already says:

- runtime owns policy and prepared input assembly,
- machine owns prepared-input consumption and proof mechanics.

If machine keeps repartitioning stores internally, then runtime is not really
owning prepared-input assembly.

The result is a blurred boundary where machine quietly reintroduces runtime
policy.

### 4.4 Why it should not be owned by `tabula-witness`

`tabula-witness` should own materialization kernels, not whole-batch proving
policy.

Witness can expose builders and narrow kernels such as:

- build execution-tier witness input,
- build one root-tier witness input,
- build one column-tier witness input.

But runtime should own:

- when those kernels are called,
- how their results are grouped,
- how they become one machine input bundle.

---

## 5. Naming Choice

The preferred name is `BatchProofPlan`, not `WitnessPartitionPlan`.

`WitnessPartitionPlan` is too narrow because the problem is broader than
splitting one store by labels.

The missing plan layer may eventually govern:

- tier routing,
- column grouping,
- proof-unit packaging,
- amortization policy,
- future topology generalization.

`BatchProofPlan` leaves room for that wider role while still being batch-local
and runtime-owned.

If an implementation wants an internal helper named `WitnessPartition`, that is
fine. But the main architectural object should describe the proof-shape role,
not just the current mechanical partitioning step.

---

## 6. Recommended Data Model

For the current C+2 machine, the ideal shape is:

```rust
struct BatchProofPlan {
    columns: Vec<ColumnTierPlan>,
    root: RootTierPlan,
}

struct ColumnTierPlan {
    key: ColumnSlotKey,
}

struct RootTierPlan {
    backend: RootBackendBundle,
}
```

The current public machine payload should evolve toward:

```rust
pub struct PreparedMachineInput {
    pub execution: PreparedTierInput,
    pub columns: Vec<PreparedColumnInput>,
    pub root: PreparedTierInput,
    pub statement: PublicStatement,
    pub statement_digest: [u8; 32],
}

pub struct PreparedTierInput {
    pub store: WitnessStore,
}

pub struct PreparedColumnInput {
    pub key: ColumnSlotKey,
    pub store: WitnessStore,
}
```

This removes the need for machine-side repartitioning entirely.

The important distinction is:

- `BatchProofPlan` is the runtime-owned plan,
- `PreparedMachineInput` is the machine-facing payload produced from that plan.

---

## 7. Root Proof And Root Witness Are One Bundle

The landed Stage 3 shape is a root backend family object wrapped by a bundle:

```rust
trait RootBackend {
    fn name(&self) -> &str;
    fn proof_backend(&self) -> Arc<dyn RootProofBackend>;
    fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer>;
}

struct RootBackendBundle {
    backend: Arc<dyn RootBackend>,
}
```

With responsibilities:

- `RootBackend`
  - ext-owned coherent root family authority
- `RootProofBackend`
  - machine-facing proof topology and verification behavior
- `RootWitnessPreparer`
  - runtime-facing root witness materialization for one batch

This mirrors the existing pattern where column backends already have distinct
runtime-facing, proof-column, and proof-preparation roles.

The most important architectural rule is:

> **If root proving is configurable, root witness preparation must be configurable
> through the same authority boundary.**

Without that, "custom root proof" is only half true.

---

## 8. Crate Ownership

The intended ownership split is:

### 8.1 `tabula-runtime`

Owns:

- `ProofPlan`
- `ProofJournal`
- `BatchProofPlan`
- conversion from runtime reduction output to `PreparedMachineInput`

Runtime is the policy owner and should be the only layer that decides how one
batch becomes one machine proving request.

### 8.2 `tabula-witness`

Owns:

- execution witness materialization kernels,
- root witness materialization kernels,
- column witness materialization kernels,
- narrow reusable store builders.

Witness should not decide whole-batch proof grouping or partition policy.

### 8.3 `tabula-machine`

Owns:

- machine topology,
- prepared-input consumption,
- trace construction from already partitioned tier inputs,
- proof generation and verification.

Machine should not inspect labels to recover runtime planning decisions.

### 8.4 `tabula-ext`

`tabula-ext` now exposes the root/backend bundle story above the runtime layer.

This keeps extension authoring stable while preventing runtime and machine from
becoming accidental authoring authorities.

---

## 9. Relationship To Future Grouping

This design is intentionally aligned with future grouped proofs.

If Tabula later moves from `C+2` to grouped column proofs, the right upgrade
path is:

```rust
struct BatchProofPlan {
    column_groups: Vec<GroupPlan>,
    root_group: GroupPlan,
}
```

The current C+2 structure is then just the degenerate case where:

- each column has its own group,
- root has one group.

That is another reason not to name the missing object after the current
`WitnessStore` split mechanic.

The architectural object is about proof-unit planning, not about one specific
partition algorithm.

---

## 10. Migration Guidance

The migration path that landed was:

1. split shared execution/root witness-store construction,
2. add runtime-internal `BatchProofPlan`,
3. reshape `PreparedMachineInput` so execution and root are already separated,
4. remove machine-side label-based partitioning,
5. bundle proof-side root behavior with runtime-side root witness preparation.

This order matters.

If machine input is generalized before runtime owns the batch-local plan, the
result will still tend to hide routing policy in ad hoc conversion code.

The plan layer should appear first, even if the first version is structurally
simple.

---

## 11. Non-Goals

This note does **not** recommend:

- making `BatchProofPlan` a public SDK vocabulary term today,
- pushing backend-specific partition rules into `ProofJournal`,
- generalizing proof topology before there is a real grouped-proof roadmap,
- turning `tabula-witness` into a second runtime orchestration layer,
- keeping machine-side fallback partitioning as a permanent escape hatch.

The immediate goal is architectural clarity, not maximum abstraction.

---

## 12. One-Sentence Rule

If a future change touches witness partitioning, ask this first:

> **Is this a static slot contract, a batch-local proof plan, or a prepared
> machine payload?**

If those three are not kept distinct, the architecture will start to blur again.
