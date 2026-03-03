# Tabula Compiler Research Architecture

> Status: Proposal v2.0 (comprehensive)
> Date: 2026-02-20
> Scope: Full-stack compiler architecture across `lang`, `ir`, `core`, `executor`, `commitment`, `proof`, and `cli`
> Audience: compiler/runtime/proof maintainers
> Decision posture: correctness-first, compatibility-second

---

## 0. Why this document exists

This document defines a complete redesign of Tabula's compilation architecture from a compiler-research perspective.
It intentionally goes beyond local fixes and proposes system-level structural changes.

This version is expanded to include:

1. Root-cause analysis with architectural causes.
2. Design principles and decision rationale.
3. Full crate impact, not only `lang/ir/cli`.
4. Formal contracts for semantics, artifacts, and proof binding.
5. Concrete migration steps with acceptance gates and rollback strategy.

The key conclusion is explicit:

- A high-confidence architecture requires coordinated changes in **all major crates**.
- Restricting scope to `lang/ir/cli` leaves core drift classes unresolved.

---

## 1. Problem framing

### 1.1 Current symptom classes

Observed failures are not random bugs; they cluster into seven structural classes:

| ID | Symptom | Why it repeats |
|---|---|---|
| R1 | Type/operation unsoundness passes compile-time and fails runtime | Operator legality is checked in multiple places with partial overlap |
| R2 | Compiled artifact semantics diverge from executed semantics | Canonicalization side effects occur after artifact decisions |
| R3 | Normal-form policy drift between docs and code | Policy is encoded in prose + pass heuristics, not in versioned semantics |
| R4 | Hash/encoding semantics differ by backend | No explicit semantic profile controls runtime/commit/proof coherence |
| R5 | Semantic validation leaks into CLI | Ownership boundary between frontend and compiler core is not enforced |
| R6 | Lowering performs implicit defaults | Syntax elaboration and semantic typing are coupled |
| R7 | Command-level correctness is weakly reported | CLI outcomes are presentation strings, not strict typed contracts |

### 1.2 Architectural interpretation

These failures indicate missing primitives:

1. No single semantic authority object.
2. No invariant-segmented IR tower.
3. No obligation model for deferred checks.
4. No artifact identity tied to semantic profile.
5. No runtime-proof contract IR as shared truth.

### 1.3 Evidence map (current repository)

This proposal is grounded in concrete code locations where drift currently appears.

| Class | Current location(s) | Architectural issue |
|---|---|---|
| R1 | `crates/ir/src/pass/typecheck.rs`, `crates/lang/src/lower/expr.rs`, `crates/executor/src/interpreter.rs` | Operator legality/typing enforced with different completeness at each layer |
| R2 | `crates/cli/src/commands/compile.rs`, `crates/ir/src/program.rs`, `crates/ir/src/pass/canonicalize/mod.rs` | Artifact emission path is not guaranteed to reflect canonicalized semantics |
| R3 | `crates/ir/src/pass/validate.rs`, `crates/ir/src/pass/canonicalize/nf4_alias_guard.rs`, `docs/spec/semantics-spec.md` | NF policy behavior is split across prose and transformation heuristics |
| R4 | `crates/core/src/traits/crypto.rs`, `crates/commitment/src/poseidon.rs` | Hash semantics change by backend implementation details |
| R5 | `crates/cli/src/io.rs` | CLI performs semantic checks that should belong to compiler core |
| R6 | `crates/lang/src/lower/mod.rs`, `crates/lang/src/lower/expr.rs` | Lowering stage mixes syntax elaboration and implicit type decisions |
| R7 | `crates/cli/src/commands/execute.rs`, `crates/executor/src/batch.rs` | Command status contract is weaker than required for strict operational correctness |

---

## 2. Design goals and non-goals

### 2.1 Goals

1. **Semantic soundness by construction**
   - Illegal programs should become unrepresentable before execution IR.
2. **Deterministic reproducibility**
   - Build output must be stable across host, order, and command mode.
3. **Proof-aligned architecture**
   - Runtime and proof bind to one explicit contract layer.
4. **Operational strictness**
   - `compile/check/execute/prove` must have machine-verifiable contracts and exit semantics.
5. **Composable evolution**
   - Future language/proof extensions must be profile-versioned, not implicit.

### 2.2 Non-goals

1. Preserve existing crate boundaries at all costs.
2. Preserve current JSON program file schema as canonical format.
3. Optimize for minimum short-term diff size.

### 2.3 Constraints

1. Determinism requirements from current execution model remain mandatory.
2. Existing proof roadmap (M11+ milestones) must remain reachable.
3. Compiler changes cannot silently alter protocol meaning; profile/hash governance is required.

---

## 3. Architecture principles and rationale

### 3.0 Research patterns adopted

This architecture explicitly adopts proven compiler and formal-verification patterns:

