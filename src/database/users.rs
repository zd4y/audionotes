use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbUser {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub password: Option<String>,
}

pub async fn get_user(pool: &PgPool, id: i32) -> sqlx::Result<Option<DbUser>> {
    sqlx::query_as("select id, email, username, password from users where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_user_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<DbUser>> {
    sqlx::query_as("select id, email, username, password from users where email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
}
