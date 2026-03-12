# Precompile/Coprocessor Patterns in ZK Proof Systems

Research into how major ZK proof systems implement precompiles, coprocessors, and extensibility mechanisms. Findings inform Tabula's Goal 6 (Extensibility API) design.

---

## 1. SP1 (Succinct)

### Architecture Overview

SP1 uses a **precompile-centric architecture** where precompiles are independent STARK tables (AIRs) connected to the main CPU table via cross-table lookups using LogUp. Built on Plonky3.

### Syscall Mechanism

Precompiles are invoked via the RISC-V `ecall` instruction. The execution flow:

1. `execute_instruction()` dispatches on opcode type
2. For `ecall`, calls `execute_ecall()`
3. `execute_ecall()` reads syscall ID from register x5, arguments from x10/x11
4. Dispatches to the registered `Syscall` implementation: `syscall_impl.execute(&mut precompile_rt, syscall, b, c)`
5. Records a `PrecompileEvent` via `self.add_precompile_event(...)`

```rust
// Simplified execution dispatch
if instruction.is_ecall_instruction() {
    (a, b, c, clk, next_pc, syscall, exit_code) =
        self.execute_ecall()?;
}
```

### Syscall Trait

The `Syscall` trait (in `sp1/src/syscall/mod.rs`) requires:

- `fn execute(&self, rt: &mut SyscallContext, syscall_code: SyscallCode, arg1: u32, arg2: u32)` -- execute the precompile logic
- `fn num_extra_cycles(&self) -> u32` -- cost accounting (each instruction = 4 cycles, precompiles add extra)

### MachineAir Trait

`MachineAir<F>` extends Plonky3's `BaseAir<F>` with:

- `fn name(&self) -> String` -- chip identifier
- `fn generate_trace(&self, input: &ExecutionRecord, output: &mut ExecutionRecord) -> RowMajorMatrix<F>` -- produce the execution trace for this chip
- `fn generate_dependencies(&self, input: &ExecutionRecord, output: &mut ExecutionRecord)` -- declare what additional proof requirements this chip needs (e.g., ByteLookupEvents)
- `fn preprocessed_trace(&self) -> Option<RowMajorMatrix<F>>` -- fixed/setup trace

All SP1 chips implement `MachineAir<F>`, `BaseAir<F>`, and `Air<AB>`.

### Interaction Bus (LogUp)

SP1 uses LogUp-based cross-table lookups:

- A table "sends" a request (tuple of field elements) on a named bus
- Another table "receives" the corresponding request
- The LogUp running sum across all sends and receives must cancel to zero

The permutation trace width per chip: `(sends.len + receives.len) / 2 + 1` columns, where two lookups share one column plus one cumulative sum column.

### Adding a New Precompile (Step-by-Step)

1. **Define SyscallCode**: Add variant to `SyscallCode` enum, update `from_u32()`
2. **FFI layer**: Create `syscall_<name>` function in `zkvm/entrypoint/src/syscalls` with `#[no_mangle]`
3. **Event struct**: Define the precompile event (captures inputs, outputs, memory accesses) in `crates/core/executor/src/events/precompiles/`
4. **Syscall implementation**: Implement `Syscall` trait for the new operation
5. **Register syscall**: Add to `default_syscall_map` in `core/src/runtime/syscall.rs`
6. **Chip implementation**: Create chip struct implementing `MachineAir<F>`, `BaseAir<F>`, `Air<AB>` in `crates/core/machine/src/syscall/precompiles/`
7. **Register chip**: Add variant to `RiscvAir` enum, update `RiscvAir::get_all()`

### Trace Generation and Proving

- Events flow from executor to `ExecutionRecord`, then each chip's `generate_trace()` filters relevant events
- `generate_dependencies()` can produce additional records (e.g., ByteLookupEvents), triggering recursive chip instantiation
- Multi-table FRI: each chip has an `is_real` flag; variable table lengths per chip
- Precompile cost is incurred only when invoked -- inactive chips have zero-length traces

### Key Design Properties

- **Local state isolation**: During precompile computation, no memory/register writes -- all intermediate state in circuit columns
- **Low-degree constraints**: Bitwise ops use minimal-degree formulations
- **64-bit decomposition**: Integers split into four sub-31-bit elements for BabyBear field arithmetic
- **Inspired by Valida**: Cross-table lookups, prover design, chip patterns drawn from Valida

---

## 2. RISC Zero

### Architecture Overview

RISC Zero uses a **monolithic circuit with accelerator extensions**. The base RV32IM circuit has three column categories: control, data, and auxiliary/accumulator. Precompiles are "specialized extension circuits" built into the zkVM's "hardware" layer.

