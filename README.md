# Tabula

Tabula is a research prototype for semantic-first proving of typed tabular
transaction batches.

The current proof-capable path starts from sealed program semantics and typed
table state, executes a stateful transaction batch, and verifies the resulting
proof against an explicit expected public statement. The artifact is therefore
statement-first: the verifier checks the sealed program, the expected
`public_statement.json`, and the proof together.

## Artifact Scope

The public artifact currently supports only a narrow subset:

- stateful transaction-batch proving
- query execution only, not query proving
- unary native user-state keys only
- public examples: `basic` and `membership`

Paper-planning notes may describe broader submission-time deliverables
such as a StarkEx-class evaluation workload, provenance verification, or
Dockerized reproducibility. Those are **not** part of the current public
artifact unless this README or [ARTIFACT.md](ARTIFACT.md) documents them
explicitly.

## Reviewer Path

If you are reviewing the artifact, use this order:

1. [README.md](README.md) for scope and claims.
2. [ARTIFACT.md](ARTIFACT.md) for the full reproduction steps.
3. [crates/cli/README.md](crates/cli/README.md) for the command surface.

`docs/README.md` is maintainer-facing internal navigation and is not part of
the reviewer path.

## Status

Tabula is research code. The implementation is still evolving, and the repository is not suitable to be used in production.

## License

Apache-2.0
