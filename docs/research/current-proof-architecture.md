# Current Proof Architecture

> Status: Reference document (Feb 2025)
> Scope: Complete description of the implemented proof system as of M10

## 1. What We Prove

Given a batch of transactions, prove the state transition:

```
oldRoot  --[tx_0, tx_1, ..., tx_{N-1}]-->  newRoot
```

**Public inputs** (verifier sees these):
- `oldRoot`, `newRoot`: state root digests
- `AppliedTxDigest`: commitment to applied transaction list
- `ProgramRoot`: commitment to registered tx type definitions
- `StaticTableRoot`: commitment to static lookup tables
- `budgets`: resource limits (max_ops, max_slots, max_accesses)

**What the proof guarantees**:
1. Each transaction executed correctly (instruction semantics)
2. Each Read returned the correct value from the state
3. Each Write produced the correct state update
4. The new state root reflects all writes correctly

## 2. Design Invariants from IR

Tabula's IR enforces structural invariants that eliminate the need for
intra-tx RAM consistency arguments. These are **compile-time guarantees**,
not runtime checks:

| Rule | Meaning | Proof Consequence |
|------|---------|-------------------|
| **NF-1** Unique-Read | At most one `Read(t,c,r)` per tx | No "same read returns same value" proof needed |
| **NF-2** Unique-Write | At most one `Write(t,c,r)` per tx | No intra-tx write coalescing |
| **NF-3** No-Read-After-Write | Can't read a key after writing it | No intra-tx memory ordering |
| **True SSA** | Each slot assigned exactly once | No local variable consistency argument |
| **Static (t,c)** | Table and column are compile-time constants | Per-(t,c) sharding of memory arguments |

**Key insight**: Within a single tx, there is NO memory consistency problem.
The proof system only needs to verify:
- Instruction arithmetic/logic is correct
- Read values match the base state
- Write values are correctly committed

## 3. The Seven Chips

### 3.1 ExecutionChip (278 cols, W=3, S=16)

**Purpose**: Proves each instruction executed correctly.

One row per instruction. Contains:
- 12 opcode one-hot selectors (Read, Write, Arith, DivMod, Cmp, Not, And, Or, Assert, Select, Hash, Lookup)
- 16 SSA slots (each W=3 limbs + null flag + written flag)
- Operand witness values (src1, src2, cond) with one-hot slot selectors
- Access log (t, c, r, is_write, val, is_null) for Read/Write instructions
- Clock counter and timestamp (tau = clk + 1)
- Per-opcode witness columns (Cmp: 27, Mul: 5, DivMod: 36)

**Constraints**:
- Opcode exactly-one (sum of selectors = 1 when is_real)
- Per-opcode semantics (Add carry chain, Mul cross-product, Cmp inequalities, etc.)
- SSA carry: non-written slots propagate to next row
- Clock monotonicity: clk increments by is_access
- Operand linkage: src1_val matches slots[selected_slot] via one-hot selector

**Sends to buses**:
- **C1 Memory** (is_access): `(t, c, r[3], tau[3], is_write, val[W], is_null)` -- 13 elements
- **C5 Poseidon** (op_hash): `(perm_input[16], perm_output[8])`
- **C9 StaticTableLookup** (op_lookup): `(t, c, r[3], val[W])`
- **C8 RangeCheck**: r halves, tau halves (gated by is_access)

### 3.2 GlobalSortedMem (67 cols, W=3)

**Purpose**: Memory consistency -- proves Read values match state,
extracts write-set for commitment update.

One row per memory access event (init rows + execution accesses),
sorted by `(t, c, r, tau)`.

Contains:
- Identity: table_id, col_id
- Key `r` and timestamp `tau` (each as KeyRangeChecked: 3 limbs + halves)
- Flags: is_init, is_write, is_last_for_key, has_written, r_changed
- Value: val[W] + val_is_null (trace encoding)
- Running memory: mem[W] + mem_is_null (carries last-written value)
- Ordering: shared StrictIneq for r-ordering or tau-ordering
- Segment detection: SameKeyDetection + LexOrderingDirection

**Constraints**:
- Sorted by (t, c, r, tau) via lex ordering + strict inequalities
- Init rows: tau=0, is_write=0, mem=val (seeds base state)
- Read consistency: is_write=0 => val = mem
- Write update: same_key & next.is_write=1 => next.mem = next.val
- Memory carry: same_key & next.is_write=0 => next.mem = local.mem
- Write-set extraction: is_last_for_key flags + has_written propagation

