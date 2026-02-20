# M11 O2 Co-Design 최적안 (통합 결론)

> Date: 2026-02-20  
> Inputs:
> - `docs/design/m11-o2-codesign-with-compiler-architecture.md`
> - `docs/design/compiler-proof-codesign.md`
> - `docs/design/m11-o2-implementation-plan.md`
> - `docs/design/compiler-research-architecture.md`

---

## 1. 비교 결론

두 문서는 방향이 거의 동일하며, 핵심 합의점은 명확하다:

1. O2를 proof 로컬 패치로 끝내면 안 된다.
2. 계약 계층(contract spine)을 지금부터 도입해야 한다.
3. C10/C11은 `tx_index`로 앵커링해야 한다.
4. statement 필드는 `Bound`/`Deferred`를 강제해야 한다.

차이는 “얼마나 공격적으로 지금 반영할지”와 “타입/스키마 고정 방식”에 있다.

---

## 2. 문서별 장단점

## 2.1 `m11-o2-codesign-with-compiler-architecture.md` 장점

1. O2 PR 흐름(C1~C8)과의 매핑이 구체적이다.
2. `AccessTuple` 타입화 제안이 명확하다.
3. delete-all 규칙을 “제약 + 계약 동시 갱신”으로 묶은 점이 좋다.
4. M11-safe / M12 / M13+로 현실적인 단계 분리가 잘 되어 있다.

## 2.2 `m11-o2-codesign-with-compiler-architecture.md` 한계

1. `AccessTuple`를 바로 공용 타입으로 강제할 때 제네릭/표현식 경계(`AB::Expr`) 설계 부담이 과소평가되어 있다.
2. C6~C8를 M12 즉시 진행하도록 권고하지만, driver/artifact 경계가 먼저 고정되지 않으면 재작업 가능성이 남는다.
3. profile/contract mismatch hard-fail의 도입 위치(어느 layer에서 fail할지)가 아직 추상적이다.

## 2.3 `compiler-proof-codesign.md` 장점

1. 현재 코드 증거를 명확히 짚는다:
   - CLI semantic ownership (`crates/tabula-cli/src/io.rs`)
   - IR semantic mutation (`crates/tabula-ir/src/pass/canonicalize/nf4_alias_guard.rs`)
   - proof 계약 분산 (`crates/tabula-proof/src/statement.rs`, `crates/tabula-proof/src/air/bus.rs`)
2. “지금/다음/그다음” 단계가 단순하고 실행 가능성이 높다.
3. anti-pattern을 분명히 제시해 회귀를 막기 쉽다.

## 2.4 `compiler-proof-codesign.md` 한계

1. PR 단위 작업 분해가 상대적으로 거칠다.
2. O2 구현자 관점의 파일 단위 조치가 부족하다.
3. C10/C11 타입화의 구체 API 스케치가 상대적으로 약하다.

---

## 3. 최적 통합 설계 (권고)

최적안은 **Option B+** 이다:

- 기본: O2 + M11-safe co-design (C1~C5)
- 보완: schema/driver/profile gate를 최소 형태로 즉시 삽입
- 유예: M12 대개편(C6~C8), O3/R1~R4

즉, “지금 닫아야 할 soundness”와 “나중 재작업을 줄이는 계약 축”만 먼저 강제한다.

---

## 4. Core Spine V1 (M11에서 반드시 고정할 것)

## 4.1 Contract Schema V1

소유 위치:
- 단기: `crates/tabula-proof/src/contract/`
- 중기: `tabula-contract`로 extraction

필수 항목:
1. bus schema version
2. statement binding map
3. deferred reason code (열거형)

## 4.2 Trace Identity V1

필수 앵커:
1. C10/C11 payload에 `tx_index` 포함

예약 슬롯:
1. `effect_ordinal_in_tx` (M12 이후 고려, 현재는 schema 예약만)

## 4.3 Binding Completeness V1

`ApplyBatchStatement` 각 필드는 반드시:
1. `BoundInAir`
2. `Deferred { reason_code, owner, milestone }`

중간 상태 금지.

---

## 5. 세부 결정 (채택 / 보류)

| 항목 | 결정 | 이유 |
|---|---|---|
| C10/C11 `tx_index` 앵커 | 채택(즉시) | G3 직접 폐쇄 |
| `AccessTuple` 타입화 | 채택(단계적) | drift 방지 효과 큼, 단 AB 경계 설계 필요 |
| delete-all contract+constraint atomic | 채택(즉시) | G2 폐쇄 + 규칙 일관성 |
| Statement binding registry | 채택(즉시) | G4 폐쇄 기반 |
| profile/contract mismatch hard-fail | 채택(최소형) | 이후 drift 방지 핵심 |
| runtime canonical E-Trace 직접 산출 | 보류(M12) | M11 일정 리스크 |
| Contract DSL codegen | 보류(M13+) | 효과 큼, 도입 비용 큼 |
| semantic_hash를 statement public input화 | 보류(v2) | 프로토콜 영향 큼 |
| O3 칩 통합 | 보류(M13 baseline 이후) | blast radius 과대 |

---

## 6. 구현 순서 (최적화된 실행안)

## Phase M11-A (필수)

1. O2 PR-1 + C1/C4:
   - contract skeleton
   - binding registry
   - deferred reason code

2. O2 PR-2 + C2/C3:
   - C10/C11 `tx_index`
   - payload builder 단일화
   - raw 수동 조립 제거 우선순위 적용 (`execution/air.rs`)

3. O2 PR-3/4:
   - touched-write closure
   - delete-all 규칙 및 C6 게이트 정합

4. O2 PR-5:
   - C6/C8 placeholder 제거
   - schema/binding snapshot gate 추가

## Phase M11-B (작게 추가)

1. proof entrypoint에 `contract_version`/`profile_id` compatibility check hook 추가
2. 아직 `.tcb`가 없어도 metadata stub로 hard-fail 동작 확보

## Phase M12 Gate

M12 시작 조건:
1. Contract Schema V1 freeze
2. Trace Identity V1 freeze
3. Binding completeness test green

그 후 C6~C8(ETR 중심 assembler) 진행.

---

## 7. 성공 기준 (최적안 기준)

아래가 모두 만족되면 “최적안 적용 성공”으로 본다.

1. G1~G4가 테스트로 재현 불가
2. bus payload drift가 타입/스냅샷 테스트에서 차단
3. statement 필드가 모두 Bound/Deferred로 분류
4. CLI semantic ownership이 점진적으로 driver로 이동 가능한 구조 확보
5. M12에서 contract extraction 시 breaking change 없이 이관 가능

---

## 8. 최종 제언

최적 설계는 “큰 그림은 공격적으로, 구현은 단계적으로”다.

1. M11은 O2를 완수하되 contract spine을 강제한다.
2. M12는 runtime/proof 입력 계약을 E-Trace + Contract IR로 통일한다.
3. M13+에서 codegen/semantic-hash-in-statement/O3를 판단한다.

이 경로가 현재 코드베이스의 리스크와 일정 제약을 동시에 만족시키는 최선이다.