1. **Typed multi-IR pipeline**
   - separate concerns by invariant strength.
2. **Effect-typed intermediate form**
   - model stateful operations explicitly, not as plain expressions.
3. **Obligation-based compilation**
   - represent deferred proofs/guards as tracked objects.
4. **Query-driven incremental compilation**
   - deterministic dependency graph and cache-aware recomputation.
5. **Contract-first proof integration**
   - statement and bus schemas are generated from a shared IR, not duplicated.

Why these patterns:
- They directly target semantic drift classes R1-R7.
- They scale better than adding ad hoc validators to a monolithic IR.
- They provide explicit extension points for future proof and language features.

### P1. Single Semantic Authority

All semantics are represented by explicit data objects and tables, not scattered pass logic.

Rationale:
- Prevents divergence between typechecker/interpreter/proof adapter.
- Enables artifact hashing over semantics.

### P2. Invariant Layering

Each IR tier is responsible for a stronger invariant set than the previous tier.

Rationale:
- Makes pass correctness reviewable.
- Supports local reasoning and fuzzing per stage.

### P3. Obligation-Carrying Compilation

Anything not proven statically becomes a tracked obligation.

Rationale:
- Removes hidden behavior changes from canonicalization.
- Makes runtime guards explicit, auditable, and hash-stable.

### P4. Artifact-Centric Execution

Execution consumes canonical artifacts by default, not ad hoc partially lowered forms.

Rationale:
- Eliminates `check OK / execute fail` classes caused by path drift.

### P5. Proof Contract as First-Class IR

Statement fields and bus schemas are generated and versioned from one contract IR.

Rationale:
- Avoids runtime/proof interface drift and underconstrained transitions.

### P6. Frontend Thinness

CLI parses args, invokes driver, formats output. It does not implement semantics.

Rationale:
- Prevents validation duplication and behavior skew across commands.

---

## 4. Why all major crates must change

### 4.1 Short answer

No, `lang/ir/cli` only is not sufficient.

### 4.2 Detailed scope table

| Crate | Must change? | Why |
|---|---|---|
| `tabula-lang` | Yes | Split syntax elaboration from typing; remove implicit defaults |
| `tabula-ir` | Yes | Replace monolithic IR/pass model with layered IR + obligations |
| `tabula-cli` | Yes | Remove semantic ownership; become driver frontend |
| `tabula-core` | Yes | Introduce semantic profile, operator tables, and shared contracts |
| `tabula-executor` | Yes | Execute LIR with explicit guards and typed outcomes |
| `tabula-commitment` | Yes | Bind hash/encoding behavior to profile and contract metadata |
| `tabula-proof` | Yes | Consume Contract IR; align statement/bus schemas to profile |
| `tabula-driver` (new) | Yes | Central orchestration, pass manager, query engine, artifact pipeline |
| `tabula-contract` (new) | Yes | Shared statement/bus/public-value schema ownership |

### 4.3 Principle-level reason

If semantics can still vary by backend, by command, or by crate, architecture is not sound.
That cannot be fixed by touching only frontend crates.

---

## 5. End-state architecture (high level)

```text
Source (.tab)
  -> Frontend (syntax)
  -> Typed HIR
  -> Effect MIR
  -> Canonical MIR
  -> Execution LIR
  -> Contract IR
  -> Canonical Bundle (.tcb)

.tcb + state + batch
  -> Runtime Executor
  -> Execution Trace IR
  -> Consistency report (typed)

.tcb + Execution Trace IR
  -> Proof Witness Builder
  -> AIR/LogUp generation
  -> Proof/Verification
```

Everything is keyed by `SemanticProfile` + `semantic_hash`.

---

## 6. Proposed workspace topology

```text
tabula/
  crates/
    tabula-front/        # lexing, parsing, spans, syntax diagnostics
    tabula-hir/          # name-resolved and typed high-level IR
    tabula-mir/          # effect graph IR + obligations
    tabula-lir/          # executable low-level IR
    tabula-contract/     # proof/runtime contract IR
    tabula-driver/       # pass manager, incremental queries, artifact IO
    tabula-runtime/      # execution over LIR + E-Trace emission
    tabula-commitment/   # profile-bound commitment/hash layer
    tabula-proof/        # proof system over Contract IR + E-Trace
    tabula-cli/          # command frontend only
    tabula-core/         # shared value types, profile, trait contracts
```

Transitional note:
- Existing `tabula-ir` can host HIR/MIR/LIR modules during migration, then split.

---

## 7. Semantic profile (mandatory)

### 7.1 Definition

`SemanticProfile` is the canonical descriptor of language/runtime/proof semantics.

