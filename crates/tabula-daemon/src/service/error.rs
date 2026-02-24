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
    pub fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::BadRequest,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn forbidden(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Forbidden,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn unprocessable(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Unprocessable,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_implemented(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotImplemented,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn conflict(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Conflict,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn http_status(&self) -> StatusCode {
        match self.kind {
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
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
