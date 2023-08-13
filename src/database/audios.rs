use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbAudio {
    pub id: i64,
    pub transcription: Option<String>,
    pub file_sha256_text: String,
    pub created_at: DateTime<Utc>,
}

pub async fn get_audios(pool: &PgPool) -> sqlx::Result<Vec<DbAudio>> {
    sqlx::query_as("SELECT id, transcription FROM audios")
        .fetch_all(pool)
        .await
}
