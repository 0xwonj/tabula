# Roadmap: M11 — M13+

> Overview-level plan for milestones after M10. Details will be refined in per-milestone
> design documents as each milestone begins. Written February 2026 against proof-spec v0.9,
> architecture v0.4.5, M10-design.md.

---

## M11: State Root + Gap Proofs

**Goal**: Complete soundness — bind column commitments to the state root, prove
non-membership via gap witnesses, and connect public inputs to the AIR.

**Depends on**: M10 (range checks required for gap witness strict inequalities)

> Status update (February 2026): core M11 state-root binding is implemented on the
> current chip architecture (`ColumnMeta -> SmtColPath -> SmtTablePath`, C9 StaticTable
> receiver, root public-value constraints). Remaining end-to-end work is tracked under
> M12 (trace assembly) and M13 (prover/verifier integration).

### Scope

| Task | Description | Complexity |
|------|-------------|------------|
| **SSMC next_key column** | Add `next_key: U64Limbs<T>` (3 cols) + constraint `next_key_i = key_{i+1}` when not last. Range-check via M10 infrastructure | Medium |
| **Gap witness lookups** | New `SsmcGapWitness` bus. SortedMem init rows send `(t,c,key,next_key,is_first,is_last)` for null reads. SSMC receives. Strict inequality constraints `key < r < next_key` | High |
| **SmtPathChip** | New AIR chip (~600 lines). Merkle path verification for 16-24 level SMT. Leaf digest = `Poseidon(0x10‖t‖c‖tag‖Com)`. Node hash via PoseidonPermutation bus. Configurable depth | High |
| **Root binding** | ColumnMeta leaf digests → SmtPathChip inclusion proofs. `oldRoot`/`newRoot` as public inputs. Meta-level SMT update proof: `oldRoot → newRoot` | High |
| **Public input binding** | Wire `ApplyBatchStatement` 6 fields to AIR via `AirBuilderWithPublicValues`. Budget enforcement (trace dimensions ≤ budgets) | Medium |
| **AppliedTxDigest** | Compute tx outcome hash chain, bind to public input | Medium |
| **ProgramRoot** | Tx type definition Merkle inclusion proof (reuse SmtPathChip) | Medium |
| **StaticTableChip** | Receiver for `StaticTableLookup` bus from M10-B3. LogUp/Lasso against `StaticTableRoot` | Medium-High |

### Key Risks

- **SmtPathChip design**: Variable-depth Merkle paths require careful column layout.
  May need configurable `const DEPTH: usize` generic parameter.
- **Public input integration**: Plonky3's `AirBuilderWithPublicValues` may have API
  constraints that affect our `InteractionAirBuilder` trait hierarchy.
- **Gap witness correctness**: Non-membership proofs for empty columns vs non-empty
  columns have different paths (ColumnMeta `is_empty_old` vs SSMC gap witness).

### Estimated Impact

- 1 new chip (SmtPathChip): ~200 cols, ~600 lines
- SSMC: +3 cols (next_key) + ~6 cols (gap witness halves) = ~54 → ~63
- ColumnMeta: +~16 cols (leaf digest composition) = ~56 → ~72
- ~40 new tests, ~10 updated

---

## M12: Trace Assembly

**Goal**: Build the production pipeline from executor output to all chip traces.

**Depends on**: M10 + M11 (all chips must be in final form before trace format is locked)

### Scope

| Task | Description | Complexity |
|------|-------------|------------|
| **Instruction lowering** | `ExecutionResult` events → `Vec<InstructionRecord>`. Track SSA slot state, operand slot indices, opcode decomposition. New module: `trace_builder/instruction.rs` | Medium |
| **SortedMem flattener** | `BatchWitness` init/access rows → `Vec<SortedMemRow>`. Flatten all columns, global sort by `(t,c,r,τ)`, filter by `KeyRoute::SortedMemory`, assign `meta_is_empty_old` per segment | Medium |
| **SSMC trace builder** | `ColumnState::Ssmc` → `Vec<SsmcEntry>` (chip type). Extract entries, recompute `hash_acc` via Poseidon, derive `mult_witness` from init rows | Medium |
| **Merge trace builder** | `MergeTrace` → `Vec<MergeRow>`. Source enum 3→4 translation (add Delete), recompute `hash_acc`, inject `(t,c)` | Medium |
| **Poseidon input collector** | Aggregate Poseidon permutation inputs from SSMC/Merge/Hash as side-output | Low |
| **Trace orchestrator** | `build_all_traces(witness, records) → AllTraces`. Single entry point for the prover | Medium |
| **Integration tests** | End-to-end: DSL program → execute → witness → traces → debug check all constraints | High |

### Key Risks

- **Type mismatches**: `tabula_commitment::ssmc::SsmcEntry` ≠ `air::chips::ssmc::SsmcEntry`.
  Need clean translation layer.
- **Hash accumulator recomputation**: Must exactly match the Poseidon hash chain used in
  SSMC/Merge AIR constraints. Any divergence → constraint failure.
- **Execution trace ↔ BatchWitness independence**: The execution chip trace comes from
  IR-level instruction records (executor), not from `BatchWitness` (witness generator).
  These are independent pipelines that must produce consistent data.

### Estimated Impact

- New module: `tabula-proof/src/trace_builder/` (~1000 lines, 5-6 files)
- ~30 new tests
- No chip column changes

---

## M13: Plonky3 Prover/Verifier

