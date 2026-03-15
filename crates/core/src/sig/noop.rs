//! No-op signature verifier.

use crate::error::TabulaError;
use crate::traits::SigVerifier;

/// Signature verifier that always returns `Ok(())`.
///
/// Placeholder until a real verifier (e.g. EdDSA-over-KoalaBear) is built.
#[derive(Debug, Clone, Copy)]
pub struct NoopSigVerifier;

impl SigVerifier for NoopSigVerifier {
    fn verify(
        &self,
        _sender: &[u8; 32],
        _message: &[u8],
        _signature: &[u8],
    ) -> Result<(), TabulaError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_sig_verifier_accepts() {
        let v = NoopSigVerifier;
        assert!(v.verify(&[0u8; 32], b"msg", b"sig").is_ok());
    }
}
