use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::api::handlers::blocking::run_blocking;
use crate::api::json::JsonBody;
use crate::protocol::error::ApiResult;
use crate::protocol::types::response::{
    ApiResponse, InstanceListResponse, InstanceResponse, ProgramListResponse, ProgramResponse,
    RunListResponse, RunResponse,
};
use crate::runtime::state::AppState;
use crate::service::{
    CreateInstanceCommand, ListInstancesCommand, ListRunsCommand, RegisterProgramCommand,
    SubmitRunCommand, VerifyOutcome,
};

pub async fn register_program(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<RegisterProgramCommand>,
) -> ApiResult<Json<ApiResponse<ProgramResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "register_program", move || {
        engine.register_program(req)
    })
    .await?;
    Ok(Json(ApiResponse::ok(ProgramResponse { program: out })))
}

pub async fn list_programs(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<ApiResponse<ProgramListResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "list_programs", move || engine.list_programs()).await?;
    Ok(Json(ApiResponse::ok(ProgramListResponse { programs: out })))
}

pub async fn get_program(
    State(state): State<Arc<AppState>>,
    Path(program_id): Path<String>,
) -> ApiResult<Json<ApiResponse<ProgramResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "get_program", move || {
        engine.get_program(&program_id)
    })
    .await?;
    Ok(Json(ApiResponse::ok(ProgramResponse { program: out })))
}

pub async fn create_instance(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<CreateInstanceCommand>,
) -> ApiResult<Json<ApiResponse<InstanceResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "create_instance", move || {
        engine.create_instance(req)
    })
    .await?;
    Ok(Json(ApiResponse::ok(InstanceResponse { instance: out })))
}

pub async fn list_instances(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListInstancesCommand>,
) -> ApiResult<Json<ApiResponse<InstanceListResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "list_instances", move || {
        engine.list_instances(query)
    })
    .await?;
    Ok(Json(ApiResponse::ok(InstanceListResponse {
        instances: out,
    })))
}

pub async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> ApiResult<Json<ApiResponse<InstanceResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "get_instance", move || {
        engine.get_instance(&instance_id)
    })
    .await?;
    Ok(Json(ApiResponse::ok(InstanceResponse { instance: out })))
}

pub async fn submit_run(
    State(state): State<Arc<AppState>>,
    JsonBody(req): JsonBody<SubmitRunCommand>,
) -> ApiResult<Json<ApiResponse<RunResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "submit_run", move || engine.submit_run(req)).await?;
    Ok(Json(ApiResponse::ok(RunResponse { run: out })))
}

pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListRunsCommand>,
) -> ApiResult<Json<ApiResponse<RunListResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "list_runs", move || engine.list_runs(query)).await?;
    Ok(Json(ApiResponse::ok(RunListResponse { runs: out })))
}

pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<ApiResponse<RunResponse>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "get_run", move || engine.get_run(&run_id)).await?;
    Ok(Json(ApiResponse::ok(RunResponse { run: out })))
}

pub async fn verify_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<ApiResponse<VerifyOutcome>>> {
    let engine = state.engine();
    let out = run_blocking(&state, "verify_run", move || engine.verify_run(&run_id)).await?;
    Ok(Json(ApiResponse::ok(out)))
}