**Bus interactions**:
- **C1 Memory (receive)**: non-init rows match Execution accesses. `mult = is_real * (1 - is_init)`
- **C2 SsmcMembership (send)**: init rows with non-null values from non-empty columns. `mult = is_real * is_init * (1 - val_is_null) * (1 - meta_is_empty_old)`
- **C4 MergeWriteSet (send)**: write-set rows. `mult = is_real * is_last_for_key * has_written`
- **C7 SortedMemMeta (send)**: one per segment. `mult = is_real * is_first_of_segment`
- **C8 RangeCheck**: r halves, tau halves, ordering diffs, lex diffs

### 3.3 GlobalSSMC (66 cols, W=3)

**Purpose**: Proves the old commitment (Com_old) for each column
is the Poseidon hash chain of its sorted entries.

One row per key-value entry in the committed state, sorted by key
within each (t,c) segment.

Contains:
- Identity: table_id, col_id
- Key (KeyRangeChecked) + value[W] (tier-1 ComEnc, non-null only)
- Boundary flags: is_first, is_last
- Hash accumulator: hash_acc[8] (running Poseidon digest)
- Hash chain input: HashChainInput (16-element permutation input)
- Ordering: key strict inequality within segment
- Segment detection + lex ordering across segments
- mult_witness, segment_is_touched

**Constraints**:
- Keys strictly ordered within segment (borrow-chain inequality)
- Hash chain: first row uses domain_tag=0x00||t||c||key||val; subsequent rows
  chain prior hash_acc into next permutation input
- Boundary: is_first at segment start, is_last at segment end

**Bus interactions**:
- **C2 SsmcMembership (receive)**: verifies init-row values from SortedMem. `mult = is_real * mult_witness`
- **C3 MergeOldList (send)**: sends all entries of touched segments. `mult = is_real * segment_is_touched`
- **C6 CommitmentVerif (send)**: at segment end, sends Com_old digest. `(t, c, tag=0, is_touched, digest=hash_acc)`
- **C5 Poseidon (send)**: every row requests a permutation
- **C8 RangeCheck**: key halves, ordering diffs, lex diffs

### 3.4 GlobalMerge (74 cols, W=3)

**Purpose**: Proves the 3-way merge (OldList + WriteSet -> NewList)
and computes the new commitment (Com_new).

One row per merge entry (old-only, write-only, both, or delete),
sorted by key within each (t,c) segment.

Contains:
- Identity: table_id, col_id
- Key (KeyRangeChecked)
- Source encoding: s1, s0 (2-bit: 00=old_only, 01=write_only, 10=both, 11=delete)
- Values: old_val[W], write_val[W], new_val[W]
- Output flag: in_new (1 = entry in NewList, 0 = deleted)
- Hash chain for NewList: hash_acc[8] + HashChainInput
- Merge tracking: is_first_in_new, has_prev_in_new, is_last_segment

**Constraints**:
- Merge semantics per source type:
  - old_only: new_val=old_val, in_new=1
  - write_only: new_val=write_val, in_new=1
  - both: new_val=write_val, in_new=1
  - delete: in_new=0, write_val=canonical_zero
- Hash chain only includes in_new=1 rows
- Keys strictly ordered within segment

**Bus interactions**:
- **C3 MergeOldList (receive)**: old entries from SSMC. `mult = is_real * (1 - is_write_only)`
- **C4 MergeWriteSet (receive)**: write entries from SortedMem. `mult = is_real * (1 - is_old_only)`
- **C6 CommitmentVerif (send)**: at segment end, sends Com_new. `(t, c, tag=1, is_touched=1, digest)`
- **C5 Poseidon (send)**: only in_new=1 rows
- **C8 RangeCheck**: key halves, ordering diffs, lex diffs

### 3.5 ColumnMeta (56 cols)

**Purpose**: Directory of all columns in the batch. Binds old/new
commitments to the state root (via SMT paths in M11).

One row per (t,c) column in the batch, strictly ordered by (t,c).

