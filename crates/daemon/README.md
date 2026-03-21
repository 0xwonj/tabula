# tabula-daemon

Local HTTP daemon for Tabula clients such as the web IDE and automation
workflows.

## Role

- exposes compiler/runtime flows over HTTP
- provides a client-neutral local control plane
- supports `inline` and `file` input references
- exposes registry/runtime APIs for program, instance, and run workflows
- offers local prove/verify execution behind the daemon boundary

`tabula-daemon` uses its local engine and `tabula-runtime` directly.

## Internal Layout

- `src/api/`: HTTP router, handlers, middleware
- `src/protocol/`: transport DTOs and API envelopes
- `src/runtime/`: daemon config and shutdown handling
- `src/service/`: local engine and file-access policy

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

- binds to `127.0.0.1` by default
- `kind=file` input is restricted to allowed roots (`--allow-path`)
- protected endpoints require bearer token when `TABULA_DAEMON_TOKEN` is set
- CORS allow-list is configurable via `--allow-origin`

## Runtime Guardrails

- `--max-concurrent-jobs`: caps concurrent CPU-bound engine work
- `--queue-timeout-ms`: fails fast with `SERVER_BUSY` when no job slot is available
- `--request-timeout-ms`: returns `REQUEST_TIMEOUT` while preserving backpressure

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
