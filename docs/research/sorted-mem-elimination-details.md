# SortedMem Elimination: Detailed Problem Analysis

> Status: Active research (Feb 2025)
> Prerequisite: [sorted-mem-elimination.md](./sorted-mem-elimination.md)

## Overview

SortedMem 제거를 위해 해결해야 할 6개 문제를 식별하고 각각의 해법을 분석한다.

| # | Problem | Difficulty | Solution |
|---|---------|-----------|----------|
| P1 | Non-null Read 검증 | Easy | Exec→SSMC 직접 LogUp |
| P2 | Null Read 검증 (비멤버십) | Medium | SSMC gap query rows |
| P3 | Empty column Read 검증 | Easy | Exec→ColumnMeta LogUp |
| P4 | Write-set 전달 | Easy | Exec→Merge 직접 LogUp |
| P5 | Inter-tx 충돌 | Hard (정책) | Non-conflicting batch 가정 |
| P6 | Timestamp 제거 | Easy | tau/tau_rc 불필요 |

---

## P1: Non-null Read Verification

### 현재 경로
```
Exec Read(t,c,r)=val → C1 Memory → SortedMem init row → C2 SsmcMembership → SSMC
```

### 새 경로
```
Exec Read(t,c,r)=val → C1 ReadMembership → SSMC (직접)
```

### 구현

ExecutionChip에서:
```
// 기존 send_memory() 대신:
multiplicity = is_real * op_read * (1 - access_is_null)
values = (access_t, access_c, access_r[3], access_val[W])
bus = ReadMembership
```

SSMC에서:
```
// 기존 C2 receive와 동일한 형태:
multiplicity = is_real * is_entry * mult_witness
values = (table_id, col_id, key[3], value[W])
bus = ReadMembership
```

### Soundness

- Prover가 Read 값을 조작하면? → SSMC에 (r, val) 엔트리가 없으므로 LogUp 불균형
- Prover가 SSMC에 가짜 엔트리 추가? → hash chain이 달라져서 Com_old ≠ oldRoot
- Prover가 SSMC 엔트리 제거? → hash chain이 달라져서 Com_old ≠ oldRoot

**결론: 안전하다.**

---

## P2: Null Read Verification (Non-Membership)

### 문제

Read(t,c,r)=null 일 때, key r이 SSMC에 없음을 증명해야 한다.
현재는 SortedMem의 gap witness (M11 설계)가 이를 담당하도록 계획되어 있었다.

### 해법: SSMC Gap Query Rows

SSMC의 정렬된 엔트리 사이에 **gap query row**를 삽입한다:

```
SSMC segment for (t=1, c=2):
  row 0: entry  key=5   val=v1     is_entry=1
  row 1: gap    key=7              is_entry=0   ← null read for key=7
  row 2: entry  key=10  val=v2     is_entry=1
  row 3: gap    key=14             is_entry=0   ← null read for key=14
  row 4: entry  key=20  val=v3     is_entry=1
```

**핵심 관찰**: 모든 행 (entry + gap)의 키가 strict ordering을 유지하면,
gap row의 위치 자체가 비멤버십 증명이 된다.

- key=7은 row 0 (key=5)과 row 2 (key=10) 사이에 있다
- strict ordering: 5 < 7 < 10
- SSMC 엔트리 키는 {5, 10, 20}이므로 7은 없다

### SSMC 변경사항

**새 컬럼** (+2):

```rust
/// 1 if this row is a committed entry, 0 if gap query.
pub is_entry: T,

/// Running flag: has any is_entry=1 row appeared in this segment.
/// Used to derive entry_is_first.
pub has_entry_in_segment: T,
```

**새 제약**:

