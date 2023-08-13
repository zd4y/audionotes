use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::{database, models::User};

pub async fn get_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<i64>,
) -> crate::Result<(StatusCode, Json<Option<User>>)> {
    let user = database::get_user(&pool, user_id).await?;

    Ok(match user {
        Some(user) => (
            StatusCode::OK,
            Json(Some(User {
                id: user.id,
                username: user.username,
            })),
        ),
        None => (StatusCode::NOT_FOUND, Json(None)),
    })
}

pub async fn change_password() {
    todo!()
}
