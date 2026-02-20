# tabula-daemon

Local HTTP daemon for the Tabula Web IDE.

## Role

- Exposes `check`, `compile`, `execute` over HTTP.
- Supports `inline` and `file` input references (artifact mode planned).
- Provides capability discovery and proof endpoint stubs.

## Run

```sh
cargo run -p tabula-daemon -- --host 127.0.0.1 --port 4317
```

Optional auth token:

```sh
TABULA_DAEMON_TOKEN=secret cargo run -p tabula-daemon
```

## API (v0)

- `GET /v1/health`
- `GET /v1/capabilities`
- `POST /v1/check`
- `POST /v1/compile`
- `POST /v1/execute`
- `POST /v1/jobs/prove` (501 stub)
- `POST /v1/jobs/verify` (501 stub)
