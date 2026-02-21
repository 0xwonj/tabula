use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;

use crate::service::error::{ErrorKind, ServiceError};

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    AuthRequired,
    CompileError,
    ProgramValidationError,
    ProgramSchemaError,
    InvalidStateCell,
    InvalidBatchTx,
    ExecutionError,
    ArtifactInputNotAvailable,
    FileIoError,
    ParseError,
    PathNotAllowed,
    ProgramNotFound,
    ProgramAlreadyRegistered,
    InstanceNotFound,
    RunNotFound,
    InstanceNotActive,
    InstanceVersionMismatch,
    TaskJoinError,
    InvalidJson,
    UnsupportedContentType,
    PayloadTooLarge,
    ServerBusy,
    RequestTimeout,
    BadRequest,
    InternalError,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
    details: Option<Value>,
}

impl ApiError {
    pub fn from_service(err: ServiceError) -> Self {
        let code = err.code();
        let mut out = match err.kind() {
            ErrorKind::BadRequest => Self::bad_request(code, err.message()),
            ErrorKind::Forbidden => Self::forbidden(code, err.message()),
            ErrorKind::Unprocessable => Self::unprocessable(code, err.message()),
            ErrorKind::NotImplemented => Self::not_implemented(code, err.message()),
            ErrorKind::NotFound => Self::not_found(code, err.message()),
            ErrorKind::Conflict => Self::conflict(code, err.message()),
            ErrorKind::Internal => Self::internal(code, err.message()),
        };
        if let Some(details) = err.details() {
            out = out.with_details(details.clone());
        }
        out
    }

    pub fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn unauthorized(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn forbidden(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn unprocessable(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_implemented(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn conflict(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn unsupported_media_type(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn payload_too_large(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn service_unavailable(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn gateway_timeout(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn generic_bad_request(message: impl Into<String>) -> Self {
        Self::bad_request(ErrorCode::BadRequest, message)
    }

    pub fn generic_internal(message: impl Into<String>) -> Self {
        Self::internal(ErrorCode::InternalError, message)
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    ok: bool,
    error: ErrorPayload,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(
                status = %self.status,
                code = ?self.code,
                message = %self.message,
                details = ?self.details,
                "API error response"
            );
        } else {
            tracing::warn!(
                status = %self.status,
                code = ?self.code,
                message = %self.message,
                details = ?self.details,
                "API error response"
            );
        }

        let body = ErrorEnvelope {
            ok: false,
            error: ErrorPayload {
                code: self.code,
                message: self.message,
                details: self.details,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_not_found_maps_to_404() {
        let service_err = ServiceError::not_found(ErrorCode::ProgramNotFound, "missing program");
        let response = ApiError::from_service(service_err).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn service_conflict_maps_to_409() {
        let service_err =
            ServiceError::conflict(ErrorCode::InstanceVersionMismatch, "version mismatch");
        let response = ApiError::from_service(service_err).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}
