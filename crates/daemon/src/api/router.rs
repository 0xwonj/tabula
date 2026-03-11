use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{Method, header},
    middleware,
    routing::{get, post},
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use crate::api::handlers;
use crate::runtime::state::AppState;

pub fn build_router(state: &Arc<AppState>) -> Router {
    let allow_origins = AllowOrigin::list(state.allow_origins().iter().cloned());
    let cors = CorsLayer::new()
        .allow_origin(allow_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let protected = Router::new()
        .route(
            "/v1/programs",
            post(handlers::register_program).get(handlers::list_programs),
        )
        .route("/v1/programs/:program_id", get(handlers::get_program))
        .route(
            "/v1/instances",
            post(handlers::create_instance).get(handlers::list_instances),
        )
        .route("/v1/instances/:instance_id", get(handlers::get_instance))
        .route(
            "/v1/runs",
            post(handlers::submit_run).get(handlers::list_runs),
        )
        .route(
            "/v1/runs/:run_id",
            get(handlers::get_run).post(handlers::verify_run),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::require_auth,
        ));

    Router::new()
        .route("/v1/health", get(handlers::health))
        .route("/v1/capabilities", get(handlers::capabilities))
        .merge(protected)
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(state.max_body_bytes()))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
