//! Utility functions for the local engine.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::json;

use tabula_driver::{
    DriverError, MetadataPolicy, ProgramSourceFormat, RegisteredProgram, parse_program_sources,
    register_program_sources,
};

use crate::protocol::error::ErrorCode;
use crate::service::error::{ServiceError, ServiceResult};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedProgramInput {
    pub(crate) sources: tabula_driver::ProgramSourceFile,
    pub(crate) metadata_policy: MetadataPolicy,
}

pub(super) fn read_guard<'a, T>(
    lock: &'a RwLock<T>,
    store_name: &str,
) -> ServiceResult<RwLockReadGuard<'a, T>> {
    lock.read().map_err(|_| {
        ServiceError::internal(
            ErrorCode::InternalError,
            format!("{store_name} store lock is poisoned"),
        )
    })
}

pub(super) fn write_guard<'a, T>(
    lock: &'a RwLock<T>,
    store_name: &str,
) -> ServiceResult<RwLockWriteGuard<'a, T>> {
    lock.write().map_err(|_| {
        ServiceError::internal(
            ErrorCode::InternalError,
            format!("{store_name} store lock is poisoned"),
        )
    })
}

pub(super) fn next_id(counter: &AtomicU64, prefix: &str) -> String {
    let value = counter.fetch_add(1, Ordering::Relaxed) + 1;
    format!("{prefix}_{value:016x}")
}

pub(super) fn register_resolved_program(
    input: &ResolvedProgramInput,
) -> ServiceResult<RegisteredProgram> {
    register_program_sources(&input.sources, input.metadata_policy).map_err(map_driver_error)
}

pub(super) fn map_driver_error(err: DriverError) -> ServiceError {
    match err {
        DriverError::ReadFile { path, source } => ServiceError::bad_request(
            ErrorCode::FileIoError,
            DriverError::ReadFile { path, source }.to_string(),
        ),
        DriverError::ParseJson { path, source } => ServiceError::bad_request(
            ErrorCode::ParseError,
            DriverError::ParseJson { path, source }.to_string(),
        ),
        DriverError::Compile { diagnostics } => {
            ServiceError::unprocessable(ErrorCode::CompileError, "program compilation failed")
                .with_details(json!({ "diagnostics": diagnostics }))
        }
        DriverError::InvalidProgram { message } => {
            ServiceError::unprocessable(ErrorCode::ProgramValidationError, message)
        }
        DriverError::MissingContractMetadata => ServiceError::unprocessable(
            ErrorCode::ProgramSchemaError,
            DriverError::MissingContractMetadata.to_string(),
        ),
        DriverError::ContractMetadataMismatch { message } => ServiceError::unprocessable(
            ErrorCode::ProgramSchemaError,
            DriverError::ContractMetadataMismatch { message }.to_string(),
        ),
        DriverError::InvalidState { message } => {
            ServiceError::bad_request(ErrorCode::InvalidStateCell, message)
        }
        DriverError::InvalidBatch { message } => {
            ServiceError::bad_request(ErrorCode::InvalidBatchTx, message)
        }
        DriverError::Execution { message } => {
            ServiceError::unprocessable(ErrorCode::ExecutionError, message)
        }
    }
}

impl super::LocalEngine {
    pub(super) fn resolve_program_input(
        &self,
        input: &super::ProgramInputRef,
    ) -> ServiceResult<ResolvedProgramInput> {
        use super::{InputRef, ProgramInline};

        match input {
            InputRef::Inline { inline } => match inline {
                ProgramInline::Source { source } => {
                    parse_program_sources(source, ProgramSourceFormat::TabSource, "<inline:source>")
                        .map(|sources| ResolvedProgramInput {
                            sources,
                            metadata_policy: MetadataPolicy::Optional,
                        })
                        .map_err(map_driver_error)
                }
                ProgramInline::Program(program) => Ok(ResolvedProgramInput {
                    sources: program.clone(),
                    metadata_policy: MetadataPolicy::Required,
                }),
            },
            InputRef::File { file_path } => self.load_program_from_file(file_path),
            InputRef::Artifact { artifact_id } => Err(ServiceError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    fn load_program_from_file(
        &self,
        path: &std::path::Path,
    ) -> ServiceResult<ResolvedProgramInput> {
        let source = self.files.read_utf8_file(path, "program")?;
        let (format, metadata_policy) = if path.extension().and_then(|e| e.to_str()) == Some("tab")
        {
            (ProgramSourceFormat::TabSource, MetadataPolicy::Optional)
        } else {
            (ProgramSourceFormat::JsonArtifact, MetadataPolicy::Required)
        };

        parse_program_sources(&source, format, &path.display().to_string())
            .map(|sources| ResolvedProgramInput {
                sources,
                metadata_policy,
            })
            .map_err(map_driver_error)
    }
}
