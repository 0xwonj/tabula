use std::sync::Arc;
use std::time::Duration;

use tracing::{error, warn};

use crate::protocol::error::{ApiError, ApiResult, ErrorCode};
use crate::runtime::state::AppState;
use crate::service::ServiceResult;

pub(super) async fn run_blocking<T, F>(
    state: &Arc<AppState>,
    op: &'static str,
    work: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ServiceResult<T> + Send + 'static,
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
                error!(operation = op, error = %e, "blocking task panicked or was cancelled");
                ApiError::internal(
                    ErrorCode::TaskJoinError,
                    format!("failed to join blocking task for {op}: {e}"),
                )
            })?
            .map_err(|e| {
                error!(operation = op, code = ?e.code(), message = %e.message(), "service error");
                ApiError::from_service(&e)
            })
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
