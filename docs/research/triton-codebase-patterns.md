# Triton VM Codebase Structure and Coding Patterns

> Analysis of the Triton VM codebase (https://github.com/TritonVM/triton-vm)
> focusing on workspace organization, trait design, table/AIR patterns,
> error handling, testing, and code quality. Evaluated for patterns
> applicable to Tabula.
>
> Repository: v2.0.0, 272 stars, Rust 2024 edition (1.89+), Apache-2.0/MIT
>
> Date: 2026-03-11

---

## 1. Workspace Organization

### Crate Layout (5 crates)

```
triton-vm/              (root workspace)
├── triton-isa/         ISA definitions: instructions, op stack, parser, program
├── triton-air/         AIR trait + constraint definitions (no runtime)
├── triton-constraint-circuit/  DAG representation of constraint polynomials
├── triton-constraint-builder/  Constraint compilation + code generation
└── triton-vm/          Prover/verifier, trace generation, STARK config
```

### Dependency DAG

```
triton-isa  ←── triton-air  ←── triton-constraint-builder
                    ↑                       ↑
            triton-constraint-circuit ──────┘
                    ↑
              triton-vm (depends on all four)
```

### Key Architectural Decision: AIR Separated from Trace

The `triton-air` crate contains **constraint definitions only** (symbolic
constraint circuits). The `triton-vm` crate contains **trace generation**
(filling tables with execution data). This separation means:

- Constraints are defined as symbolic DAGs, not runtime code
- The build script in `triton-vm` compiles constraints to Rust + TASM at build time
- The AIR crate has zero runtime dependencies (no ndarray, no rayon)

This is a clean separation that Tabula partially mirrors (chips define AIR in
`air.rs`, traces in `trace.rs`), but Triton goes further by putting AIR in a
separate crate with a build-time compilation step.

### Comparison with Tabula (16 crates)

Tabula's crate count (16) is higher, but the layering philosophy is similar:
foundation (core/ir) -> execution (executor) -> proving (stark/chips/witness)
-> orchestration (machine/driver). Triton is more monolithic in `triton-vm`
(prover, verifier, tables, traces all in one crate) while Tabula splits these
across `stark`, `chips`, `witness`, and `machine`.

**Applicable pattern**: Triton's separation of AIR definitions into a standalone
crate with no runtime deps is cleaner than Tabula's current approach where
`air.rs` and `trace.rs` live in the same `chips` crate. However, for Tabula's
scale (9 global + 5 shard chips), the in-crate separation works well enough.

---

## 2. Table/AIR Definition Patterns

### The Two-Trait Architecture

Triton uses two traits to define a table:

**`AIR` trait** (in `triton-air`, sealed):
```rust
trait AIR {
    type MainColumn: MasterMainColumn + EnumCount;
    type AuxColumn: MasterAuxColumn + EnumCount;

    fn initial_constraints(circuit_builder) -> Vec<ConstraintCircuitMonad>;
    fn consistency_constraints(circuit_builder) -> Vec<ConstraintCircuitMonad>;
    fn transition_constraints(circuit_builder) -> Vec<ConstraintCircuitMonad>;
    fn terminal_constraints(circuit_builder) -> Vec<ConstraintCircuitMonad>;
}
```

**`TraceTable` trait** (in `triton-vm`, extends AIR):
```rust
trait TraceTable: AIR {
    type FillParam;
    type FillReturnInfo;

    fn fill(main_table, aet, &Self::FillParam) -> Self::FillReturnInfo;
    fn pad(main_table, padded_height);
    fn extend(main_table, aux_table, &Challenges);
}
```

### Column Definition Pattern

Columns are defined as enums with derive macros:

```rust
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, EnumCount, EnumIter)]
pub enum ProcessorMainColumn {
    CLK, IP, CI, NIA, IB0, IB1, ..., ST0, ST1, ..., ST15, // 37 variants
}

pub enum ProcessorAuxColumn {
    InputTableEvalArg, OutputTableEvalArg, ..., // 11 variants
}
```

Two traits map enum variants to numeric indices:

```rust
trait MasterMainColumn {
    fn main_index(&self) -> usize;       // local to this table
    fn master_main_index(&self) -> usize; // global across all tables
}

trait MasterAuxColumn {
    fn aux_index(&self) -> usize;
    fn master_aux_index(&self) -> usize;
}
```

Global indices are computed by chaining table boundaries:
```
PROGRAM_TABLE_START = 0
PROCESSOR_TABLE_START = PROGRAM_TABLE_START + ProgramMainColumn::COUNT
OPSTACK_TABLE_START = PROCESSOR_TABLE_START + ProcessorMainColumn::COUNT
...
```

### Per-Table File Layout

Each table has TWO files in DIFFERENT crates:

```
triton-air/src/table/processor.rs    # AIR constraints (symbolic)
triton-vm/src/table/processor.rs     # Trace fill/pad/extend (runtime)
```

This is enforced by the crate boundary. The AIR file returns `Vec<ConstraintCircuitMonad>`
(symbolic DAGs), while the trace file operates on `ArrayViewMut2<BFieldElement>`.

### Constraint Expression Pattern

Constraints are built using a circuit builder with natural arithmetic syntax:

```rust
fn initial_constraints(circuit_builder: &ConstraintCircuitBuilder<SingleRowIndicator>)
    -> Vec<ConstraintCircuitMonad<SingleRowIndicator>>
{
    let clk = circuit_builder.input(MainRow(MainColumn::CLK.master_main_index()));
    let ip = circuit_builder.input(MainRow(MainColumn::IP.master_main_index()));

    let clk_is_0 = clk;
    let ip_is_0 = ip;

    vec![clk_is_0, ip_is_0, ...]
}
```

Transition constraints use `DualRowIndicator` with `CurrentMainRow` / `NextMainRow`
variants for accessing current and next row values.

### Comparison with Tabula

Tabula uses the Plonky3 `Air` trait with an imperative `AirBuilder`:
```rust
impl<AB: AirBuilder> Air<AB> for MyChip {
    fn eval(&self, builder: &mut AB) { ... }
}
```

Triton uses symbolic circuit DAGs compiled at build time. The tradeoffs:

| Aspect | Triton (symbolic DAG) | Tabula (imperative builder) |
|--------|----------------------|---------------------------|
| Constraint analysis | Full DAG available for CSE, degree analysis | Opaque to analysis |
| Code generation | Generates optimized Rust + TASM at build time | Single runtime path |
| Flexibility | Fixed constraint set (9 tables) | Dynamic chip registration |
| Build complexity | Requires build.rs, generated code | Direct compilation |

**Applicable pattern**: Triton's symbolic approach enables their 1790x CSE
optimization, but requires a fixed set of tables known at build time. Tabula's
extensibility model (register chips at runtime) is incompatible with build-time
constraint compilation. However, Tabula could adopt a symbolic intermediate form
for constraint analysis without requiring build-time compilation.

---

## 3. Trait Design

### Core Traits

| Trait | Location | Purpose |
|-------|----------|---------|
| `AIR` | triton-air | Constraint definitions (sealed) |
| `TraceTable` | triton-vm | Trace generation lifecycle |
| `MasterTable` | triton-vm | Master table operations (private) |
| `MasterMainColumn` | triton-air | Column index mapping (main) |
| `MasterAuxColumn` | triton-air | Column index mapping (aux) |
| `CrossTableArg` | triton-air | Cross-table argument protocol |
| `InputIndicator` | triton-constraint-circuit | Constraint variable positions (sealed) |
| `Evaluable` | triton-vm (generated) | Runtime constraint evaluation |
| `IntegralMemoryLayout` | triton-vm | Memory region validation |

### Sealing Pattern

Both `AIR` and `InputIndicator` are sealed traits:
```rust
mod private { pub trait Seal {} }
// Only internal types implement Seal, preventing external impls
```

This is deliberate: Triton has exactly 9 tables and 2 input indicators
(single-row, dual-row). The system is closed by design.

### Cross-Table Argument Trait

```rust
trait CrossTableArg {
    fn default_initial() -> XFieldElement;
    fn compute_terminal(symbols: &[BFieldElement], challenge: XFieldElement)
        -> XFieldElement;
}
```

Three implementations:
- `PermArg`: multiplicative accumulation (product of linear factors)
- `EvalArg`: Horner evaluation (polynomial evaluation at a point)
- `LookupArg`: additive inverse accumulation (sum of inverse differences)

### Comparison with Tabula

Tabula's trait design is more extensible:
- `ChipId(u16)` / `BusId(u16)` are open newtypes (vs sealed enums)
- `define_bus!` macro generates typed send/receive (vs manual argument setup)
- No sealed traits (chips can be registered at runtime)

**Applicable pattern**: Triton's `CrossTableArg` trait cleanly abstracts the
three argument types. Tabula's LogUp bus subsumes all three (LogUp generalizes
permutation and evaluation arguments), so this specific abstraction isn't needed.
However, the sealing pattern is worth noting as an explicit architectural decision.

