//! Common API response types.

use crate::service::Capabilities;
use serde::Serialize;

/// Health check response.
#[derive(Debug, Clone, Serialize)]
#[allow(missing_docs)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

impl HealthResponse {
    /// Build a healthy response with crate version.
    pub fn ok() -> Self {
        Self {
            ok: true,
            status: "ok",
            service: "tabula-daemon",
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

/// Capabilities response returned by the `/capabilities` endpoint.
#[derive(Debug, Clone, Serialize)]
#[allow(missing_docs)]
pub struct CapabilitiesResponse {
    pub ok: bool,
    #[serde(flatten)]
    pub capabilities: Capabilities,
}

impl From<Capabilities> for CapabilitiesResponse {
    fn from(value: Capabilities) -> Self {
        Self {
            ok: true,
            capabilities: value,
        }
    }
}
