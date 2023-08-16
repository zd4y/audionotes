use std::{io, pin::Pin, task::Poll};

use anyhow::Context;
use axum::{
    body::{Bytes, StreamBody},
    extract::{BodyStream, Path},
    http::StatusCode,
    BoxError, Extension, Json,
};
use futures::{Stream, TryStreamExt};
use sqlx::PgPool;
use tokio::{fs::File, io::BufWriter};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::{api_error::ApiError, database, models::Audio, Claims};

const MAX_BYTES_TO_SAVE: usize = 25 * 1_000_000;

pub async fn get_audio(
    Extension(pool): Extension<PgPool>,
    claims: Claims,
    Path(audio_id): Path<i32>,
) -> crate::Result<Json<Audio>> {
    let audio = database::get_audio_by(&pool, audio_id, claims.user_id).await?;
    match audio {
        Some(audio) if audio.user_id == claims.user_id => Ok(Json(Audio {
            id: audio.id,
            transcription: audio.transcription,
            created_at: audio.created_at,
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

    let path_str = format!("{}/{}", crate::UPLOADS_DIRECTORY, audio.id);
    let path = std::path::Path::new(&path_str);
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

pub async fn new_audio(
    Extension(pool): Extension<PgPool>,
    claims: Claims,
    body: BodyStream,
) -> crate::Result<StatusCode> {
    let id = database::insert_audio_by(&pool, claims.user_id).await?;
    // TODO: use file's sha256 as path
    let path = id.to_string();
    match stream_to_file(&path, body).await {
        Ok(()) => Ok(StatusCode::CREATED),
        Err(err) => {
            database::delete_audio(&pool, id).await?;
            Err(err)
        }
    }
}

// Save a `Stream` to a file
async fn stream_to_file<S, E>(file_path: &str, stream: S) -> crate::Result<()>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    let mut bytes_copied = 0;
    let mut path = Default::default();
    async {
        // Convert the stream into an `AsyncRead`.
        futures::pin_mut!(stream);
        let body_with_io_error = stream
            .take_bytes(MAX_BYTES_TO_SAVE + 1)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        path = std::path::Path::new(crate::UPLOADS_DIRECTORY).join(file_path);
        let mut file = BufWriter::new(File::create(&path).await?);

        // Copy the body into the file.
        bytes_copied = tokio::io::copy(&mut body_reader, &mut file).await?;

        Ok::<_, io::Error>(())
    }
    .await
    .context("failed to save file")?;

    if bytes_copied
        > MAX_BYTES_TO_SAVE
            .try_into()
            .context("failed to convert MAX_BYTES_TO_SAVE to u64")?
    {
        tokio::fs::remove_file(path)
            .await
            .context("failed to delete file")?;
        return Err(ApiError::ExceededFileSizeLimit);
    }

    Ok(())
}

struct TakeBytes<S> {
    inner: S,
    seen: usize,
    limit: usize,
}

impl<S, E> Stream for TakeBytes<Pin<&mut S>>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    type Item = Result<Bytes, E>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if self.seen >= self.limit {
            return Poll::Ready(None); // Stream is over
        }

        let self_mut = self.get_mut();
        let inner = self_mut.inner.as_mut().poll_next(cx);
        if let Poll::Ready(Some(ref v)) = inner {
            let seen = v.as_ref().map(|b| b.len()).unwrap_or_default();
            self_mut.seen += seen;
        }
        inner
    }
}

trait TakeBytesExt: Sized {
    fn take_bytes(self, limit: usize) -> TakeBytes<Self>;
}

impl<S, E> TakeBytesExt for Pin<&mut S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    fn take_bytes(self, limit: usize) -> TakeBytes<Self> {
        TakeBytes {
            inner: self,
            limit,
            seen: 0,
        }
    }
}