---

## 4. Error Handling

### Error Type Organization

Triton uses `thiserror` throughout with a hierarchical error structure:

**In `triton-isa`**: Domain errors for ISA operations
- `InstructionError` (7 variants: IP overflow, assertion failure, etc.)
- `OpStackError`, `OpStackElementError`, `NumberOfWordsError`
- `ParseError`, `ProgramDecodingError`

**In `triton-vm`**: Proving system errors
- `ArithmeticDomainError` (2 variants)
- `ProofStreamError` (5 variants)
- `LdtParameterError` (7 variants)
- `LdtProvingError` (1 variant)
- `LdtVerificationError` (5 variants)
- `ProvingError` (3 variants, composes subtypes)
- `VerificationError` (5 variants)

**Special case**: `VMError` is a struct (not enum) wrapping `InstructionError`
plus the full `VMState` at crash time.

### Error Re-export Pattern

`triton-isa/src/error.rs` is a barrel file that re-exports errors from their
defining modules. `triton-vm/src/error.rs` re-exports ISA errors and defines
its own proving/verification errors. The prelude re-exports the user-facing
subset.

### Result Type Usage

No custom `Result` type alias. Functions return `Result<T, SpecificErrorType>`
with the specific error type, not a catch-all. For example:
```rust
fn prove() -> Result<Proof, ProvingError>
fn verify() -> Result<(), VerificationError>
fn padded_height() -> Result<usize, ProofStreamError>
```

