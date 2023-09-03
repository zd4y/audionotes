use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbAudio {
    pub id: i32,
    pub transcription: Option<String>,
    pub created_at: DateTime<Utc>,
    pub user_id: i32,
}

pub async fn get_audio_by(
    pool: &PgPool,
    audio_id: i32,
    user_id: i32,
) -> sqlx::Result<Option<DbAudio>> {
    sqlx::query_as(
        "select id, transcription, created_at, user_id from audios where id = $1 and user_id = $2",
    )
    .bind(audio_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_audios_by(pool: &PgPool, user_id: i32) -> sqlx::Result<Vec<DbAudio>> {
    sqlx::query_as(
        "select id, transcription, created_at, user_id
         from audios
         where user_id = $1
         order by id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn insert_audio_by(pool: &PgPool, user_id: i32) -> sqlx::Result<i32> {
    let id: (i32,) = sqlx::query_as("insert into audios(user_id) values ($1) returning id")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(id.0)
}

pub async fn update_audio_transcription(
    pool: &PgPool,
    audio_id: i32,
    new_transcription: &str,
) -> sqlx::Result<()> {
    sqlx::query("update audios set transcription = $1 where id = $2")
        .bind(new_transcription)
        .bind(audio_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_audio(pool: &PgPool, user_id: i32, audio_id: i32) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from audios where user_id = $1 and id = $2")
        .bind(user_id)
        .bind(audio_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}
