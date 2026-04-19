# SP-4 Runtime Symmetric Prepared Handles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land two symmetric "prepare once, drive many" runtime handles — `PreparedProver` and `PreparedVerifier` — promote `VerifierState` to a public type, and reduce `TabulaRuntime` to an execute-only facade, without perturbing proof bytes.

**Architecture:** Split today's `RuntimeBuilder::build` into a prepared-state factory that both `PreparedProver` and `TabulaRuntime` consume; rename the verify handle 1:1 to `PreparedVerifier`; hoist `ChipKitRegistry` construction onto `PreparedProver`; keep `KitScratch` allocation per-call so the SP-3 per-batch contract stays honest. `TabulaRuntime::prove` delegates to `PreparedProver::prove` during the transition, then is deleted.

**Tech Stack:** Rust 2021 workspace, `cargo fmt` / `cargo clippy --all-features --all-targets -D warnings` / `cargo test --workspace --all-features`, Plonky3 STARK machine, Koala-Bear field. No new dependencies. Uses existing `ChipKitRegistry` / `KitScratch` / `prepare_execution_store` from `tabula-witness` + `tabula-stark`.

**Canonical references:**
- Spec: `docs/superpowers/specs/2026-04-19-sp4-runtime-prepared-handles-design.md`
- Umbrella: `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md` §4 SP-4
- SP-3 boundary: `docs/superpowers/specs/2026-04-19-sp3-witness-chip-kit-design.md` "SP-4 boundary left by SP-3"
- Architecture invariants: `docs/design/architecture.md`

---

## Global Invariants

Every commit MUST satisfy the following before it lands. Run at the end of each task, before the `git commit` step:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-features --all-targets -- -D warnings`
3. `cargo test --workspace --all-features`
4. Byte-identical proofs for the `basic` and `membership` examples versus the `s0-reference/` snapshot captured in S0. Ran via the check script added in S0. (See §S0 step 4 for the exact command.)
5. `PreparedProver` and `PreparedVerifier` are `Send + Sync`. Enforced by a const-function static-assert helper that lives next to each struct definition.

If any of (1)–(5) regress, **stop and fix the root cause**. Do not advance to the next task.

---

## Design Patterns and Code-Quality Rules

These rules apply to every task in this plan. Review them before you start a task, and re-check before you commit.

- **Separation of concerns.** Prepared-once construction lives in a single factory function. Per-prove batch work (KitScratch alloc, relation pre-stuff, prepare_execution_store) lives inside `PreparedProver::prove`. Do not hoist per-batch work onto handles; do not push prepared-once work into `prove`.
- **Newtype over tuple.** Introduce a named `PreparedRuntimeState` struct rather than reusing `RuntimeProgramState` implicitly or returning tuples from the factory. Named fields document the invariants; tuples rot.
- **No duplicated verify path.** `VerifierCore` and `verify_public_statement_with_context` are inlined into `PreparedVerifier::verify` in S4 — the duplicate verify code path is removed, not renamed.
- **`&self`, not `&mut self`.** `PreparedProver::prove` takes `&self`. Per-prove mutable state (KitScratch, BTreeMaps) lives in locals. This is the load-bearing `Send + Sync` invariant from spec §2.4/§2.5.
- **Do not touch wire formats.** `TabulaProof`, `PublicStatement`, `BoundStatement`, envelope encoding — unchanged bits. Byte-identity determinism check in §S0 is the tripwire.
- **Workspace `missing_docs = warn` / `unused = deny`.** Every new `pub` item carries a doc comment. Never paper over an `unused` warning with `#[allow(unused)]`; delete or wire it up.
- **Feature gating.** `PreparedProver` / `prepare_prover` / `PreparedProverBuilder` are `#[cfg(feature = "prove")]`. `PreparedVerifier` / `prepare_verifier` / `VerifierState` are `#[cfg(feature = "verify")]`. `TabulaRuntime` and `RuntimeBuilder` remain `#[cfg(feature = "verify")]` (execute surface).
- **No `static_assertions` dependency.** The workspace does not have this dep; do not add it. Use the const-fn trick shown in S1 step 5 instead.
- **Names must match across tasks.** `PreparedVerifier`, `PreparedVerifierBuilder`, `prepare_verifier`, `PreparedProver`, `PreparedProverBuilder`, `prepare_prover`, `PreparedRuntimeState`. Do not drift ("PreparedProvingHandle", "prover_state", etc.).
- **Commit cadence.** Commit at every `git commit` step in this plan. No "batch everything into one commit" at the end of a stage. Small commits preserve bisectability.

---

## File Structure

### Files created
- `s0-reference/` (top-level, gitignored) — captured reference `proof.bin` + `public_statement.json` per example; deleted when SP-4 ships.
- `scripts/check-proof-byte-identity.sh` (new, **committed**) — compares current examples vs `s0-reference/`. Helper for S1–S4 gates; deleted in S5 cleanup.

### Files modified
- `crates/runtime/src/lib.rs` — re-exports rename.
- `crates/runtime/src/verifier.rs` — rename, `VerifierState` `pub`, add `prepare_verifier` free fn, add Send+Sync static assert, delete `VerifierCore` + `verify_public_statement_with_context` in S4.
- `crates/runtime/src/engine.rs` — introduce `PreparedRuntimeState`, factor `build_prepared_runtime_state`, add `prover.rs` module below (next item).
- `crates/runtime/src/prover.rs` (new file) — `PreparedProver`, `PreparedProverBuilder`, `prepare_prover`. Houses today's `TabulaRuntime::prove` body + `prepare_proof_request`. This file is introduced in S2 specifically to keep `engine.rs` below its current ~3kLOC rather than grow it. Keeping prove pipeline code in one module also makes the SP-5 decomposition cleaner.
- `crates/runtime/tests/architecture_dependencies.rs` — string-match assertions (line 132-135, 218-220, 241-243 per gitStatus snapshot) rewritten to match new names + new module path.
- `crates/sdk/src/sdk.rs` — `Verifier` → `PreparedVerifier`, `TabulaRuntime` cache split into an execute cache + a prove cache (the prove cache now stores `Arc<PreparedProver>`; the execute cache keeps `Arc<TabulaRuntime>`). `prepare_verifier` method renamed internally to avoid collision with `tabula_runtime::prepare_verifier`; external shape unchanged.
- `crates/sdk/src/program/runner.rs` — prove path uses `PreparedProver::prove`.
- `crates/sdk/src/program/verifier.rs` — holds `Arc<tabula_runtime::PreparedVerifier>`.
- `crates/sdk/src/interop.rs` — exports updated (`PreparedVerifier` added, `Verifier` removed after callers migrate).
- `crates/cli/src/commands/prove.rs` — unchanged shape; the SDK-level rename is transparent.
- `crates/cli/src/handoff/receipt_bridge.rs` — type path reference if impacted (audit in S3).
- `crates/runtime/README.md` — rewritten runtime surface section.
- `docs/design/architecture.md` — runtime section updated in S5.
- `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md` — SP-4 marked **Landed** with a link to the landed-notes amendment.
- `docs/superpowers/specs/2026-04-19-sp4-runtime-prepared-handles-design.md` — landed-notes amendment appended in S5.

### Files not touched
- `crates/witness/**`, `crates/stark/**`, `crates/chips/**`, `crates/machine/**`, `crates/core/**`, `crates/contract/**`, `crates/compiler/**`, `crates/ext/**` — SP-4 is strictly a runtime + SDK/CLI migration. If you feel the need to touch one of these crates, stop and re-read the spec §6 non-goals.

---

## Stage Map

| Stage | Goal | Byte-identity gate |
|-------|------|--------------------|
| S0 | Capture reference proofs | established |
| S1 | Rename Verifier → PreparedVerifier; promote `VerifierState` | passes |
| S2 | Factor `PreparedRuntimeState`; introduce `PreparedProver`; `TabulaRuntime::prove` delegates | passes, double-prove byte-identical |
| S3 | Migrate SDK / CLI to prepared handles | passes |
| S4 | Delete `TabulaRuntime::prove` + VerifierCore; narrow `TabulaRuntime` to execute-only facade | passes |
| S5 | Documentation | `cargo doc` clean, umbrella marked landed |

---

## Stage S0 — Reference Snapshot

**Goal:** Freeze pre-SP-4 proof bytes as the determinism oracle.

**Files:**
- Create: `s0-reference/basic/proof.bin`, `s0-reference/basic/public_statement.json`
- Create: `s0-reference/membership/proof.bin`, `s0-reference/membership/public_statement.json`
- Create: `scripts/check-proof-byte-identity.sh`
- Modify: `.gitignore` — add `/s0-reference/` entry

### Task S0.1: Capture reference proofs

- [ ] **Step 1: Verify starting state is clean**

Run: `git status --porcelain`
Expected: no uncommitted changes (if there are, stash or commit unrelated work first).

- [ ] **Step 2: Verify baseline tests pass before starting**

Run: `cargo test --workspace --all-features`
Expected: 0 failures. If any test fails before we've touched code, STOP and diagnose — do not proceed onto a broken baseline.

- [ ] **Step 3: Generate reference proofs for both examples**

Run the `basic` and `membership` examples end-to-end and copy their proof artifacts into `s0-reference/`. The exact commands depend on the current example harness — use whatever `cargo run` invocation (or xtask / justfile target) the example README documents. Place the outputs so that:

```
s0-reference/
  basic/
    proof.bin
    public_statement.json
  membership/
    proof.bin
    public_statement.json
```

If the example runner writes proofs to a different path by default, copy them into `s0-reference/<example>/` rather than redirecting the runner — the runner's default output is the thing we need to re-check at each gate.

- [ ] **Step 4: Write the byte-identity check script**

Create `scripts/check-proof-byte-identity.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Regenerates basic + membership example proofs and compares them
# byte-for-byte against s0-reference/. Used as the determinism gate
# for SP-4 S1-S4.

ref_dir="s0-reference"
if [[ ! -d "${ref_dir}" ]]; then
    echo "error: ${ref_dir}/ missing — run S0.1 first" >&2
    exit 2
fi

for example in basic membership; do
    echo "--- regenerating proof for ${example} ---"
    # Adjust this line to match the actual example-run command
    # the repo uses (the equivalent of whatever S0.1 step 3 ran).
    bash "scripts/run-example.sh" "${example}"

    echo "--- diffing ${example} ---"
    # The example harness is assumed to emit its proof at
    # examples/<name>/target/proof.bin — if it doesn't, fix the path.
    diff -q "examples/${example}/target/proof.bin" \
            "${ref_dir}/${example}/proof.bin" \
        || { echo "BYTE DIVERGENCE for ${example}"; exit 1; }
    diff -q "examples/${example}/target/public_statement.json" \
            "${ref_dir}/${example}/public_statement.json" \
        || { echo "STATEMENT DIVERGENCE for ${example}"; exit 1; }
done

echo "OK: all reference proofs match byte-for-byte."
```

