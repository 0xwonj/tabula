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
# KNOWN PRE-EXISTING ISSUE: `--features verify` alone currently fails to compile.
# crates/runtime/src/semantics.rs uses p3_field::PrimeCharacteristicRing and
# KoalaBear::ZERO unconditionally, but p3-field is only in the `prove` feature
# set (not `verify`). Additionally, engine.rs has unused imports that are only
# used under `prove`. This is a pre-existing bug on main/SP-1.5 HEAD, not
# introduced by SP-5 Task 0. The verify-only shape failure is tracked as a
# concern for SP-5 runtime decomposition work.
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
