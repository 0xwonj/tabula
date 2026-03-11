use axum::{
    async_trait,
    extract::{FromRequest, Json, Request, rejection::JsonRejection},
    http::StatusCode,
};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::protocol::error::{ApiError, ErrorCode};

/// JSON extractor that guarantees daemon-wide error envelope shape.
pub struct JsonBody<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for JsonBody<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rej) => Err(map_json_rejection(&rej)),
        }
    }
}

fn map_json_rejection(rej: &JsonRejection) -> ApiError {
    let status = rej.status();
    let message = rej.body_text();

    let err = match status {
        StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError::payload_too_large(ErrorCode::PayloadTooLarge, message.clone())
        }
        StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            ApiError::unsupported_media_type(ErrorCode::UnsupportedContentType, message.clone())
        }
        _ => ApiError::bad_request(ErrorCode::InvalidJson, message.clone()),
    };

    err.with_details(json!({
        "status": status.as_u16(),
        "reason": message,
    }))
}