```rust
pub struct SemanticProfile {
    pub profile_id: String,               // e.g., "tabula-mainnet-v1"
    pub lang_version: u32,
    pub ir_version: u32,
    pub contract_version: u32,

    pub arithmetic: ArithmeticPolicy,
    pub comparison: ComparisonPolicy,
    pub alias_policy: AliasPolicy,
    pub null_policy: NullPolicy,

    pub hash_policy: HashPolicy,
    pub codec_policy: CodecPolicy,

    pub statement_policy: StatementPolicy,
    pub bus_schema_policy: BusSchemaPolicy,
}
```

### 7.2 Why profile is required

Without explicit profile binding:

1. `hash_ir` encoding may differ by hasher backend.
2. alias/NF interpretation may drift by pass logic.
3. proof statement fields can be partially bound without protocol clarity.

Profile hash makes these differences explicit and rejectable.

### 7.3 Compatibility rules

1. `compile` writes `profile_hash` into artifact metadata.
2. `execute` requires runtime-selected profile hash equality.
3. `prove` requires artifact and proof backend profile hash equality.
4. mismatch is hard error, never warning.

---

## 8. IR tower specification

### 8.1 IR-0: Surface AST (S-AST)

Purpose:
- Source-faithful syntax tree with spans and comments.

Allowed unresolved states:
- unresolved identifiers
- untyped literals

Forbidden:
- semantic defaults (no fallback type decisions)

### 8.2 IR-1: Typed HIR

Purpose:
- complete name resolution
- principal type assignment
- explicit nullability type model

Key properties:
- no unresolved symbols
- each expression has one principal type
- row key expressions typed as `RowKeyTerm`

### 8.3 IR-2: Effect MIR

Purpose:
- separate pure computation from state effects
- represent transaction body as effect graph + SSA values

Core nodes:
1. Pure value ops (`Arith`, `Cmp`, `Logic`, `Select`, `Hash`)
2. Effect ops (`Read`, `Write`, `Lookup`, `Emit`, `Assert`)
3. Guard ops (`RuntimeGuard`, `CheckObligation`)

Core metadata:
- value type map
- effect ordering edges
- alias obligations
- static proof obligations

### 8.4 IR-3: Canonical MIR (C-MIR)

Purpose:
- deterministic normalization boundary and semantic hashing input

Canonicalization includes:
1. stable symbol ordering
2. stable slot numbering
3. stable obligation ordering
4. explicit materialization of deferred guards

After C-MIR:
- no semantic rewriting allowed.

### 8.5 IR-4: Execution LIR (X-LIR)

Purpose:
- runtime-efficient form with fully resolved checks

Requirements:
1. all operators legal by table
2. all guards explicit instructions
3. no unresolved obligation
4. strict typed instruction operands

### 8.6 IR-5: Contract IR (K-IR)

Purpose:
- single source for runtime/proof contract

Contains:
1. statement field layout
2. public value encoding layout
3. bus tuple schemas
4. interaction tags and ownership
5. binding rules from E-Trace to statement fields

---

## 9. Type system and operator algebra

### 9.1 Operator signature table

Introduce one shared table in `tabula-core` (or dedicated semantics module).

| Operator class | Allowed domains | Result type |
|---|---|---|
| `Add/Sub/Mul` | `U64×U64`, `I64×I64` | same as operands |
| `Div/Mod` | `U64×U64`, `I64×I64` + divide-by-zero policy | same as operands |
| `Eq/Ne` | all first-class value types with same-type requirement | `Bool` |
| `Lt/Lte/Gt/Gte` | ordered types only (`U64`, `I64`) | `Bool` |
| `And/Or/Not` | `Bool` | `Bool` |
| `Select` | `Bool` condition + same-type branches | branch type |

Every stage uses this same table:
- HIR checker
- MIR validator
- LIR generation
- runtime defensive checks (assert-only consistency)

### 9.2 Nullability model

Compile-time model:
- `Nullable<T>` or equivalent in HIR/MIR.

Runtime model:
- lowered to explicit `(value, is_null)` pair only at LIR boundary.

Benefits:
1. better type diagnostics
2. less duplicated null checks
3. explicit semantics in artifact

---

## 10. Obligation system

### 10.1 Obligation types

```rust
pub enum Obligation {
    DistinctRows {
        table: TableId,
        col: ColId,
        row_a: RowTerm,
        row_b: RowTerm,
        origin: EffectEdgeId,
    },
    RangeBound {
        value: ValueRef,
        bound: BoundSpec,
        origin: NodeId,
    },
    TypeRefinement {
        value: ValueRef,
        required: ValueType,
        origin: NodeId,
    },
}
```

### 10.2 Discharge states

```rust
pub enum Discharge {
    ProvenStatic(ProofWitnessId),
    MaterializedRuntime(GuardNodeId),
    DeferredByPolicy(PolicyReasonId),
}
```

Rules:
1. no obligation may be dropped silently.
2. C-MIR must contain discharge record for each obligation.
3. artifact hash covers obligations and discharge decisions.

### 10.3 Replacing NF-4 hidden insertion

Current style:
- pass inserts `Cmp(Ne)+Assert` heuristically.

