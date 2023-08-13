use std::io;

use axum::{
    body::Bytes,
    extract::{BodyStream, Path, State},
    http::StatusCode,
    BoxError, Json,
};
use futures::{Stream, TryStreamExt};
use sqlx::PgPool;
use tokio::{fs::File, io::BufWriter};
use tokio_util::io::StreamReader;

use crate::{api_error::ApiError, database, models::Audio};

pub async fn all_audios_by(
    State(pool): State<PgPool>,
    Path(user_id): Path<i64>,
) -> crate::Result<(StatusCode, Json<Vec<Audio>>)> {
    let audios = database::get_audios_by(&pool, user_id).await?;
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
    State(pool): State<PgPool>,
    Path(user_id): Path<i64>,
    body: BodyStream,
) -> crate::Result<StatusCode> {
    let id = database::new_audio_by(&pool, user_id).await?;
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
