//! `tabula inspect-proof`

use anyhow::Context as _;

use crate::app::AppContext;
use crate::cli::InspectProofArgs;
use crate::output::{inspect_proof_output, render_inspect_proof};

/// Inspect one canonical `proof.bin` without treating transport metadata as authoritative.
pub(crate) fn run(_ctx: &AppContext, args: &InspectProofArgs) -> anyhow::Result<()> {
    let bytes = std::fs::read(&args.proof)
        .with_context(|| format!("failed to read {}", args.proof.display()))?;
    let envelope = tabula_sdk::interop::decode_proof_envelope(&bytes)?;
    let proof = tabula_sdk::Proof::decode_binary(&bytes)?;
    let output = inspect_proof_output(&proof, &envelope);
    if AppContext::wants_json(args.json) {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", render_inspect_proof(&output));
    }
    Ok(())
}
