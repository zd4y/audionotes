use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbAudio {
    pub id: i64,
    pub transcription: Option<String>,
    pub file_sha256_text: String,
    pub created_at: DateTime<Utc>,
    pub user_id: i64,
}

pub async fn get_audios_by(pool: &PgPool, user_id: i64) -> sqlx::Result<Vec<DbAudio>> {
    sqlx::query_as("SELECT id, transcription FROM audios WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await
}