New style:
1. create `DistinctRows` obligation during effect analysis.
2. try static discharge.
3. if unresolved, materialize explicit runtime guard node with provenance.

This preserves soundness and traceability.

---

## 11. Pass manager and invariant contracts

### 11.1 Pass contract

```rust
pub trait Pass<I, O> {
    fn name(&self) -> &'static str;
    fn requires(&self) -> InvariantSet;
    fn ensures(&self) -> InvariantSet;
    fn run(&self, input: I, ctx: &PassContext) -> Result<(O, PassReport), DiagnosticSet>;
}
```

### 11.2 Invariant registry examples

- `Inv::SyntaxWellFormed`
- `Inv::NamesResolved`
- `Inv::PrincipalTyped`
- `Inv::OperatorLegal`
- `Inv::EffectGraphAcyclic`
- `Inv::ObligationsTracked`
- `Inv::ObligationsDischarged`
- `Inv::CanonicalOrdering`
- `Inv::ArtifactSerializable`

### 11.3 Driver enforcement

1. pre-pass: assert `requires` satisfied.
2. post-pass: verify `ensures` via validators.
3. fail-fast on invariant mismatch.

No optional warning mode for invariant failures.

---

## 12. Query engine and incremental compilation

### 12.1 Query graph

1. `parse(file) -> S-AST`
2. `resolve(ast) -> HIR`
3. `lower_effect(hir) -> MIR`
4. `canonicalize(mir, profile) -> C-MIR`
5. `lower_lir(cmir) -> X-LIR`
6. `build_contract(cmir, profile) -> K-IR`
7. `package(cmir, xlir, kir, profile) -> TCB`

### 12.2 Determinism requirements

1. query keys are canonicalized file IDs + profile ID.
2. no non-deterministic map iteration in query outputs.
3. stable sorting rules for all sets.

### 12.3 Cache policy

- cache by `(query, inputs_hash, profile_hash, toolchain_version)`.
- invalidate transitively on profile change.

---

## 13. Canonical artifact (`.tcb`) specification

### 13.1 Goals

1. Single canonical unit for compile/check/execute/prove.
2. Embed semantic identity and compatibility metadata.
3. Preserve debug maps and diagnostics provenance.

### 13.2 Top-level layout

```text
TabulaCanonicalBundle v1
  header
  metadata
  profile
  schema_set
  canonical_mir
  execution_lir
  contract_ir
  diagnostics_index (optional)
  debug_map (optional)
  signatures (optional)
```

### 13.3 Header

| Field | Description |
|---|---|
| `magic` | fixed marker |
| `bundle_version` | artifact format version |
| `profile_hash` | hash of canonical profile object |
| `semantic_hash` | hash of semantic body sections |
| `producer` | compiler build metadata |

### 13.4 Semantic hash formula

```text
semantic_hash = H(
  canonical(schema_set)
  || canonical(canonical_mir)
  || canonical(contract_ir)
  || canonical(profile)
)
```

Notes:
- X-LIR is derivable from C-MIR and profile. It can be included in semantic hash by policy.

### 13.5 Compatibility rules

1. major `bundle_version` mismatch: hard reject.
2. profile hash mismatch: hard reject.
3. semantic hash mismatch after re-canonicalization: hard reject.

---

## 14. Driver API specification

### 14.1 Core API

```rust
pub enum PipelineRequest {
    Compile { input: ProgramInput, profile: ProfileSelector },
    Check { input: ProgramInput, profile: ProfileSelector, strict: bool },
    Execute { artifact: ArtifactInput, state: StateInput, batch: BatchInput, opts: ExecuteOpts },
    Prove { artifact: ArtifactInput, trace: TraceInput, opts: ProveOpts },
    Verify { artifact: ArtifactInput, proof: ProofInput, opts: VerifyOpts },
}

pub enum PipelineResponse {
    Compile(CompileResult),
    Check(CheckResult),
    Execute(ExecuteResult),
    Prove(ProveResult),
    Verify(VerifyResult),
}
```

### 14.2 Result contracts

Results are typed and stable; CLI only renders them.

Example:

```rust
pub struct ExecuteResult {
    pub status: ExecuteStatus,
    pub tx_outcomes: Vec<TxOutcomeRecord>,
    pub read_set: Vec<ReadRecord>,
    pub write_set: Vec<WriteRecord>,
    pub consistency: ConsistencyReport,
    pub trace: Option<ExecutionTrace>,
}

pub enum ExecuteStatus {
    Success,
    Failed(ExecuteFailure),
}
```

No status-by-string pattern for correctness-critical outcomes.

---

## 15. CLI architecture (thin frontend)

### 15.1 Command responsibility

`tabula-cli` does:
1. argument parsing
2. input file loading
3. driver request dispatch
4. output formatting (text/json)

