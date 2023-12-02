use std::{io, path::PathBuf, time::Duration};

use anyhow::Context;
use axum::{
    body::{Bytes, StreamBody},
    extract::{BodyStream, Path},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    BoxError, Extension, Json,
};
use futures::{future::BoxFuture, FutureExt, Stream, TryStreamExt};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::{fs::File, io::BufWriter};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::{
    database,
    models::{Audio, Tag},
    ApiError, AppState, Claims,
};

const AUDIO_FILE_EXTENSION: &str = ".webm";
const AUDIO_FILE_MIMETYPE: &str = "audio/webm";

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
    Extension(pool): Extension<PgPool>,
    claims: Claims,
    Path(audio_id): Path<i32>,
) -> crate::Result<StreamBody<ReaderStream<tokio::fs::File>>> {
    let audio = match database::get_audio_by(&pool, audio_id, claims.user_id).await? {
        Some(audio) => audio,
        None => return Err(ApiError::NotFound),
    };

    if audio.user_id != claims.user_id {
        return Err(ApiError::NotFound);
    }

    let path = get_audio_file_path(audio.id);
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(err) => {
            tracing::error!("tokio::fs::File::open error in get_audio_file_by: {}", err);
            return Err(ApiError::InternalServerError);
        }
    };

    let stream = ReaderStream::new(file);
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
    Extension(pool): Extension<PgPool>,
    Path(audio_id): Path<i32>,
    claims: Claims,
) -> crate::Result<StatusCode> {
    let deleted = database::delete_audio(&pool, claims.user_id, audio_id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    tokio::fs::remove_file(get_audio_file_path(audio_id))
        .await
        .context("failed to remove audio file")?;
    Ok(StatusCode::OK)
}

pub async fn new_audio(
    Extension(state): Extension<AppState>,
    claims: Claims,
    headers: HeaderMap,
    body: BodyStream,
) -> crate::Result<StatusCode> {
    let content_type = headers.get(CONTENT_TYPE).ok_or(ApiError::BadRequest)?;
    if content_type.to_str().map_err(|_| ApiError::BadRequest)? != AUDIO_FILE_MIMETYPE {
        return Err(ApiError::BadRequest);
    };

    let id = database::insert_audio_by(&state.pool, claims.user_id).await?;
    // TODO: use file's sha256 as path
    let path = get_audio_file_path(id);
    stream_to_file(&path, body).await?;

    tokio::spawn(async move {
        if let Err(err) =
            transcribe_and_update_retrying(&state, id, &path, &claims.language, None).await
        {
            tracing::error!("failed to transcribe and update retrying: {err}")
        }
    });

    Ok(StatusCode::CREATED)
}

// Save a `Stream` to a file
async fn stream_to_file<S, E>(path: &std::path::Path, stream: S) -> crate::Result<u64>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    Ok(async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        let mut file = BufWriter::new(File::create(path).await?);

        // Copy the body into the file.
        let total_bytes = tokio::io::copy(&mut body_reader, &mut file).await?;

        Ok::<_, io::Error>(total_bytes)
    }
    .await
    .context("failed to save file")?)
}

pub(crate) fn get_audio_file_path(audio_id: i32) -> PathBuf {
    std::path::Path::new(crate::UPLOADS_DIRECTORY)
        .join(format!("{}{}", audio_id, AUDIO_FILE_EXTENSION))
}

pub(crate) fn transcribe_and_update_retrying<'a>(
    state: &'a AppState,
    audio_id: i32,
    file_path: &'a std::path::Path,
    language: &'a str,
    failed_audio_transcription_id: Option<i32>,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        if let Some(failed_audio_transcription_id) = failed_audio_transcription_id {
            let retries = database::get_failed_audio_transcription_retries(&state.pool, failed_audio_transcription_id).await.context("failed to get audio transcription retries")?;
            if retries >= 3 {
                anyhow::bail!("reached maximum retries for failed audio transcription with id: {failed_audio_transcription_id}");
            }

            // wait before retrying, a minute for each retry
            let duration = Duration::from_secs(60 * (retries + 1) as u64);
            tracing::info!("retrying transcription of audio {audio_id} in {duration:?}");
            tokio::time::sleep(duration).await;
        }

        match transcribe_and_update(state, audio_id, file_path, language).await {
            Ok(()) => match failed_audio_transcription_id {
                Some(failed_audio_transcription_id) => {
                    database::delete_failed_audio_transcription(&state.pool, failed_audio_transcription_id).await?;
                    Ok(())
                },
                None => Ok(())
            }
            Err(err) => {
                tracing::error!("failed to transcribe audio with id {audio_id}: {err}");

                let failed_audio_transcription_id = match failed_audio_transcription_id {
                    Some(failed_audio_transcription_id) => {
                        database::update_failed_audio_transcription(&state.pool, failed_audio_transcription_id).await?;
                        failed_audio_transcription_id
                    },
                    None => {
                        database::insert_failed_audio_transcription(&state.pool, audio_id, language).await?
                    }
                };

                transcribe_and_update_retrying(
                    state,
                    audio_id,
                    file_path,
                    language,
                    Some(failed_audio_transcription_id),
                )
                .await
            }
        }
    }.boxed()
}

async fn transcribe_and_update(
    state: &AppState,
    audio_id: i32,
    file_path: &std::path::Path,
    language: &str,
) -> anyhow::Result<()> {
    let transcription = state
        .stt
        .transcribe(file_path, language)
        .await?;
    database::update_audio_transcription(&state.pool, audio_id, &transcription)
        .await
        .context("failed to update audio transcription")?;
    Ok(())
}
