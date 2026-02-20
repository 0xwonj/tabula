# tabula-web-ide

Leptos(CSR) 기반 Tabula Web IDE.

## Features

- Program IDE (`.tab` source)
- State table editor + raw JSON editor
- Transaction batch builder + raw JSON editor
- Daemon integration: health/capabilities/check/compile/execute
- Proof workflow:
  - daemon `/v1/jobs/prove` / `/v1/jobs/verify` 호출
  - endpoint 미구현 시 로컬 demo proof fallback 생성/검증
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
trunk serve --manifest-path crates/tabula-web-ide/Cargo.toml --open
```

브라우저에서 daemon URL/token을 입력 후 Connect -> Check/Compile/Execute 순서로 사용.
