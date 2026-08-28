# Internal Documentation

This directory is maintainer-facing navigation for the repository's internal
docs tree.

Artifact reviewers should start with [`../README.md`](../README.md),
[`../ARTIFACT.md`](../ARTIFACT.md), and
[`../crates/cli/README.md`](../crates/cli/README.md). Most material under
`docs/` is supporting or exploratory and is not part of the narrow public
artifact path.

## Maintainer Read Order

If you are maintaining or extending the codebase, use this order:

1. [`design/architecture.md`](design/architecture.md) for the current
   cross-crate architecture.
2. crate `README.md` files under [`../crates/`](../crates/) for crate-local
   boundaries and ownership. The SDK and extension crates also now carry
   lightweight top-level README files describing their role.
3. [`research/`](research/) only as supporting material.

Everything else in `docs/` is supporting material.

## How To Read The Docs Tree

Use the directories by intent, not by age or filename:

| Path | What it is for | How much to trust it |
| --- | --- | --- |
| `docs/design/` | durable cross-crate design docs | canonical |
| `docs/research/` | exploration, tradeoff analysis, external references | informative, but not authoritative |

## Canonical Maintainer Docs

The canonical current-state maintainer docs should stay small:

- [`design/architecture.md`](design/architecture.md)
- crate-level `README.md` files
- narrowly scoped design docs that represent real current-state contracts

If those documents disagree with research material, prefer the canonical set.

## Where New Docs Should Go

Use `docs/design/` for:

- durable cross-crate design documents
- current architectural contracts that should survive implementation churn

Use `docs/research/` for:

- exploration of alternatives
- tradeoff analysis
- external-system comparisons
- design investigation that informs decisions without becoming current architecture

## Documentation Rules

- Keep canonical docs few and deliberate.
- Remove superseded design docs from active directories. Preserve only useful
  historical context as explicitly non-authoritative research material.
- If a change is crate-local, prefer updating the relevant crate `README.md`
  instead of adding a new cross-crate design document.
- This file should stay a maintainer navigation note, not a reviewer entry
  point or a constantly changing index of every memo.
