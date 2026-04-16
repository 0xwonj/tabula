# tabula-sdk

`tabula-sdk` is the high-level Rust embedding surface for sealed Tabula
programs.

Applications and tests that want to compile programs, execute queries or
stateful batches, and optionally access runtime-backed prove/verify flows from
Rust should start here rather than in lower orchestration crates.

Public artifact evaluation does not require this crate. Reviewers should use
the root [README.md](../../README.md), [ARTIFACT.md](../../ARTIFACT.md), and
[crates/cli/README.md](../cli/README.md).

## Role

- high-level program loading and schema access
- Rust embedding for query execution and stateful batch execution
- feature-gated access to prove/verify flows above lower runtime and backend
  layers

## Boundary

- prefer this crate for application-facing Rust integration
- use `tabula-runtime` when you need lower-level execution or verifier
  orchestration details
- treat `tabula-ext` as an internal lower seam rather than a public embedding
  surface
