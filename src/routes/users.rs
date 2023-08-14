use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::{api_error::ApiError, database, models::User};

pub async fn get_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<i32>,
) -> crate::Result<(StatusCode, Json<User>)> {
    let user = database::get_user(&pool, user_id).await?;

    match user {
        Some(user) => Ok((
            StatusCode::OK,
            Json(User {
                id: user.id,
                username: user.username,
            }),
        )),
        None => Err(ApiError::NotFound),
    }
}

pub async fn change_password() {
    todo!()
}
