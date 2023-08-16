use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use zxcvbn::feedback::Feedback;

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    InternalServerError,
    NotFound,
    Unauthorized,
    BadRequest,
    WeakPassword(Feedback),
    ExceededFileSizeLimit,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status_code, msg) = match self {
            ApiError::InternalServerError => {
                tracing::error!("sending error response: {:?}", self);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Not found"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            ApiError::BadRequest => (StatusCode::BAD_REQUEST, "Bad request"),
            ApiError::WeakPassword(feedback) => {
                let suggestions = feedback
                    .suggestions()
                    .iter()
                    .map(|suggestion| suggestion.to_string())
                    .collect::<Vec<_>>();
                let warning = feedback.warning().map(|warning| warning.to_string());
                let body = Json(json!({
                    "error": "Weak password",
                    "suggestions": suggestions,
                    "warning": warning
                }));
                return (StatusCode::BAD_REQUEST, body).into_response();
            }
            ApiError::ExceededFileSizeLimit => {
                (StatusCode::BAD_REQUEST, "Exceeded file size limit")
            }
        };
        let body = Json(json!({ "error": msg }));
        (status_code, body).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!("sqlx error: {}", error);
        ApiError::InternalServerError
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!("anyhow error: {}", error);
        ApiError::InternalServerError
    }
}