### Comparison with Tabula

Tabula uses `ProveError` in the machine crate, similar to Triton's `ProvingError`.
Both projects use `thiserror`. Triton's error hierarchy is more granular
(separate types for LDT parameters, LDT proving, LDT verification vs a single
error type) which makes error handling more precise at call sites.

**Applicable pattern**: Triton's granularity is good practice. The `VMError`
pattern of capturing full machine state at crash time is particularly useful
for debugging.

---

## 5. Directory Structure

### triton-vm/src/ (main crate)

```
triton-vm/src/
├── lib.rs                  # Module declarations + top-level prove/verify functions
├── prelude.rs              # Wildcard import convenience module
├── vm.rs                   # VM execution + AET generation
├── aet.rs                  # AlgebraicExecutionTrace struct
├── stark.rs                # Stark config + Prover/Verifier entry points
├── proof.rs                # Claim + Proof structs
├── proof_item.rs           # ProofItem enum (13 variants, macro-generated)
├── proof_stream.rs         # Fiat-Shamir proof streaming
├── challenges.rs           # Challenge sampling + derivation
├── config.rs               # Runtime config (LDE caching)
├── constraints.rs          # Constraint evaluation (test module)
├── error.rs                # Error type aggregation
├── arithmetic_domain.rs    # Arithmetic domain (coset operations)
├── memory_layout.rs        # Memory regions for TASM evaluation
├── profiler.rs             # Macro-based profiling infrastructure
├── execution_trace_profiler.rs  # Per-table height profiling
├── ndarray_helper.rs       # ndarray utilities
├── example_programs.rs     # Test programs
├── shared_tests.rs         # Shared test infrastructure
├── table.rs                # TraceTable trait + module declarations
├── table/
│   ├── master_table.rs     # MasterMainTable + MasterAuxTable orchestration
│   ├── auxiliary_table.rs  # Evaluable trait (build-time generated)
│   ├── degree_lowering.rs  # Degree lowering table (build-time generated)
│   ├── program.rs          # Program table trace
│   ├── processor.rs        # Processor table trace
│   ├── op_stack.rs         # OpStack table trace
│   ├── ram.rs              # RAM table trace
│   ├── jump_stack.rs       # JumpStack table trace
│   ├── hash.rs             # Hash table trace
│   ├── cascade.rs          # Cascade table trace
│   ├── lookup.rs           # Lookup table trace
│   └── u32.rs              # U32 table trace
└── low_degree_test/
    ├── mod.rs              # LDT trait + dispatch
    ├── fri.rs              # FRI implementation
    └── stir.rs             # STIR implementation
```

### triton-air/src/ (AIR crate)

