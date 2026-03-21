//! Core trait definitions — pluggable abstractions for crypto-agnosticism.
//!
//! All cryptographic and policy decisions are abstracted behind these traits.
//! The executor and commitment layers are parameterized, not hardcoded.

mod codec;
mod crypto;
mod nonce;
mod state;

pub use codec::ValueCodec;
pub use crypto::{BatchDigester, DOMAIN_TAG_HASH_IR, Hasher, MembershipScheme, SigVerifier};
pub use nonce::NoncePolicy;
pub use state::{StateView, StaticTableProvider};
