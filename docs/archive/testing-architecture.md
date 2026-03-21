# Testing Architecture

Canonical testing rules for the Tabula workspace.

This document defines where tests live, when `tabula-testing` should be used,
and what must remain crate-local.

## Core Rule

Use the **closest valid layer**.

- file-local `mod tests`: one file, small invariant, private implementation detail
- `src/testing`: crate-private white-box helpers reused inside one crate
- `tests/common`: crate-local black-box helpers reused across integration tests in one crate
- `tabula-testing`: cross-crate black-box fixtures, harnesses, assertions, and generic file helpers

`tabula-testing` is not a dumping ground for test code. It owns only stable,
generic, public-API-based testing infrastructure.

## `tabula-testing` Surface

Use these modules as the canonical path for generic black-box tests.

- `fixtures`: named scenario fixtures and reusable state/batch/artifact inputs
- `runtime`: execute/prove/verify harness on public runtime seams
- `witness`: shared compile/execute/witness/trace harness
- `assertions`: semantic state/batch/artifact/proof assertions
- `fs`: generic tempdir and JSON file primitives

If a new generic integration test can be written with these modules, use them
instead of inline source/state/batch assembly.

## Fixture Admission Rules

A new fixture belongs in `tabula-testing` only if all of these are true.

- It is black-box and does not require private crate internals.
- It has at least **two generic consumers** across crates, targets, or seams.
- It is scenario-oriented and reusable, not a one-off malformed edge case.
- It is not adapter-specific to `cli`, `daemon`, or `web`.

Do not add placeholder fixtures. Do not add thin aliases around existing
shared fixtures unless they encode a genuinely different canonical scenario.

## What Stays Local

Keep these in the owning crate.

- white-box seams and fake backends
- parser/lowering tests where source text itself is the assertion target
- scheme-specific runtime seam tests
- proptest data generation
- low-level chip row builders and witness math helpers
- interpreter-only doubles and local execution scaffolding

If moving a helper to `tabula-testing` would require widening product-crate
visibility, keep it local.

## Reviewer Checklist

For any new generic integration or smoke test:

- Prefer `tabula_testing::fixtures::*` over inline source/state/batch setup.
- Prefer `tabula_testing::runtime::*` or `tabula_testing::witness::*` over
  hand-written pipeline scaffolding.
- Prefer `tabula_testing::assertions::*` over repeated manual projections.
- Reject adapter-specific helpers added to `tabula-testing`.
- Reject white-box seams moved into `tabula-testing`.

For crate-local tests:

- keep file-local invariants close to the code under test
- keep `src/testing` private
- keep `tests/common` black-box

## Local and CI Test Commands

CI uses `cargo nextest` for test binaries and `cargo test --doc` for doctests.

Canonical commands:

```bash
cargo nextest run -p tabula-testing
cargo nextest run -p tabula-runtime --features prove
cargo nextest run -p tabula-witness --test main
cargo nextest run -p tabula-cli -p tabula-executor -p tabula-daemon
cargo test --workspace --doc
```