```
// 1. Boolean
is_entry is boolean
has_entry_in_segment is boolean

// 2. Key ordering: 모든 real 행에 대해 (entry든 gap이든)
//    기존 key_ordering 제약 그대로 유지 — 변경 없음

// 3. Hash chain: entry만 참여
//    Gap row: hash_acc 전달 (변경 없음)
NOT is_entry => next.hash_acc = local.hash_acc  (within segment)

//    Entry row: Poseidon 결과로 hash_acc 갱신
//    (기존 hash_chain constraint, is_entry로 gate)

// 4. entry_is_first 유도 (별도 컬럼 불필요, 인라인)
entry_is_first := is_entry * (1 - has_entry_in_segment)

// 5. has_entry_in_segment 전이
//    Segment 시작: 0
//    같은 segment: has_entry_in_segment_next = has_entry_in_segment OR is_entry
//    tc_changed: reset to 0

// 6. Boundary flags 수정
//    is_first / is_last는 첫/끝 REAL ROW (entry든 gap이든)
//    CommitmentVerif는 is_last 시점의 hash_acc 사용
//    gap row의 hash_acc = 마지막 entry의 hash_acc (carry)이므로 정확
```

**버스 변경**:

```
// Poseidon 전송: entry만
C5 mult = is_real * is_entry  (기존: is_real)

// NonMembership 전송: gap만
C2_new mult = is_real * (1 - is_entry)
values = (table_id, col_id, key[3])

// MergeOldList: entry만
C3 mult = is_real * is_entry * segment_is_touched

// CommitmentVerif: 변경 없음 (is_last 시점의 hash_acc)
C6 mult = is_real * is_last  (hash_acc는 gap row에서도 carry됨)

// SsmcMembership: entry만
C1_new mult = is_real * is_entry * mult_witness
```

### 경계 케이스

**첫 엔트리 전의 gap** (key < 모든 entry):
```
  row 0: gap    key=2        is_entry=0
  row 1: entry  key=5  ...   is_entry=1
```
- is_first=1 (row 0은 첫 real row)
- strict ordering: 2 < 5 (row 0 → row 1)
- key=2는 모든 엔트리보다 작으므로 집합에 없다 ✓

**마지막 엔트리 후의 gap** (key > 모든 entry):
```
  row 3: entry  key=20 ...   is_entry=1
  row 4: gap    key=25       is_entry=0
```
- is_last=1 (row 4는 마지막 real row 또는 row 3이 is_last)

Wait — is_last는 어떤 행에 붙는가? 현재 is_last는 "segment의 마지막 real row".
Gap이 마지막이면 is_last=1이 gap row에 붙는다. hash_acc는 entry key=20에서
carry됐으므로 CommitmentVerif는 올바른 Com_old를 전송한다.

실제로 is_last를 entry가 아닌 gap에 붙이면, CommitmentVerif 전송이 gap row에서
일어난다. 이때 hash_acc는 마지막 entry의 것과 같으므로 문제 없다.

**빈 gap (null read 0개)**: Gap row가 없으면 기존 SSMC와 동일하게 작동.
is_entry=1만 있고, has_entry_in_segment은 첫 entry에서 바로 1.

### Soundness 분석

**Prover가 없는 키에 대해 non-null Read를 주장하면?**
- ReadMembership에 (t,c,r,val) 전송
- SSMC에 key=r인 entry가 없음
- LogUp 불균형 → 실패

**Prover가 있는 키에 대해 null Read를 주장하면?**
- NonMembership에 (t,c,r) 전송
- SSMC에 gap row key=r 배치 시도
- 하지만 entry key=r도 존재 → strict ordering 위반 (같은 키 두 번)
- 또는 entry를 제거 → hash chain 변경 → Com_old ≠ oldRoot
- → 실패

**Prover가 gap row를 잘못된 위치에 배치하면?**
- strict ordering 제약 위반 → 실패

**결론: 안전하다.**

---

## P3: Empty Column Read Verification

### 문제

빈 컬럼 (is_empty_old=1)에서 Read하면 항상 null이다.
SSMC에는 이 컬럼의 segment가 없으므로 P2로 처리 불가.

### 해법: EmptyColRead Bus

ExecutionChip에 `is_empty_col` witness 컬럼 추가 (+1 col).

```
// Execution:
mult = is_real * op_read * access_is_null * is_empty_col
values = (access_t, access_c)
bus = EmptyColRead (send)

// ColumnMeta:
mult = is_real * empty_read_count  // prover witness
values = (table_id, col_id)
bus = EmptyColRead (receive)
constraint: EmptyColRead receive 시 is_empty_old = 1
```

### Soundness

