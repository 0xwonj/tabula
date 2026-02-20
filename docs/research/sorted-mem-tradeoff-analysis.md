# SortedMem Elimination: Trade-off Analysis

> Status: Analysis complete (Feb 2025)

## 1. Quantitative Analysis

### 1.1 Column Savings

| Chip | Current | After | Delta |
|------|---------|-------|-------|
| ExecutionChip | 278 | 272 | -6 |
| **GlobalSortedMem** | **67** | **0** | **-67** |
| GlobalSSMC | 66 | 68 | +2 |
| GlobalMerge | 74 | 74 | 0 |
| ColumnMeta | 56 | 56 | 0 |
| PoseidonChip | 112 | 112 | 0 |
| RangeCheckChip | 2 | 2 | 0 |
| **Total** | **655** | **584** | **-71 (-11%)** |

### 1.2 Trace Cell Analysis (Representative Workload)

**Workload**: 100 txs, 30 instructions/tx, 15 accesses/tx

| Parameter | Value |
|-----------|-------|
| I (total instructions) | 3,000 |
| A (total accesses) | 1,500 |
| K (unique keys) | 1,200 |
| M (SSMC entries) | 5,000 |
| W (writes) | 700 |
| G (null reads, non-empty) | 100 |
| C (columns touched) | 50 |
| Poseidon perms | ~10,000 |

**Trace heights** (padded to power of 2):

| Chip | Current H | After H | Notes |
|------|----------|---------|-------|
| Execution | 4,096 | 4,096 | I=3000→4096 |
| SortedMem | 4,096 | — | K+A=2700→4096 |
| SSMC | 8,192 | 8,192 | M+G=5100→8192 |
| Merge | 8,192 | 8,192 | M+W=5700→8192 |
| ColumnMeta | 64 | 64 | C=50→64 |
| Poseidon | 262,144 | 262,144 | 10K×21≈210K→262144 |
| RangeCheck | 65,536 | 65,536 | fixed 2^16 |

**Total trace cells** (width × height):

| Chip | Current | After | Delta |
|------|---------|-------|-------|
| Execution | 1,138,688 | 1,114,112 | -24,576 |
| SortedMem | 274,432 | 0 | **-274,432** |
| SSMC | 540,672 | 557,056 | +16,384 |
| Merge | 606,208 | 606,208 | 0 |
| ColumnMeta | 3,584 | 3,584 | 0 |
| Poseidon | 29,360,128 | 29,360,128 | 0 |
| RangeCheck | 131,072 | 131,072 | 0 |
| **Total** | **32,054,784** | **31,772,160** | **-282,624** |

**Trace cell savings: 282K / 32M = 0.9%**

### 1.3 Why the Savings Are Small

```
Prover cost breakdown (current):

  Poseidon     ████████████████████████████████████████████ 91.6%
  Execution    ████                                         3.6%
  Merge        ██                                           1.9%
  SSMC         ██                                           1.7%
  SortedMem    █                                            0.9%
  RangeCheck   ▌                                            0.4%
  ColumnMeta   ▏                                            0.01%
```

**Poseidon이 전체 prover 비용의 91.6%를 차지한다.**

SortedMem은 전체의 0.9%에 불과하며, 이를 제거해도 prover 성능에
유의미한 차이가 없다.

### 1.4 NTT Cost (More Precise)

NTT cost ∝ width × height × log₂(height):

| Chip | Current NTT | After NTT | Delta |
|------|------------|-----------|-------|
| Execution | 13,664,256 | 13,369,344 | -294,912 |
| SortedMem | 3,293,184 | 0 | **-3,293,184** |
| SSMC | 7,028,736 | 7,241,728 | +212,992 |
| Merge | 7,884,800 | 7,884,800 | 0 |
| Poseidon | 527,958,016 | 527,958,016 | 0 |
| RangeCheck | 2,097,152 | 2,097,152 | 0 |
| **Total** | **562M** | **558.6M** | **-3.4M** |

**NTT savings: 3.4M / 562M = 0.6%**

### 1.5 Code Savings

| Category | Current | After | Delta |
|----------|---------|-------|-------|
| SortedMem src | 844 | 0 | -844 |
| SortedMem tests | 506 | 0 | -506 |
| Memory bus code | 99 | 0 | -99 |
| SSMC modifications | — | +100 | +100 |
| Execution modifications | — | +50 | +50 |
| New bus traits | — | +80 | +80 |
| New tests | — | +200 | +200 |
| **Net** | **1,449** | **430** | **-1,019** |

**코드 절감: ~1,000 lines (proof crate 전체 17K 중 6%)**

---

## 2. Qualitative Analysis

### 2.1 Architecture Simplification (HIGH value)

| Aspect | Current | After |
|--------|---------|-------|
| Chips | 7 | 6 |
| LogUp buses | 9 | 9 (but simpler) |
| Data flow hops for Read | 4 (Exec→Sorted→SSMC→Meta) | 2 (Exec→SSMC) |
| Data flow hops for Write | 4 (Exec→Sorted→Merge→Meta) | 2 (Exec→Merge) |
| Intermediate state | mem[W], mem_is_null carry | none |
| Soundness audit surface | 7 chips × N buses | 6 chips × N buses |

SortedMem은 Execution과 SSMC/Merge 사이의 **불필요한 중간 레이어**였다.
제거하면 데이터 흐름이 직접적이 되어 soundness 추론이 단순해진다.

### 2.2 Design Alignment (HIGH value)

Tabula의 핵심 설계 철학:

> SSA + Normal Form으로 intra-tx RAM consistency를 구조적으로 제거한다.

현재 아키텍처는 이 철학과 모순된다:
- NF가 intra-tx 문제를 제거했는데, SortedMem이 inter-tx를 위해 다시 도입됨
- SortedMem은 Cairo/Miden에서 온 패턴이며, Tabula의 구조적 장점을 활용하지 않음

