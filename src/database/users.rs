use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbUser {
    pub id: i32,
    pub email: String,
    pub language: String,
    pub password: Option<String>,
}

pub async fn get_user(pool: &PgPool, id: i32) -> sqlx::Result<Option<DbUser>> {
    sqlx::query_as("select id, email, language, password from users where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_user_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<DbUser>> {
    sqlx::query_as("select id, email, language, password from users where email = $1")
        .bind(email.to_lowercase())
        .fetch_optional(pool)
        .await
}

pub async fn update_user_password(
    pool: &PgPool,
    user_id: i32,
    new_password: String,
) -> sqlx::Result<()> {
    sqlx::query("update users set password = $1 where id = $2")
        .bind(new_password)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
