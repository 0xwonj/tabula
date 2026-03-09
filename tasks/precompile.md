# Precompile Framework

> Status: ⬜ Blocked on [composition.md](composition.md) (needs BusId)
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §3 (Computation Extension), §10 (Precompile System)

## Goal

App developers can add custom instructions (precompiles) without modifying Tabula. Follows the proven pattern: Hash → PoseidonChip, Lookup → StaticTableChip.

## Tasks

### Precompile IR variant (~50 LOC)

> Ready — no dependencies (can start before BusId)

- [ ] Add `Instruction::Precompile` variant in `ir/src/instruction.rs`
  ```rust
  Precompile {
      id: PrecompileId,
      dst_slots: Vec<Slot>,
      inputs: Vec<ValueExpr>,
  }
  ```
- [ ] Define `PrecompileId(u16)` newtype
- [ ] IR validation in `Program::register()` — verify declared precompiles exist

### PrecompileHandler trait (~50 LOC)

> Blocked on: Precompile IR variant

- [ ] Define trait in `executor/src/precompile.rs`
  ```rust
  pub trait PrecompileHandler: Send + Sync {
      fn id(&self) -> PrecompileId;
      fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
  }
  ```
- [ ] Handler registry in executor (`Vec<Box<dyn PrecompileHandler>>`)
- [ ] Dispatch in `interpreter.rs` for `Instruction::Precompile`
- [ ] Test: identity precompile

### ExecutionChip wiring (~100 LOC)

> Blocked on: PrecompileHandler + BusId

- [ ] `op_precompile` selector in ExecutionChip
- [ ] `PrecompileBus` definition (bus message: precompile_id, inputs, outputs)
- [ ] ExecutionChip `eval()` — precompile bus send
- [ ] Test: bus send/receive balance

### DSL syntax (~200 LOC)

> Blocked on: ExecutionChip wiring

- [ ] AST node for precompile calls in `lang/src/ast.rs`
- [ ] Parser support in `lang/src/parse/`
- [ ] Lowering: AST → `Instruction::Precompile` in `lang/src/lower/`
- [ ] E2E test: DSL → compile → execute → verify

## App Developer Experience

```rust
struct Sha256Precompile;
impl PrecompileHandler for Sha256Precompile {
    fn id(&self) -> PrecompileId { PrecompileId(1) }
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, _> { ... }
}

struct Sha256Chip;
impl Air<AB> for Sha256Chip { ... }

let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_default_commitments()
    .with_precompile(Sha256Precompile, Sha256Chip)
    .build()?;
```

## Verification

```bash
cargo check --workspace
cargo test --workspace
```
