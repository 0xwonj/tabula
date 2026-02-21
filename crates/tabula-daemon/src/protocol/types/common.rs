use crate::service::Capabilities;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            status: "ok",
            service: "tabula-daemon",
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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
