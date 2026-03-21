# Documentation

This directory is for documentation that helps maintainers, contributors, and
AI agents understand the project without guessing which writing is current,
which writing is exploratory, and which writing is only historical.

The goal is not to keep a perfect index of every document. The goal is to keep
document authority clear.

## Read In This Order

If you are trying to understand the current project, use this order:

1. [`../README.md`](../README.md)
   Repository overview, workspace shape, and day-to-day entry points.
2. [`design/architecture.md`](design/architecture.md)
   Canonical cross-crate architecture for the current codebase.
3. crate `README.md` files under [`../crates/`](../crates/)
   Crate-local contracts, design intent, and ownership boundaries.

Everything else in `docs/` is supporting material.

## How To Read The Docs Tree

Use the directories by intent, not by age or filename:

| Path | What it is for | How much to trust it |
| --- | --- | --- |
| `docs/design/` | durable cross-crate design docs | canonical |
| `docs/notes/` | short-lived working notes and transient implementation writing | useful, but not authoritative |
| `docs/research/` | exploration, tradeoff analysis, external references | informative, but not authoritative |
| `docs/archive/` | superseded historical docs kept for context | historical only |

## What Should Be Canonical

The canonical current-state documentation set should stay small:

- the root [`../README.md`](../README.md)
- [`design/architecture.md`](design/architecture.md)
- crate-level `README.md` files

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
- This file should explain how to navigate the docs tree. It should not become
  a constantly changing index of every note and memo.

## Related

- [`archive/README.md`](archive/README.md) explains how to treat archived docs
