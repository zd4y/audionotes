use std::time::Duration;

use anyhow::Context;
use axum::{
    body::StreamBody,
    extract::{BodyStream, Path},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    Extension, Json,
};
use futures::{future::BoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{instrument, Instrument};

use crate::{
    audio_storage::AudioStream,
    database,
    models::{Audio, Tag},
    ApiError, AppState, Claims,
};

pub const AUDIO_FILE_MIMETYPE: &str = "audio/webm";

pub async fn get_audio(
    Extension(pool): Extension<PgPool>,
    claims: Claims,
    Path(audio_id): Path<i32>,
) -> crate::Result<Json<Audio>> {
    let audio = database::get_audio_by(&pool, audio_id, claims.user_id).await?;
    let audio_tags = database::get_audio_tags(&pool, audio_id)
        .await?
        .into_iter()
        .map(Tag::from)
        .collect();
    match audio {
        Some(audio) if audio.user_id == claims.user_id => Ok(Json(Audio {
            id: audio.id,
            transcription: audio.transcription,
            created_at: audio.created_at,
            tags: audio_tags,
        })),
        None | Some(_) => Err(ApiError::NotFound),
    }
}

pub async fn get_audio_file(
    Extension(state): Extension<AppState>,
    claims: Claims,
    Path(audio_id): Path<i32>,
) -> crate::Result<StreamBody<AudioStream>> {
    let audio = match database::get_audio_by(&state.pool, audio_id, claims.user_id).await? {
        Some(audio) => audio,
        None => return Err(ApiError::NotFound),
    };

    if audio.user_id != claims.user_id {
        return Err(ApiError::NotFound);
    }

    let stream = state.storage.get(audio.id).await?;
    let body = StreamBody::new(stream);

    Ok(body)
}

pub async fn all_audios(
    Extension(pool): Extension<PgPool>,
    claims: Claims,
) -> crate::Result<(StatusCode, Json<Vec<Audio>>)> {
    let audios = database::get_audios_by(&pool, claims.user_id).await?;
    let mut audios_tags = database::get_audios_tags(&pool, claims.user_id).await?;
    let audios = audios
        .into_iter()
        .map(|audio| {
            let tags = audios_tags
                .remove(&audio.id)
                .unwrap_or_default()
                .into_iter()
                .map(Tag::from)
                .collect();
            Audio {
                id: audio.id,
                transcription: audio.transcription,
                created_at: audio.created_at,
                tags,
            }
        })
        .collect();
    Ok((StatusCode::OK, Json(audios)))
}

#[derive(Deserialize)]
pub struct TagAudioPayload {
    name: String,
    color: Option<String>,
}

pub async fn tag_audio(
    Extension(pool): Extension<PgPool>,
    Path(audio_id): Path<i32>,
    claims: Claims,
    Json(payload): Json<TagAudioPayload>,
) -> crate::Result<StatusCode> {
    let audio = database::get_audio_by(&pool, audio_id, claims.user_id).await?;
    match audio {
        Some(a) if a.user_id == claims.user_id => {}
        _ => return Err(ApiError::NotFound),
    }
    let db_tag =
        database::get_or_create_tag(&pool, claims.user_id, &payload.name, payload.color).await?;
    database::tag_audio(&pool, db_tag.id, audio_id).await?;
    Ok(StatusCode::OK)
}

pub async fn all_tags(
    Extension(pool): Extension<PgPool>,
    claims: Claims,
) -> crate::Result<(StatusCode, Json<Vec<Tag>>)> {
    let tags = database::get_all_tags(&pool, claims.user_id)
        .await?
        .into_iter()
        .map(Tag::from)
        .collect();
    Ok((StatusCode::OK, Json(tags)))
}

pub async fn delete_audio(
    Extension(state): Extension<AppState>,
    Path(audio_id): Path<i32>,
    claims: Claims,
) -> crate::Result<StatusCode> {
    let deleted = database::delete_audio(&state.pool, claims.user_id, audio_id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    state
        .storage
        .delete(audio_id)
        .await
        .context("failed to remove audio file")?;
    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct NewAudioBody {
    id: i32,
}

pub async fn new_audio(
    Extension(state): Extension<AppState>,
    claims: Claims,
    headers: HeaderMap,
    body: BodyStream,
) -> crate::Result<(StatusCode, Json<NewAudioBody>)> {
    let content_type = headers.get(CONTENT_TYPE).ok_or(ApiError::BadRequest)?;
    if content_type.to_str().map_err(|_| ApiError::BadRequest)? != AUDIO_FILE_MIMETYPE {
        return Err(ApiError::BadRequest);
    };

    let id = database::insert_audio_by(&state.pool, claims.user_id).await?;
    tokio::spawn(async move {
        if let Err(err) = state.storage.store(id, body).await {
            tracing::error!(?err, audio_id = id, "failed to store audio");
        }

        if let Err(err) = transcribe_and_update_retrying(&state, id, &claims.language, None).await {
            tracing::error!(?err, "failed to transcribe and update retrying")
        }
    });

    Ok((StatusCode::CREATED, Json(NewAudioBody { id })))
}

#[instrument]
pub(crate) fn transcribe_and_update_retrying<'a>(
    state: &'a AppState,
    audio_id: i32,
    language: &'a str,
    failed_audio_transcription_id: Option<i32>,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        if let Some(failed_audio_transcription_id) = failed_audio_transcription_id {
            match database::get_failed_audio_transcription_retries(&state.pool, failed_audio_transcription_id).await.context("failed to get audio transcription retries")? {
                Some(retries) if retries >= 3 => {
                    anyhow::bail!("reached maximum retries for failed audio transcription with id: {failed_audio_transcription_id}");
                }
                Some(_retries) => {}
                None => return Ok(())
            }
        }

        tracing::info!("getting transcription of audio {audio_id}");

        match transcribe_and_update(state, audio_id, language).await {
            Ok(()) => match failed_audio_transcription_id {
                Some(failed_audio_transcription_id) => {
                    database::delete_failed_audio_transcription(&state.pool, failed_audio_transcription_id).await?;
                    Ok(())
                },
                None => Ok(())
            }
            Err(err) => {
                tracing::error!(?err, audio_id, "failed to transcribe audio");

                let failed_audio_transcription_id = match failed_audio_transcription_id {
                    Some(failed_audio_transcription_id) => {
                        database::update_failed_audio_transcription(&state.pool, failed_audio_transcription_id).await?;
                        failed_audio_transcription_id
                    },
                    None => {
                        database::insert_failed_audio_transcription(&state.pool, audio_id, language).await?
                    }
                };

                // wait a minute before retrying
                let duration = Duration::from_secs(60u64);
                tracing::info!("retrying transcription of audio {audio_id} in {duration:?}");
                tokio::time::sleep(duration).await;

                transcribe_and_update_retrying(
                    state,
                    audio_id,
                    language,
                    Some(failed_audio_transcription_id),
                )
                .await
            }
        }
    }
    .in_current_span()
    .boxed()
}

#[instrument]
async fn transcribe_and_update(
    state: &AppState,
    audio_id: i32,
    language: &str,
) -> anyhow::Result<()> {
    let file = state.storage.get(audio_id).await?;
    let transcription = state.stt.transcribe(file, language).await?;
    database::update_audio_transcription(&state.pool, audio_id, &transcription)
        .await
        .context("failed to update audio transcription")?;
    Ok(())
}
