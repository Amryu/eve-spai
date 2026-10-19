use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden")]
    Forbidden,
    /// Refused, with the reason a person can act on.
    #[error("{0}")]
    ForbiddenMsg(String),
    /// Something this answer depends on is out of reach for now.
    #[error("{0}")]
    Unavailable(String),
    #[error("not found")]
    NotFound,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("too many requests")]
    TooManyRequests,
    /// A route of an earlier protocol, answered with what to do instead.
    #[error("{0}")]
    Gone(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden | AppError::ForbiddenMsg(_) => StatusCode::FORBIDDEN,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            AppError::Gone(_) => StatusCode::GONE,
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let message = match &self {
            AppError::Internal(_) => "internal server error".to_string(),
            other => other.to_string(),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}