```
triton-air/src/
├── lib.rs               # AIR trait (sealed) + TARGET_DEGREE constant
├── challenge_id.rs      # ChallengeId enum (67 variants)
├── cross_table_argument.rs  # PermArg, EvalArg, LookupArg + GrandCrossTableArg
├── table.rs             # TableId enum + column count constants
├── table_column.rs      # Column enums + MasterMainColumn/MasterAuxColumn traits
└── table/
    ├── program.rs       # Program table AIR constraints
    ├── processor.rs     # Processor table AIR constraints
    ├── op_stack.rs      # OpStack table AIR constraints
    ├── ram.rs           # RAM table AIR constraints
    ├── jump_stack.rs    # JumpStack table AIR constraints
    ├── hash.rs          # Hash table AIR constraints
    ├── cascade.rs       # Cascade table AIR constraints
    ├── lookup.rs        # Lookup table AIR constraints
    └── u32.rs           # U32 table AIR constraints
```

### Observations

1. **Mirror structure**: `triton-air/src/table/*.rs` mirrors
   `triton-vm/src/table/*.rs` -- same 9 files, same names, different concerns.
2. **Flat modules**: No deep nesting. The deepest path is 2 levels (`table/processor.rs`).
3. **Build-generated code**: Two files (`auxiliary_table.rs`, `degree_lowering.rs`)
   include generated code via `include!` macro.
4. **No `mod.rs` pattern**: Uses Rust 2018+ file-based module resolution
   (except `low_degree_test/mod.rs`).

---

## 6. Testing Patterns

### Test Organization

1. **In-file unit tests**: Most files have a `#[cfg(test)] mod tests` section
   at the bottom. RAM table tests verify Bezout coefficients; AET tests verify
   trace recording; etc.

2. **Property-based testing**: Heavy use of `proptest` throughout:
   - Custom strategies via `#[derive(Arbitrary)]` on all public types
   - `test-strategy` crate for declarative test data generation
   - `proptest-regressions/` directory for regression cases

3. **Shared test infrastructure** (`shared_tests.rs`):
   - `TestableProgram`: Builder pattern for configuring test execution
   - `generate_proof_artifacts()`: Exposes intermediate state for unit tests
   - `DigestCorruptor`: Negative testing support

4. **Auto-trait verification**: Both `triton-air` and `triton-vm` have tests
   confirming all public types implement `Sized + Send + Sync + Unpin`.

5. **Snapshot testing**: Uses `insta` crate for snapshot-based assertions.

6. **Constraint uniqueness testing**: Schwartz-Zippel lemma evaluation to verify
   constraints are not trivially zero or duplicates of each other.

### Test Examples

```rust
// Property-based test for cross-table arguments
proptest! {
    fn evaluation_argument_is_polynomial_evaluation(
        symbols: Vec<BFieldElement>,
        initial: XFieldElement,
        challenge: XFieldElement,
    ) {
        let terminal = EvalArg::compute_terminal(&symbols, initial, challenge);
        // Compare against explicit polynomial evaluation
    }
}

// Auto-trait verification
fn _all_public_types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Claim>();
    assert_send_sync::<Proof>();
    // ... 23+ types checked
}
```

### Comparison with Tabula

Tabula uses standard `#[test]` functions and integration tests in `tests/`.
Triton's `proptest` usage is more extensive. The auto-trait verification pattern
is worth adopting -- it catches accidental `!Send` or `!Sync` introductions.

**Applicable patterns**:
- Auto-trait verification for all public types
- Constraint uniqueness testing via Schwartz-Zippel
- Builder-pattern test infrastructure (`TestableProgram` analog)

---

## 7. Configuration Patterns

### Stark Configuration

```rust
pub struct Stark {
    pub security_level: usize,
    pub log2_ldt_expansion_factor: usize,
    pub ldt_choice: LdtChoice,   // FRI or STIR
    pub soundness: Soundness,    // Proven or Conjectured
}
```

Provides `prove()` and `verify()` methods that delegate to `Prover` and `Verifier`
structs respectively. The `Stark` struct is the user-facing entry point;
`Prover`/`Verifier` are internal implementation details.

### Runtime Configuration

Minimal: a single `thread_local!` config controlling LDE trace caching:
```rust
fn overwrite_lde_trace_caching_to(decision: CacheDecision);
fn cache_lde_trace() -> Option<CacheDecision>;
```

Configurable via environment variable `TVM_LDE_TRACE=cache|no_cache` or
runtime API call.

### Domain Hierarchy

Four nested arithmetic domains derived from trace height:
1. `trace`: execution trace height (power of 2)
2. `randomized_trace`: 2x trace (zero-knowledge padding)
3. `quotient`: supports constraint polynomial degrees
4. `ldt`: expansion-factor scaled for low-degree testing

