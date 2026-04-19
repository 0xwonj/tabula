#!/usr/bin/env bash
# SP-5 Feature-Matrix Smoke Build
#
# Verifies that the workspace builds cleanly under all three feature shapes:
#   - default (no proof features)
#   - verify  (proof verification only)
#   - prove   (full prover + verifier)
#
# Note: `cargo build --workspace --features X` is invalid for virtual manifests.
# We target tabula-cli, which sits at the top of the feature-propagation chain
# (cli → sdk → runtime → ext) and exercises the full prove/verify path.
#
# KNOWN PRE-EXISTING ISSUE: `--features verify` alone still fails to compile.
# The initial Task 0 investigation fixed three unconditional imports
# (engine.rs `PublicStatement` / `TabulaProof`, semantics.rs
# `p3_field::PrimeCharacteristicRing`) and gated `pub mod semantics` behind
# the verify feature. The remaining 13 errors under `--features verify` are
# dead-code warnings (workspace `unused = deny`) in semantics.rs for items
# consumed only under `prove` (ProofJournal, PublicStatementMaterialization,
# canonical_public_context, etc.). Gating each individual item is invasive
# and touches semantics.rs, which is explicitly out of SP-5 scope (§4.2).
# Tracked as a follow-up "verify-only refinement" task.
#
# Wiring into CI is a follow-up (no CI system is present in this repo yet).

set -euo pipefail

FAIL_COUNT=0

echo "=== default (no proof features) ==="
cargo build --workspace

echo ""
echo "=== --features verify (via tabula-cli) ==="
if cargo build -p tabula-cli --features verify 2>&1; then
    echo "verify: PASS"
else
    echo "verify: KNOWN_BROKEN (pre-existing; see header comment)"
    FAIL_COUNT=$((FAIL_COUNT + 1))
fi

echo ""
echo "=== --features prove (via tabula-cli) ==="
cargo build -p tabula-cli --features prove

echo ""
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo "All three feature shapes built successfully."
else
    echo "default + prove: OK. verify-only: KNOWN_BROKEN (pre-existing issue, not SP-5 regression)."
    exit 1
fi
