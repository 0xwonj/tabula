# Tabula Code Architecture Review

**Date:** 2026-02-13
**Scope:** Entire workspace (6 crates, ~15,400 LOC, 56 `.rs` files)
**References:** SP1, OpenVM, Valida, Plonky3, Rust best practices

---

## 1. Current State Summary

### 1.1 Workspace Layout

```
tabula/
  Cargo.toml              # virtual manifest, workspace.dependencies
  crates/
    tabula-core/          # types, IR, traits, errors, events       (1,900 LOC)
    tabula-executor/      # batch execution engine                  (4,200 LOC)
    tabula-commitment/    # field-level crypto (SMT, SSMC, Poseidon)(2,300 LOC)
    tabula-proof/         # witness gen + AIR constraints            (2,400 LOC)
    tabula-lang/          # DSL compiler (lex → parse → lower)      (3,600 LOC)
    tabula-cli/           # CLI entry point + JSON I/O              (700 LOC)
```

### 1.2 Dependency DAG

```
tabula-core            (no deps)
  ├→ tabula-executor
  ├→ tabula-commitment
  ├→ tabula-lang
  └→ tabula-proof → tabula-commitment
tabula-cli → tabula-core + tabula-executor
```

Clean, acyclic. No circular dependencies.

### 1.3 Files Over 400 Lines

| File | LOC | Nature |
|------|-----|--------|
| `lang/lower.rs` | 1,550 | AST → IR lowering |
| `executor/program.rs` | 1,529 | NF validation + type inference |
| `proof/witness.rs` | 1,160 | Witness generation orchestrator |
| `lang/parser.rs` | 984 | Recursive descent parser |
| `executor/interpreter.rs` | 834 | Instruction execution loop |
| `executor/batch.rs` | 553 | Batch orchestrator |
| `lang/lexer.rs` | 522 | Tokenizer |
| `commitment/hybrid.rs` | 521 | Hybrid VC dispatcher |
| `core/types.rs` | 502 | Value enum + arithmetic |
| `commitment/ssmc.rs` | 483 | SSMC implementation |
| `executor/overlay.rs` | 447 | Write-buffer + read-cache |
| `core/mock.rs` | 414 | Mock implementations |

---

## 2. Reference Project Comparison

### 2.1 Workspace Scale & Organization

| Project | Crates | LOC | Layout | Naming |
|---------|--------|-----|--------|--------|
| **Tabula** | 6 | 15K | `crates/` | `tabula-{name}` |
| **Plonky3** | 36 | ~60K | flat | `p3-{name}` |
| **Valida** | 18 | ~30K | flat | bare names |
| **SP1** | ~21 | ~150K | nested (`core/executor`, `recursion/circuit`) | `sp1-{name}` |
| **OpenVM** | ~71 | ~300K | `crates/` + `extensions/` | `openvm-{name}` |

**Tabula observation:** 6 crates / 15K LOC is appropriate for current scale. No need to proliferate crates yet. Plonky3 and Valida show that flat `crates/` layout scales well to 30-60K LOC.

### 2.2 Key Patterns Across Projects

#### Crate Separation Strategy

| Concern | SP1 | OpenVM | Valida | Tabula |
|---------|-----|--------|--------|--------|
| Core types | `sp1-stark` | `openvm-circuit-primitives` | `machine/` | `tabula-core` |
| Executor | `sp1-core-executor` | `openvm-circuit` (VM) | `cpu/` | `tabula-executor` |
| Prover | `sp1-prover` | `openvm-sdk` | `verifier/` | `tabula-proof` |
| Chips | embedded in `sp1-core-machine` | per-extension crates | per-chip crates (`alu_u32/`) | `tabula-proof/air/chips/` |
| SDK/CLI | `sp1-sdk` | `openvm-cli` | N/A | `tabula-cli` |

**Key insight:** SP1 separates executor and machine (chips) into two sub-crates under `core/`. Valida goes further — each chip is its own crate. OpenVM uses `extensions/` directories. **Tabula's current approach (chips as modules in `tabula-proof`) is correct for 1-5 chips. When chips exceed ~8, consider a `tabula-chips` crate or per-chip module directories.**

#### Trait Design Philosophy

| Project | Style | Trait Count | Associated Types | Object Safety |
|---------|-------|-------------|------------------|---------------|
| **SP1** | Fat traits + defaults | `MachineAir` (11 methods, 3 required) | `Record`, `Program` | No (generic) |
| **OpenVM** | Thin composable (SubAir) | Many small traits | GAT `AirContext` | No |
| **Valida** | Concrete, minimal | `Machine` (4 methods), `Chip` (5 methods) | None | HRTB bounded |
| **Plonky3** | Deep hierarchy | `Field` 5-level hierarchy, `AirBuilder` | 4 associated types | Mixed |
| **Tabula** | Thin, object-safe | 8 traits, 1-4 methods each | `FieldRepr`, `Proof` | Yes (dyn-compatible) |

