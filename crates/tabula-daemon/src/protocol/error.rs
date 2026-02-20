use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;

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
    ProofNotAvailable,
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
