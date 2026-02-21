# M12 Completion Gate (Strict)

작성일: 2026-02-21  
범위: `tabula-proof` 중심의 trace assembly 완료 기준

## 목적

M12를 “부분 구현”이 아니라 release gate 기준으로 고정한다.  
아래 게이트를 모두 만족해야 M12 완료로 본다.

## Definition of Done

### M12-G1. 단일 Trace Orchestrator

- 하나의 진입점에서 다음 trace를 생성:
  - `Execution`
  - `InterTxOrder`
  - `StateColumn`
  - `ColumnMeta`
  - `StaticTable`
  - `SmtColPath`
  - `SmtTablePath`
  - `Poseidon`
  - `RangeCheck`
- 동일 입력에 대해 trace 생성은 결정적이어야 한다.

### M12-G2. Contract Spine / Fail-Closed

- `ContractMetadataEnvelope` 검증이 fail-closed로 동작:
  - unknown/newer schema version 즉시 실패
  - profile mismatch fallback 금지
- statement binding은 `BoundInAir | Deferred(...)` 완전성 유지

### M12-G3. E-Trace Identity Anchor

- `tx_index` + `effect_ordinal_in_tx`가 execution→witness→trace 경로에서 보존
- C10/C11 tuple 스키마(`tx_index` 포함)와 불일치 시 실패

### M12-G4. Bus Gate Tests (No Placeholder)

- C5/C6/C8/C9/C10/C11/C13/C14/C15/C16 버스의 positive/negative 테스트 존재
- placeholder 테스트 금지

### M12-G5. End-to-End Trace Assembly Test

- 최소 1개 fixture에서:
  - DSL compile
  - execute
  - witness 생성
  - all-chip trace 생성
  - chip constraint + bus balance 검증

## 현재 미완료 포인트 (진행 기준)

- all-chip orchestrator에서 Poseidon/RangeCheck 자동 조립
- execution lowering production path 확정
- all-chip E2E fixture 강화
