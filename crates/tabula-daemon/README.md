# tabula-daemon

Local HTTP daemon for Tabula clients (Web IDE, CLI helpers, automations).

## Role

- Client-neutral local control plane over Tabula crates.
- Exposes `check`, `compile`, `execute` over HTTP.
- Supports `inline` and `file` input references (artifact mode planned).
- Provides capability discovery and proof endpoint stubs.

## Run

```sh
cargo run -p tabula-daemon -- \
  --host 127.0.0.1 \
  --port 4317 \
  --allow-path /Users/me/projects \
  --allow-origin https://play.example.com \
  --max-concurrent-jobs 8 \
  --queue-timeout-ms 2000 \
  --request-timeout-ms 30000
```

Optional auth token:

```sh
TABULA_DAEMON_TOKEN=secret cargo run -p tabula-daemon
```

## Security Defaults

- Binds to `127.0.0.1` by default.
- `kind=file` input is restricted to allowed roots (`--allow-path`).
- Protected endpoints require bearer token when `TABULA_DAEMON_TOKEN` is set.
- CORS allow-list is configurable via `--allow-origin`.

## Runtime Guardrails

- `--max-concurrent-jobs`: caps concurrent CPU-bound engine work.
- `--queue-timeout-ms`: fails fast with `SERVER_BUSY` when no job slot is available.
- `--request-timeout-ms`: returns `REQUEST_TIMEOUT` while preserving backpressure.

## API (v0)

- `GET /v1/health`
- `GET /v1/capabilities`
- `POST /v1/check`
- `POST /v1/compile`
- `POST /v1/execute`
- `POST /v1/jobs/prove` (501 stub)
- `POST /v1/jobs/verify` (501 stub)