`tabula-cli` does **not**:
1. type checking
2. schema coverage checking
3. IR canonicalization policy decisions
4. statement/profile consistency decisions

### 15.2 Exit code policy

| Code | Meaning |
|---|---|
| 0 | successful request and successful semantic/execution/proof outcome |
| 1 | semantic or runtime/proof failure |
| 2 | input/usage error |
| 3 | internal compiler/driver invariant violation |

### 15.3 JSON output schema policy

- versioned output envelope:
  - `schema_version`
  - `command`
  - `status`
  - `payload`
  - `diagnostics`

---

## 16. `tabula-core` redesign

### 16.1 New responsibilities

1. host the semantic profile types.
2. host shared operator legality tables.
3. define cross-layer diagnostics IDs.
4. define stable contract identifiers used by runtime/proof.

### 16.2 Hash and codec contracts

Current issue:
- `Hasher::hash_ir` default and backend override semantics can diverge.

Target:
1. `hash_ir` semantics selected by `HashPolicy` from profile.
2. backend implements primitive hash only.
3. encoding and domain separation are controlled by profile-bound adapter layer.

### 16.3 Proposed trait direction

```rust
pub trait PrimitiveHasher {
    fn hash_bytes(&self, bytes: &[u8]) -> Digest;
    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest;
}

pub trait IrHashSemantics {
    fn hash_ir(&self, values: &[Value], policy: &HashPolicy, codec: &dyn ValueCodec) -> Digest;
}
```

This separates primitive cryptography from semantic encoding policy.

---

## 17. `tabula-lang` redesign

### 17.1 Required changes

1. Remove fallback typing in lowering.
2. Split parser output and typing elaboration.
3. Produce HIR with explicit nullable and row-term semantics.

### 17.2 Diagnostic improvements

Each error includes:
- source span
- stage (`parse`, `resolve`, `type`) 
- error code (stable)
- suggested fix when available

### 17.3 Output contract

`tabula-lang` should no longer output directly executable IR.
It outputs S-AST and HIR only.

---

## 18. `tabula-ir` redesign

### 18.1 Current-to-target transition

Current:
- monolithic instruction IR
- canonicalize/typecheck/validate sequence with semantic mutation

Target:
1. MIR as effect graph + obligations
2. C-MIR as immutable semantic hash boundary
3. LIR lowering separated and deterministic

### 18.2 Strict pass sequencing

1. `ResolvePass`: S-AST -> HIR
2. `TypePass`: HIR typing + operator legality
3. `EffectPass`: HIR -> MIR
4. `ObligationPass`: generate obligations
5. `DischargePass`: static discharge + runtime materialization
6. `CanonicalPass`: MIR -> C-MIR
7. `LirPass`: C-MIR -> X-LIR

### 18.3 Forbidden operations after C-MIR

No pass may:
1. add/remove guards
2. rewrite alias policies
3. reinterpret typed operators

Only codegen/layout lowering allowed.

---

## 19. `tabula-runtime` / `tabula-executor` redesign

### 19.1 Core changes

1. consume X-LIR + profile only.
2. enforce strict typed execution outcomes.
3. emit Execution Trace IR (E-Trace) aligned with Contract IR.

### 19.2 Consistency contract

Current style of string report must be replaced by structured report:

```rust
pub struct ConsistencyReport {
    pub status: ConsistencyStatus,
    pub violations: Vec<ConsistencyViolation>,
}
```

Any non-empty violation list implies non-zero command outcome.

### 19.3 Determinism controls

1. deterministic ordering for read/write sets
2. deterministic tx outcome indexing
3. deterministic trace row ordering

---

## 20. `tabula-commitment` redesign

### 20.1 Profile-bound behavior

Commitment semantics must not be implicit in hasher implementation.

Must be controlled by:
1. `HashPolicy`
2. `CodecPolicy`
3. `ContractVersion`

### 20.2 Required outputs

Commitment layer should export metadata proving which profile and contract rules were used.

### 20.3 Leaf/hash evolution governance

Any change in leaf digest or domain-separation formula must:
1. bump relevant policy version
2. update profile hash
3. fail cross-profile execution/proof interop by default

---

## 21. `tabula-contract` (new)

### 21.1 Purpose

Single ownership of all runtime/proof shared schemas.

### 21.2 Contents

1. `StatementSchema`
2. `PublicValueLayout`
3. `BusSchemaRegistry`
4. `BindingRules`
5. `DeferredFieldRegistry`

### 21.3 Example

```rust
pub struct StatementSchema {
    pub fields: Vec<StatementField>,
    pub version: u32,
}

pub struct BusSchemaRegistry {
    pub buses: Vec<BusSchema>,
    pub version: u32,
}
```

### 21.4 Rule

Neither runtime nor proof may define bus tuple shape independently.
They import from `tabula-contract`.

---

## 22. `tabula-proof` redesign

### 22.1 Input boundary