### ECALL Mechanism

- User code executes `ecall` instruction
- zkVM traps to kernel mode (dual user/kernel execution model)
- Kernel dispatches to the appropriate syscall handler
- Handler may delegate to host for non-deterministic advice or invoke an accelerator circuit

### Circuit Integration

Unlike SP1's multi-table approach, RISC Zero integrates precompile circuits into the auxiliary/accumulator columns:

- **Permutation arguments**: Grand product accumulators for memory verification (migrating to log derivatives)
- **Lookup arguments**: PLOOKUP for range checks (migrating to log derivatives)
- **Bigint accelerator**: Host provides product as non-deterministic advice; verifier randomness enforces `a(r) * b(r) == c(r)` polynomial constraint

### Available Precompiles

| Precompile | Cycles |
|-----------|--------|
| SHA-256 compress | 68 cycles/64-byte block |
| SHA-256 init | 6 cycles |
| 256-bit modular multiply | 10 cycles |
| RSA | accelerator circuit |
| Keccak | 2x proving boost for ETH blocks |
| secp256k1, P-256, Curve25519, BLS12-381 | via patched crates |

### Application-Defined Precompiles (v1.2)

A breakthrough architectural innovation: precompiles shipped with the application, not built into the zkVM.

- Uses **Fiat-Shamir randomness** extended to enable precompiles for elliptic curve and algebraic primitives
- Developers deploy new precompiles without updating on-chain verifiers or coordinating with provers
- Host provides non-deterministic advice; the circuit uses verifier randomness to check algebraic relations
- Avoids sequential dependencies: proving can start before witness generation completes

### Key Design Properties

- **Patched crate pattern**: Rather than exposing precompile APIs, RISC Zero patches popular Rust crates (sha2, k256, p256, rsa, etc.) to internally route to precompile circuits
- **Formal verification focus**: Partnered with Veridise/Picus to formally verify circuit determinism (Keccak circuit verified)
- **Horizontal scalability**: Architecture avoids sequential dependencies in precompile design
- **Security caveat**: Precompiles do not guarantee constant-time execution/proving (timing side-channel risk)

---

## 3. OpenVM

### Architecture Overview

OpenVM uses a **"no-CPU" modular architecture** where instruction execution is distributed across multiple chips. There is no central CPU chip; state transitions are enforced entirely through interaction buses. This avoids the performance bottleneck of materializing a complete execution transcript.

### Extension Framework (Three Layers)

Each VM extension consists of three coordinated components:

1. **Guest library** (`openvm-$name-guest`): High-level Rust code defining custom operations, compiled to RISC-V with custom instructions
2. **Transpiler extension** (`openvm-$name-transpiler`): Implements `TranspilerExtension` trait to convert custom RISC-V instructions into OpenVM assembly
3. **Circuit extension** (`openvm-$name-circuit`): Implements chips that handle the new opcodes

### VmExtension Traits (Three Complementary Traits)

```rust
// 1. Registers custom instruction executors by opcode
trait VmExecutionExtension<F> {
    type Executor;  // executor enum
    fn build(&self, builder: &mut ExecutorInventoryBuilder<F>) -> Self::Executor;
}

// 2. Adds algebraic constraints (AIRs) to the circuit
trait VmCircuitExtension<SC> {
    fn build(&self, inventory: &mut AirInventory<SC>);
}

// 3. Backend-specific trace generation
trait VmProverExtension<E, RA, EXT> {
    fn build(&self, ...);
}
```

Execution and circuit extensions typically coexist on one struct; prover extensions live on separate zero-sized types (orphan rule navigation).

### Chip Registration

- `add_executor_chip()`: Associates chip with executor, maintaining executor-index-to-chip mapping
- `add_periphery_chip()`: Adds non-executor-affiliated chips (like range checkers)
- **Critical ordering**: Chips must be added in the order matching executor registration order

### Instruction Routing

Two instruction patterns:

- **Intrinsics**: Read/write RISC-V registers and memory in address spaces 1 and 2
- **Kernels**: Operate over arbitrary address spaces

Setup intrinsics use the `rs2` operand to specify which chip handles the instruction.

### Required System Chips

Program, Public Values, Connector, Range Checker, Memory (multi-chip), Poseidon2 -- must exist in every VM instantiation.

### Key Design Properties