Contains:
- Identity: table_id, col_id, tag (0=SSMC, 1=SMT)
- Commitments: com_old[8], com_new[8]
- Flags: is_empty_old, is_empty_new, is_touched, has_sorted_mem
- Com_empty verification: empty_perm_input[16], empty_perm_output[8], has_empty_check
- Ordering: table_diff_iz, col_diff_iz (uniqueness), lex direction

**Constraints**:
- Strict (t,c) uniqueness (no duplicate columns)
- Untouched: is_touched=0 => com_new = com_old
- Empty transition: is_empty_old=1 & is_touched=1 => is_empty_new=0
- Com_empty verification: Poseidon(0x00||t||c||padding) when empty

**Bus interactions**:
- **C7 SortedMemMeta (receive)**: one per segment from SortedMem. `mult = is_real * has_sorted_mem`
- **C6 CommitmentVerif (receive)**: Com_old from SSMC (tag=0), Com_new from Merge (tag=1)
- **C5 Poseidon (send)**: Com_empty hash verification
- **C8 RangeCheck**: lex diffs

### 3.6 PoseidonChip (93 main + 19 preprocessed cols)

**Purpose**: Shared Poseidon2 permutation service. All chips that need
hashing send to the PoseidonPermutation bus; this chip performs the
actual permutation and provides the output.

21 rows per permutation (Poseidon2: 8 full + 13 partial rounds).

**Bus interactions**:
- **C5 Poseidon (receive)**: input[16] + output[8] from all requesting chips

### 3.7 RangeCheckChip (2 cols)

**Purpose**: Proves that field elements are in [0, 2^16) via lookup table.

**Bus interactions**:
- **C8 RangeCheck (receive)**: single value per send, verified against the table

## 4. Bus Topology

```
                    C1 Memory
ExecutionChip  ─────────────────►  GlobalSortedMem
    │                                   │
    │ C5 Poseidon                       │ C2 SsmcMembership
    │ C9 StaticTableLookup              ▼
    │                              GlobalSSMC
    │                                   │
    │                                   │ C3 MergeOldList
    │                                   ▼
    │  C4 MergeWriteSet            GlobalMerge
    │  (SortedMem ──►)                  │
    │                                   │ C6 CommitmentVerif
    │                                   ▼
    │                              ColumnMeta
    │                                   │
    │  C7 SortedMemMeta                 │ C5 Poseidon
    │  (SortedMem ──►)                  ▼
    │                              PoseidonChip
    │                                   │
    └───────────► C8 RangeCheck ◄───────┘
                  RangeCheckChip
```

Nine LogUp buses:
| ID | Bus | Width | Sender(s) | Receiver(s) |
|----|-----|-------|-----------|-------------|
| 1 | Memory | 13 | Execution | SortedMem |
| 2 | SsmcMembership | 8 | SortedMem | SSMC |
| 3 | MergeOldList | 8 | SSMC | Merge |
| 4 | MergeWriteSet | 9 | SortedMem | Merge |
| 5 | PoseidonPerm | 24 | all hashers | PoseidonChip |
| 6 | CommitmentVerif | 12 | SSMC, Merge | ColumnMeta |
| 7 | SortedMemMeta | 3 | SortedMem | ColumnMeta |
| 8 | RangeCheck | 1 | all chips | RangeCheckChip |
| 9 | StaticTableLookup | 8 | Execution | (deferred M11) |

## 5. Data Flow: End-to-End

### 5.1 Read Path

```
tx reads (t,c,r):
  1. ExecutionChip: Read instruction, access_val = v, tau = clk+1
  2. ExecutionChip sends (t,c,r,tau,is_write=0,v,is_null) via C1
  3. SortedMem receives via C1, matches to access row
  4. SortedMem init row (tau=0) has same value v (seeded from base state)
  5. SortedMem sends (t,c,r,v) via C2 to SSMC (for non-null, non-empty)
  6. SSMC receives via C2, verifying (r,v) exists in sorted entries
  7. SSMC computes hash chain digest, sends to ColumnMeta via C6
  8. ColumnMeta verifies com_old matches SSMC digest
  9. [M11] ColumnMeta verifies com_old inclusion in oldRoot via SMT path
```

### 5.2 Write Path