Proof system consumes:
1. Contract IR
2. Execution Trace IR
3. Semantic profile
4. canonical artifact metadata

### 22.2 Benefits

1. statement binding is explicit and versioned.
2. cross-chip bus schemas are centrally controlled.
3. deferred statement fields are visible and testable.

### 22.3 Required checks

Proof pipeline must fail if:
1. contract version mismatch
2. profile mismatch
3. missing binding for mandatory statement field
4. bus schema mismatch with runtime trace

---

## 23. `tabula-driver` (new)

### 23.1 Responsibilities

1. pipeline orchestration
2. pass/invariant validation
3. artifact build/load/save
4. profile selection and compatibility checks
5. diagnostics aggregation

### 23.2 Why separate crate

1. keeps CLI thin
2. keeps compiler logic reusable by tests/tools/services
3. creates one execution path for all commands

### 23.3 Internal modules

```text
driver/
  pipeline/
  pass_manager/
  query/
  artifact/
  profile/
  diagnostics/
```

---

## 24. Formal command semantics

### 24.1 `compile`

Input:
- source or compatibility JSON
- selected profile

Output:
- canonical `.tcb`
- compile diagnostics and pass report

Invariant:
- output always reflects post-canonical semantics.

### 24.2 `check`

Input:
- source or `.tcb`
- selected profile

Output:
- invariant report
- diagnostics

Invariant:
- no hidden mutation to user artifact in check-only mode.

### 24.3 `execute`

Input:
- `.tcb` preferred (source allowed via explicit `--compile-first`)
- state
- batch

Output:
- typed execution result
- optional E-Trace

Invariant:
- runtime profile hash must equal artifact profile hash.

### 24.4 `prove` / `verify`

Input:
- `.tcb` + trace/proof

Invariant:
- profile + contract versions must match exactly.

---

## 25. Diagnostics and observability

### 25.1 Error taxonomy

| Prefix | Domain |
|---|---|
| `LEX` | lexical/syntax |
| `RES` | name resolution |
| `TYP` | typing/operator legality |
| `EFF` | effect graph/ordering |
| `OBL` | obligation generation/discharge |
| `CAN` | canonicalization invariants |
| `ART` | artifact format/hash/profile |
| `RUN` | runtime execution |
| `CON` | consistency checks |
| `PRF` | proof/witness/bus binding |
| `DRV` | driver/pass manager infrastructure |

### 25.2 Diagnostic payload

Each diagnostic should include:
1. code
2. severity
3. stage
4. source location (if available)
5. IR entity IDs
6. optional remediation hints

### 25.3 Telemetry counters

Recommended counters:
- obligations generated/resolved/materialized
- guard materialization count by tx type
- pass runtime and cache hit ratio
- profile mismatch incidents

---

## 26. Security and soundness considerations

### 26.1 Threat classes addressed

1. semantic confusion across command paths
2. backend-dependent hash meaning
3. underconstrained proof bindings
4. hidden guard insertion semantics

### 26.2 Required safeguards

1. profile hash gating in all critical operations
2. semantic hash validation on artifact load
3. no implicit fallback typing
4. strict proof binding completeness checks

### 26.3 Residual risks

1. migration phase with mixed old/new artifacts
2. temporary dual-path support complexity

Mitigation:
- compatibility adapter must be explicit and version-gated.

---

## 27. Testing strategy

### 27.1 Unit tests

Per module for:
1. operator legality table
2. obligation generation/discharge
3. canonical ordering and serialization stability

### 27.2 Property tests

1. well-typed MIR cannot produce illegal LIR ops
2. canonicalization idempotence
3. profile mismatch always rejected

### 27.3 Differential tests

Run program through:
1. C-MIR reference evaluator
2. X-LIR executor
3. trace replay checker

Expect identical state transitions and outcomes.

### 27.4 Metamorphic tests

1. alpha-renaming invariance
2. declaration order invariance after canonicalization
3. equivalent expression rewrite invariance

### 27.5 Cross-crate integration tests

1. artifact compile -> execute -> prove pipeline
2. bus schema contract matching runtime/proof
3. statement binding completeness

---

## 28. Migration roadmap (practical)

### Phase 0: Baseline capture

Deliverables:
1. freeze current behavior corpus
2. mark known unsound patterns as failing regression tests

Gate:
- reproducible baseline test suite exists.

### Phase 1: Driver introduction

Deliverables:
1. add `tabula-driver`
2. route current CLI commands through driver wrapper

Gate:
- no command behavior regressions except explicitly fixed bugs.

### Phase 2: SemanticProfile introduction

Deliverables:
1. define profile object in `core`
2. embed profile hash in compile outputs
3. execute/prove profile mismatch hard failure

Gate:
- profile mismatch tests pass.

### Phase 3: Artifact canonicalization fix

Deliverables:
1. compile outputs post-canonical semantics only
2. add semantic hash verification

