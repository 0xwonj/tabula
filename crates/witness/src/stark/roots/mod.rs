//! Root-tier helpers for the STARK witness backend.

mod paths;
mod store;

pub use store::{SmtRootStoreContext, prepare_smt_root_store};
