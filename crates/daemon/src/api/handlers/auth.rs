use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use serde_json::json;

use crate::protocol::error::{ApiError, ApiResult, ErrorCode};
use crate::runtime::state::AppState;

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> ApiResult<Response> {
    let Some(expected) = state.auth_token() else {
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
            ApiError::unauthorized(ErrorCode::AuthRequired, "missing or invalid bearer token")
                .with_details(json!({
                    "header": "Authorization: Bearer <token>"
                })),
        )
    }
}
