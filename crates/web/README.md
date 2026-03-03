# tabula-web-ide

Leptos(CSR) 기반 Tabula Web IDE.

## Features

- Program IDE (`.tab` source)
- State table editor + raw JSON editor
- Transaction batch builder + raw JSON editor
- Daemon integration: health/capabilities + stateful runtime API
- Program/instance/run workflow:
  - `POST /v1/programs` (register)
  - `POST /v1/instances` (create)
  - `POST /v1/runs` (submit execute/prove)
  - `POST /v1/runs/{run_id}` (verify)
- Proof workflow:
  - run submit 시 `prove=true`로 receipt 생성
  - run verify 호출로 proof 검증 상태 전이
- Verify gate 기반 state apply
- Run history, diagnostics, compiled IR, trace, RW diff
- Workspace/proof import-export
- LocalStorage 자동 저장

## Run

1. daemon 실행

```bash
TABULA_DAEMON_TOKEN=secret cargo run -p tabula-daemon -- \
  --host 127.0.0.1 --port 4317 \
  --allow-path /tmp --allow-path /Users/me/projects \
  --allow-origin http://127.0.0.1:8080 --allow-origin http://localhost:8080
```

2. trunk 설치 (최초 1회)

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

3. web ide 실행

```bash
cd crates/web
trunk serve --open
```

브라우저에서 daemon URL/token을 입력 후 Connect -> Check/Compile -> Execute/Prove -> Verify 순서로 사용.
