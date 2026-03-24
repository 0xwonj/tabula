# Program Redesign Notes

This directory contains the redesign document bundle produced for the new
Tabula language/compiler architecture.

These notes belong together because they define one continuous design:

- the `program`-first DSL
- HIR / MIR / canonical IR layering
- typing and effect discipline
- final seam decisions
- implementation roadmap

## Recommended Reading Order

1. [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md)
2. [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md)
3. [program-typing-and-effect-system.md](program-typing-and-effect-system.md)
4. [program-final-seam-decisions.md](program-final-seam-decisions.md)
5. [program-canonical-ir-design.md](program-canonical-ir-design.md)
6. [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md)
7. [program-mir-design.md](program-mir-design.md)
8. [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md)
9. [program-hir-design.md](program-hir-design.md)
10. [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md)
11. [program-rewrite-roadmap.md](program-rewrite-roadmap.md)

## Scope Of This Bundle

These notes define:

- the target DSL and compiler architecture
- exact HIR / MIR / canonical IR contracts
- the static typing/effect model
- the finalized seam decisions
- the staged rewrite plan

Shared notes that remain one level up in `docs/notes/` are intentionally not
duplicated here, because they are referenced by multiple note clusters:

- [../canonical-vocabulary.md](../canonical-vocabulary.md)
- [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md)
- [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md)
