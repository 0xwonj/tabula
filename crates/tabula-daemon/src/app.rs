use axum::{
    Json, Router,
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use serde_json::json;

use crate::error::ApiError;
use crate::model::{
    CapabilitiesResponse, CheckRequest, CheckResponse, CompileRequest, CompileResponse,
    ExecuteRequest, ExecuteResponse, HealthResponse,
};
use crate::service::{handle_check, handle_compile, handle_execute};

#[derive(Clone, Debug)]
pub struct AppState {
    auth_token: Option<String>,
}

impl AppState {
    pub fn new(auth_token: Option<String>) -> Self {
        Self { auth_token }
    }
}

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/check", post(check))
        .route("/v1/compile", post(compile))
        .route("/v1/execute", post(execute))
        .route("/v1/jobs/prove", post(prove_stub))
        .route("/v1/jobs/verify", post(verify_stub))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/capabilities", get(capabilities))
        .merge(protected)
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

async fn capabilities() -> Json<CapabilitiesResponse> {
    Json(CapabilitiesResponse::v0())
}

async fn check(
    State(_state): State<AppState>,
    Json(req): Json<CheckRequest>,
) -> Result<Json<CheckResponse>, ApiError> {
    Ok(Json(handle_check(req)?))
}

async fn compile(
    State(_state): State<AppState>,
    Json(req): Json<CompileRequest>,
) -> Result<Json<CompileResponse>, ApiError> {
    Ok(Json(handle_compile(req)?))
}

async fn execute(
    State(_state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiError> {
    Ok(Json(handle_execute(req)?))
}

async fn prove_stub(State(_state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PROOF_NOT_AVAILABLE",
        "proof generation is not available yet",
    ))
}

async fn verify_stub(State(_state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_implemented(
        "PROOF_NOT_AVAILABLE",
        "proof verification is not available yet",
    ))
}

async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(expected) = state.auth_token.as_deref() else {
        return Ok(next.run(request).await);
    };

    let provided = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if provided == Some(expected) {
        Ok(next.run(request).await)
    } else {
        Err(
            ApiError::unauthorized("AUTH_REQUIRED", "missing or invalid bearer token")
                .with_details(json!({
                    "header": "Authorization: Bearer <token>"
                })),
        )
    }
}