> **If `scripts/run-example.sh` does not exist yet**: write it in this step as a thin wrapper around whatever command S0.1 step 3 used. Keep the surface (`scripts/run-example.sh <example>`) stable through S5 so every gate uses the same entry point. If the example runner has a different existing entry point in the workspace, substitute that path and keep the rest of the script identical.

`chmod +x scripts/check-proof-byte-identity.sh`

- [ ] **Step 5: Sanity-run the gate**

Run: `bash scripts/check-proof-byte-identity.sh`
Expected: `OK: all reference proofs match byte-for-byte.`

If it does not, fix the script path wiring — do not proceed until a fresh run reproduces the captured reference byte-for-byte.

- [ ] **Step 6: Ignore the reference directory**

Append to `.gitignore`:

```
# SP-4 byte-identity gate; temporary, removed in S5.
/s0-reference/
```

- [ ] **Step 7: Commit the script + gitignore**

```bash
git add .gitignore scripts/check-proof-byte-identity.sh scripts/run-example.sh
git commit -m "$(cat <<'EOF'
chore(sp4): add byte-identity gate script for runtime refactor

Captures a pre-SP-4 proof snapshot under s0-reference/ (gitignored)
and provides scripts/check-proof-byte-identity.sh to re-verify
byte-for-byte determinism of basic + membership proofs at every
SP-4 stage boundary.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

**Gate:** `bash scripts/check-proof-byte-identity.sh` passes. Global invariants (1)–(3) pass.

---

## Stage S1 — Promote `VerifierState`; Rename to `PreparedVerifier`

**Goal:** Verify-side handle rename lands cleanly in a single commit with zero semantic change.

### Task S1.1: Promote `VerifierState` to public and rename the verify types

**Files:**
- Modify: `crates/runtime/src/verifier.rs`
- Modify: `crates/runtime/src/lib.rs`
- Modify: `crates/runtime/src/engine.rs` (imports + internal call site)
- Modify: `crates/runtime/tests/architecture_dependencies.rs`

- [ ] **Step 1: Edit `crates/runtime/src/verifier.rs` — rename & promote**

Replace lines 27-176 of `verifier.rs` with the new shape. All other content (the `VerifierCore` helper block, `relation_table_root_from_proof`, `execution_chip_digest_from_proof`, `verify_proved_public_statement_digests`, `verify_public_statement_with_context`) is unchanged in this stage. Target shape:

```rust
/// Prepared verifier state derived from the sealed artifact and machine setup.
///
/// Public so downstream consumers (SDK, tests, future prover) can name
/// the prepared-once state without going through a builder.
pub struct VerifierState {
    /// Artifact-bound transcript context sealed at prepare time.
    pub context: ArtifactContext,
    /// Relation-policy decision derived from program analysis.
    pub relation_policy: RelationPolicy,
    /// STARK machine backing verification.
    pub machine: TabulaMachine,
}

impl VerifierState {
    fn verifier_core(&self) -> VerifierCore<'_> {
        VerifierCore {
            context: &self.context,
            relation_policy: self.relation_policy,
            machine: &self.machine,
        }
    }
}

/// Verifier built once per registered native program.
///
/// Cheap to share via [`Arc`]; [`PreparedVerifier::verify`] takes
/// `&self` so callers can drive it from multiple threads.
pub struct PreparedVerifier {
    prepared: VerifierState,
}

/// Fluent builder for [`PreparedVerifier`].
pub struct PreparedVerifierBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl PreparedVerifier {
    /// Create a builder for one registered native program.
    pub fn builder(
        registered_program: RegisteredProgram,
    ) -> Result<PreparedVerifierBuilder, RuntimeError> {
        PreparedVerifierBuilder::new(registered_program)
    }

    /// Borrow the prepared verify-side state.
    pub fn state(&self) -> &VerifierState {
        &self.prepared
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.prepared.context.binding
    }

    /// The STARK machine backing this verifier.
    pub fn machine(&self) -> &TabulaMachine {
        &self.prepared.machine
    }

    /// Verify one native proof against an externally supplied expected public
    /// statement and return the artifact-bound statement on success.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected_public_statement: &PublicStatement,
    ) -> Result<BoundStatement, RuntimeError> {
        self.prepared
            .verifier_core()
            .verify_public_statement(proof, expected_public_statement)?;
        Ok(BoundStatement::new(
            self.prepared.context.clone(),
            expected_public_statement.clone(),
        ))
    }
}

