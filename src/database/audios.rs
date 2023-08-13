use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbAudio {
    pub id: i64,
    pub transcription: Option<String>,
    pub created_at: DateTime<Utc>,
    pub user_id: i64,
}

pub async fn get_audios_by(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<DbAudio>> {
    sqlx::query_as("select id, transcription, created_at, user_id from audios where user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn new_audio_by(pool: &PgPool, user_id: i64) -> sqlx::Result<i64> {
    let id: (i64,) = sqlx::query_as("insert into audios(user_id) values ($1) returning id")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(id.0)
}