- **No forking required**: Extensions compose without modifying core codebase
- **Bus-enforced correctness**: No centralized controller in the circuit; all state transitions proven via interaction buses
- **Reverse ordering**: AIRs added later appear earlier in the verifying key (dependency resolution)
- **Phantom instructions**: Certain instructions (I/O, hints) are "phantom" -- they affect host execution but have no circuit constraints

---

## 4. Valida

### Architecture Overview

Valida uses a **Harvard architecture** with separate program code and main memory, featuring a CPU with multiple coprocessors connected via communication buses. Built on Plonky3.

### Chip Trait

The `Chip` trait requires implementations for multiple constraint builder contexts:

```rust
trait Chip<M, SC>:
    for<'a> Air<ProverConstraintFolder<'a, M, SC>>
    + for<'a> Air<VerifierConstraintFolder<'a, M, SC>>
    + for<'a> Air<SymbolicAirBuilder<'a, M, SC>>
    + for<'a> Air<DebugConstraintBuilder<'a, M, SC>>
{
    fn generate_trace(&self, machine: &M) -> RowMajorMatrix<SC::Val>;
    fn local_sends(&self) -> Vec<Interaction>;
    fn local_receives(&self) -> Vec<Interaction>;
    fn global_sends(&self) -> Vec<Interaction>;
    fn global_receives(&self) -> Vec<Interaction>;
}
```

### Bus Communication

All chip interactions proven together in a single permutation argument:

- Each `Interaction` has: `fields` (columns), `count` (multiplicity), `argument_index` (Local or Global bus ID)
- Sends and receives are reduced to additive inverses via RLC (random linear combination)
- Cumulative sum across all interactions must equal zero
- Permutation traces include: one column per interaction (multiplicative inverse of RLC), plus running sum column

### Two Bus Scopes

- **Local buses**: Chip-internal interactions (within one sub-proof)
- **Global buses**: Machine-wide interactions (proven across all chips simultaneously)

### Keccak Coprocessor Example

Concrete coprocessor implementation pattern:

- **Pointer Bus**: CPU passes base address pointer to KeccakF chip
- **Memory Bus**: Ensures consistency between KeccakF trace and memory representation
- Input: 50 sequential memory addresses (preimage)
- Output: next 50 sequential addresses (postimage)
- All 24 Keccak rounds computed in single chip setting
- Performance: 8x speedup (183s to 23s for 500 hashes)

### Proving Architecture

- One sub-argument per chip: each chip's trace polynomials go through constraint evaluation, linear combination, quotient by zerofier
- Interaction traces contain send/receive values between chips
- Execution traces encoded as univariate polynomials via Barycentric Lagrange interpolation

### Key Design Properties

- **SP1 drew heavily from Valida**: Cross-table lookup design, chip patterns, prover architecture
- **Modular coprocessor addition**: New chips plug in without modifying core
- **Single cumulative permutation argument**: All global interactions validated together
- **Two-level bus hierarchy**: Local (intra-chip) vs. global (cross-chip)

---

## 5. Triton VM

### Architecture Overview

Triton VM is a **fixed-table architecture** with a specific set of tables (Processor, Program, OpStack, RAM, JumpStack, Hash, Cascade, Lookup, U32). Tables are linked via three types of cryptographic arguments.

### Three Cross-Table Argument Types

1. **Permutation Arguments**: Two lists contain same elements in arbitrary order
   - Running products: `rp_A(i) = prod_{j<=i}(alpha - a_j)`, `rp_B(i) = prod_{j<=i}(alpha - b_j)`
   - Final running products must match
   - Security: false positive rate n/|F_p^3| <= 2^{-160}

2. **Evaluation Arguments**: Two lists are identical (same order)
   - Elements interpreted as polynomial coefficients
   - Used when row ordering matters

3. **Lookup Arguments (LogUp)**: All elements of list A appear in list B
   - Logarithmic derivative: `sum(1/(X - a_i)) = sum(m_i/(X - b_i))` where m_i = multiplicities
   - Extension columns accumulate fractions; base columns record multiplicity
   - Most computationally efficient subset verification

### Hash Coprocessor Architecture

The hash coprocessor consists of three sub-tables:

- **Hash Table** (67 base + 20 auxiliary columns): Performs Tip5 permutation
- **Cascade Table**: Translates 16-bit limb lookups to 8-bit S-box lookups
- **Lookup Table**: Contains precomputed S-box values

Hash Table operates in four modes:
- `program_hashing` (mode=1): Initial program hash
- `sponge` (mode=2): Sponge instructions
- `hash` (mode=3): Hash operations
- `pad` (mode=0): Padding rows

### Cross-Table Communication Pattern