제거하면 설계 철학과 구현이 일치한다:
- SSA + NF → intra-tx memory 불필요 (기존)
- Non-conflicting batches → inter-tx memory 불필요 (신규)
- **결과: 어떤 형태의 sorted memory도 불필요**

### 2.3 Parallelization Potential (HIGH value)

**현재**: 배치 전체가 하나의 STARK. SortedMem이 모든 tx의 접근을
하나의 글로벌 테이블로 합치므로, tx별 독립 증명이 불가능.

**제거 후**: 각 tx가 독립적으로 증명 가능:
```
Phase 1 (병렬, tx별):
  - Execution trace 생성
  - Read → SSMC membership 검증
  - Write → Merge 입력 준비

Phase 2 (배치 레벨):
  - 모든 Write 합산 → Merge
  - SSMC → Merge → ColumnMeta → SMT
```

Phase 1이 전체 비용의 대부분을 차지하며, 이것이 완전 병렬화된다.
N개 tx × M개 코어 = O(1) 지연 (이상적).

이것은 SortedMem 제거의 **가장 큰 잠재적 이점**이다.

### 2.4 M11 Simplification (MEDIUM value)

| M11 Component | Current (SortedMem 기반) | After |
|---------------|------------------------|-------|
| Gap witness | SortedMem → SSMC bus (신규 D6) | SSMC 내부 (gap rows) |
| SmtPathChip | 변경 없음 | 변경 없음 |
| StaticTableChip | 변경 없음 | 변경 없음 |
| Public inputs | 변경 없음 | 변경 없음 |

Gap witness가 SSMC 내부에서 자연스럽게 처리되어,
별도 버스 (SsmcGapWitness)가 불필요하다.

---

## 3. Costs

### 3.1 Protocol Restriction (MEDIUM cost)

Non-conflicting batch 가정은 sequencer에 다음 제약을 부과한다:

| 패턴 | 영향 |
|------|------|
| 독립적 tx (다른 키) | 영향 없음 ✓ |
| 같은 키 Read + Read | 허용 (둘 다 base state에서 읽음) ✓ |
| tx_A Write(k), tx_B Read(k) | **금지** — 별도 배치 필요 ✗ |
| tx_A Write(k), tx_B Write(k) | **금지** — 별도 배치 필요 ✗ |

**실제 영향**:
- AMM pool: swap tx마다 잔고 갱신 → hot key → 배치 크기 1
- Counter increment: 순차 실행 필요 → 별도 배치
- Transfer (A→B): 보내는 쪽/받는 쪽 키가 다르면 병렬 가능

대부분의 L2 시스템은 이미 유사한 최적화를 적용하고 있다
(Starknet의 sequencer, Polygon의 parallel EVM 등).

### 3.2 Specification Rewrite (LOW cost)

| Document | 변경 범위 |
|----------|---------|
| proof-spec §7 (Layer C) | 전면 재작성 |
| proof-spec §8 (Memory Consistency) | 전면 재작성 → 직접 참조 모델 |
| semantics-spec §2.2 (Batch Semantics) | parallel 모델 추가 |
| architecture.md | SortedMem 제거 반영 |

### 3.3 Implementation Effort (LOW cost)

| 작업 | 예상 LOC |
|------|---------|
| SortedMem 칩 삭제 | -844 (src) -506 (tests) |
| Memory bus 삭제 | -99 |
| SSMC gap query 추가 | +100 (air) +80 (trace) +150 (tests) |
| Execution bus 변경 | +50 (air) +30 (trace) +50 (tests) |
| ColumnMeta 수정 | +20 |
| Bus trait 변경 | +80 |
| **순 변경** | **-889 lines** |

현재 proof crate 17K LOC 대비 5% 감소.

---

## 4. Summary: Is It Worth It?

### Prover 성능 관점: **아니다** (0.6% NTT 절감)

SortedMem은 전체 prover 비용의 0.9%. Poseidon이 91.6%.
성능만 놓고 보면 SortedMem 최적화는 우선순위가 낮다.

### 아키텍처 관점: **그렇다**

| 기준 | 가치 |
|------|------|
| Design alignment | Tabula의 핵심 철학 (SSA = no memory arg) 완성 |
| Simplicity | 7 chips → 6, 데이터 흐름 간소화 |
| Soundness reasoning | 중간 레이어 제거로 감사 표면 축소 |
| Parallelization | tx별 독립 증명 가능 (미래 확장성) |
| M11 simplification | Gap proof가 자연스러운 위치에 |

### 추천

**Phase 1**: Non-conflicting batch 가정을 프로토콜에 채택할 수 있는지 결정.
이것이 핵심 게이트. 만약 채택 불가능하면, SortedMem은 유지해야 한다.

**Phase 2**: 채택 가능하면, SortedMem 제거를 M11 전에 실행.
M11 설계가 SortedMem에 의존하므로, 제거 후 M11을 재설계하는 것이
나중에 수정하는 것보다 효율적이다.

**Phase 3 (미래)**: 진짜 성능 병목인 Poseidon 최적화에 집중.
이것이 prover 성능의 91%를 좌우한다.

---

## 5. Key Insight

> SortedMem 제거의 가치는 **prover 성능이 아니라 설계 정합성**에 있다.
>
> Tabula는 "SSA + NF로 memory consistency를 구조적으로 제거"하는 시스템이다.
> SortedMem이 존재하면 이 주장이 반쪽짜리가 된다.
> 제거하면 Tabula의 핵심 설계 결정이 끝까지 관철된다.
>
> 성능 최적화는 별도로, Poseidon 최적화 (sponge batch, circle STARK 등)가
> 100배 이상 효과적이다.
