//! Utility functions for the local engine.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::json;

use tabula_compiler::{
    CompilerError, SealedProgram, compile_program_source, parse_artifact, register_artifact,
    register_program_definition,
};

use crate::protocol::error::ErrorCode;
use crate::service::ProgramInputRef;
use crate::service::error::{ServiceError, ServiceResult};

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
        CompilerError::ArtifactMismatch { detail } => {
            ServiceError::unprocessable(ErrorCode::ProgramSchemaError, detail.clone())
        }
        CompilerError::ContractMetadataMismatch(source) => {
            ServiceError::unprocessable(ErrorCode::ProgramSchemaError, source.to_string())
        }
    }
}

impl super::LocalEngine {
    pub(super) fn compile_program_input(
        &self,
        input: &ProgramInputRef,
    ) -> ServiceResult<SealedProgram> {
        use super::{InputRef, ProgramInline};

        match input {
            InputRef::Inline { inline } => match inline {
                ProgramInline::Source { source } => compile_program_source(source)
                    .and_then(|definition| register_program_definition(&definition))
                    .map_err(|e| map_compiler_error(&e)),
                ProgramInline::Program(program) => {
                    register_artifact(program).map_err(|e| map_compiler_error(&e))
                }
            },
            InputRef::File { file_path } => self.load_program_from_file(file_path),
            InputRef::Artifact { artifact_id } => Err(ServiceError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    fn load_program_from_file(&self, path: &std::path::Path) -> ServiceResult<SealedProgram> {
        let source = self.files.read_utf8_file(path, "program")?;
        if path.extension().and_then(|e| e.to_str()) == Some("tab") {
            compile_program_source(&source)
                .and_then(|definition| register_program_definition(&definition))
                .map_err(|e| map_compiler_error(&e))
        } else {
            parse_artifact(&source, &path.display().to_string())
                .and_then(|artifact| register_artifact(&artifact))
                .map_err(|e| map_compiler_error(&e))
        }
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
