use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbAudio {
    pub id: i32,
    pub transcription: Option<String>,
    pub created_at: DateTime<Utc>,
    pub user_id: i32,
}

pub async fn get_audios_by(pool: &PgPool, user_id: i32) -> sqlx::Result<Vec<DbAudio>> {
    sqlx::query_as("select id, transcription, created_at, user_id from audios where user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn new_audio_by(pool: &PgPool, user_id: i32) -> sqlx::Result<i32> {
    let id: (i32,) = sqlx::query_as("insert into audios(user_id) values ($1) returning id")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(id.0)
}
