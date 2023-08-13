use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbUser {
    pub id: i64,
    pub username: String,
    pub password: String,
}

pub async fn get_user(pool: &PgPool, id: i64) -> sqlx::Result<Option<DbUser>> {
    sqlx::query_as("select id, username from users where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
}
