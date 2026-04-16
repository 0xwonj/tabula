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
3. `docs/notes/`, `docs/research/`, and `docs/archive/` only as supporting
   material.

Everything else in `docs/` is supporting material.

## How To Read The Docs Tree

Use the directories by intent, not by age or filename:

| Path | What it is for | How much to trust it |
| --- | --- | --- |
| `docs/design/` | durable cross-crate design docs | canonical |
| `docs/notes/` | short-lived working notes and transient implementation writing | useful, but not authoritative |
| `docs/research/` | exploration, tradeoff analysis, external references | informative, but not authoritative |
| `docs/archive/` | superseded historical docs kept for context | historical only |

## Canonical Maintainer Docs

The canonical current-state maintainer docs should stay small:

- [`design/architecture.md`](design/architecture.md)
- crate-level `README.md` files
- narrowly scoped design docs that represent real current-state contracts

If those documents disagree with notes, research, or archive material, prefer
the canonical set.

## Where New Docs Should Go

Use `docs/design/` for:

- durable cross-crate design documents
- current architectural contracts that should survive implementation churn

Use `docs/notes/` for:

- temporary implementation notes
- AI working documents
- vocabulary cleanup notes
- short-lived thinking that may be deleted or archived later

Use `docs/research/` for:

- exploration of alternatives
- tradeoff analysis
- external-system comparisons
- design investigation that informs decisions without becoming current architecture

Use `docs/archive/` for:

- superseded design documents
- historical plans
- old rationale worth keeping for context

## Documentation Rules

- Keep canonical docs few and deliberate.
- Do not leave superseded design docs in active directories just because they
  still contain useful history. Move them to `docs/archive/`.
- Do not promote temporary notes into `docs/design/` unless they have become a
  real current-state contract.
- If a change is crate-local, prefer updating the relevant crate `README.md`
  instead of adding a new cross-crate design document.
- This file should stay a maintainer navigation note, not a reviewer entry
  point or a constantly changing index of every memo.
