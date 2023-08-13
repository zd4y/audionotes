use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;

use crate::models::User;

pub async fn get_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<i64>,
) -> crate::Result<(StatusCode, Json<Option<User>>)> {
    let row: Option<(i64, String)> = sqlx::query_as("select id, username from users where id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await?;

    Ok(match row {
        Some(row) => (
            StatusCode::OK,
            Json(Some(User {
                id: row.0,
                username: row.1,
            })),
        ),
        None => (StatusCode::NOT_FOUND, Json(None)),
    })
}

pub async fn change_password() {
    todo!()
}
