use axum::{http::StatusCode, response::IntoResponse};

pub type Result<T> = std::result::Result<T, ApiError>;

pub enum ApiError {
    InternalServerError,
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!("sqlx error: {}", error);
        ApiError::InternalServerError
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ApiError::InternalServerError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
        }
    }
}