### Feature Flags

Single feature: `no_profile` (default enabled). When disabled, activates
macro-based profiling instrumentation. This avoids the typical `profile`
feature flag approach that forces all dependencies to opt in.

### Comparison with Tabula

Tabula's `StarkConfig` in the `stark` crate is similar but uses the Plonky3
configuration types. The feature-flag-for-profiling approach is worth noting
as a clean pattern.

---

## 8. Type System Usage

### Parameterized Instruction Type

```rust
pub enum AnInstruction<Dest: PartialEq + Default> {
    Push(BFieldElement),
    Call(Dest),
    // ... 45 variants
}
type Instruction = AnInstruction<BFieldElement>;  // runtime: numeric addresses
// Assembly time: AnInstruction<String>             // labels as strings
```

The `Dest` parameter enables the same enum to represent both assembly-time
(labeled) and runtime (resolved) instructions. The `map_call_address()` method
provides a functor for conversion.

### Enum-Based Column Indexing

All column indices are enum variants with `EnumCount` + `EnumIter` from `strum`.
This provides compile-time column counts (`Column::COUNT`) and iteration without
maintaining parallel constants.

### No Const Generics

Despite Rust 2024 edition, Triton does not use const generics for table widths
or stack depth. Instead:
- Table widths: `Column::COUNT` from `EnumCount`
- Stack depth: `OpStackElement::COUNT` (16)
- Challenge count: `ChallengeId::COUNT`

This is a deliberate simplicity choice. Const generics would add complexity
without clear benefit for a fixed-table system.

### Associated Types in Traits

The `AIR` trait uses associated types for column enums:
```rust
trait AIR {
    type MainColumn: MasterMainColumn + EnumCount;
    type AuxColumn: MasterAuxColumn + EnumCount;
}
```

The `TraceTable` trait uses associated types for varying fill parameters:
```rust
trait TraceTable: AIR {
    type FillParam;       // e.g., ClkJumpDiffs for ProcessorTable
    type FillReturnInfo;  // e.g., () or ClkJumpDiffs
}
```

### Comparison with Tabula

Tabula uses const generics more aggressively (e.g., `[T; WIDTH]` in column
structs). Triton's enum-based approach with `EnumCount` achieves similar
compile-time safety with arguably better ergonomics (named columns vs indices).
Tabula's `ChipId(u16)` / `BusId(u16)` newtypes serve a similar role to
Triton's `TableId` / `ChallengeId` enums but are open rather than closed.

---

## 9. Re-export and Public API Patterns

### Prelude Module

Triton provides a `prelude` module designed for wildcard import:

```rust
pub use triton_vm::prelude::*;
// Gives you: BFieldElement, XFieldElement, Digest, Program, Claim, Proof,
//            Stark, Prover, Verifier, VM, VMState, PublicInput, NonDeterminism,
//            all error types, AIR, TableId, assembly macros
```

### Top-Level Convenience Functions

```rust
pub fn prove_program(program, input, non_determinism) -> Result<(Stark, Claim, Proof), ProvingError>;
pub fn prove(stark, claim, program, aet) -> Result<Proof, ProvingError>;
pub fn verify(stark, claim, proof) -> Result<(), VerificationError>;
```

These are simple wrappers around `Stark::prove()` / `Stark::verify()` that
provide a minimal API surface for common usage.

### Dependency Re-export

The root `lib.rs` re-exports core dependencies:
```rust
pub use triton_air as air;
pub use triton_isa as isa;
pub use twenty_first;
```

This ensures users don't need to add `triton-air` or `triton-isa` to their
Cargo.toml -- they can access everything through `triton-vm`.

### Error Barrel File

`error.rs` acts as a barrel module, re-exporting error types from their
defining modules. Users import from `triton_vm::error::*` or the prelude.

### Comparison with Tabula

Tabula re-exports through `tabula-core` but doesn't have a single prelude
module. The convenience-function pattern (top-level `prove`/`verify`) is
a good UX pattern that Tabula could adopt in its public-facing crate.

---

## 10. Code Quality Observations

### Documentation Style

- Module-level doc comments explain purpose and invariants
- Method-level docs use imperative voice ("Evaluating polynomial f(x) = ...")
- Mathematical notation in docs (polynomials, summations)
- No `#[deny(missing_docs)]` enforcement, but good coverage on public API

### Lint Configuration

45+ clippy lints enabled at workspace level in `Cargo.toml`:
```toml
[workspace.lints.clippy]
cast_lossless = "warn"
cloned_instead_of_copied = "warn"
copy_iterator = "warn"
...
```

