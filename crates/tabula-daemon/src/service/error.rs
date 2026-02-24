//! Service-layer error type used by the engine.

use axum::http::StatusCode;
use serde_json::Value;

use crate::protocol::error::ErrorCode;

/// Service-layer result type.
pub type ServiceResult<T> = Result<T, ServiceError>;

/// High-level error class for HTTP mapping.
#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    /// Invalid caller input.
    BadRequest,
    /// Access denied.
    Forbidden,
    /// Semantically invalid request content.
    Unprocessable,
    /// Feature not yet implemented.
    NotImplemented,
    /// Resource does not exist.
    NotFound,
    /// State conflict with current server-side version.
    Conflict,
    /// Server-side failure.
    Internal,
}

impl ErrorKind {
    /// Map this error kind to an HTTP status code.
    pub fn http_status(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Typed service-layer error.
#[derive(Debug, Clone)]
pub struct ServiceError {
    kind: ErrorKind,
    code: ErrorCode,
    message: String,
    details: Option<Value>,
}

#[allow(missing_docs)]
impl ServiceError {
    /// Core constructor — all factory methods delegate here.
    pub fn new(kind: ErrorKind, code: ErrorCode, message: impl Into<String>) -> Self {
        Self { kind, code, message: message.into(), details: None }
    }

    pub fn bad_request(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::BadRequest, code, msg) }
    pub fn forbidden(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::Forbidden, code, msg) }
    pub fn unprocessable(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::Unprocessable, code, msg) }
    pub fn not_implemented(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::NotImplemented, code, msg) }
    pub fn not_found(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::NotFound, code, msg) }
    pub fn conflict(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::Conflict, code, msg) }
    pub fn internal(code: ErrorCode, msg: impl Into<String>) -> Self { Self::new(ErrorKind::Internal, code, msg) }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn http_status(&self) -> StatusCode {
        self.kind.http_status()
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> Option<&Value> {
        self.details.as_ref()
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}
