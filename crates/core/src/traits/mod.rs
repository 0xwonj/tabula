//! Core trait definitions — pluggable abstractions for crypto-agnosticism.
//!
//! All cryptographic and policy decisions are abstracted behind these traits.
//! The executor and commitment layers are parameterized, not hardcoded.

mod crypto;
mod state;

pub use crypto::{BatchDigester, DOMAIN_TAG_HASH_IR, Hasher, MembershipScheme};
pub use state::{StateView, StaticTableProvider};
