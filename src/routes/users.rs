use axum::{extract::Path, http::StatusCode, Json};

use crate::models::User;

pub async fn get_user(Path(user_id): Path<u32>) -> (StatusCode, Json<User>) {
    (
        StatusCode::OK,
        Json(User {
            id: user_id,
            username: "john".to_string(),
        }),
    )
}
