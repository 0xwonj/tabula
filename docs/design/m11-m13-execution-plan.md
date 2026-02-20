# M11-M13 Execution Plan

This plan is derived from:
- `docs/design/m11-design.md`
- `docs/design/roadmap-m11-m13.md`
- current chip architecture (`Execution + InterTxOrder + StateColumn + ColumnMeta + SmtPath + StaticTable`)

## M11 Success Criteria

M11 is complete when all items below are true.

1. State-root binding is sound end-to-end.
- `ColumnMeta -> SmtColPath` on `SmtLeafDigest`.
- `SmtColPath -> SmtTablePath` on `SmtTableRoot`.
- `old_root/new_root` are enforced via AIR public values on `SmtTablePath` root rows.

2. SmtPath structural soundness is enforced in AIR.
- first real row starts at leaf.
- last real row before padding is root.
- path continuity holds (`next.node = local.parent`) within path segments.
- C15/C16 send/receive cannot be bypassed by flipping `is_leaf/is_root`.

3. Static table lookup wiring is complete.
- `Execution` sends C9 lookup tuples.
- `StaticTable` receives C9 tuples.
- receiver supports lookup multiplicity (one static row can match repeated lookups).

4. Hybrid root-domain safety is explicit.
- table-level SMT depth guard rejects out-of-range `TableId`.

5. Regression quality gate is green.
- `cargo test -p tabula-proof --tests --features stark`
- `cargo clippy -p tabula-proof --all-targets --features stark`
- `cargo test --workspace`

## M11 Implementation Checklist

- [x] AIR public-value binding for `old_root/new_root` in `SmtTablePath`.
- [x] SmtPath path-structure hardening + negative tests.
- [x] C15/C16 integration tests (ColumnMeta ↔ SmtColPath ↔ SmtTablePath).
- [x] C9 StaticTable receiver integration.
- [x] C9 multiplicity witness support in StaticTable.
- [x] C9 cross-chip balance test (Execution ↔ StaticTable).
- [x] Hybrid table-id range guard and panic test.

## Explicit M11 Boundaries

The following remain outside M11 and are carried into later milestones:
- full trace orchestrator (`trace_builder`) from executor/witness to all chips (M12)
- prover/verifier integration (`prove`/`verify`) and permutation argument construction (M13)
- full `ApplyBatchStatement` binding beyond state roots (`program_root`, `applied_tx_digest`, `static_table_root`, budgets) during proof generation path (M13, after M12 contracts freeze)

## M12 Plan

1. Add `trace_builder/` pipeline with one orchestrator entrypoint.
2. Convert executor/witness outputs to all chip traces in one pass.
3. Freeze chip input contracts and add deterministic fixtures.
4. Add integration tests that run all chips together from one batch fixture.

## M13 Plan

1. Add Plonky3 STARK prover/verifier dependencies and config.
2. Implement permutation/cumulative argument trace generation.
3. Build multi-chip proof orchestration and verification.
4. Add true end-to-end test: DSL -> execute -> witness -> traces -> prove -> verify.
