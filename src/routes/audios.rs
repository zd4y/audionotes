use std::io;

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
    let audio = database::get_audio_by(&pool, audio_id, claims.user_id).await?;

    let audio = match audio {
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
    let id = database::new_audio_by(&pool, claims.user_id).await?;
    let path = id.to_string();
    stream_to_file(&path, body).await?;
    Ok(StatusCode::CREATED)
}

// Save a `Stream` to a file
async fn stream_to_file<S, E>(path: &str, stream: S) -> crate::Result<()>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        let path = std::path::Path::new(crate::UPLOADS_DIRECTORY).join(path);
        let mut file = BufWriter::new(File::create(path).await?);

        // Copy the body into the file.
        tokio::io::copy(&mut body_reader, &mut file).await?;

        Ok::<_, io::Error>(())
    }
    .await
    .map_err(|_| ApiError::InternalServerError)
}