Hash Table communicates via five running evaluation columns:
- `RunningEvaluationReceiveChunk`: Program chunks from Program Table
- `RunningEvaluationHashInput`: Hash input from Processor stack
- `RunningEvaluationHashDigest`: Hash output back to Processor
- `RunningEvaluationSponge`: Sponge operands with Processor
- 16 `LookupClientLogDerivative` columns: Limb lookups in Cascade Table

### Constraint Categories

Each table defines four constraint types:
- **Initial constraints**: First row conditions
- **Consistency constraints**: Per-row invariants
- **Transition constraints**: Row-to-row relationships
- **Terminal constraints**: Final row conditions

### Key Design Properties

- **Fixed table set**: Not designed for extensibility -- tables are hardcoded
- **Rich argument vocabulary**: Three argument types (permutation, evaluation, lookup) vs. most systems using only LogUp
- **Detailed specification**: Most thoroughly specified of all systems examined
- **Cascade architecture for S-boxes**: Elegant solution for width translation (16-bit to 8-bit lookups)
- **All verification in extension field**: F_p^3 with p = 2^64 - 2^32 + 1

---

## 6. Comparative Analysis

### Architectural Approaches

| System | Architecture | Extensibility | Bus Mechanism |
|--------|-------------|--------------|---------------|
| SP1 | Multi-table STARK | Open (new chips via enum) | LogUp cross-table lookup |
| RISC Zero | Monolithic + accelerators | Semi-open (app-defined precompiles) | Permutation + lookup arguments |
| OpenVM | No-CPU distributed chips | Fully open (VmExtension trait) | Interaction buses |
| Valida | CPU + coprocessor buses | Open (chip trait) | LogUp permutation argument |
| Triton VM | Fixed table set | Closed (hardcoded tables) | Permutation + evaluation + lookup |

### Precompile Invocation Patterns

| Pattern | Systems | Pros | Cons |
|---------|---------|------|------|
| **Syscall/ecall** | SP1, RISC Zero | Natural RISC-V integration, familiar model | Tied to RISC-V ISA |
| **Custom opcodes** | OpenVM | Maximum flexibility, no CPU bottleneck | More complex transpiler needed |
| **Fixed instructions** | Triton VM | Simple, well-specified | Not extensible |
| **Bus messages** | Valida | Clean separation, modular | Requires bus design upfront |

### Extension Registration Patterns

| Pattern | System | Description |
|---------|--------|-------------|
| **Enum variant** | SP1 | Add variant to `RiscvAir`, update `get_all()` |
| **Trait object** | OpenVM | Implement `VmExtension` traits, register via builder |
| **Chip trait** | Valida | Implement `Chip` trait with send/receive declarations |
| **None** | Triton VM | Hardcoded table set |

### Trace Generation Patterns

| System | Pattern |
|--------|---------|
| SP1 | Executor records events -> each chip's `generate_trace()` filters relevant events -> `generate_dependencies()` for recursive requirements |
| RISC Zero | Executor generates witness -> control/data/auxiliary columns filled -> accelerator columns via non-deterministic advice |
| OpenVM | Per-chip executors run instructions -> traces generated in reverse insertion order -> dependency resolution |
| Valida | Machine executes -> each chip's `generate_trace(&machine)` reads relevant state -> interaction traces computed |
| Triton VM | VM executes -> each table fills its trace -> cross-table arguments computed as auxiliary columns |

---

## 7. Design Lessons and Pitfalls

### Soundness Risks

1. **Under-constrained precompiles**: Missing constraints allow malicious provers to forge proofs. Determining if a circuit is under/over-constrained is NP-complete.
2. **Bus interaction completeness**: Every send must have a matching receive and vice versa. An unmatched interaction breaks soundness.
3. **Non-deterministic advice**: When host provides witness data (RISC Zero pattern), constraints must fully verify the result -- the host is untrusted.
4. **Timing side channels**: RISC Zero explicitly warns that precompiles do not guarantee constant-time execution.

### Architectural Best Practices

1. **LogUp over grand products**: LogUp (logarithmic derivatives) is more efficient than running product permutation arguments. All modern systems are converging on LogUp.
2. **Local state isolation**: SP1 keeps all precompile intermediate state in circuit columns -- no memory/register writes during precompile execution. This simplifies constraints.
3. **is_real flags**: SP1's approach of giving each chip an `is_real` boolean allows variable-length traces per chip without wasting proving resources on inactive chips.
4. **Two-level bus hierarchy**: Valida's local/global bus distinction is a useful pattern -- local buses for intra-chip connections, global for cross-chip.
5. **Dependency-driven trace generation**: SP1's `generate_dependencies()` pattern where chips can trigger additional proof requirements is elegant for composability.

