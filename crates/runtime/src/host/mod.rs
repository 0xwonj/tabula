//! Host environment: runtime registries, scheme backends, and built-in installations.

mod builtins;
mod environment;
mod installed;
mod registries;

pub use builtins::{SmtScheme, SsmcScheme};
pub use environment::HostEnvironment;
pub use installed::InstalledSchemes;
pub use registries::RuntimeRegistries;

pub(crate) use builtins::default_backend_factories;
pub(crate) use installed::SchemeFactoryMap;