```
tx writes (t,c,r) = v:
  1. ExecutionChip: Write instruction, access_val = v, tau = clk+1
  2. ExecutionChip sends (t,c,r,tau,is_write=1,v,is_null) via C1
  3. SortedMem receives via C1, matches to access row
  4. SortedMem: is_last_for_key=1, has_written=1 (this is the write-set entry)
  5. SortedMem sends (t,c,r,v,is_delete) via C4 to Merge
  6. Merge receives old entry from SSMC via C3, write entry via C4
  7. Merge computes 3-way merge, produces NewList hash chain
  8. Merge sends com_new digest to ColumnMeta via C6
  9. ColumnMeta verifies transition: com_old -> com_new for touched columns
 10. [M11] ColumnMeta proves newRoot reflects com_new via SMT path
```

### 5.3 Inter-Tx Flow (Multiple Txs in Batch)

When tx_0 writes (t,c,r)=v1 and tx_1 reads (t,c,r):

```
SortedMem sorted by (t,c,r,tau):
  row 0: (t,c,r, tau=0, init, val=base_val)     -- init from base state
  row 1: (t,c,r, tau=3, write, val=v1)            -- tx_0's write
  row 2: (t,c,r, tau=7, read, val=v1)             -- tx_1's read

Memory carry:
  row 0: mem=base_val
  row 1: mem=v1 (write updates mem)
  row 2: mem=v1 (read sees v1, which is correct)
```

The sorted ordering ensures tx_1 sees tx_0's write.

## 6. Role Analysis: What Each Component Actually Does

### SortedMem's Four Roles

| Role | Description | Who Needs It |
|------|-------------|-------------|
| **R1: Read verification** | Init rows bind Read values to base state | Soundness of Reads |
| **R2: Inter-tx ordering** | Sorting by tau ensures later txs see earlier writes | Multi-tx batches |
| **R3: Write-set extraction** | is_last_for_key + has_written identifies final writes | Merge input |
| **R4: Segment metadata** | Tells ColumnMeta which columns were accessed | ColumnMeta completeness |

### SSMC's Three Roles

| Role | Description |
|------|-------------|
| **Old commitment proof** | Hash chain of sorted entries = Com_old |
| **Membership verification** | LogUp proves (key,value) exists in committed set |
| **Old entries for Merge** | Sends entries to Merge for 3-way merge |

### Merge's Two Roles

| Role | Description |
|------|-------------|
| **3-way merge proof** | OldList + WriteSet -> NewList with correct semantics |
| **New commitment computation** | Hash chain of NewList entries = Com_new |

## 7. Column Budget

| Chip | Cols (W=3) | % of total |
|------|-----------|------------|
| ExecutionChip | 278 | 48% |
| GlobalMerge | 74 | 13% |
| GlobalSortedMem | 67 | 12% |
| GlobalSSMC | 66 | 11% |
| ColumnMeta | 56 | 10% |
| PoseidonChip | 93+19 | (shared) |
| RangeCheckChip | 2 | <1% |
| **Total** | **558 + 112** | |

SortedMem accounts for 12% of the total column budget.

## 8. Soundness Chain

The proof is sound because every link in the chain is enforced:

```
Public Input: oldRoot
    │
    ▼ [M11: SMT inclusion proof]
ColumnMeta: com_old for each (t,c)
    │
    ▼ [C6: CommitmentVerif LogUp]
SSMC: hash_chain(entries) = com_old
    │
    ▼ [C2: SsmcMembership LogUp]
SortedMem: init_row.val = SSMC entry value
    │
    ▼ [Memory carry constraints]
SortedMem: access_row.val = mem (for reads)
    │
    ▼ [C1: Memory LogUp]
Execution: Read returns correct value
    │
    ▼ [Instruction constraints]
Execution: computation is correct
    │
    ▼ [C1: Memory LogUp]
SortedMem: Write values received
    │
    ▼ [Write-set extraction]
SortedMem: final writes identified
    │
    ▼ [C4: MergeWriteSet LogUp]
Merge: receives write-set
    │
    ▼ [C3: MergeOldList LogUp from SSMC]
Merge: receives old entries
    │
    ▼ [Merge constraints]
Merge: NewList = correct merge of OldList + WriteSet
    │
    ▼ [Hash chain]
Merge: hash_chain(NewList) = com_new
    │
    ▼ [C6: CommitmentVerif LogUp]
ColumnMeta: com_new for each touched (t,c)
    │
    ▼ [M11: SMT update proof]
Public Input: newRoot
```

Every arrow is either an AIR constraint or a LogUp bus,
both of which are cryptographically enforced by the STARK.
