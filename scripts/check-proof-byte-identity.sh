#!/usr/bin/env bash
# check-proof-byte-identity.sh
#
# SP-4 byte-identity gate: regenerates the basic and membership example proofs
# into a scratch directory and diffs them byte-for-byte against the s0-reference/
# snapshot captured in S0.
#
# Usage: bash scripts/check-proof-byte-identity.sh
# Run from the repo root.
#
# Exit codes:
#   0 — all reference proofs match byte-for-byte
#   1 — one or more proofs diverged, or a command failed
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI="${REPO_ROOT}/target/debug/tabula-cli"
REFERENCE_DIR="${REPO_ROOT}/s0-reference"
SCRATCH_BASE="/tmp/tabula-gate"

# ── Build CLI (no-op if already fresh) ────────────────────────────────────────
echo "Building tabula-cli --features prove ..."
cargo build -p tabula-cli --features prove --manifest-path "${REPO_ROOT}/Cargo.toml" 2>&1

# ── Helper: run full example flow and compare against reference ────────────────
check_example() {
    local name="$1"
    local scratch="${SCRATCH_BASE}-${name}"

    echo ""
    echo "=== ${name} ==="
    rm -rf "${scratch}"

    "${CLI}" example "${name}" --dir "${scratch}"
    "${CLI}" execute \
        --program  "${scratch}/program.tab" \
        --state    "${scratch}/state.json" \
        --batch    "${scratch}/batch.json" \
        --context  "${scratch}/context.json" \
        --receipt-out "${scratch}/receipt.bin"
    "${CLI}" prove \
        --program  "${scratch}/program.tab" \
        --receipt  "${scratch}/receipt.bin" \
        --proof-out "${scratch}/proof.bin" \
        --public-statement-out "${scratch}/public_statement.json" \
        --summary-out "${scratch}/proof_summary.json"

    local ref_proof="${REFERENCE_DIR}/${name}/proof.bin"
    local ref_stmt="${REFERENCE_DIR}/${name}/public_statement.json"

    if [[ ! -f "${ref_proof}" ]]; then
        echo "ERROR: reference not found: ${ref_proof}"
        exit 1
    fi
    if [[ ! -f "${ref_stmt}" ]]; then
        echo "ERROR: reference not found: ${ref_stmt}"
        exit 1
    fi

    if ! diff -q "${scratch}/proof.bin" "${ref_proof}" > /dev/null; then
        echo "FAIL: proof.bin diverged for ${name}"
        echo "  generated: ${scratch}/proof.bin"
        echo "  reference: ${ref_proof}"
        exit 1
    fi

    if ! diff -q "${scratch}/public_statement.json" "${ref_stmt}" > /dev/null; then
        echo "FAIL: public_statement.json diverged for ${name}"
        echo "  generated: ${scratch}/public_statement.json"
        echo "  reference: ${ref_stmt}"
        exit 1
    fi

    echo "  proof.bin:             byte-identical"
    echo "  public_statement.json: byte-identical"
}

# ── Run both examples ──────────────────────────────────────────────────────────
check_example basic
check_example membership

echo ""
echo "OK: all reference proofs match byte-for-byte."
