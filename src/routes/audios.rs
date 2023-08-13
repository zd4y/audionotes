use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use sqlx::PgPool;

use crate::{database, models::Audio};

pub async fn all_audios(
    State(pool): State<PgPool>,
) -> crate::Result<(StatusCode, Json<Vec<Audio>>)> {
    let audios = database::get_audios(&pool).await?;
    let audios = audios
        .into_iter()
        .map(|audio| Audio {
            id: audio.id,
            transcription: audio.transcription,
            created_at: audio.created_at,
        })
        .collect();
    Ok((StatusCode::OK, Json(audios)))
}

pub async fn new_audio() -> (StatusCode, Json<Audio>) {
    (
        StatusCode::CREATED,
        Json(Audio {
            id: 1,
            transcription: None,
            created_at: Utc::now(),
        }),
    )
}
