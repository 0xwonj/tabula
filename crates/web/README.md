# tabula-web

Leptos (CSR) based Tabula web IDE.

## Features

- program editor for `.tab` source
- state table editor and raw JSON editor
- transaction batch builder and raw JSON editor
- daemon integration: health/capabilities and stateful runtime APIs
- program/instance/run workflow
- proof submit/verify workflow
- verify-gated state apply
- run history, diagnostics, compiled IR, trace, read/write diff
- workspace and proof import/export
- LocalStorage autosave

## Run

1. Start the daemon.

```bash
TABULA_DAEMON_TOKEN=secret cargo run -p tabula-daemon -- \
  --host 127.0.0.1 --port 4317 \
  --allow-path /tmp --allow-path /Users/me/projects \
  --allow-origin http://127.0.0.1:8080 --allow-origin http://localhost:8080
```

2. Install trunk once.

```bash
cargo install trunk
rustup target add wasm32-unknown-unknown
```

3. Run the web IDE.

```bash
cd crates/web
trunk serve --open
```

Connect the browser UI to the daemon, then use the flow:
check/compile -> execute/prove -> verify.