- Prover가 is_empty_col=1로 속이면 (실제 비어있지 않은 컬럼)?
  → ColumnMeta의 is_empty_old=0 → Com_old ≠ Com_empty → EmptyColRead 튜플 불일치
  (실제로는 is_empty_old=0이면 receive multiplicity가 0이므로 LogUp 불균형)

- Prover가 is_empty_col=0으로 속이면 (실제 빈 컬럼)?
  → NonMembership bus로 가지만, 빈 컬럼은 SSMC segment 없음
  → gap row 배치 불가 → LogUp 불균형

**Prover는 정직할 수밖에 없다.**

---

## P4: Write-Set Delivery

### 현재 경로
```
Exec Write → C1 Memory → SortedMem → is_last_for_key+has_written → C4 → Merge
```

### 새 경로
```
Exec Write → C4 WriteSet → Merge (직접)
```

### 구현

```
// Execution:
multiplicity = is_real * op_write
values = (access_t, access_c, access_r[3], access_val[W], access_is_null)
bus = MergeWriteSet (send)

// Merge: 기존 C4 receive와 동일
multiplicity = is_real * (1 - is_old_only)
```

### Non-conflicting batch 가정 필요

같은 키에 대해 두 tx가 Write하면, Execution에서 두 번 send하지만
Merge에는 해당 키에 대해 한 row만 있다. LogUp 불균형.

**따라서 P4는 "같은 (t,c,r)에 대해 배치 내 최대 1개 Write"를 전제로 한다.**

---

## P5: Inter-Tx Conflict Policy

### 문제의 본질

Non-conflicting batch 가정이 없으면 SortedMem이 필요한 이유:

```
tx_0: Write(t,c,r) = v1
tx_1: Read(t,c,r) → should see v1 (not base state)
```

SortedMem 없이는 tx_1의 Read가 base state 값을 반환하며,
tx_0의 Write를 볼 수 없다.

### 가정의 정의

**Non-conflicting batch**: 배치 내 모든 tx 쌍 (tx_i, tx_j)에 대해,
tx_i가 키 k에 Write하면, tx_j는 키 k에 Read하지 않는다.