impl PreparedVerifierBuilder {
    fn new(registered_program: RegisteredProgram) -> Result<Self, RuntimeError> {
        registered_program
            .validate_sealed_artifact()
            .map_err(RuntimeError::CompilerValidation)?;
        Ok(Self {
            registered_program,
            host_environment: HostEnvironment::standard()?,
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: RootBackendBundle::standard(),
            #[cfg(not(feature = "prove"))]
            root_proof_backend: Arc::new(SmtRootProofBackend),
        })
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root proof backend bundle.
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, root_backend_bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    /// Override the proof-side root backend.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend(
        mut self,
        root_proof_backend: impl RootProofBackend + 'static,
    ) -> Self {
        self.root_proof_backend = Arc::new(root_proof_backend);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend_arc(
        mut self,
        root_proof_backend: Arc<dyn RootProofBackend>,
    ) -> Self {
        self.root_proof_backend = root_proof_backend;
        self
    }

    /// Build the prepared verifier.
    pub fn build(self) -> Result<PreparedVerifier, RuntimeError> {
        validate_core_first_program(self.registered_program.program())?;
        #[cfg(feature = "prove")]
        let proof_backend = self.root_backend_bundle.proof_backend();
        #[cfg(not(feature = "prove"))]
        let proof_backend = Arc::clone(&self.root_proof_backend);
        #[cfg(feature = "prove")]
        let accepted_root_binding_families =
            self.root_backend_bundle.supported_root_binding_families();
        #[cfg(not(feature = "prove"))]
        let accepted_root_binding_families = proof_backend.supported_root_binding_families();
        let program_setup = resolve_program_setup(
            &self.registered_program,
            self.host_environment.schemes().factories(),
            self.host_environment.runtime_registries().type_runtimes(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
            accepted_root_binding_families,
        )?;
        let machine = build_registered_program_machine(
            &program_setup,
            &self.machine_stark_config,
            proof_backend,
        )?;
        Ok(PreparedVerifier {
            prepared: VerifierState {
                context: program_setup.artifact_context,
                relation_policy: program_setup.relation_policy,
                machine,
            },
        })
    }
}

/// Convenience constructor: `prepare_verifier(reg)` is sugar over
/// `PreparedVerifier::builder(reg)?.build()` using the standard host
/// environment, machine config, and root backend.
pub fn prepare_verifier(
    registered_program: RegisteredProgram,
) -> Result<PreparedVerifier, RuntimeError> {
    PreparedVerifier::builder(registered_program)?.build()
}
```

Everything below (`VerifierCore`, `verify_public_statement_with_context`, helpers) stays exactly as-is.

- [ ] **Step 2: Update `crates/runtime/src/lib.rs` re-exports**

Replace line 67:

```rust
pub use verifier::{Verifier, VerifierBuilder};
```

with:

```rust
pub use verifier::{
    PreparedVerifier, PreparedVerifierBuilder, VerifierState, prepare_verifier,
};
```

And update the `# Feature gating` doc comment block (lines 33-37). Replace:

```
//! - **`verify`**: adds [`Verifier`] and [`VerifierBuilder`] for
//!   native proof verification against registered programs.
```

with:

```
//! - **`verify`**: adds [`PreparedVerifier`], [`PreparedVerifierBuilder`],
//!   the [`prepare_verifier`] free function, and the public
//!   [`VerifierState`] type for native proof verification.
```

Update the module-level doc comment at line 22-25:

```
//! artifact, and the prepared verifier state lives in the runtime verifier
//! surface rather than the contract layer.
```

can stay — it already reads correctly with `VerifierState` as the prepared shape.

- [ ] **Step 3: Update `crates/runtime/tests/architecture_dependencies.rs`**

Three assertion blocks to update:

Lines 132-135 (the re-export check): replace

```rust
pub use verifier::{Verifier, VerifierBuilder};
```

inside the `contains(...)` string with

```rust
pub use verifier::{
    PreparedVerifier, PreparedVerifierBuilder, VerifierState, prepare_verifier,
};
```

Lines 218-220 and 241-243 (the `pub struct Verifier {` / `pub struct VerifierBuilder` string asserts): replace both occurrences of `"pub struct Verifier {"` with `"pub struct PreparedVerifier {"` and both occurrences of `"pub struct VerifierBuilder"` with `"pub struct PreparedVerifierBuilder"`.

Also grep the file once more for the literal `"Verifier"` and walk every hit to ensure no stale string-match is left pointing at the old name. For every hit that references our own types, update it. Ignore hits that reference `tabula_machine::BackendVerifier` (different type) or the engine's check `"pub struct Verifier"` which the `native_proof_path_stays_bridge_free` test already asserts is **absent** from `engine.rs` — that assertion is still correct since the new name is also absent, so it still passes; we leave it in place.

- [ ] **Step 4: Update the one external caller inside the runtime crate**

`engine.rs` only ever references `verify_public_statement_with_context` (line 67 import, line 850 call) — no references to `Verifier` itself. No change needed in `engine.rs` for this step. Double-check via:

Run: `grep -nE 'Verifier[^S]|VerifierBuilder' crates/runtime/src/` and inspect that the only hits are in `verifier.rs` (the types themselves). If anything else shows up, update it to the new name before committing.

- [ ] **Step 5: Add the Send+Sync static assert inside `verifier.rs`**

Append at the bottom of `crates/runtime/src/verifier.rs` (still inside the file, outside any impl block):

```rust
// Static guarantee that PreparedVerifier is cheap to share across threads.
// The SDK's cache and any future concurrent driver relies on this.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PreparedVerifier>();
    assert_send_sync::<VerifierState>();
};
```

- [ ] **Step 6: Temporarily shim SDK callers to compile**

`crates/sdk/src/sdk.rs` and `crates/sdk/src/program/verifier.rs` still name `tabula_runtime::Verifier`. Rename these two files' type references in lockstep so the workspace still compiles — a full SDK-surface migration happens in S3, but the symbol rename must land atomically with the runtime rename:

In `crates/sdk/src/sdk.rs`:

- Line 26: `Mutex<BTreeMap<String, Arc<tabula_runtime::Verifier>>>` → `Mutex<BTreeMap<String, Arc<tabula_runtime::PreparedVerifier>>>`
- Line 126: `-> Result<Arc<tabula_runtime::Verifier>, SdkError>` → `-> Result<Arc<tabula_runtime::PreparedVerifier>, SdkError>`
- Line 171: `-> Result<tabula_runtime::Verifier, SdkError>` → `-> Result<tabula_runtime::PreparedVerifier, SdkError>`
- Line 172: `tabula_runtime::Verifier::builder(...)` → `tabula_runtime::PreparedVerifier::builder(...)`

In `crates/sdk/src/program/verifier.rs`:

- Line 9: `prepared: Arc<tabula_runtime::Verifier>` → `prepared: Arc<tabula_runtime::PreparedVerifier>`
- Lines 25-27: keep `self.prepared.verify_public_statement(...)` call — **wait**, we renamed the method to `verify`. Instead use:

```rust
self.prepared
    .verify(&proof.proof, expected_public_statement)?;
Ok(())
```

This is a pure call-site rename. The SDK keeps its outer method name `verify_public_statement` unchanged (that is an SDK-surface concern, not SP-4 scope); internally it calls the runtime's `verify`.

In `crates/sdk/src/interop.rs`: grep for `tabula_runtime::Verifier` references; rename any hit to `tabula_runtime::PreparedVerifier`.

Run: `grep -rn 'tabula_runtime::Verifier\b' crates/`
Expected: no hits after the edits. If any remain, rename them.

- [ ] **Step 7: Global invariants pass**

Run, in order:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-features --all-targets -- -D warnings`
3. `cargo test --workspace --all-features`
4. `bash scripts/check-proof-byte-identity.sh`

Expected: all four pass. The byte-identity check is the real gate — renames alone cannot change proof bytes, and if they do, something is wrong.

- [ ] **Step 8: Commit**

```bash
git add crates/runtime crates/sdk
git commit -m "$(cat <<'EOF'
refactor(runtime): rename Verifier to PreparedVerifier; promote VerifierState

Promotes VerifierState to a documented public type, renames
Verifier/VerifierBuilder -> PreparedVerifier/PreparedVerifierBuilder,
renames verify_public_statement -> verify (now returning
BoundStatement on success), and introduces prepare_verifier as
a free-function constructor. Pure rename + surface reshape; no
semantic change. Byte-identical proofs confirmed against the
s0-reference snapshot. Lands SP-4 §4 S1.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

**Gate:** Global invariants (1)-(5) pass.

---

## Stage S2 — Introduce `PreparedProver` + `prepare_prover`

**Goal:** Factor `RuntimeBuilder::build` into a shared prepared-state factory; stand up `PreparedProver` carrying the prepared-once `ChipKitRegistry`; keep `TabulaRuntime::prove` compiling by delegating to `PreparedProver::prove`.

### Task S2.1: Introduce `PreparedRuntimeState`; factor prepared-state construction

**Files:**
- Modify: `crates/runtime/src/engine.rs`

- [ ] **Step 1: Replace `RuntimeProgramState` with a renamed `PreparedRuntimeState`**

`RuntimeProgramState` is already the right shape conceptually, but it is private + named for the old mental model. Rename it to `PreparedRuntimeState` (make it `pub(crate)`) and move it to a named position in `engine.rs` so it is easy to spot. Rename every field-access site inside `engine.rs` (`runtime_program.semantic` etc. keep working because the field names are unchanged; only the type name changes).

Locate the definition at line ~429-443 and the surrounding `ColumnProofSlot` helper; rename in place:

```rust
#[cfg(feature = "prove")]
#[derive(Clone)]
struct ColumnProofSlot { /* unchanged */ }

/// Prepared runtime state derived from a registered program.
///
/// Shared between `TabulaRuntime` (execute surface) and `PreparedProver`
/// (prove surface). Construction is feature-gated; the fields marked
/// `#[cfg(feature = "prove")]` are carried only on the prove build.
pub(crate) struct PreparedRuntimeState {
    pub(crate) semantic: runtime_ir::RuntimeProgram,
    pub(crate) state: ResolvedStateRuntime,
    #[cfg(feature = "prove")]
    pub(crate) column_slots: Vec<ColumnProofSlot>,
    pub(crate) artifact_context: ArtifactContext,
    pub(crate) relation_policy: RelationPolicy,
    #[cfg(feature = "prove")]
    pub(crate) uses_ir_hash: bool,
    pub(crate) static_table_artifact: StaticTableArtifact,
    #[cfg(feature = "prove")]
    pub(crate) tuple_encoding_defaults: TupleEncodingDefaults,
    pub(crate) type_runtimes: TypeRuntimeRegistry,
    pub(crate) encoding_runtimes: EncodingRuntimeRegistry,
}
```

Then replace every occurrence of `RuntimeProgramState` in `engine.rs` with `PreparedRuntimeState`. (Use Edit with `replace_all`.)

- [ ] **Step 2: Extract a prepared-state factory**

Introduce a new `pub(crate)` free function at the top of the `#[cfg(feature = "verify")] mod engine { ... }` body — I'll describe its placement as: immediately after the `ColumnProofSlot` struct definition and before `RuntimeBuilder`. Its body is today's `RuntimeBuilder::build` minus the final `TabulaRuntime { ... }` struct-literal line, broken out of the impl method:

```rust
#[cfg(feature = "verify")]
pub(crate) struct PreparedRuntimeBuild {
    pub(crate) runtime_program: PreparedRuntimeState,
    pub(crate) machine: TabulaMachine,
    #[cfg(feature = "prove")]
    pub(crate) root_backend_bundle: RootBackendBundle,
}

#[cfg(feature = "verify")]
pub(crate) fn build_prepared_runtime(
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")] root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))] root_proof_backend: Arc<dyn RootProofBackend>,
) -> Result<PreparedRuntimeBuild, RuntimeError> {
    validate_core_first_program(registered_program.program())?;
    let type_runtimes = host_environment
        .runtime_registries()
        .type_runtimes()
        .clone();
    let encoding_runtimes = host_environment
        .runtime_registries()
        .encoding_runtimes()
        .clone();
    #[cfg(feature = "prove")]
    let proof_backend = root_backend_bundle.proof_backend();
    #[cfg(not(feature = "prove"))]
    let proof_backend = Arc::clone(&root_proof_backend);
    #[cfg(feature = "prove")]
    let accepted_root_binding_families =
        root_backend_bundle.supported_root_binding_families();
    #[cfg(not(feature = "prove"))]
    let accepted_root_binding_families = proof_backend.supported_root_binding_families();
    let program_setup = resolve_program_setup(
        &registered_program,
        host_environment.schemes().factories(),
        &type_runtimes,
        &encoding_runtimes,
        accepted_root_binding_families,
    )?;
    #[cfg(feature = "prove")]
    let column_slots = program_setup
        .resolved_state
        .backends()
        .map(|backend| ColumnProofSlot {
            table: backend.table_id,
            col: backend.col_id,
            proof_backend: Arc::clone(&backend.proof_backend),
        })
        .collect::<Vec<_>>();

    let semantic = runtime_ir::RuntimeProgram::from_validated_program(
        registered_program.validated_program().clone(),
    )
    .map_err(|error| RuntimeError::ValidationFailed {
        detail: error.to_string(),
    })?;

    let machine =
        build_registered_program_machine(&program_setup, &machine_stark_config, proof_backend)?;

    let runtime_program = PreparedRuntimeState {
        semantic,
        state: program_setup.resolved_state.clone(),
        #[cfg(feature = "prove")]
        column_slots,
        artifact_context: program_setup.artifact_context,
        relation_policy: program_setup.relation_policy,
        #[cfg(feature = "prove")]
        uses_ir_hash: program_setup.uses_ir_hash,
        static_table_artifact: registered_program.static_table_artifact().clone(),
        #[cfg(feature = "prove")]
        tuple_encoding_defaults: registered_program.tuple_encoding_defaults().clone(),
        type_runtimes,
        encoding_runtimes,
    };

    Ok(PreparedRuntimeBuild {
        runtime_program,
        machine,
        #[cfg(feature = "prove")]
        root_backend_bundle,
    })
}
```

- [ ] **Step 3: Thin out `RuntimeBuilder::build`**

Replace the body of `RuntimeBuilder::build` (lines 512-587 today) with:

```rust
/// Build the native runtime.
pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
    let prepared = build_prepared_runtime(
        self.registered_program,
        self.host_environment,
        self.machine_stark_config,
        #[cfg(feature = "prove")]
        self.root_backend_bundle,
        #[cfg(not(feature = "prove"))]
        self.root_proof_backend,
    )?;
    Ok(TabulaRuntime {
        runtime_program: prepared.runtime_program,
        #[cfg(feature = "prove")]
        root_backend_bundle: prepared.root_backend_bundle,
        machine: prepared.machine,
    })
}
```

No other `RuntimeBuilder` method changes.

- [ ] **Step 4: Global invariants**

Run:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-features --all-targets -- -D warnings`
3. `cargo test --workspace --all-features`
4. `bash scripts/check-proof-byte-identity.sh`

All pass. The factoring should be semantics-preserving — byte-identity is the tripwire.

- [ ] **Step 5: Commit**

```bash
git add crates/runtime/src/engine.rs
git commit -m "$(cat <<'EOF'
refactor(runtime): factor prepared-state construction into build_prepared_runtime

Renames RuntimeProgramState -> PreparedRuntimeState and extracts
RuntimeBuilder::build's prepared-once body into a shared factory
function usable by both TabulaRuntime (execute surface) and the
forthcoming PreparedProver (prove surface). Pure refactor; byte-
identical proofs confirmed. Prep work for SP-4 §4 S2.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S2.2: Stand up `PreparedProver` with prepared-once `ChipKitRegistry`

**Files:**
- Create: `crates/runtime/src/prover.rs`
- Modify: `crates/runtime/src/lib.rs`
- Modify: `crates/runtime/src/engine.rs`
- Modify: `crates/runtime/tests/architecture_dependencies.rs`

- [ ] **Step 1: Add the new module to `lib.rs`**

In `crates/runtime/src/lib.rs`, add after line 51 (`mod verifier;`):

```rust
#[cfg(feature = "prove")]
mod prover;
```

And add to the re-export block at the bottom:

```rust
#[cfg(feature = "prove")]
pub use prover::{PreparedProver, PreparedProverBuilder, prepare_prover};
```

Also extend the feature-gating doc comment on `lib.rs` (lines 35-37):

```
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`],
//!   [`PreparedProver`], [`PreparedProverBuilder`], the
//!   [`prepare_prover`] free function, and the full native
//!   witness → trace → prove pipeline. Implies `verify`.
```

- [ ] **Step 2: Move `ChipKitRegistry` construction into a helper function**

In `engine.rs`, today's `prepare_proof_artifacts` constructs a fresh `ChipKitRegistry` at lines 1653-1659:

```rust
let mut kit_registry = ChipKitRegistry::new();
for backend in crate::bootstrap::program::execution_backends_for(
    runtime_program.uses_ir_hash,
    runtime_program.relation_policy,
) {
    kit_registry.register_all(backend.witness_kits());
}
```

Extract into a named `pub(crate)` function next to `build_prepared_runtime`:

```rust
#[cfg(feature = "prove")]
pub(crate) fn build_chip_kit_registry(state: &PreparedRuntimeState) -> ChipKitRegistry {
    let mut kit_registry = ChipKitRegistry::new();
    for backend in crate::bootstrap::program::execution_backends_for(
        state.uses_ir_hash,
        state.relation_policy,
    ) {
        kit_registry.register_all(backend.witness_kits());
    }
    kit_registry
}
```

- [ ] **Step 3: Adjust `prepare_proof_artifacts` to consume a registry reference**

Change its signature to accept `&ChipKitRegistry` rather than building one internally:

```rust
#[cfg(feature = "prove")]
fn prepare_proof_artifacts(
    runtime_program: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    snapshot: &CommittedStateSnapshot,
    txs: &[TxCall],
    context: &ContextValues,
    executed: &exec::ExecutionJournal,
) -> Result<PreparedArtifacts, RuntimeError> { ... }
```

Inside the body, delete the `let mut kit_registry = ChipKitRegistry::new(); for backend in ... { kit_registry.register_all(...); }` block (lines 1653-1659) and replace the `prepare_execution_store(&mut lowered, &kit_registry)` call with `prepare_execution_store(&mut lowered, kit_registry)`.

- [ ] **Step 4: Thread the registry through `prepare_proof_request`**

`TabulaRuntime::prepare_proof_request` at line 891 currently calls `prepare_proof_artifacts(&self.runtime_program, &self.root_backend_bundle, ...)`. Replace with:

```rust
let proof_artifacts = prepare_proof_artifacts(
    &self.runtime_program,
    &self.root_backend_bundle,
    &self.kit_registry,
    input.snapshot,
    &typed_txs,
    &typed_context,
    input.executed,
)?;
```

Add a `kit_registry: ChipKitRegistry` field to `TabulaRuntime` (next to `machine`) under `#[cfg(feature = "prove")]`:

```rust
pub struct TabulaRuntime {
    runtime_program: PreparedRuntimeState,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(feature = "prove")]
    kit_registry: ChipKitRegistry,
    machine: TabulaMachine,
}
```

And populate it in `RuntimeBuilder::build`:

```rust
Ok(TabulaRuntime {
    runtime_program: prepared.runtime_program,
    #[cfg(feature = "prove")]
    root_backend_bundle: prepared.root_backend_bundle,
    #[cfg(feature = "prove")]
    kit_registry: build_chip_kit_registry(&TabulaRuntime_prepared_ref_here /* see below */),
    machine: prepared.machine,
})
```

> The registry needs `&PreparedRuntimeState` *after* the `TabulaRuntime` fields have been laid out — easiest fix is to construct `kit_registry` BEFORE the struct literal:

```rust
pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
    let prepared = build_prepared_runtime(
        self.registered_program,
        self.host_environment,
        self.machine_stark_config,
        #[cfg(feature = "prove")]
        self.root_backend_bundle,
        #[cfg(not(feature = "prove"))]
        self.root_proof_backend,
    )?;
    #[cfg(feature = "prove")]
    let kit_registry = build_chip_kit_registry(&prepared.runtime_program);
    Ok(TabulaRuntime {
        runtime_program: prepared.runtime_program,
        #[cfg(feature = "prove")]
        root_backend_bundle: prepared.root_backend_bundle,
        #[cfg(feature = "prove")]
        kit_registry,
        machine: prepared.machine,
    })
}
```

- [ ] **Step 5: Verify the byte-identity gate before moving on**

Run `cargo test --workspace --all-features && bash scripts/check-proof-byte-identity.sh`. This step pre-checks that hoisting the registry onto `TabulaRuntime` itself — without `PreparedProver` yet — has not perturbed proof bytes. If the gate fails here, the registry hoisting is buggy and must be fixed before any further work. **Do not commit yet.**

- [ ] **Step 6: Write the new `prover.rs`**

Create `crates/runtime/src/prover.rs`:

```rust
//! Prepared prover handle for one registered native program.
//!
//! [`PreparedProver`] is the canonical way to get a prove-capable
//! runtime handle. It owns the prepared-once state (`VerifierState`,
//! machine, chip-kit registry, root backend bundle) and exposes
//! [`PreparedProver::prove`] for per-batch proving. The handle is
//! `Send + Sync` and cheap to share via [`std::sync::Arc`].

use tabula_compiler::RegisteredProgram;
use tabula_contract::{BoundStatement, ProgramBinding, PublicStatement};
use tabula_core::Digest;
use tabula_ext::root::RootBackendBundle;
use tabula_machine::{TabulaMachine, TabulaStarkConfig};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};
use tabula_witness::stark::ChipKitRegistry;

use crate::engine::{
    PreparedRuntimeState, ProveInput, ProveResult, VerifiedResult, build_chip_kit_registry,
    build_prepared_runtime, prepare_proof_request_on_prepared_state,
};
use crate::error::RuntimeError;
use crate::host::HostEnvironment;
use crate::verifier::VerifierState;

/// Prepared prover handle for one registered native program.
pub struct PreparedProver {
    runtime_program: PreparedRuntimeState,
    root_backend_bundle: RootBackendBundle,
    kit_registry: ChipKitRegistry,
    machine: TabulaMachine,
}

/// Fluent builder for [`PreparedProver`].
pub struct PreparedProverBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    root_backend_bundle: RootBackendBundle,
}

impl PreparedProver {
    /// Create a builder for one registered native program.
    pub fn builder(
        registered_program: RegisteredProgram,
    ) -> Result<PreparedProverBuilder, RuntimeError> {
        PreparedProverBuilder::new(registered_program)
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.runtime_program.artifact_context.binding
    }

    /// Borrow the transcript-bound static relation-table root.
    pub fn static_table_root(&self) -> Digest {
        self.runtime_program.artifact_context.static_table_root
    }

    /// The STARK machine backing this prover.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Installed type runtimes.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.runtime_program.type_runtimes
    }

    /// Installed encoding runtimes.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.runtime_program.encoding_runtimes
    }

    /// Borrow the prepared verify-side state. Shared with [`crate::PreparedVerifier`].
    pub fn state(&self) -> VerifierState {
        VerifierState {
            context: self.runtime_program.artifact_context.clone(),
            relation_policy: self.runtime_program.relation_policy,
            machine: self.machine.clone(),
        }
    }

    /// Generate a proof for one already-executed tx batch.
    ///
    /// `&self` is load-bearing: the prepared state is shared-read, and
    /// all per-batch mutable state (KitScratch, column artifacts) lives
    /// in locals inside this call. Calling `prove` twice on the same
    /// handle with the same input must produce byte-identical output.
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        prepare_proof_request_on_prepared_state(
            &self.runtime_program,
            &self.root_backend_bundle,
            &self.kit_registry,
            &self.machine,
            input,
        )
    }

    /// Generate and verify a proof in one call. Convenience wrapper.
    pub fn prove_and_verify(
        &self,
        verifier: &crate::PreparedVerifier,
        input: &ProveInput<'_>,
    ) -> Result<VerifiedResult, RuntimeError> {
        let prove_result = self.prove(input)?;
        verifier.verify(&prove_result.proof, &prove_result.public_statement)?;
        Ok(VerifiedResult {
            proof: prove_result.proof,
            envelope: prove_result.envelope,
            public_statement: prove_result.public_statement,
            verified: true,
            summary: prove_result.summary,
        })
    }
}

impl PreparedProverBuilder {
    fn new(registered_program: RegisteredProgram) -> Result<Self, RuntimeError> {
        registered_program
            .validate_sealed_artifact()
            .map_err(RuntimeError::CompilerValidation)?;
        Ok(Self {
            registered_program,
            host_environment: HostEnvironment::standard()?,
            machine_stark_config: tabula_machine::default_config(),
            root_backend_bundle: RootBackendBundle::standard(),
        })
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root proof backend bundle.
    pub fn with_root_backend_bundle(mut self, root_backend_bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    /// Build the prepared prover.
    pub fn build(self) -> Result<PreparedProver, RuntimeError> {
        let prepared = build_prepared_runtime(
            self.registered_program,
            self.host_environment,
            self.machine_stark_config,
            self.root_backend_bundle,
        )?;
        let kit_registry = build_chip_kit_registry(&prepared.runtime_program);
        Ok(PreparedProver {
            runtime_program: prepared.runtime_program,
            root_backend_bundle: prepared.root_backend_bundle,
            kit_registry,
            machine: prepared.machine,
        })
    }
}

/// Convenience constructor: `prepare_prover(reg)` is sugar over
/// `PreparedProver::builder(reg)?.build()` with standard defaults.
pub fn prepare_prover(
    registered_program: RegisteredProgram,
) -> Result<PreparedProver, RuntimeError> {
    PreparedProver::builder(registered_program)?.build()
}

// Load-bearing: PreparedProver is Send + Sync so it can live behind
// Arc across threads without additional synchronization. See spec
// §2.4 and §2.5 — any non-Sync field added here breaks SDK sharing.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PreparedProver>();
};
```

- [ ] **Step 7: Expose the prove-body helper from `engine.rs`**

The `prove` body in `engine.rs` line 830-842 depends on `prepare_proof_request`, which itself depends on `&self`. Refactor `prepare_proof_request` into a free function and route today's `TabulaRuntime::prove` + new `PreparedProver::prove` through it. In `engine.rs`:

1. Add a new `pub(crate)` free function (next to `prepare_proof_artifacts`):

```rust
#[cfg(feature = "prove")]
pub(crate) fn prepare_proof_request_on_prepared_state(
    runtime_program: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    machine: &TabulaMachine,
    input: &ProveInput<'_>,
) -> Result<ProveResult, RuntimeError> {
    // Decode context + batch + applied-tx digest. (Copy lines 895-905 of today's
    // prepare_proof_request verbatim, but read from `runtime_program` instead
    // of `self`.)
    let typed_context = decode_context_input_on_state(runtime_program, input.context)?;
    let typed_txs = decode_entry_batch_on_state(runtime_program, input.batch)?;
    let applied_tx_digest = runtime_ir::compute_applied_tx_digest(
        input.batch,
        &runtime_program.type_runtimes,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| RuntimeError::StatementBuild {
        detail: error.to_string(),
    })?;
    let proof_artifacts = prepare_proof_artifacts(
        runtime_program,
        root_backend_bundle,
        kit_registry,
        input.snapshot,
        &typed_txs,
        &typed_context,
        input.executed,
    )?;
    let public_statement = materialize_public_statement_on_state(
        runtime_program,
        &typed_context,
        runtime_ir::PublicStatementMaterialization {
            applied_tx_digest,
            old_state_root: proof_artifacts.public_statement.old_root.to_bytes(),
            new_state_root: proof_artifacts.public_statement.new_root.to_bytes(),
        },
        input.executed,
    )?;
    let bound = BoundStatement::new(
        runtime_program.artifact_context.clone(),
        public_statement.clone(),
    );
    let binding_digest =
        bound
            .binding_digest()
            .map_err(|error| RuntimeError::StatementBuild {
                detail: error.to_string(),
            })?;
    let machine_input = proof_artifacts.into_prepared_machine_input(binding_digest);
    let (proof, envelope) = BackendProver::new(machine)
        .prove_envelope(machine_input)
        .map_err(RuntimeError::Proving)?;
    let summary = ProofSummary::from_proof(&proof);
    Ok(ProveResult {
        proof,
        envelope,
        public_statement,
        summary,
    })
}
```

2. Extract the three `TabulaRuntime` helpers that `prepare_proof_request_on_prepared_state` needs into `pub(crate)` free functions operating on `&PreparedRuntimeState`:

```rust
#[cfg(feature = "verify")]
pub(crate) fn decode_entry_batch_on_state(
    state: &PreparedRuntimeState,
    batch: &ir::EntryBatch,
) -> Result<Vec<TxCall>, RuntimeError> {
    batch
        .calls
        .iter()
        .map(|call| decode_entry_call_on_state(state, call))
        .collect()
}

#[cfg(feature = "verify")]
fn decode_entry_call_on_state(
    state: &PreparedRuntimeState,
    call: &ir::EntryCall,
) -> Result<TxCall, RuntimeError> {
    // Lift lines 949-966 of engine.rs verbatim, substituting
    // `state.semantic.execution().entry_definition(...)` for
    // `self.execution_program().entry_definition(...)` and
    // `&state.type_runtimes` for `self.type_runtimes()`.
    // (Full body omitted here for brevity; copy-paste the
    // existing 18-line method body and adjust the two receivers.)
    # unimplemented!() // REPLACE — plan-reader: lift from engine.rs:949
}

#[cfg(feature = "verify")]
pub(crate) fn decode_context_input_on_state(
    state: &PreparedRuntimeState,
    context: &ir::ContextInput,
) -> Result<ContextValues, RuntimeError> {
    // Lift lines 1024-1055 of engine.rs verbatim, substituting
    // state.semantic.execution().context_field(...) and
    // &state.type_runtimes for the `self.` receivers.
    # unimplemented!() // REPLACE — plan-reader: lift from engine.rs:1024
}

#[cfg(feature = "prove")]
pub(crate) fn materialize_public_statement_on_state(
    state: &PreparedRuntimeState,
    context: &ContextValues,
    materialization: runtime_ir::PublicStatementMaterialization,
    execution_journal: &exec::ExecutionJournal,
) -> Result<PublicStatement, RuntimeError> {
    runtime_ir::materialize_public_statement(
        state.semantic.proof(),
        context,
        execution_journal,
        materialization,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| RuntimeError::StatementBuild {
        detail: error.to_string(),
    })
}

#[cfg(feature = "verify")]
fn decode_params_on_state(
    state: &PreparedRuntimeState,
    expected: &[ir::ParamDecl],
    params: &[PortableValue],
) -> Result<Vec<TypedValue>, RuntimeError> {
    // Lift lines 987-1022 of engine.rs verbatim.
    # unimplemented!() // REPLACE — plan-reader: lift from engine.rs:987
}
```

> **Plan-reader note on the `# unimplemented!()` placeholders above.** The three bodies are mechanical translations of existing `TabulaRuntime` methods at the cited line numbers. Copy the current method body verbatim, then change `self.execution_program()` to `state.semantic.execution()`, `self.type_runtimes()` to `&state.type_runtimes`, and `self.runtime_program.X` to `state.X`. No other edits. These are not "TODOs" — the full text already exists in `engine.rs` and only the receiver substitution is new.

3. Rewrite `TabulaRuntime::decode_entry_batch`, `decode_entry_call`, `decode_query_params`, `decode_params`, `decode_context_input`, and `materialize_public_statement_typed` to delegate to these free functions:

```rust
fn decode_entry_batch(&self, batch: &ir::EntryBatch) -> Result<Vec<TxCall>, RuntimeError> {
    decode_entry_batch_on_state(&self.runtime_program, batch)
}
// …etc for the other five.
```

- [ ] **Step 8: Rewrite `TabulaRuntime::prove` and `prepare_proof_request` as delegation**

Replace `TabulaRuntime::prove` (lines 830-842) with:

```rust
/// Generate a proof for one already-executed tx batch.
///
/// Deprecated in SP-4 S4; prefer [`crate::PreparedProver::prove`].
/// Delegates to the prepared-prover code path during SP-4 S2/S3 so
/// the byte-identity gate stays honest through the migration.
#[cfg(feature = "prove")]
pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
    prepare_proof_request_on_prepared_state(
        &self.runtime_program,
        &self.root_backend_bundle,
        &self.kit_registry,
        &self.machine,
        input,
    )
}
```

Delete `TabulaRuntime::prepare_proof_request` (the `#[cfg(feature = "prove")] fn` at line 890-931). Also simplify `TabulaRuntime::prove_and_verify` and `TabulaRuntime::execute_and_prove` to use the new `prove`.

> **Do not** delete `TabulaRuntime::verify_public_statement` yet — that happens in S4. In S3 callers migrate to `PreparedVerifier::verify`; in S4 the duplicate method is removed.

- [ ] **Step 9: Invariants after the S2.2 work**

Run the four-step global invariant check. The byte-identity gate **must** pass and, additionally, run the gate twice back-to-back to make sure a second prove on the same runtime instance still produces the same bytes (this is the per-call-KitScratch tripwire from spec §5 / §2.5):

```
bash scripts/check-proof-byte-identity.sh
bash scripts/check-proof-byte-identity.sh
```

Expected: both runs report `OK`.

- [ ] **Step 10: Write a prove-determinism unit test**

Add to `crates/runtime/src/prover.rs` (inside a `#[cfg(test)] mod tests { ... }` block) a test that:

1. Builds a minimal example (reuse an existing in-crate test fixture from `crates/runtime/src/engine.rs`'s test module — see engine.rs lines ~2053, 2077, 2716 for existing fixture patterns; copy the smallest program that prove-compiles and factor the setup into a helper so the test is self-contained).
2. Builds one `PreparedProver`.
3. Calls `prove(input)` twice on the same `&prover` with the same input.
4. Asserts `proof1.proof.encode_to_vec() == proof2.proof.encode_to_vec()`.

```rust
#[cfg(test)]
#[cfg(feature = "prove")]
mod tests {
    use super::*;
    // Fixture helper — intentionally copied (not factored yet, to keep the
    // test self-contained). The engine.rs fixture patterns near line 2053
    // show the shape to adapt.

    #[test]
    fn prove_twice_on_same_handle_is_byte_identical() {
        let prover = /* build PreparedProver from the simplest program fixture available */;
        let input = /* minimal valid ProveInput */;

        let result1 = prover.prove(&input).expect("first prove");
        let result2 = prover.prove(&input).expect("second prove");

        let bytes1 = borsh::to_vec(&result1.proof).expect("serialize first proof");
        let bytes2 = borsh::to_vec(&result2.proof).expect("serialize second proof");
        assert_eq!(bytes1, bytes2, "prove must be deterministic per-handle");

        assert_eq!(result1.public_statement, result2.public_statement);
    }
}
```

> **Plan-reader note.** If `borsh::to_vec` isn't the current proof serializer, use whatever `TabulaProof` exposes (check `proof.bin` write in the CLI prove handler at `crates/cli/src/commands/prove.rs:24` — `proof.encode_binary()` is the public entry point). Match that encoding.

Run the new test: `cargo test --workspace --all-features prove_twice_on_same_handle_is_byte_identical`. Expected: PASS.

- [ ] **Step 11: Update `architecture_dependencies.rs` re-export string match**

The re-export assertion (lines 131-137 per S1 edits) must now also contain `pub use prover::` to reflect the new prove module. Update the `contains(...)` chain:

```rust
assert!(
    runtime_lib.contains(
        "pub use engine::{CommittedStateSnapshot, ExecutionReceipt, RuntimeBuilder, TabulaRuntime};"
    ) && runtime_lib.contains("pub use tabula_contract::{BoundStatement, PublicStatement};")
        && runtime_lib.contains("pub use engine::{ProveInput, ProveResult, VerifiedResult};")
        && runtime_lib.contains(
            "pub use verifier::{\n    PreparedVerifier, PreparedVerifierBuilder, VerifierState, prepare_verifier,\n};"
        )
        && runtime_lib.contains(
            "pub use prover::{PreparedProver, PreparedProverBuilder, prepare_prover};"
        ),
    "runtime root must re-export the canonical native runtime, prover, and verifier types"
);
```

> The exact multi-line string in the verifier re-export must match `rustfmt`'s choice — run `cargo fmt` first, open `lib.rs`, copy the formatter's exact whitespace, paste into the `contains(...)` argument.

Also extend the `live_runtime_sources_are_legacy_free` compiled paths list (line 135-145 of the test) to include `crates/runtime/src/prover.rs`.

- [ ] **Step 12: Commit**

```bash
git add crates/runtime crates/runtime/tests
git commit -m "$(cat <<'EOF'
feat(runtime): introduce PreparedProver + prepare_prover

Stands up the prove-capable prepared handle mirroring PreparedVerifier.
PreparedProver hoists the ChipKitRegistry construction to handle-build
time while keeping KitScratch allocation per-prove (SP-3 boundary).
TabulaRuntime::prove is retained for S3 call-site migration and now
delegates to the shared prepared-state code path.

Adds a Send+Sync static assertion on PreparedProver and a prove-twice
determinism test that doubles as the per-call KitScratch tripwire
from spec §2.5. Lands SP-4 §4 S2.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

**Gate:** Global invariants (1)-(5) pass. Plus: double-prove byte-identity test passes.

---

## Stage S3 — Migrate SDK and CLI to Prepared Handles

**Goal:** Every external prove/verify call site goes through `PreparedProver` / `PreparedVerifier`. `TabulaRuntime::prove` has zero external callers at the end of this stage (the method still exists; S4 deletes it).

### Task S3.1: SDK prove path → `PreparedProver`

**Files:**
- Modify: `crates/sdk/src/sdk.rs`
- Modify: `crates/sdk/src/program/runner.rs`
- Modify: `crates/sdk/src/interop.rs`

- [ ] **Step 1: Split the SDK runtime cache**

`Sdk::prepare_runtime` today returns `Arc<tabula_runtime::TabulaRuntime>` for both execute and prove. We split responsibilities:

- Keep `runtime_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::TabulaRuntime>>>` and `prepare_runtime` — used by execute.
- Add `prover_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::PreparedProver>>>` (prove feature only).
- Add `Sdk::prepare_prepared_prover` (method name avoids collision with the runtime free fn `tabula_runtime::prepare_prover`).

Edit `crates/sdk/src/sdk.rs`:

1. Add field inside `SdkInner` (line 21-27):

```rust
#[cfg(feature = "prove")]
pub(crate) prover_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::PreparedProver>>>,
```

2. Populate it in `from_environment` (lines 42-51):

```rust
pub fn from_environment(environment: Environment) -> Self {
    Self {
        inner: Arc::new(SdkInner {
            environment,
            #[cfg(feature = "execute")]
            runtime_cache: Mutex::new(BTreeMap::new()),
            #[cfg(feature = "verify")]
            verifier_cache: Mutex::new(BTreeMap::new()),
            #[cfg(feature = "prove")]
            prover_cache: Mutex::new(BTreeMap::new()),
        }),
    }
}
```

3. Add two methods, mirroring `prepare_runtime` / `prepare_verifier`:

```rust
#[cfg(feature = "prove")]
pub(crate) fn prepare_prepared_prover(
    &self,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::PreparedProver>, SdkError> {
    let key = self.cache_key("prover", artifact);
    let cache = self
        .inner
        .prover_cache
        .lock()
        .map_err(|_| SdkError::Synchronization {
            detail: "sdk prover cache mutex poisoned".to_string(),
        })?;
    if let Some(prover) = cache.get(&key) {
        return Ok(Arc::clone(prover));
    }
    drop(cache);

    let built = Arc::new(self.build_prover(artifact)?);
    let mut cache = self
        .inner
        .prover_cache
        .lock()
        .map_err(|_| SdkError::Synchronization {
            detail: "sdk prover cache mutex poisoned".to_string(),
        })?;
    if let Some(prover) = cache.get(&key) {
        return Ok(Arc::clone(prover));
    }
    cache.insert(key, Arc::clone(&built));
    Ok(built)
}

#[cfg(feature = "prove")]
fn build_prover(
    &self,
    artifact: &Artifact,
) -> Result<tabula_runtime::PreparedProver, SdkError> {
    let builder = tabula_runtime::PreparedProver::builder(artifact.registered().clone())
        .map_err(SdkError::from)?
        .with_host_environment(self.inner.environment.inner.host_environment.clone())
        .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone())
        .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
    builder.build().map_err(SdkError::from)
}
```

- [ ] **Step 2: Route `Runner::prove` through the prover cache**

In `crates/sdk/src/program/runner.rs`, replace the `prove` body (lines 317-329):

```rust
#[cfg(feature = "prove")]
pub fn prove(&self, receipt: &ExecutionReceipt) -> Result<Proof, SdkError> {
    if receipt.program_digest != self.program.artifact().digest() {
        return Err(SdkError::ExecutionProgramMismatch);
    }
    let prover = self.program.sdk().prepare_prepared_prover(self.program.artifact())?;
    let result = prover.prove(&tabula_runtime::ProveInput {
        snapshot: &receipt.inner.snapshot,
        batch: &receipt.inner.batch,
        context: &receipt.inner.context,
        executed: &receipt.inner.journal,
    })?;
    Ok(Proof::from_prove_result(result))
}
```

Execute-path (`runtime()` at line 349-351) is unchanged — it still wants `TabulaRuntime` for `execute_batch_receipt` / `project_logical_state` / `materialize_logical_state`.

- [ ] **Step 3: Add the interop passthrough**

In `crates/sdk/src/interop.rs`, mirror the existing `prepare_runtime` free fn (line 292-298):

```rust
/// Prepare the cached native prover for one artifact.
#[cfg(feature = "prove")]
pub fn prepare_prover(
    sdk: &Sdk,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::PreparedProver>, SdkError> {
    sdk.prepare_prepared_prover(artifact)
}
```

Also add, near the `pub use tabula_runtime::TabulaRuntime` line (31), prove-feature re-exports:

```rust
#[cfg(feature = "prove")]
pub use tabula_runtime::{PreparedProver, PreparedProverBuilder};
```

And extend the verify-feature re-exports similarly (next to line 44):

```rust
pub use tabula_runtime::{PreparedVerifier, PreparedVerifierBuilder, VerifierState};
```

- [ ] **Step 4: Update SDK cache tests**

`crates/sdk/src/sdk.rs` tests (lines 245-337) exercise `prepare_runtime` and `prepare_verifier`. Add parallel tests for `prepare_prepared_prover`:

```rust
#[cfg(all(feature = "compile", feature = "prove"))]
#[test]
fn prepare_prover_reuses_cached_instance() {
    let sdk = Sdk::standard().expect("build standard sdk");
    let artifact = compile_simple_artifact(&sdk);

    let first = sdk
        .prepare_prepared_prover(&artifact)
        .expect("build prover");
    let second = sdk
        .prepare_prepared_prover(&artifact)
        .expect("reuse prover");

    assert!(Arc::ptr_eq(&first, &second));
    let cache = sdk.inner.prover_cache.lock().expect("prover cache");
    assert_eq!(cache.len(), 1);
}

#[cfg(all(feature = "compile", feature = "prove"))]
#[test]
fn prepare_prover_build_failure_does_not_poison_cache() {
    let sdk = sdk_with_empty_runtime_host();
    let artifact = compile_simple_artifact(&sdk);

    let Err(first) = sdk.prepare_prepared_prover(&artifact) else {
        panic!("prover build must fail without host environment");
    };
    let Err(second) = sdk.prepare_prepared_prover(&artifact) else {
        panic!("repeated prover build failure must stay recoverable");
    };

    assert!(matches!(
        first,
        SdkError::Runtime(RuntimeError::ValidationFailed { .. })
    ));
    assert!(matches!(
        second,
        SdkError::Runtime(RuntimeError::ValidationFailed { .. })
    ));
    assert!(sdk.inner.prover_cache.lock().is_ok());
}
```

- [ ] **Step 5: Invariants + commit**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
bash scripts/check-proof-byte-identity.sh
```

All pass.

```bash
git add crates/sdk
git commit -m "$(cat <<'EOF'
refactor(sdk): route prove path through PreparedProver

Splits the SDK runtime cache into a TabulaRuntime execute cache
plus a new PreparedProver prove cache. Runner::prove now pulls
from the prove cache; execute paths keep using TabulaRuntime
unchanged. Adds interop::prepare_prover and exposes
PreparedProver / PreparedProverBuilder through the interop
surface. Lands SP-4 §4 S3 (prove side).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S3.2: SDK verify path → `PreparedVerifier::verify`

**Files:**
- Modify: `crates/sdk/src/program/verifier.rs`
- Modify: `crates/sdk/src/sdk.rs`
- Modify: `crates/sdk/src/interop.rs`

- [ ] **Step 1: Rename the SDK private method**

In `crates/sdk/src/sdk.rs`, rename `Sdk::prepare_verifier` (line 123) to `Sdk::prepare_prepared_verifier` and adjust the build helper name from `build_verifier` to `build_prepared_verifier` — this avoids name collision with the newly re-exported `tabula_runtime::prepare_verifier` free function at SDK call sites.

Also update the call site in `crates/sdk/src/program/verifier.rs` line 15:

```rust
prepared: program.sdk().prepare_prepared_verifier(program.artifact())?,
```

- [ ] **Step 2: Add interop passthrough**

In `crates/sdk/src/interop.rs`, add under the `prepare_runtime` section:

```rust
/// Prepare the cached native verifier for one artifact.
#[cfg(feature = "verify")]
pub fn prepare_verifier(
    sdk: &Sdk,
    artifact: &Artifact,
) -> Result<Arc<tabula_runtime::PreparedVerifier>, SdkError> {
    sdk.prepare_prepared_verifier(artifact)
}
```

(The name collision with `tabula_runtime::prepare_verifier` is resolved by path qualification at every import site.)

- [ ] **Step 3: Invariants + commit**

Run the four checks. Commit:

```bash
git add crates/sdk
git commit -m "$(cat <<'EOF'
refactor(sdk): route verify path through PreparedVerifier::verify

Renames Sdk::prepare_verifier to prepare_prepared_verifier to
avoid collision with tabula_runtime::prepare_verifier at import
sites. SDK-surface Verifier::verify_public_statement is unchanged;
it now calls PreparedVerifier::verify internally (ignoring the
returned BoundStatement, preserving wire compatibility). Lands
SP-4 §4 S3 (verify side).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S3.3: Audit CLI and other consumers

**Files:**
- Audit: `crates/cli/src/**`, `crates/ext/**/*`, `crates/machine/tests/**`, `examples/**/src/**`

- [ ] **Step 1: Grep for every external consumer of the removed shape**

Run (manually, not via `TabulaRuntime::prove` in the runtime crate itself):

```
grep -rn 'TabulaRuntime::prove\|\.prove(&tabula_runtime::\|prepare_runtime.*prove\|runtime\.prove(' crates examples
```

Record each hit. Expected hits outside `crates/runtime/**`:

- `crates/sdk/src/program/runner.rs:322` — already migrated in S3.1.
- `crates/cli/**` — `crates/cli/src/commands/prove.rs` routes through `loaded.program.runner().prove(&receipt)` (line 21) — the SDK `Runner::prove` — this is transparent, already migrated.
- Anything else: migrate it inline. Replace any direct `TabulaRuntime::prove` call with `prepare_prover(registered)?.prove(...)` (or the SDK `prepare_prover` interop fn where an `Sdk` is in scope).

- [ ] **Step 2: Assert zero external callers remain**

```
grep -rn 'TabulaRuntime::prove\|\.prove(&tabula_runtime::' crates examples | grep -v '^crates/runtime/'
```

Expected output: empty.

- [ ] **Step 3: Grep for removed verify method**

```
grep -rn '\.verify_public_statement(' crates examples | grep -v '^crates/runtime/' | grep -v '^crates/sdk/src/program/verifier.rs'
```

Investigate each remaining hit and migrate to `PreparedVerifier::verify` if the call is against the runtime `TabulaRuntime` shape. (SDK's own `Verifier::verify_public_statement` is a different method that we're keeping.)

- [ ] **Step 4: Invariants + commit any migrations**

```bash
git add -A
git status  # sanity-check what's staged
git commit -m "$(cat <<'EOF'
refactor: migrate remaining prove/verify call sites to prepared handles

Migrates all non-runtime-crate callers of TabulaRuntime::prove and
verify_public_statement to PreparedProver::prove and
PreparedVerifier::verify. Leaves TabulaRuntime::prove in place as
an internal delegator (deleted in S4). Lands SP-4 §4 S3 audit.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

(If no callers required changes, skip the commit.)

**Gate:** Global invariants (1)-(5) pass. Grep for external callers returns empty.

---

## Stage S4 — Delete `TabulaRuntime::prove` and Duplicate Verify Path

**Goal:** Narrow `TabulaRuntime` to the execute-only surface; eliminate the `VerifierCore` helper as a now-orphan duplication.

### Task S4.1: Remove prove surface from `TabulaRuntime`

**Files:**
- Modify: `crates/runtime/src/engine.rs`
- Modify: `crates/runtime/src/lib.rs` (doc comment)
- Modify: `crates/runtime/tests/architecture_dependencies.rs` (assertions)

- [ ] **Step 1: Delete `TabulaRuntime::prove`, `prove_and_verify`, `execute_and_prove`, `verify_public_statement`**

In `engine.rs`:

- Delete lines ~830-842 (`pub fn prove`).
- Delete lines ~860-871 (`pub fn prove_and_verify`).
- Delete lines ~875-888 (`pub fn execute_and_prove`).
- Delete lines ~845-857 (`pub fn verify_public_statement`). Callers switched to `PreparedVerifier::verify` in S3.

- [ ] **Step 2: Drop the `root_backend_bundle` field and `kit_registry` field**

`TabulaRuntime` no longer proves, so it does not need the root backend bundle or chip-kit registry. In `engine.rs`:

```rust
pub struct TabulaRuntime {
    runtime_program: PreparedRuntimeState,
    machine: TabulaMachine,
}
```

(Both `#[cfg(feature = "prove")]` fields `root_backend_bundle` and `kit_registry` are deleted.)

Update `RuntimeBuilder::build` to stop populating them, and drop the `#[cfg(feature = "prove")] root_backend_bundle: RootBackendBundle,` and the `#[cfg(not(feature = "prove"))] root_proof_backend: Arc<dyn RootProofBackend>,` fields on `RuntimeBuilder` entirely if they have zero remaining purpose. (The `with_root_backend_bundle` / `with_root_proof_backend` / `with_root_proof_backend_arc` methods on `RuntimeBuilder` correspondingly go away.)

> **Why this is safe.** Execute-only `TabulaRuntime` does not touch root-proof backends: `execute_batch`, `execute_batch_receipt`, `execute_query`, `materialize_logical_state`, `decode_committed_snapshot`, `project_logical_state`, `empty_state_snapshot` — none reference the bundle today. A grep inside the bodies of these methods in `engine.rs` confirms zero uses; perform this grep before deletion.

```
grep -n 'root_backend_bundle\|root_proof_backend' crates/runtime/src/engine.rs
```

Expected after deletion: zero hits in `engine.rs` outside the `build_prepared_runtime` / `prepare_proof_request_on_prepared_state` free functions (those are prove-side and remain).

- [ ] **Step 3: Update `architecture_dependencies.rs`**

The `native_proof_path_stays_bridge_free` test (around line 218) already asserts the absence of `"struct VerifierCore"`, `"pub struct VerifierBuilder"`, `"pub struct Verifier {"` in `engine.rs` — those assertions stay and remain accurate. Extend with:

```rust
assert_source_omits(
    "crates/runtime/src/engine.rs",
    &[
        "fn prove(&self, input: &ProveInput",
        "pub fn prove_and_verify(",
        "pub fn execute_and_prove(",
        "pub fn verify_public_statement(",
    ],
);
```

Add this as a new test or extend `native_proof_path_stays_bridge_free`. Also update the re-export check so `pub use engine::{ProveInput, ProveResult, VerifiedResult};` remains valid — those types stay in `engine.rs` as shared vocabulary (both `TabulaRuntime` and `PreparedProver` refer to them).

- [ ] **Step 4: Invariants + commit**

```bash
git add crates/runtime
git commit -m "$(cat <<'EOF'
refactor(runtime): narrow TabulaRuntime to execute-only facade

Removes TabulaRuntime::prove, prove_and_verify, execute_and_prove,
and verify_public_statement (callers migrated in S3). Drops the
prove-only root_backend_bundle and kit_registry fields, shrinking
TabulaRuntime to the execute surface that PreparedProver cannot
express. Lands SP-4 §4 S4. Byte-identical proofs confirmed.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S4.2: Inline `VerifierCore` into `PreparedVerifier::verify`

**Files:**
- Modify: `crates/runtime/src/verifier.rs`
- Modify: `crates/runtime/tests/architecture_dependencies.rs`

- [ ] **Step 1: Fold `VerifierCore` + `verify_public_statement_with_context` into `PreparedVerifier`**

After S4.1, the only caller of `verify_public_statement_with_context` is gone (it was invoked from `TabulaRuntime::verify_public_statement`, now deleted). `VerifierCore` has exactly one other caller: `VerifierState::verifier_core`.

Inline the logic directly into `PreparedVerifier::verify`:

1. Delete the `VerifierCore` struct and its impl block (lines ~297-349 of the current verifier.rs).
2. Delete `verify_public_statement_with_context` (lines ~351-364).
3. Delete `VerifierState::verifier_core` helper.
4. Move the body of `VerifierCore::verify_public_statement` directly into `PreparedVerifier::verify`, adapting the signature to return `Result<BoundStatement, RuntimeError>`:

```rust
impl PreparedVerifier {
    /// Verify one native proof against an externally supplied expected public
    /// statement and return the artifact-bound statement on success.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected_public_statement: &PublicStatement,
    ) -> Result<BoundStatement, RuntimeError> {
        let bound = BoundStatement::new(
            self.prepared.context.clone(),
            expected_public_statement.clone(),
        );
        let expected_binding_digest =
            bound
                .binding_digest()
                .map_err(|error| RuntimeError::StatementBuild {
                    detail: error.to_string(),
                })?;
        if proof.binding_digest != expected_binding_digest {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof binding digest does not match the artifact-bound public statement"
                    .to_string(),
            });
        }
        verify_proved_public_statement_digests(
            proof,
            &self.prepared.machine,
            expected_public_statement,
        )?;
        match relation_table_root_from_proof(proof, &self.prepared.machine)? {
            Some(root) if self.prepared.relation_policy.requires_artifact_root() => {
                if root != self.prepared.context.static_table_root {
                    return Err(RuntimeError::ValidationFailed {
                        detail: "relation table chip root does not match the verifier artifact"
                            .to_string(),
                    });
                }
            }
            None if self.prepared.relation_policy.requires_artifact_root() => {
                return Err(RuntimeError::ValidationFailed {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        BackendVerifier::new(&self.prepared.machine)
            .verify_proof(proof)
            .map_err(RuntimeError::Verification)?;
        Ok(bound)
    }
}
```

5. The `relation_table_root_from_proof` helper (line 179+) stays — it's used by extension code elsewhere in the crate (grep to confirm).

Run: `grep -n 'relation_table_root_from_proof\|execution_chip_digest_from_proof\|verify_proved_public_statement_digests' crates/runtime/src/`. All internal callers should now be `verifier.rs` itself. If anything else calls these helpers, keep them `pub(crate)` as today.

- [ ] **Step 2: Update `architecture_dependencies.rs`**

The `verifier_path_is_single_sourced_in_verifier_module` test (around line 241) asserts `"struct VerifierCore"`, `"pub struct VerifierBuilder"`, `"pub struct Verifier"` all exist in `verifier.rs`. After S1 we already flipped two of those to `PreparedVerifier`/`PreparedVerifierBuilder`. After S4.2 `VerifierCore` is gone. Update:

```rust
assert!(
    verifier_source.contains("pub struct PreparedVerifierBuilder")
        && verifier_source.contains("pub struct PreparedVerifier"),
    "runtime verifier module must own the canonical verification path"
);
```

Drop the `VerifierCore` string assertion. (It is now absent by design.)

Also update the `native_proof_path_stays_bridge_free` test (line 218 area) — it checks that `"struct VerifierCore"` is **absent** from `engine.rs`. Still correct, no change needed; but note that the assertion now reads "absent-on-purpose-after-S4" rather than "absent because it lives in verifier.rs". Add a comment above the relevant assertion in the test explaining this, so future readers don't get confused:

```rust
// VerifierCore was deleted in SP-4 S4 after the duplicate verify
// path through TabulaRuntime was removed. This assertion remains
// as a guard against accidental reintroduction in engine.rs.
```

- [ ] **Step 3: Invariants + commit**

```bash
git add crates/runtime
git commit -m "$(cat <<'EOF'
refactor(runtime): inline VerifierCore into PreparedVerifier::verify

After S4.1 removed TabulaRuntime::verify_public_statement, the
VerifierCore helper had a single caller. Inlines the body into
PreparedVerifier::verify and deletes the now-dead
verify_public_statement_with_context entrypoint. Single verification
path now lives on PreparedVerifier; byte-identical proofs
confirmed. Completes SP-4 §4 S4.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

**Gate:** Global invariants (1)-(5) pass.

---

## Stage S5 — Documentation and Landing

**Goal:** Canonical docs describe the two-handle shape; umbrella marks SP-4 landed; stale notes are corrected.

### Task S5.1: Rewrite `crates/runtime/README.md` runtime-surface section

**Files:**
- Modify: `crates/runtime/README.md`

- [ ] **Step 1: Read the current README**

Run `cat crates/runtime/README.md`. Identify the section describing `Verifier` + `TabulaRuntime::prove`.

- [ ] **Step 2: Rewrite to describe the two-handle shape**

Replace the relevant section(s) with prose introducing:

- `PreparedVerifier` via `prepare_verifier(reg)` or the builder; verify-only, returns `BoundStatement`.
- `PreparedProver` via `prepare_prover(reg)` or the builder; prove-only; `Send + Sync`, cheap to share.
- `TabulaRuntime` as execute-only facade for `execute_batch*`, `execute_query`, state snapshot helpers.
- Feature gating: `verify` → `PreparedVerifier` + `TabulaRuntime`; `prove` → adds `PreparedProver`.

Keep the tone short and pointer-heavy. Do not duplicate `docs/design/architecture.md`.

- [ ] **Step 3: Commit**

```bash
git add crates/runtime/README.md
git commit -m "$(cat <<'EOF'
docs(runtime): describe the two-handle prepared shape

Rewrites the README runtime-surface section to describe
PreparedProver / PreparedVerifier and the narrowed
TabulaRuntime execute-only facade. Lands SP-4 §4 S5 docs.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S5.2: Update `docs/design/architecture.md` runtime section

**Files:**
- Modify: `docs/design/architecture.md`

- [ ] **Step 1: Locate the runtime section**

Run: `grep -n '^##' docs/design/architecture.md` to find the runtime section heading. Read the current prose and dependency diagram.

- [ ] **Step 2: Update prose + any diagram references**

Replace mentions of `Verifier` with `PreparedVerifier`. Replace mentions of `TabulaRuntime::prove` with `PreparedProver::prove`. Describe `TabulaRuntime` as execute-only facade. Update the "prepare once, drive many" language to make the two handles explicit.

Preserve the layer-dependency rule: runtime depends on contract + chips + machine + ext; the refactor did not change any cross-crate dependency direction.

- [ ] **Step 3: `cargo doc` clean**

Run: `cargo doc --workspace --no-deps --all-features 2>&1 | tee /tmp/cargo-doc.log`

Grep the log for warnings: `grep -E 'warning|error' /tmp/cargo-doc.log`. Expected: no warnings attributable to SP-4-authored items. (Pre-existing warnings unrelated to SP-4 may stand if and only if they existed before SP-4 started; record them in the commit message if they do.)

- [ ] **Step 4: Commit**

```bash
git add docs/design/architecture.md
git commit -m "$(cat <<'EOF'
docs(architecture): reflect SP-4 prepared-handle shape

Updates the runtime section of docs/design/architecture.md to
describe PreparedProver / PreparedVerifier as the canonical
prepare-once handles, and TabulaRuntime as the execute-only
facade whose final disposition is owned by SP-5.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

### Task S5.3: Mark SP-4 landed in umbrella and note stale references

**Files:**
- Modify: `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md`
- Modify: `docs/superpowers/specs/2026-04-19-sp4-runtime-prepared-handles-design.md`
- Audit: `docs/notes/**`

- [ ] **Step 1: Append an SP-4 Landed amendment to the SP-4 design spec**

At the bottom of `docs/superpowers/specs/2026-04-19-sp4-runtime-prepared-handles-design.md`, append:

```markdown
---

## SP-4 Landed Notes (2026-04-19)

Implementation landed per stages S0–S5. Summary of material deviations
from the original spec §3/§4:

- `PreparedVerifier::verify` returns `Result<BoundStatement, RuntimeError>`
  rather than spec §1's `Result<BoundStatement, VerifyError>`; there
  is no separate `VerifyError` type — `RuntimeError` continues to
  cover both prove and verify errors per spec §3.4's "No new error
  hierarchy." This matches `PreparedProver::prove`'s
  `Result<ProveResult, RuntimeError>`.
- The SDK's internal method is spelled
  `Sdk::prepare_prepared_prover` / `prepare_prepared_verifier` to
  avoid collision with the runtime free functions
  `tabula_runtime::prepare_prover` / `prepare_verifier` at every
  import site. External SDK interop surface exposes both under the
  natural names `interop::prepare_prover` / `interop::prepare_verifier`.
- `TabulaRuntime` lost its `root_backend_bundle` field entirely in S4
  (not just its `prove` method). The execute surface never consumed
  this field, so carrying it would have been dead state.
- `PreparedRuntimeState` was introduced as a `pub(crate)` newtype
  factored from the old private `RuntimeProgramState`. Both
  `TabulaRuntime` and `PreparedProver` embed it by value.

### Known follow-ups

- `prepare_verifier` vs the verifier builder — SP-4 ships both. SP-6
  call on whether to drop the builder in favor of an options struct.
- `TabulaRuntime` facade disposition (rename / fold into execute
  module / leave) — SP-5 owns.
- Stale `docs/notes/*.md` references that pre-date SP-4 may still use
  `Verifier` / `TabulaRuntime::prove`; authority lives in `docs/design/`
  and crate READMEs now.
```

- [ ] **Step 2: Mark SP-4 in the umbrella**

In `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md` §4 SP-4, prepend:

```markdown
> **Status: Landed 2026-04-19.** See
> [SP-4 design spec](2026-04-19-sp4-runtime-prepared-handles-design.md)
> and its Landed Notes section for details.
```

- [ ] **Step 3: Audit `docs/notes/` for stale references**

Run:

```
grep -rn 'Verifier\b\|TabulaRuntime::prove\|runtime::Verifier' docs/notes
```

For each hit, decide: (a) add a one-line footnote pointing at the current surface, or (b) update the note inline if it is small. Do not rewrite notes wholesale — `docs/notes/` is non-authoritative per `CLAUDE.md`. A minimal "superseded by SP-4" footnote is enough.

- [ ] **Step 4: Remove the byte-identity gate script and reference directory**

SP-4 is landed. The gate's job is done. Remove:

```
rm -rf s0-reference/
git rm scripts/check-proof-byte-identity.sh scripts/run-example.sh  # only if the latter was new in S0
# leave .gitignore entry in place — harmless
```

Remove the `/s0-reference/` line from `.gitignore` if SP-5 will not reuse the pattern. (If unsure, leave the ignore entry; it costs nothing.)

- [ ] **Step 5: Final invariant sweep**

```
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps --all-features
```

All pass. No new `missing_docs` warnings.

- [ ] **Step 6: Commit**

```bash
git add docs scripts .gitignore
git rm -f s0-reference/**  # if not already removed in step 4
git commit -m "$(cat <<'EOF'
docs(sp4): mark SP-4 landed; remove byte-identity gate scaffolding

Appends Landed Notes to the SP-4 design spec summarizing material
deviations from the original design, marks SP-4 Landed in the
architecture-refactoring umbrella, audits docs/notes for stale
runtime-surface references, and removes the S0 byte-identity gate
scaffolding (script + reference snapshot) now that SP-4 is landed.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

**Gate:** Global invariants (1)–(3) pass (the byte-identity gate is intentionally decommissioned at this point); `cargo doc` clean; umbrella shows SP-4 Landed.

---

## Success Criteria

SP-4 is considered complete when **all** of the following hold:

### Surface

- [ ] `tabula_runtime::PreparedVerifier`, `PreparedVerifierBuilder`, `prepare_verifier`, `VerifierState` all exist and are `pub` under feature `verify`.
- [ ] `tabula_runtime::PreparedProver`, `PreparedProverBuilder`, `prepare_prover` all exist and are `pub` under feature `prove`.
- [ ] `tabula_runtime::TabulaRuntime` carries only execute-surface methods (`execute_batch`, `execute_batch_receipt`, `execute_query`, `materialize_logical_state`, `decode_committed_snapshot`, `project_logical_state`, `empty_state_snapshot`, plus read-only accessors). No `prove`, no `prove_and_verify`, no `execute_and_prove`, no `verify_public_statement`.
- [ ] `tabula_runtime::Verifier` / `VerifierBuilder` no longer exist (removed, not re-exported).

### Behaviour

- [ ] `basic` + `membership` example proofs are byte-identical to the pre-SP-4 snapshot.
- [ ] `PreparedProver::prove(&self, input)` called twice on the same handle with the same input produces byte-identical proofs. Covered by `prover.rs::tests::prove_twice_on_same_handle_is_byte_identical`.
- [ ] `KitScratch` is freshly allocated per prove call (confirmed by reading `prepare_proof_request_on_prepared_state` — the scratch is a local, not a handle field).
- [ ] `ChipKitRegistry` is constructed once per `PreparedProver::build` (confirmed by reading `PreparedProverBuilder::build`).

### Types

- [ ] `PreparedProver: Send + Sync` enforced by `const _: fn() = ...` assertion in `prover.rs`.
- [ ] `PreparedVerifier: Send + Sync` and `VerifierState: Send + Sync` enforced similarly in `verifier.rs`.
- [ ] No `#[allow(unused)]`, no `static_assertions` dependency added.

### Tests

- [ ] `cargo test --workspace --all-features` passes.
- [ ] `crates/runtime/tests/architecture_dependencies.rs` asserts the new re-exports and absence of the old names.
- [ ] SDK cache tests for `prepare_prepared_prover` cover the happy path + failure-no-poison + poisoning path.

### Quality

- [ ] `cargo fmt --all -- --check` clean.
- [ ] `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean.
- [ ] `cargo doc --workspace --no-deps --all-features` clean (no new `missing_docs` warnings).

### Docs

- [ ] `crates/runtime/README.md` describes the two-handle shape.
- [ ] `docs/design/architecture.md` runtime section reflects the change.
- [ ] SP-4 design spec carries a Landed Notes section.
- [ ] Umbrella spec shows SP-4 Landed.

### Deltas not included

- Per spec §6: no engine.rs decomposition (SP-5), no RegisteredProgram rename (SP-1 follow-up), no SDK cache removal (SP-6), no wire format changes, no builder-knob pruning (SP-6), no new error hierarchy.

---

## Self-Review Checklist

Before starting execution, the plan was reviewed against the spec:

- **§1 goals**: each of the five goal bullets has a concrete owning stage (VerifierState public → S1; PreparedVerifier → S1; PreparedProver → S2; Send+Sync → S1/S2 static asserts; TabulaRuntime shrink → S4).
- **§2 resolved decisions**: 2.1 (execute facade survives) → S4; 2.2 (RegisteredProgram unchanged) → respected throughout; 2.3 (four knobs retained) → S1 preserves on `PreparedVerifierBuilder`, S2 preserves on `PreparedProverBuilder`; 2.4 (Send+Sync) → S1/S2 static asserts; 2.5 (SP-3 boundary) → S2.2 step 6 describes the per-call KitScratch contract explicitly; 2.6 (VerifierState public) → S1 task 1.
- **§4 stages**: S1–S5 map 1:1 to this plan's S1–S5 with identical numbering.
- **§5 verification**: byte-identity gate is global invariant (4). Double-prove test is S2.2 step 10.
- **§6 non-goals**: "Deltas not included" section above lists them.
- **§8 risks**: silent hot-path perturbation → gate (4); Send+Sync regression → static asserts; call-site churn → S1 done in a single commit + atomic SDK shim in S1.6; drift from SP-3 → S2.2 explicit &self signature + double-prove test.

No placeholders remain except the three `# unimplemented!()` stubs in S2.2 step 7 items 2, which are explicitly annotated as copy-from-engine.rs-line-NNNN instructions — the full text already lives in the existing source at the cited lines.

No type or method-name drift identified: `PreparedVerifier`, `PreparedProver`, `PreparedRuntimeState`, `build_prepared_runtime`, `build_chip_kit_registry`, `prepare_proof_request_on_prepared_state` are used consistently across tasks.

---

Plan complete. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
