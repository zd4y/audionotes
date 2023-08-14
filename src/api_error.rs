use axum::{http::StatusCode, response::IntoResponse};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    InternalServerError,
    NotFound,
    Unauthorized,
    BadRequest,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status_code, msg) = match self {
            ApiError::InternalServerError => {
                tracing::error!("sending error response: {:?}", self);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Not found"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            ApiError::BadRequest => (StatusCode::BAD_REQUEST, "Bad request"),
        };
        (status_code, msg).into_response()
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