Gate:
- no compile/check/execute drift cases remain.

### Phase 4: HIR/MIR split

Deliverables:
1. remove lowering fallbacks
2. add typed HIR
3. add effect MIR

Gate:
- operator unsoundness classes are compile-time failures.

### Phase 5: Obligation system

Deliverables:
1. represent alias/distinctness as obligations
2. replace hidden NF guard insertion

Gate:
- every obligation has discharge record.

### Phase 6: Contract IR introduction

Deliverables:
1. add `tabula-contract`
2. centralize statement and bus schemas

Gate:
- proof/runtime schema drift tests pass.

### Phase 7: Runtime/proof rebinding

Deliverables:
1. runtime emits E-Trace aligned with K-IR
2. proof consumes K-IR and E-Trace

Gate:
- end-to-end compile->execute->prove path validates on canonical artifacts.

### Phase 8: Legacy path removal

Deliverables:
1. remove CLI semantic validators
2. deprecate legacy direct JSON semantics path

Gate:
- all CI gates green on new architecture only.

---

## 29. Risk matrix and mitigation

| Risk | Impact | Probability | Mitigation |
|---|---|---|---|
| Migration complexity slows delivery | High | Medium | phase gates + compatibility adapters |
| Temporary dual-path confusion | Medium | High | strict feature flags, deprecation schedule |
| Proof pipeline integration churn | High | Medium | early Contract IR introduction |
| Artifact format change adoption friction | Medium | Medium | conversion tooling + compatibility mode |
| Cache/query bugs in new driver | Medium | Medium | deterministic replay tests |

Rollback principle:
- each phase must remain revertible via feature gate until gate criteria pass.

---

## 30. Governance and versioning policy

### 30.1 What requires version bump

1. operator legality changes
2. hash/codec semantic changes
3. obligation semantics changes
4. statement field layout changes
5. bus tuple schema changes

### 30.2 Version vectors

- `profile_version`
- `ir_version`
- `contract_version`
- `artifact_version`

Any incompatible change increments corresponding major component.

### 30.3 Change process

1. design note update
2. compatibility impact statement
3. test-plan update
4. migration plan update

---

## 31. Alternatives considered

### A1. Patch current architecture only

Rejected because:
- fixes local defects but keeps semantic drift vectors open.

### A2. Keep single IR, add more validators

Rejected because:
- validators cannot replace missing invariant-segmented representation.

### A3. Keep CLI semantic checks for convenience

Rejected because:
- duplicates ownership, reintroduces divergence by command path.

---

## 32. Concrete next implementation packages

### Package 1: Profile foundation

Crates:
- `core`, `cli`, `executor`, `commitment`, `proof`

Tasks:
1. add `SemanticProfile` types
2. embed/profile hash checks
3. wire strict mismatch errors

### Package 2: Canonical artifact correction

Crates:
- `driver` (new), `cli`, `ir`

Tasks:
1. canonical bundle writer/reader
2. compile emits post-canonical artifacts only
3. semantic hash verification path

### Package 3: HIR/MIR foundations

Crates:
- `lang`, `ir` (or split crates)

Tasks:
1. typed HIR introduction
2. effect MIR introduction
3. remove implicit typing defaults

### Package 4: Obligation engine

Crates:
- `ir`, `runtime`, `core`

Tasks:
1. obligation model
2. static discharge + runtime materialization
3. obligation provenance and diagnostics

### Package 5: Contract IR and proof/runtime unification

Crates:
- `contract` (new), `proof`, `runtime`, `commitment`

Tasks:
1. statement/bus schema registry
2. binding rules generation
3. runtime trace alignment + proof consumption

---

## 33. Acceptance criteria for completion

Architecture is considered complete when all are true:

1. All commands are driver-routed.
2. CLI contains no semantic validation logic.
3. Canonical artifact is default interchange format.
4. Profile hash mismatch always hard-fails.
5. Operator unsoundness classes are compile-time rejects.
6. Every alias-sensitive path is represented by obligation discharge records.
7. Runtime and proof share contract schemas from one crate.
8. CI includes regression tests for R1-R7 classes.

---

## 34. Appendix A: Minimal API sketches

### A.1 Profile and artifact

```rust
pub struct ArtifactMeta {
    pub bundle_version: u32,
    pub profile_hash: [u8; 32],
    pub semantic_hash: [u8; 32],
}

pub struct CanonicalBundle {
    pub meta: ArtifactMeta,
    pub profile: SemanticProfile,
    pub schemas: SchemaSet,
    pub c_mir: CanonicalMir,
    pub x_lir: ExecutionLir,
    pub k_ir: ContractIr,
}
```

### A.2 Obligation report

```rust
pub struct ObligationReport {
    pub generated: Vec<ObligationRecord>,
    pub discharged_static: Vec<DischargeRecord>,
    pub discharged_runtime: Vec<DischargeRecord>,
    pub unresolved: Vec<ObligationRecord>,
}
```

