use anyhow::Context;
use axum::{async_trait, extract::BodyStream, BoxError};
use futures::{Stream, TryStreamExt};
use std::{
    io,
    path::{Path, PathBuf},
};
use tokio::{fs::File, io::BufWriter};
use tokio_util::{bytes::Bytes, io::StreamReader};

pub const AUDIO_FILE_EXTENSION: &str = ".webm";
const UPLOADS_DIRECTORY: &str = "uploads";

#[async_trait]
pub trait AudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<File>;

    async fn store(&self, audio_id: i32, stream: BodyStream) -> anyhow::Result<()>;

    async fn delete(&self, audio_id: i32) -> anyhow::Result<()>;
}

pub struct LocalAudioStorage;

pub struct MockAudioStorage;

impl LocalAudioStorage {
    pub async fn new() -> anyhow::Result<LocalAudioStorage> {
        if !Path::new(UPLOADS_DIRECTORY).exists() {
            tokio::fs::create_dir(UPLOADS_DIRECTORY)
                .await
                .context("failed to create the uploads directory")?;
        }
        Ok(LocalAudioStorage)
    }
}

#[async_trait]
impl AudioStorage for LocalAudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<File> {
        let file = tokio::fs::File::open(self.get_path(audio_id)).await?;
        Ok(file)
    }

    async fn store(&self, audio_id: i32, stream: BodyStream) -> anyhow::Result<()> {
        self.stream_to_file(audio_id, stream).await?;
        Ok(())
    }

    async fn delete(&self, audio_id: i32) -> anyhow::Result<()> {
        tokio::fs::remove_file(self.get_path(audio_id)).await?;
        Ok(())
    }
}

impl LocalAudioStorage {
    fn get_path(&self, audio_id: i32) -> PathBuf {
        // TODO: use file's sha256 as path
        std::path::Path::new(UPLOADS_DIRECTORY)
            .join(format!("{}{}", audio_id, AUDIO_FILE_EXTENSION))
    }

    // Save a `Stream` to a file
    async fn stream_to_file<S, E>(&self, audio_id: i32, stream: S) -> anyhow::Result<u64>
    where
        S: Stream<Item = Result<Bytes, E>>,
        E: Into<BoxError>,
    {
        let path = self.get_path(audio_id);
        async {
            // Convert the stream into an `AsyncRead`.
            let body_with_io_error =
                stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
            let body_reader = StreamReader::new(body_with_io_error);
            futures::pin_mut!(body_reader);

            // Create the file. `File` implements `AsyncWrite`.
            let mut file = BufWriter::new(File::create(path).await?);

            // Copy the body into the file.
            let total_bytes = tokio::io::copy(&mut body_reader, &mut file).await?;

            Ok::<_, io::Error>(total_bytes)
        }
        .await
        .context("failed to save file")
    }
}

#[async_trait]
impl AudioStorage for MockAudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<File> {
        tracing::info!("retrieving audio file {audio_id}");
        unimplemented!()
    }

    async fn store(&self, audio_id: i32, _stream: BodyStream) -> anyhow::Result<()> {
        tracing::info!("storing audio {audio_id}");
        Ok(())
    }

    async fn delete(&self, audio_id: i32) -> anyhow::Result<()> {
        tracing::info!("deleting audio {audio_id}");
        Ok(())
    }
}