### Flexibility vs. Efficiency Trade-offs

- **Most flexible**: OpenVM (no-CPU, fully distributed, VmExtension traits)
- **Most efficient**: Triton VM / RISC Zero (fixed tables/circuits, deeply optimized)
- **Best balance**: SP1 / Valida (open chip registration, LogUp buses, reasonable extension surface)

### Precompile Type Safety

- SP1: Syscall wrapper functions marked `unsafe` due to raw pointer operations; safe wrappers provided
- RISC Zero: Patched crate pattern hides precompile details behind existing safe APIs
- OpenVM: Type safety via `TranspilerExtension` checking instruction stream validity
- Formal verification (RISC Zero + Veridise/Picus) is the gold standard for precompile soundness

---

## 8. Relevance to Tabula

### Current Tabula Architecture Parallels

Tabula already has several patterns that align well with precompile/coprocessor design:

- **ChipId/BusId newtypes**: Open identifiers, no closed enum bottleneck (like Valida's approach)
- **define_bus! macro**: Typed send/receive methods (similar to Valida's interaction declarations)
- **Three-tier proof**: Execution -> Column -> Root (analogous to SP1's multi-table approach)
- **Plonky3 foundation**: Same framework as SP1 and Valida

### Design Considerations for Goal 6

1. **Extension trait pattern**: OpenVM's three-trait split (execution, circuit, prover) is the most composable. Tabula could adapt this with: `TabulaExtension` (execution), `TabulaCircuitExtension` (AIR), `TabulaWitnessExtension` (trace generation).

2. **Registration mechanism**: Tabula's `ChipRegistry` already supports dynamic registration. Adding extension-contributed chips should follow the builder pattern (OpenVM) rather than the enum pattern (SP1).

3. **Bus communication**: Tabula's existing bus system (`define_bus!`, `BusId`) provides the foundation. Extensions should declare their bus interactions at registration time (Valida's `local_sends/receives` pattern).

4. **Witness generation**: SP1's event-based pattern (executor records events, chips filter) maps well to Tabula's `TraceRecorder` -> chip trace generation pipeline.

5. **Cost accounting**: SP1's `num_extra_cycles()` pattern is useful for resource metering. Extensions should declare their proving cost.

6. **Soundness**: Every extension must declare complete send/receive pairs. The bus balance check (cumulative sum = 0) is the primary soundness mechanism. Consider requiring formal constraint descriptions at registration.

---

## Sources

- [Introducing SP1](https://blog.succinct.xyz/introducing-sp1/)
- [SP1 Precompiles 101](https://hackmd.io/@grandchildrice/By-6PQickx)
- [SP1 GitHub](https://github.com/succinctlabs/sp1)
- [Sphinx (SP1 fork)](https://argument.xyz/blog/sphinx-oss/)
- [SP1 Intrinsics (Scroll)](https://github.com/scroll-tech/sp1-intrinsics)
- [RISC Zero Precompiles Documentation](https://dev.risczero.com/api/zkvm/precompiles)
- [RISC Zero zkVM 1.2 Application-Defined Precompiles](https://risczero.com/blog/risczero-zkvm-1.2)
- [RISC Zero STARK Protocol](https://dev.risczero.com/proof-system/proof-system-sequence-diagram)
- [RISC Zero zkVM Specification](https://dev.risczero.com/api/zkvm/zkvm-specification)
- [OpenVM Specifications](https://docs.openvm.dev/specs/openvm/overview/)
- [OpenVM Whitepaper](https://openvm.dev/whitepaper.pdf)
- [OpenVM GitHub](https://github.com/openvm-org/openvm)
- [Triton VM Cross-Table Arguments](https://neptune.cash/learn/tvm-cross-table-args/)
- [Triton VM Hash Table Specification](https://triton-vm.org/spec/hash-table.html)
- [Triton VM Permutation Argument](https://triton-vm.org/spec/permutation-argument.html)
- [Valida Prover Architecture](https://lita.gitbook.io/lita-documentation/architecture/valida-zk-vm/technical-design-prover)
- [Valida Keccak Chip](https://www.lita.foundation/blog/keccak-acceleration-chip-and-benchmarks)
- [Valida GitHub](https://github.com/valida-xyz/valida)
- [zkVM Security: What Could Go Wrong?](https://blog.zksecurity.xyz/posts/zkvm-security/)
- [Plonky3 Proving System](https://lita.gitbook.io/lita-documentation/architecture/proving-system-plonky3)
