use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::IntoResponse;
use http::HeaderValue;

use crate::api::handlers::blocking::run_blocking;
use crate::runtime::config::ServerConfig;
use crate::runtime::state::AppState;
use crate::service::{FileAccessPolicy, LocalEngine};
use tabula_testing::fs::{TempDir, tempdir};

fn test_state(queue_timeout_ms: u64, request_timeout_ms: u64) -> (Arc<AppState>, TempDir) {
    let root = tempdir();
    let allowed_root = root.path().to_path_buf();
    let policy = FileAccessPolicy::new(vec![allowed_root.clone()]).expect("policy");
    let engine = Arc::new(LocalEngine::new(policy));
    (
        Arc::new(AppState::new(
            ServerConfig {
                bind_addr: "127.0.0.1:0".parse().expect("valid bind addr"),
                auth_token: None,
                allowed_roots: vec![allowed_root],
                allow_origins: vec![HeaderValue::from_static("http://localhost:3000")],
                max_body_bytes: 1024,
                max_concurrent_jobs: 1,
                queue_timeout: Duration::from_millis(queue_timeout_ms),
                request_timeout: Duration::from_millis(request_timeout_ms),
            },
            engine,
        )),
        root,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_keeps_backpressure_until_blocking_task_finishes() {
    let (state, _root) = test_state(20, 1);

    let first = run_blocking(&state, "timeout_case", || {
        std::thread::sleep(Duration::from_millis(100));
        Ok::<_, crate::service::ServiceError>(())
    })
    .await;

    let first_status = first
        .expect_err("first request should timeout")
        .into_response()
        .status();
    assert_eq!(first_status, StatusCode::GATEWAY_TIMEOUT);

    let started = Instant::now();
    let second = run_blocking(&state, "queued_case", || {
        Ok::<_, crate::service::ServiceError>(())
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
