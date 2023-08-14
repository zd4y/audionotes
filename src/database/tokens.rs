use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow, Debug)]
pub struct DbToken {
    pub user_id: i32,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn get_user_tokens(pool: &PgPool, user_id: i32) -> sqlx::Result<Vec<DbToken>> {
    sqlx::query_as(
        "select user_id, token, expires_at from password_reset_tokens where user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn insert_token(pool: &PgPool, user_id: i32, token: String) -> sqlx::Result<()> {
    sqlx::query("insert into password_reset_tokens (user_id, token) values ($1, $2)")
        .bind(user_id)
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_user_tokens(pool: &PgPool, user_id: i32) -> sqlx::Result<()> {
    sqlx::query("delete from password_reset_tokens where user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_token(pool: &PgPool, user_id: i32, token: String) -> sqlx::Result<()> {
    sqlx::query("delete from password_reset_tokens where user_id = $1 and token = $2")
        .bind(user_id)
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}