**Goal**: Generate and verify actual STARK proofs.

**Depends on**: M10 + M11 + M12

### Scope

| Task | Description | Complexity |
|------|-------------|------------|
| **Plonky3 dependencies** | Add `p3-uni-stark`, `p3-fri`, `p3-commit`, `p3-dft`, `p3-merkle-tree`, `p3-challenger` to workspace. Resolve version compatibility (0.4.x availability) | Medium |
| **StarkConfig** | Assemble `TabulaStarkConfig`: BabyBear / Poseidon2 / FRI / BinomialExtension<4>. FRI params: `log_blowup=2, num_queries=28, pow_bits=8` | Low |
| **Permutation trace gen** | `generate_permutation_trace()`: From `Interaction<F>` descriptors + challenges → `RowMajorMatrix<EF>` per chip (cumulative sum columns in BabyBear^4) | High |
| **Permutation constraints** | `eval_permutation()`: Constrain cumulative sum transitions in extension field. New `PermutationAirBuilder` implementation | High |
| **Multi-chip prover** | Orchestrate 8+ chips: commit main traces → sample challenges → generate perm traces → commit perm traces → prove each chip → collect cumulative sums | High |
| **Multi-chip verifier** | Verify each chip proof + check `Σ cumulative_sums = 0` cross-chip | Medium |
| **End-to-end test** | DSL program → execute → witness → traces → prove → verify → success | High |

### Key Risks & Open Questions

1. **`p3-uni-stark` availability**: Version 0.4.x may not be published on crates.io.
   Fallback: git-pin Plonky3 repo or use SP1's fork (`0.2.3-succinct`).

2. **`p3-uni-stark` is single-AIR**: No native multi-chip support. Must either:
   - (a) Prove each chip independently + cross-chip cumsum verification (SP1 pattern)
   - (b) Depend on `openvm-stark-backend` (multi-chip orchestrator)
   - (c) Roll custom multi-chip layer on top of `p3-commit`/`p3-fri`

   Recommendation: Option (a) — minimal, proven pattern.

3. **PairBuilder in prover**: `ProverConstraintFolder` in `p3-uni-stark` may not implement
   `PairBuilder` (preprocessed trace access). PoseidonChip requires this. May need to
   extend the prover folder or use `prove_with_preprocessed()` if available in 0.4.x.

4. **Extension field commitment**: Permutation trace is over `BabyBear^4`. PCS must handle
   both base and extension field traces. `ExtensionMmcs` from `p3-commit` wraps `ValMmcs`
   for this purpose.

5. **Proof size and performance**: First proofs will be unoptimized. Performance tuning
   (template chips, Layout B, threshold calibration) is post-M13.

### Estimated Impact

- New files: `config.rs`, `permutation.rs`, `prover.rs`, `verifier.rs` (~1500 lines)
- New Cargo.toml deps: ~6 Plonky3 crates
- ~20 new tests (including slow end-to-end integration tests)
- No chip column changes

---

## M14+: Production Readiness (Future)

| Task | Description | Priority |
|------|-------------|----------|
| **Proof chaining (B6)** | Sequential batch proofs: `root_0 → root_1 → root_2`. Requires recursive composition or batched verification | High |
| **Benchmark (B7)** | SSMC vs SMT threshold calibration. Measure proving time per column size. Set `HYBRID_THRESHOLD` | Medium |
| **Template chips (Phase 3)** | Fused Transfer / ReadComputeWrite chips for common tx patterns. Same LogUp bus fingerprints as interpreter | Low |
| **Layout B operand linkage** | Replace slot-column carry with LogUp def-use chain. Reduces Execution width by ~48 cols | Low |
| **Conditional branching (CB1-3)** | Basic block CFG in IR + DSL if/else + AIR constraints for block transitions | Feature |
| **Sponge optimization** | SSMC hash chain → sponge (multiple absorbs per permutation). Reduces Poseidon calls | Low |

---

## Dependency Graph

```
✅ M10: Constraint Completeness
  │   Range checks, lex ordering, opcodes (Cmp/Hash/Lookup/Mul/DivMod), Com_empty
  │
  ▼
✅ M11: State Root + Gap Proofs (core)
  │   SmtPathChip, SSMC next_key + gap witness, public inputs, StaticTableChip
  │
  ▼
► M12: Trace Assembly
  │   Instruction lowering, BatchWitness → chip traces, integration tests
  │
  ▼
M13: Plonky3 Prover/Verifier
  │   Permutation trace, StarkConfig, prove/verify, end-to-end test
  │
  ▼
✅ Working End-to-End Proof System (single batch)
  │
  ▼
M14+: Proof chaining, benchmarks, optimizations
```

**Critical path**: M10 → M11 → M12 → M13

**Estimated total remaining**: ~5500 LOC, ~155 tests, 4 milestones

---

## Summary Table

| Milestone | New Chips | New Cols | LOC | Tests | Key Deliverable |
|-----------|-----------|----------|-----|-------|-----------------|
| M10 | 0 | +126 | ~2500 | ~65 | Sound constraint system |
| M11 | 1 (SmtPath) | ~220 | ~1200 | ~40 | State root verification |
| M12 | 0 | 0 | ~1000 | ~30 | Trace assembly pipeline |
| M13 | 0 | 0 | ~1500 | ~20 | STARK proof generation |
| **Total** | **1** | **~346** | **~6200** | **~155** | **End-to-end proof** |
