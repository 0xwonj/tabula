//! Utility functions for the local engine.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::json;

use tabula_artifact::CompiledProgram;
use tabula_compiler::{
    CompilerError, MetadataPolicy, ProgramSourceFormat, parse_program_sources,
    register_program_sources,
};

use crate::protocol::error::ErrorCode;
use crate::service::error::{ServiceError, ServiceResult};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedProgramInput {
    pub(crate) sources: tabula_compiler::ProgramSourceFile,
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
) -> ServiceResult<CompiledProgram> {
    register_program_sources(&input.sources, input.metadata_policy)
        .map_err(|e| map_compiler_error(&e))
}

pub(super) fn map_compiler_error(err: &CompilerError) -> ServiceError {
    match err {
        CompilerError::ReadFile { .. } => {
            ServiceError::bad_request(ErrorCode::FileIoError, err.to_string())
        }
        CompilerError::ParseJson { .. } => {
            ServiceError::bad_request(ErrorCode::ParseError, err.to_string())
        }
        CompilerError::Compile { diagnostics } => {
            ServiceError::unprocessable(ErrorCode::CompileError, "program compilation failed")
                .with_details(json!({ "diagnostics": diagnostics }))
        }
        CompilerError::InvalidProgram(source) => {
            ServiceError::unprocessable(ErrorCode::ProgramValidationError, source.to_string())
        }
        CompilerError::MissingContractMetadata => {
            ServiceError::unprocessable(ErrorCode::ProgramSchemaError, err.to_string())
        }
        CompilerError::ContractMetadataMismatch(source) => {
            ServiceError::unprocessable(ErrorCode::ProgramSchemaError, source.to_string())
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
                        .map_err(|e| map_compiler_error(&e))
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
            .map_err(|e| map_compiler_error(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_increments_sequentially() {
        let counter = AtomicU64::new(0);
        assert_eq!(next_id(&counter, "pfx"), "pfx_0000000000000001");
        assert_eq!(next_id(&counter, "pfx"), "pfx_0000000000000002");
        assert_eq!(next_id(&counter, "pfx"), "pfx_0000000000000003");
    }

    #[test]
    fn next_id_uses_prefix() {
        let counter = AtomicU64::new(0);
        let id = next_id(&counter, "inst");
        assert!(id.starts_with("inst_"));
        let counter2 = AtomicU64::new(0);
        let id2 = next_id(&counter2, "run");
        assert!(id2.starts_with("run_"));
    }

    #[test]
    fn read_guard_succeeds_on_clean_lock() {
        let lock = RwLock::new(42u32);
        let guard = read_guard(&lock, "test").expect("should succeed");
        assert_eq!(*guard, 42);
    }

    #[test]
    fn write_guard_succeeds_on_clean_lock() {
        let lock = RwLock::new(42u32);
        let mut guard = write_guard(&lock, "test").expect("should succeed");
        *guard = 99;
        drop(guard);
        assert_eq!(*lock.read().unwrap(), 99);
    }
}
