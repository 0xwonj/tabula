use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::models::{
    CapabilitiesResponse, CreateInstanceResponse, DaemonErrorEnvelope, HealthResponse,
    RegisterProgramResponse, StateSnapshot, SubmitRunResponse, TransactionBatch, VerifyRunResponse,
};

#[derive(Debug, Clone)]
pub struct ApiClient {
    base_url: String,
    token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApiClientError {
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
    pub details: Option<Value>,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: normalize_base_url(&base_url.into()),
            token,
        }
    }

    pub async fn health(&self) -> Result<HealthResponse, ApiClientError> {
        self.send_get("/v1/health").await
    }

    pub async fn capabilities(&self) -> Result<CapabilitiesResponse, ApiClientError> {
        self.send_get("/v1/capabilities").await
    }

    pub async fn register_program(
        &self,
        source: &str,
    ) -> Result<RegisterProgramResponse, ApiClientError> {
        let payload = json!({
            "program": {
                "kind": "inline",
                "inline": {
                    "source": source
                }
            }
        });
        self.send_post("/v1/programs", payload).await
    }

    pub async fn create_instance(
        &self,
        program_id: &str,
        state: StateSnapshot,
    ) -> Result<CreateInstanceResponse, ApiClientError> {
        let payload = json!({
            "program_id": program_id,
            "state": {
                "kind": "inline",
                "inline": state
            },
        });
        self.send_post("/v1/instances", payload).await
    }

    pub async fn submit_run(
        &self,
        instance_id: &str,
        batch: TransactionBatch,
        include_trace: bool,
        prove: bool,
        verify: bool,
        commit: bool,
        expected_instance_version: Option<u64>,
    ) -> Result<SubmitRunResponse, ApiClientError> {
        let payload = json!({
            "instance_id": instance_id,
            "batch": {
                "kind": "inline",
                "inline": batch
            },
            "include_trace": include_trace,
            "prove": prove,
            "verify": verify,
            "commit": commit,
            "expected_instance_version": expected_instance_version,
        });

        self.send_post("/v1/runs", payload).await
    }

    #[allow(dead_code)]
    pub async fn verify_run(&self, run_id: &str) -> Result<VerifyRunResponse, ApiClientError> {
        self.send_post(&format!("/v1/runs/{run_id}"), json!({}))
            .await
    }

    async fn send_get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiClientError> {
        let url = self.url(path);
        let mut request = Request::get(&url);
        if let Some(token) = self.token.as_deref() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }

        let response = request.send().await.map_err(|e| ApiClientError {
            status: None,
            code: None,
            message: format!("network error: {e}"),
            details: None,
        })?;

        parse_response(response).await
    }

    async fn send_post<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: Value,
    ) -> Result<T, ApiClientError> {
        let url = self.url(path);
        let payload_str = serde_json::to_string(&payload).map_err(|e| ApiClientError {
            status: None,
            code: None,
            message: format!("failed to encode request JSON: {e}"),
            details: None,
        })?;

        let mut request = Request::post(&url).header("Content-Type", "application/json");
        if let Some(token) = self.token.as_deref() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }

        let response = request
            .body(payload_str)
            .map_err(|e| ApiClientError {
                status: None,
                code: None,
                message: format!("failed to build request body: {e}"),
                details: None,
            })?
            .send()
            .await
            .map_err(|e| ApiClientError {
                status: None,
                code: None,
                message: format!("network error: {e}"),
                details: None,
            })?;

        parse_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

fn normalize_base_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.ends_with('/') {
        trimmed.trim_end_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}

async fn parse_response<T: DeserializeOwned>(
    response: gloo_net::http::Response,
) -> Result<T, ApiClientError> {
    let status = response.status();
    let text = response.text().await.map_err(|e| ApiClientError {
        status: Some(status),
        code: None,
        message: format!("failed to read response body: {e}"),
        details: None,
    })?;

    if (200..300).contains(&status) {
        serde_json::from_str::<T>(&text).map_err(|e| ApiClientError {
            status: Some(status),
            code: None,
            message: format!("failed to decode success response: {e}"),
            details: Some(json!({ "raw": text })),
        })
    } else if let Ok(err) = serde_json::from_str::<DaemonErrorEnvelope>(&text) {
        Err(ApiClientError {
            status: Some(status),
            code: Some(err.error.code),
            message: err.error.message,
            details: err.error.details,
        })
    } else {
        Err(ApiClientError {
            status: Some(status),
            code: None,
            message: format!("HTTP {status}: {text}"),
            details: None,
        })
    }
}
