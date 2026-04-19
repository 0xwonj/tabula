#!/usr/bin/env bash
# SP-5 Byte-Identity Gate
#
# Captures sha256 hashes of proof.bin and public_statement.json for the
# "basic" and "membership" examples. These hashes constitute the SP-1.5
# HEAD baseline; every subsequent SP-5 refactor task must reproduce them.
#
# Usage (capture baseline):
#   scripts/sp5_byte_identity.sh | sort \
#     > docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt
#
# Verify (compare against saved baseline):
#   diff <(scripts/sp5_byte_identity.sh | sort) \
#        <(sort docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt)
#
# Wiring into CI is a follow-up (no CI system is present in this repo yet).
# When CI is added, run this on PRs touching crates/runtime/** or crates/sdk/**.

set -euo pipefail

EXAMPLES=(basic membership)
WORK="${WORK:-$(mktemp -d)}"

cargo build --quiet -p tabula-cli --features prove

for ex in "${EXAMPLES[@]}"; do
    dir="$WORK/$ex"
    rm -rf "$dir" && mkdir -p "$dir"
    # Redirect CLI progress output to stderr so stdout is hash-only and diffable.
    target/debug/tabula-cli example "$ex" --dir "$dir" >&2
    target/debug/tabula-cli execute \
        --program "$dir/program.tab" \
        --state "$dir/state.json" \
        --batch "$dir/batch.json" \
        --context "$dir/context.json" \
        --receipt-out "$dir/receipt.bin" >&2
    target/debug/tabula-cli prove \
        --program "$dir/program.tab" \
        --receipt "$dir/receipt.bin" \
        --proof-out "$dir/proof.bin" \
        --public-statement-out "$dir/public_statement.json" \
        --summary-out "$dir/proof_summary.json" >&2
    # Hash with relative paths (cd into WORK so output is "basic/proof.bin" etc.)
    # Only this goes to stdout so the baseline comparison recipe works cleanly.
    (cd "$WORK" && sha256sum "$ex/proof.bin" "$ex/public_statement.json")
done
