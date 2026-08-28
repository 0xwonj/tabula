#!/usr/bin/env bash
# Feature-Matrix Smoke Check
#
# Verifies that the workspace type-checks cleanly under all three feature shapes:
#   - default (no proof features)
#   - verify  (proof verification only)
#   - prove   (full prover + verifier)
#
# Note: `cargo check --workspace --features X` is invalid for virtual manifests.
# We target tabula-cli, which sits at the top of the feature-propagation chain
# (cli → sdk → runtime → ext) and exercises the full prove/verify path.
#
set -euo pipefail

echo "=== default (no proof features) ==="
cargo check --workspace --locked

echo ""
echo "=== --features verify (via tabula-cli) ==="
cargo check -p tabula-cli --features verify --locked

echo ""
echo "=== --features prove (via tabula-cli) ==="
cargo check -p tabula-cli --features prove --locked

echo ""
echo "All three feature shapes checked successfully."
