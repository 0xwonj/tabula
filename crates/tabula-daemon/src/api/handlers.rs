use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use serde_json::json;
use tracing::warn;

use crate::api::json::JsonBody;
use crate::kernel::domain::{CheckCommand, CompileCommand, ExecuteCommand};
use crate::protocol::error::{ApiError, ApiResult, ErrorCode};
use crate::protocol::types::{
    CapabilitiesResponse, CheckRequest, CheckResponse, CompileRequest, CompileResponse,
    ExecuteRequest, ExecuteResponse, HealthResponse,
};
use crate::runtime::state::AppState;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

pub async fn capabilities(State(state): State<Arc<AppState>>) -> Json<CapabilitiesResponse> {
    Json(state.engine().capabilities().into())
}

pub async fn check(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    let cmd: CheckCommand = req.into();
    let engine = state.engine();
    let out = run_blocking(&state, "check", move || engine.check(cmd)).await?;
    Ok(Json(out.into()))
}

pub async fn compile(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<CompileRequest>,
) -> ApiResult<Json<CompileResponse>> {
    let cmd: CompileCommand = req.into();
    let engine = state.engine();
    let out = run_blocking(&state, "compile", move || engine.compile(cmd)).await?;
    Ok(Json(out.into()))
}

pub async fn execute(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<ExecuteRequest>,
) -> ApiResult<Json<ExecuteResponse>> {
    let cmd: ExecuteCommand = req.into();
    let engine = state.engine();
    let out = run_blocking(&state, "execute", move || engine.execute(cmd)).await?;
    Ok(Json(out.into()))
}

pub async fn prove_stub(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let engine = state.engine();
    let out = run_blocking(&state, "prove_stub", move || engine.prove_stub()).await?;
    Ok(Json(out))
}

pub async fn verify_stub(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let engine = state.engine();
    let out = run_blocking(&state, "verify_stub", move || engine.verify_stub()).await?;
    Ok(Json(out))
}

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

async fn run_blocking<T, F>(state: &Arc<AppState>, op: &'static str, work: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    let permit = tokio::time::timeout(state.queue_timeout(), state.limiter().acquire_owned())
        .await
        .map_err(|_| {
            ApiError::service_unavailable(
                ErrorCode::ServerBusy,
                format!("server is busy while scheduling {op}"),
            )
        })?
        .map_err(|_| {
            ApiError::internal(
                ErrorCode::InternalError,
                "request limiter is closed unexpectedly",
            )
        })?;

    let mut join = tokio::task::spawn_blocking(work);
    let request_timeout = state.request_timeout();
    let sleep = tokio::time::sleep(request_timeout);
    tokio::pin!(sleep);

    tokio::select! {
        joined = &mut join => {
            drop(permit);
            joined.map_err(|e| {
                ApiError::internal(
                    ErrorCode::TaskJoinError,
                    format!("failed to join blocking task for {op}: {e}"),
                )
            })?
        }
        _ = &mut sleep => {
            // `spawn_blocking` tasks cannot be forcefully cancelled once running.
            // Keep the permit until the blocking task completes to preserve backpressure.
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) = join.await {
                    warn!(operation = op, error = %e, "blocking task failed after request timeout");
                }
            });

            Err(ApiError::gateway_timeout(
                ErrorCode::RequestTimeout,
                format!(
                    "request timed out after {}ms while running {op}",
                    duration_millis(request_timeout)
                ),
            ))
        }
    }
}

fn duration_millis(d: Duration) -> u128 {
    d.as_millis()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use http::HeaderValue;

    use crate::kernel::domain::{
        Capabilities, CheckCommand, CheckResult, CompileCommand, CompileResult, ExecuteCommand,
        ExecuteResult,
    };
    use crate::kernel::engine::KernelEngine;
    use crate::runtime::config::ServerConfig;
    use crate::runtime::state::AppState;

    use super::run_blocking;

    struct NoopEngine;

    impl KernelEngine for NoopEngine {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                service_role: "test",
                clients: vec![],
                compile: false,
                check: false,
                execute: false,
                prove: false,
                verify: false,
                input_modes: vec![],
            }
        }

        fn check(&self, _req: CheckCommand) -> crate::protocol::error::ApiResult<CheckResult> {
            unreachable!("not used in this test")
        }

        fn compile(
            &self,
            _req: CompileCommand,
        ) -> crate::protocol::error::ApiResult<CompileResult> {
            unreachable!("not used in this test")
        }

        fn execute(
            &self,
            _req: ExecuteCommand,
        ) -> crate::protocol::error::ApiResult<ExecuteResult> {
            unreachable!("not used in this test")
        }
    }

    fn test_state(queue_timeout_ms: u64, request_timeout_ms: u64) -> Arc<AppState> {
        Arc::new(AppState::new(
            ServerConfig {
                bind_addr: "127.0.0.1:0".parse().expect("valid bind addr"),
                auth_token: None,
                allowed_roots: vec![std::env::temp_dir()],
                allow_origins: vec![HeaderValue::from_static("http://localhost:3000")],
                max_body_bytes: 1024,
                max_concurrent_jobs: 1,
                queue_timeout: Duration::from_millis(queue_timeout_ms),
                request_timeout: Duration::from_millis(request_timeout_ms),
            },
            Arc::new(NoopEngine),
        ))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timeout_keeps_backpressure_until_blocking_task_finishes() {
        let state = test_state(20, 1);

        let first = run_blocking(&state, "timeout_case", || {
            std::thread::sleep(Duration::from_millis(100));
            Ok::<_, crate::protocol::error::ApiError>(())
        })
        .await;

        let first_status = first
            .expect_err("first request should timeout")
            .into_response()
            .status();
        assert_eq!(first_status, StatusCode::GATEWAY_TIMEOUT);

        let started = Instant::now();
        let second = run_blocking(&state, "queued_case", || {
            Ok::<_, crate::protocol::error::ApiError>(())
        })
        .await;
        let waited = started.elapsed();

        let second_status = second
            .expect_err("second request should fail fast with queue timeout")
            .into_response()
            .status();
        assert_eq!(second_status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            waited >= Duration::from_millis(15),
            "expected queue wait, got {waited:?}"
        );
    }
}
