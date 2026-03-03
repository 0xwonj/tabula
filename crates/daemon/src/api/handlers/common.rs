use std::sync::Arc;

use axum::{Json, extract::State};

use crate::protocol::types::{CapabilitiesResponse, HealthResponse};
use crate::runtime::state::AppState;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

pub async fn capabilities(State(state): State<Arc<AppState>>) -> Json<CapabilitiesResponse> {
    Json(state.engine().capabilities().into())
}