This is managed at workspace level, not per-crate.

### Macro Usage

- `proof_items!` macro generates the `ProofItem` enum with metadata methods
- `profiler!` macro for instrumentation (no-op when `no_profile` feature is on)
- `triton_program!` / `triton_asm!` / `triton_instr!` macros for assembly DSL
- `include!` for build-generated constraint code
- `strum` derive macros (`EnumCount`, `EnumIter`) used pervasively

### Parallelism Pattern

Rayon parallel iteration is used extensively in trace generation:
```rust
// Pattern: parallel zip of mutable slices
extension_functions
    .into_par_iter()
    .zip_eq(aux_table.axis_iter_mut(Axis(1)))
    .for_each(|(generator, column)| generator(column));
```

### AlgebraicExecutionTrace Design

Uses `IndexMap` (not `HashMap`) for deterministic iteration order. This is
critical for proof reproducibility. Multiplicities are tracked as `u32` values
indexed by instruction location or lookup key.

### Build-Time Constraint Compilation

The `build.rs` pipeline:
1. `Constraints::all()` collects symbolic constraints from all 9 tables
2. `lower_to_target_degree_through_substitutions()` reduces degree to 4
3. `RustBackend` generates optimized Rust evaluation code
4. `TasmBackend` generates Triton assembly evaluation code
5. `syn` + `prettyplease` format the output
6. Generated files are included via `include!` macro

This is the most distinctive engineering pattern in the codebase. It enables
the same constraint definitions to produce both native Rust and in-VM
evaluation code.

---

## 11. Patterns Applicable to Tabula

### Strongly Recommended

1. **Auto-trait verification tests**: Add `assert_send_sync` tests for all
   public types in each crate. Catches regressions from `Rc` or `Cell`
   introduction.

2. **Constraint uniqueness testing**: Use Schwartz-Zippel evaluation to verify
   constraints are non-trivial and non-duplicate. Apply to all chips.

3. **Deterministic collections**: Use `IndexMap`/`IndexSet` in witness
   generation where iteration order affects proof output.

4. **Workspace-level lint configuration**: Consolidate clippy lints in root
   `Cargo.toml` rather than per-crate.

### Worth Considering

5. **Column enum pattern with `EnumCount`**: Replace numeric column width
   constants with enum variants that auto-count. Provides named access
   and compile-time width computation.

6. **Error type granularity**: Separate proving errors from verification errors
   from parameter validation errors. Currently Tabula has `ProveError` but
   could benefit from finer-grained types.

7. **Prelude module**: Create a `tabula::prelude` that re-exports the
   user-facing API subset for ergonomic imports.

8. **Profiler feature inversion**: Use `no_profile` as default feature
   (profiling off by default) rather than `profile` as opt-in. Avoids
   dependency coordination issues.

### Not Applicable

9. **Build-time constraint compilation**: Requires fixed table set known at
   build time. Incompatible with Tabula's runtime chip registration model.

10. **Sealed AIR trait**: Tabula's extensibility model (register custom chips)
    requires an open trait. Sealing would break the extension story.

11. **Mirror directory structure** (AIR in one crate, traces in another with
    same file names): Adds navigational overhead. Tabula's 3-file pattern
    (columns.rs / air.rs / trace.rs) in a single directory is more cohesive.

---

## 12. Summary Statistics

| Metric | Triton VM |
|--------|-----------|
| Workspace crates | 5 |
| Tables/chips | 9 (fixed, sealed) |
| Main columns total | ~159 (across 9 tables) |
| Aux columns total | ~48 (across 9 tables) |
| Challenge IDs | 67 |
| Instruction count | 45 (7-bit encoded) |
| Error types | 8 enums + 1 struct |
| Cross-table argument types | 3 (Perm, Eval, Lookup) |
| Constraint categories | 4 (initial, consistency, transition, terminal) |
| Build-time code generation | Yes (constraints -> Rust + TASM) |
| Primary testing framework | proptest (property-based) |
| Parallelism | rayon (pervasive in trace generation) |
| Field | Goldilocks 64-bit (via twenty-first) |
| Hash | Tip5 |

---

## References

- Repository: https://github.com/TritonVM/triton-vm (v2.0.0)
- Specification: https://triton-vm.org/spec/
- twenty-first (field/hash library): https://github.com/Neptune-Crypto/twenty-first
- Companion analysis: `docs/research/triton-codesign-analysis.md` (ISA/AIR philosophy)