**Tabula observation:** Current trait design is closest to Valida's simplicity. This is good — Tabula's traits are used primarily as injection points (executor doesn't care about crypto), not as extension points (SP1/OpenVM). **Keep traits thin and object-safe for now.** The `BatchEnv` pattern (4 `dyn` trait objects) works well at this scale.

#### Error Handling

| Project | Library Crate | App/SDK | Crate |
|---------|--------------|---------|-------|
| SP1 | Per-domain enums | `anyhow` | thiserror + anyhow |
| OpenVM | Per-domain enums | Custom | Manual Display |
| Valida | Minimal enums | — | Manual Display |
| Plonky3 | Per-crate enums | — | thiserror |
| **Tabula** | **Single monolithic enum** | `anyhow` (CLI) | thiserror |

**Key difference:** All reference projects use **per-domain error types** (e.g., SP1 has `MachineVerificationError`, `ExecutionError` separately). Tabula uses a single `TabulaError` with 22+ variants spanning execution, NF validation, encoding, consistency. See §3.2 for recommendation.

#### Feature Flags

| Pattern | Used By | Tabula Status |
|---------|---------|---------------|
| `stark` gate for Plonky3 deps | SP1, OpenVM | **Already done** |
| `mock` / `test-utils` for test infra | SP1, OpenVM | **Already done** (`mock` feature) |
| `debug` for constraint debugging | SP1 | Not yet (could add) |
| `parallel` for rayon | Plonky3 (`p3-maybe-rayon`) | Not yet needed |
| `no_std` baseline | Plonky3, OpenVM guest | Not applicable |

#### Testing

| Pattern | Used By | Tabula Status |
|---------|---------|---------------|
| Co-located `#[cfg(test)]` | All | **Done** |
| Property-based (proptest) | Plonky3, Tabula-executor | **Done** (executor) |
| Dedicated test utility crate | Plonky3 (`p3-field-testing`) | Not needed at scale |
| Debug constraint checker | SP1, OpenVM, Valida | **Done** (`debug.rs`) |
| Separate integration test crates | OpenVM | Not needed yet |
| Criterion benchmarks | All | **Not yet** — add when performance matters |

---

## 3. Recommendations

### 3.1 File Splitting (Priority: Medium)

Three files warrant extraction:

#### A. `executor/program.rs` (1,529 LOC) → split into 2

Currently mixes two concerns:
1. **Registration + type inference** (`compile_body`, `infer_slot_type`, `infer_numeric_result`) — ~700 LOC
2. **Normal-form validation** (`validate_normal_form`, `check_nf1..4`, helper structs) — ~700 LOC

```
program.rs           (registration, BodyTypeInfo, Program struct)  ~500 LOC
program/validate.rs  (NF-1..4 validation logic)                   ~700 LOC
program/infer.rs     (type inference: compile_body, infer_*)       ~400 LOC
```

**Reference:** SP1 separates program validation from execution in different crates entirely. At Tabula's scale, sub-modules within `executor/` are sufficient.

#### B. `proof/witness.rs` (1,160 LOC) → split into 2

Currently mixes:
1. **WitnessGenerator orchestration** (`generate()` main flow) — ~400 LOC
2. **Per-column witness building** (`build_column_witness`, `build_init_rows`, `build_access_rows`) — ~500 LOC
3. **Type map + helpers** — ~200 LOC

```
witness.rs           (WitnessGenerator, generate() orchestration)  ~400 LOC
witness/column.rs    (build_column_witness, init/access row logic) ~500 LOC
```

#### C. `lang/lower.rs` (1,550 LOC) — acceptable as-is

Compiler lowering is inherently a single pass. SP1's and OpenVM's equivalents are similarly monolithic. Breaking it apart creates coupling without benefit. **Keep but add section comments.**

### 3.2 Error Type Restructuring (Priority: Medium-High)

**Problem:** `TabulaError` is a 22-variant monolith. Every crate depends on it, but most variants are only produced by one crate:

| Variant Group | Producer | Consumer |
|---|---|---|
| `NfUniqueRead/Write/ReadAfterWrite/AmbiguousAlias` | executor (program.rs) | executor |
| `ArithmeticOverflow/DivisionByZero/SlotOutOfBounds` | executor (interpreter) | executor |
| `InvalidNonce/SignatureInvalid` | executor (batch) | executor |
| `TableNotFound/ColumnNotFound/CellNotFound` | multiple | multiple |
| `ConsistencyError/EncodingError` | proof, commitment | proof |
| `InvalidIr` | executor, lang | all |

**Recommendation:** Introduce per-domain error enums that compose into `TabulaError`:

```rust
// core/error.rs — keep as unified boundary error
pub enum TabulaError {
    State(StateError),       // TableNotFound, ColumnNotFound, CellNotFound, RowNotFound
    Type(TypeError),         // TypeMismatch, ArithmeticOverflow, DivisionByZero
    Execution(ExecError),    // SlotOutOfBounds, ParamOutOfBounds, AssertionFailed
    Validation(NfError),     // NF-1..4, InvalidIr, ParamSchemaMismatch
    Auth(AuthError),         // InvalidNonce, SignatureInvalid
    Encoding(String),        // EncodingError, ConsistencyError
    Custom(String),
}
```

Each sub-enum gets its own `thiserror` derive. `From` impls allow ergonomic `?` propagation. This follows the **SP1/Plonky3 pattern** of per-domain errors.

**Trade-off:** More types to define, but better error locality and exhaustive matching per domain. Existing code using `TabulaError` continues to work via `From` impls.

### 3.3 Free Functions vs Methods (Priority: Low)

**Current pattern:** Most logic is in free functions (`execute_batch()`, `classify_keys()`, `generate_column_meta_trace()`).

**Reference projects:**
- SP1: `MachineAir::generate_trace(&self, ...)` — methods on chip structs
- Valida: `Machine::run(&mut self, ...)` — methods on machine struct
- OpenVM: Mix of methods and builder functions

**Assessment:** Tabula's free-function style is fine for stateless operations. The key stateful types (`Program`, `Overlay`, `WitnessGenerator`) already use methods. No action needed.

**One exception:** `build_column_witness()` and related helpers in `witness.rs` are private functions that only operate on `WitnessGenerator`'s data. If witness.rs is split (§3.1B), consider making them methods or moving to a `ColumnWitnessBuilder` helper struct.

### 3.4 Re-export Strategy (Priority: Low)

**Current:** `tabula-core` exposes all modules as `pub mod`. Users write `tabula_core::types::Value`.

**Reference:**
- Plonky3: `pub use module::*` in every `lib.rs` (flat namespace)
- SP1: Same, wildcard re-exports
- Valida: Selective re-exports

**Recommendation:** Keep current approach (explicit module paths). At 15K LOC with 6 crates, namespaced imports are clearer than flat re-exports. Revisit if external API surface becomes a concern.

### 3.5 Workspace Configuration (Priority: Low)

**Already good:**
- `workspace.dependencies` for version alignment
- `workspace.package` for metadata
- `resolver = "2"`
- `edition = "2024"`

**Minor improvement:** Add `workspace.lints` for shared lint configuration:

```toml
[workspace.lints.rust]
unused = "deny"

[workspace.lints.clippy]
all = "warn"
```

Then in each crate's `Cargo.toml`: `[lints] workspace = true`. This replaces the per-crate `#![deny(unused)]` attributes.

### 3.6 Testing Gaps (Priority: Medium)

| Area | Current | Recommended |
|------|---------|-------------|
| CLI commands | 4 unit tests | Add handler-level tests (mock executor) |
| Property tests | Executor only (proptest) | Add for commitment (SSMC merge, SMT update) |
| Benchmarks | None | Add criterion for witness gen + commitment |
| Fuzz testing | None | Consider for parser (lang crate) |

### 3.7 Future Crate Splits (Priority: Not Yet)

When the project grows beyond ~30K LOC, consider:

| Trigger | Action |
|---------|--------|
| >8 AIR chips in `tabula-proof` | Extract `tabula-chips` crate (SP1 `core-machine` pattern) |
| External SDK users | Extract `tabula-sdk` facade crate with curated re-exports |
| Multiple prover backends | Extract `tabula-prover` from `tabula-proof` (SP1 pattern) |

**Not now.** Premature crate splitting at 15K LOC adds coordination overhead without benefit.

---

## 4. Summary Matrix

| Concern | Current | SP1/OpenVM/Valida Best Practice | Gap | Priority |
|---------|---------|------|-----|----------|
| Crate structure | 6 crates, clean DAG | Appropriate for scale | None | — |
| File sizes | 12 files >400 LOC | Extract `program.rs`, `witness.rs` | Medium | **Medium** |
| Error handling | Monolithic `TabulaError` | Per-domain sub-enums | Significant | **Medium-High** |
| Trait design | 8 thin, object-safe traits | Good (matches Valida) | None | — |
| Feature flags | `stark`, `mock` | Add `workspace.lints` | Minor | **Low** |
| Testing | 225 tests, 2% density | Property tests for commitment, CLI tests | Medium | **Medium** |
| Re-exports | Module paths | Fine at current scale | None | — |
| Free functions | Stateless = free fn, stateful = methods | Correct | None | — |
| Chip organization | 3-file pattern in `air/chips/` | Matches SP1/Valida | None | — |
| Documentation | Architecture doc + chip guide | Add crate-level README where missing | Minor | **Low** |

### What NOT to do (confirmed by reference analysis)

1. **Don't add proc macros** (SP1's `MachineAir` derive) — overkill for <10 chips
2. **Don't introduce SubAir GATs** (OpenVM) — complex, not needed without composable chip fragments
3. **Don't split into per-chip crates** (Valida) — premature at current scale
4. **Don't add `no_std` support** — not a guest library
5. **Don't add a `maybe-rayon` toggle** — no parallelism pressure yet
6. **Don't flatten re-exports** — explicit module paths are clearer at this scale
