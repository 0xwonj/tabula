# tabula-daemon

Local HTTP daemon for Tabula clients (Web IDE, CLI helpers, automations).

## Role

- Client-neutral local control plane over Tabula crates.
- Delegates domain workflows to `tabula-orchestrator` (adapter-thin architecture).
- Reuses orchestrator command/result contracts directly (no daemon-local domain DTO duplication).
- Exposes stateful runtime workflow APIs over HTTP.
- Supports `inline` and `file` input references (artifact mode planned).
- Provides capability discovery plus run-level receipt `prove` / `verify`.
- Provides registry/runtime APIs (`programs`, `instances`, `runs`) for IDE workflows.

## Internal Layout

- `src/api/handlers/common.rs`: health/capabilities.
- `src/api/handlers/stateful.rs`: program/instance/run HTTP handlers.
- `src/api/handlers/auth.rs`: bearer auth middleware.
- `src/api/handlers/blocking.rs`: `spawn_blocking` execution wrapper, backpressure/timeout policy.
- `src/protocol/types/common.rs`: shared envelopes.
- `src/protocol/types/stateful.rs`: stateful transport DTOs.

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
- `POST /v1/programs`
- `GET /v1/programs`
- `GET /v1/programs/{program_id}`
- `POST /v1/instances`
- `GET /v1/instances?program_id=...`
- `GET /v1/instances/{instance_id}`
- `POST /v1/runs`
- `GET /v1/runs?instance_id=...&limit=...`
- `GET /v1/runs/{run_id}`
- `POST /v1/runs/{run_id}` (verify run proof)