허용:
- tx_i Read(k), tx_j Read(k) → 둘 다 base state에서 읽음 ✓
- tx_i Write(k), tx_j Read(k') where k≠k' → 다른 키 ✓
- tx_i Write(k), tx_j Write(k') where k≠k' → 다른 키 ✓

금지:
- tx_i Write(k), tx_j Read(k) → 충돌 ✗
- tx_i Write(k), tx_j Write(k) → 충돌 ✗

### 프로토콜 레벨 실현 가능성

**가능한 이유**:
1. Sequencer가 tx 의존성 그래프를 분석하여 충돌 없는 배치를 구성
2. 대부분의 L2/rollup 시스템이 유사한 최적화를 이미 수행
3. 충돌 tx는 별도 배치에 배치 (처리량 약간 감소)

**어려운 케이스**:
1. Hot key (카운터, 글로벌 상태): 모든 tx가 접근 → 배치 크기 1로 제한
2. AMM pool: swap tx마다 잔고 갱신 → 순차 배치 필요

**평가**: 실현 가능하지만 프로토콜 설계에 영향을 준다.
Hot key 패턴은 별도 메커니즘 (atomic counter 등)으로 처리 가능.

### In-proof conflict check (선택사항)

Proof 내에서 non-conflicting을 검증하는 경량 테이블:

```
ConflictCheckCols:
  is_real, tx_index, t, c, r[3], is_write   (12 cols)
```

모든 access를 (t, c, r, tx_index) 순으로 정렬.
같은 (t,c,r)의 연속 행에서 하나가 write이면 tx_index가 같아야 한다.

이것은 SortedMem (67 cols)보다 훨씬 작다 (12 cols).
Memory carry (mem, mem_is_null), write-set extraction, hash/ordering 등이 불필요.

---

## P6: Timestamp Elimination

### 현재

- `clk`: access instruction counter (1 col)
- `tau`: clk + 1 when is_access (1 col)
- `tau_rc`: KeyRangeChecked for tau (6 cols: 3 limbs + 3 halves)

tau는 오직 C1 Memory bus를 통해 SortedMem으로 전송된다.

### SortedMem 제거 시

tau가 어떤 버스에도 사용되지 않으므로 제거 가능:

- `tau` 제거: -1 col
- `tau_rc` 제거: -6 cols
- tau 관련 range check sends 제거: -4 RC sends

clk는 Budget 검증 (max_accesses)에 유용할 수 있으나,
trace length로 이미 암시적 검증이 가능하므로 제거 가능.

**잠재적 절약: ExecutionChip -7~8 cols**

---

## Column Budget Summary

| Chip | Current | Post-Elimination | Delta |
|------|---------|-----------------|-------|
| ExecutionChip | 278 | 272 | -6 (remove tau_rc, add is_empty_col) |
| GlobalSortedMem | 67 | 0 | **-67** |
| GlobalSSMC | 66 | 68 | +2 (is_entry, has_entry_in_segment) |
| GlobalMerge | 74 | 74 | 0 |
| ColumnMeta | 56 | 56 | ~0 (remove has_sorted_mem, add empty_read) |
| PoseidonChip | 112 | 112 | 0 |
| RangeCheckChip | 2 | 2 | 0 |
| **Total** | **655** | **584** | **-71** |

Row 절약: SortedMem의 O(A) 행 제거 (A = 전체 access 수).
SSMC gap row 추가는 O(G) (G = null read 수, 보통 G << A).

---

## Open Questions

### Q1: SSMC의 hash_acc carry는 정확한가?

Entry가 아닌 gap row에서 hash_acc를 carry할 때, 다음 entry의
hash_chain_input이 prev.hash_acc를 참조한다. Gap row를 통해 carry된
hash_acc는 마지막 entry의 것과 동일하므로 정확하다.

단, 첫 entry 전의 gap row: hash_acc가 미정의. 하지만 첫 entry의
hash_chain_input은 `entry_is_first`일 때 fresh start (domain tag)이므로
prev.hash_acc를 사용하지 않는다. 따라서 안전하다.

### Q2: SSMC segment에 entry가 0개인 경우?

SSMC segment는 is_empty_old=0 (비어있지 않은 컬럼)에만 존재.
비어있지 않으면 최소 1개 entry가 있다.
따라서 entry=0인 segment는 불가능하며, has_entry_in_segment는
반드시 1이 된다.

### Q3: M11 설계와의 충돌?

M11은 SortedMem에서 gap witness를 source로 가정했다.
SortedMem 제거 시 M11은 재설계가 필요:
- Gap witness → SSMC gap query rows로 대체
- SmtPathChip: ColumnMeta의 com_old/com_new를 leaf로 → 변경 없음
- StaticTableChip: 변경 없음

M11의 대부분은 SSMC-independent (SMT path, public inputs)이므로
영향은 gap witness 관련 부분에 한정된다.

### Q4: SortedMem 관련 테스트는?

현재 359 테스트 중 SortedMem 관련:
- sorted_mem/air 테스트: ~30개 (제거)
- sorted_mem/trace 테스트: ~20개 (제거)
- integration/bus.rs의 Memory bus 테스트: 수정 필요
- witness/generator 테스트: 수정 필요

새로 추가될 테스트:
- SSMC gap query 제약: ~15개
- ReadMembership bus: ~5개
- NonMembership bus: ~5개
- EmptyColRead bus: ~5개
- Execution direct sends: ~10개

### Q5: Batch semantics 변경이 필요한가?

현재 semantics-spec §2.2:
```
S_{i+1} = apply(S_i, WriteSet_i)
```

Non-conflicting 가정 하에서는 모든 tx가 BaseState에서 읽으므로:
```
WriteSet_i = execute(BaseState, tx_i)  // S_i 대신 BaseState
```

이것은 **parallel batch semantics**:
```
for each tx_i (independent):
    WriteSet_i = execute(BaseState, tx_i)
WriteSet_batch = union(WriteSet_0, ..., WriteSet_{N-1})  // 충돌 없음
newState = apply(BaseState, WriteSet_batch)
```

semantics-spec의 sequential 정의와 호환되면서도 더 단순하다.
충돌이 없으면 sequential과 parallel 결과가 동일하기 때문이다.