### A.3 Check result

```rust
pub struct CheckResult {
    pub valid: bool,
    pub invariants: Vec<InvariantOutcome>,
    pub diagnostics: Vec<Diagnostic>,
    pub pass_reports: Vec<PassReport>,
}
```

---

## 35. Appendix B: Mapping old -> new responsibilities

| Existing location | New owner |
|---|---|
| CLI schema coverage checks | Driver semantic validation pass |
| IR hidden NF guard insertion | MIR obligation + discharge pass |
| backend-specific hash_ir semantics | core profile-bound hash adapter |
| proof-local statement layout assumptions | contract IR registry |
| command-specific execution status strings | typed driver/runtime result contracts |

---

## 36. Appendix C: File-level migration map (current tree)

This appendix maps concrete current files to target ownership and action type.

| Current file/module | Action | Target ownership |
|---|---|---|
| `crates/lang/src/lower/mod.rs` | Split into HIR elaboration and MIR lowering boundaries | `tabula-front` + `tabula-hir` |
| `crates/lang/src/lower/expr.rs` | Remove implicit fallback typing; emit typed expression terms only | `tabula-hir` |
| `crates/ir/src/program.rs` | Replace in-place mutation pipeline with pass manager invocation and artifact staging | `tabula-driver` + `tabula-mir` |
| `crates/ir/src/pass/typecheck.rs` | Replace with profile-bound operator legality engine | `tabula-hir` / `tabula-mir` |
| `crates/ir/src/pass/validate.rs` | Convert NF checks into obligation verification framework | `tabula-mir` |
| `crates/ir/src/pass/canonicalize/mod.rs` | Limit to deterministic normalization only; no semantic mutation | `tabula-mir` |
| `crates/ir/src/pass/canonicalize/nf4_alias_guard.rs` | Replace with obligation materialization pass with provenance | `tabula-mir` |
| `crates/cli/src/io.rs` | Remove semantic validation logic; keep serialization adapters only | `tabula-cli` |
| `crates/cli/src/commands/compile.rs` | Call driver compile endpoint and write `.tcb` | `tabula-cli` |
| `crates/cli/src/commands/check.rs` | Call driver check endpoint and render diagnostics | `tabula-cli` |
| `crates/cli/src/commands/execute.rs` | Call driver execute endpoint and enforce strict exit contract | `tabula-cli` |
| `crates/core/src/traits/crypto.rs` | Separate primitive hash traits from semantic hash policy adapter | `tabula-core` |
| `crates/executor/src/batch.rs` | Consume typed X-LIR and emit typed execution result and E-Trace | `tabula-runtime` |
| `crates/executor/src/interpreter.rs` | Move to LIR interpreter with profile-bound guard semantics | `tabula-runtime` |
| `crates/commitment/src/poseidon.rs` | Bind behavior via profile policy adapters, not backend overrides alone | `tabula-commitment` |
| `crates/commitment/src/hybrid.rs` | Consume contract-defined leaf/commit semantics metadata | `tabula-commitment` + `tabula-contract` |
| `crates/proof/src/statement.rs` | Replace isolated statement struct with contract-owned schema import | `tabula-contract` + `tabula-proof` |
| `crates/proof/src/air/bus.rs` | Generate/consume bus schemas from contract registry | `tabula-contract` + `tabula-proof` |
| `crates/proof/src/air/interaction.rs` | Align interaction tags and tuple arity to contract versions | `tabula-contract` + `tabula-proof` |

Migration rule:
- during transition, each file should expose compatibility adapters behind feature flags and explicit version guards.

---

## 37. Appendix D: Priority matrix for implementation order

| Priority | Theme | Why now |
|---|---|---|
| P0 | SemanticProfile + profile hash gates | Immediately blocks backend/path semantic drift |
| P0 | Canonical artifact emission from post-canonical IR | Immediately blocks compile/execute mismatch |
| P1 | Driver centralization | Eliminates command-path divergence and validation leakage |
| P1 | Operator legality unification | Closes high-frequency type unsoundness class |
| P1 | Obligation framework skeleton | Establishes explicit model for alias and deferred checks |
| P2 | Contract IR introduction | Required for robust runtime/proof coherence |
| P2 | Runtime E-Trace alignment | Required for proof-binding correctness and observability |
| P2 | Proof schema import from contract crate | Prevents long-term statement/bus drift |
| P3 | Full crate split (`front/hir/mir/lir`) | Improves modularity once semantic foundation is stable |

---

## 38. Final decision statement

Tabula should adopt the full-stack compiler architecture in this document.

Reason:
- It is the only approach that simultaneously closes semantic drift, preserves proof correctness trajectories, and creates a stable platform for future language/runtime/proof evolution.

Implementation should proceed phase-by-phase with strict gates, but the **target architecture must remain holistic**, not partial.
