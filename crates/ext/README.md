# tabula-ext

`tabula-ext` is an internal and partly experimental seam for extension-backed
authoring, runtime, verification, and proving capability wiring.

It is not part of the current public artifact surface or reviewer path.

## Role

- host extension-oriented capabilities shared across higher layers
- provide feature-gated seams consumed by `tabula-sdk`, `tabula-runtime`, and
  `tabula-cli`
- keep backend and extension integration details out of the public artifact
  story unless they become part of the supported subset

## Boundary

- maintainers may touch this crate when changing extension or backend
  integration
- artifact reviewers can ignore this crate
