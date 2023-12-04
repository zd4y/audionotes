use anyhow::Context;
use axum::{async_trait, extract::BodyStream, BoxError};
use azure_core::Pageable;
use azure_storage::StorageCredentials;
use azure_storage_blobs::{
    blob::{operations::GetBlobResponse, BlobBlockType, BlockList},
    prelude::{BlobClient, ClientBuilder},
};
use futures::{Stream, StreamExt, TryStreamExt};
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
};
use tokio::{fs::File, io::BufWriter};
use tokio_util::{
    bytes::{BufMut, Bytes, BytesMut},
    io::{ReaderStream, StreamReader},
};

use crate::routes::audios::AUDIO_FILE_MIMETYPE;

pub const AUDIO_FILE_EXTENSION: &str = ".webm";
const UPLOADS_DIRECTORY: &str = "uploads";

#[async_trait]
pub trait AudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<AudioStream>;

    async fn store(&self, audio_id: i32, stream: BodyStream) -> anyhow::Result<()>;

    async fn delete(&self, audio_id: i32) -> anyhow::Result<()>;
}

pub struct LocalAudioStorage;

pub struct MockAudioStorage;

pub struct AzureAudioStorage {
    storage_credentials: StorageCredentials,
    account: String,
    container: String,
}

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
    async fn get(&self, audio_id: i32) -> anyhow::Result<AudioStream> {
        let file = tokio::fs::File::open(self.get_path(audio_id)).await?;
        Ok(AudioStream::from_file(file))
    }

    async fn store(&self, audio_id: i32, stream: BodyStream) -> anyhow::Result<()> {
        let path = self.get_path(audio_id);
        stream_to_file(&path, stream).await?;
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
}

impl AzureAudioStorage {
    pub fn new(account: &str, access_key: &str, container: &str) -> AzureAudioStorage {
        let storage_credentials = StorageCredentials::access_key(account, access_key.to_string());
        AzureAudioStorage {
            storage_credentials,
            account: account.to_string(),
            container: container.to_string(),
        }
    }

    fn get_client(&self, audio_id: i32) -> BlobClient {
        let blob_name = format!("{}{}", audio_id, AUDIO_FILE_EXTENSION);
        ClientBuilder::new(&self.account, self.storage_credentials.clone())
            .blob_client(&self.container, blob_name)
    }
}

#[async_trait]
impl AudioStorage for AzureAudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<AudioStream> {
        let blob_client = self.get_client(audio_id);
        let stream = blob_client
            .get()
            .chunk_size(2u64 * 1024 * 1024)
            .into_stream();
        Ok(AudioStream::from_pageable(stream))
    }

    async fn store(&self, audio_id: i32, mut stream: BodyStream) -> anyhow::Result<()> {
        let blob_client = self.get_client(audio_id);

        let mut block_list = BlockList::default();

        let mut i = 0;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            let block_id = format!("{:08X}", i);
            blob_client.put_block(block_id.clone(), bytes).await?;
            i += 1;
            block_list
                .blocks
                .push(BlobBlockType::new_uncommitted(block_id));
        }
        blob_client
            .put_block_list(block_list)
            .content_type(AUDIO_FILE_MIMETYPE)
            .await?;

        Ok(())
    }

    async fn delete(&self, audio_id: i32) -> anyhow::Result<()> {
        let blob_client = self.get_client(audio_id);
        blob_client.delete().await?;
        Ok(())
    }
}

#[async_trait]
impl AudioStorage for MockAudioStorage {
    async fn get(&self, audio_id: i32) -> anyhow::Result<AudioStream> {
        tracing::info!("retrieving audio file {audio_id}");
        let file = tokio::task::spawn_blocking(tempfile::tempfile).await??;
        let file = tokio::fs::File::from_std(file);
        Ok(AudioStream::from_file(file))
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

// Save a `Stream` to a file
pub async fn stream_to_file<S, E>(path: &Path, stream: S) -> anyhow::Result<u64>
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
        let mut file = BufWriter::new(File::create(path).await?);

        // Copy the body into the file.
        let total_bytes = tokio::io::copy(&mut body_reader, &mut file).await?;

        Ok::<_, io::Error>(total_bytes)
    }
    .await
    .context("failed to save file")
}

pub struct AudioStream {
    stream: Pin<Box<dyn Stream<Item = anyhow::Result<Bytes>> + Send + 'static>>,
}

impl AudioStream {
    pub async fn into_bytes(mut self) -> anyhow::Result<Bytes> {
        let mut result = BytesMut::new();
        while let Some(value) = self.stream.next().await {
            let bytes = value?;
            result.put(bytes);
        }
        Ok(result.freeze())
    }

    fn from_pageable(pageable: Pageable<GetBlobResponse, azure_core::Error>) -> AudioStream {
        let stream = pageable.then(|value| async move {
            let mut body = value?.data;
            let mut bytes = BytesMut::new();
            while let Some(value) = body.next().await {
                bytes.put(value?);
            }
            Ok::<_, anyhow::Error>(bytes.freeze())
        });
        AudioStream {
            stream: Box::pin(stream),
        }
    }

    fn from_file(file: File) -> AudioStream {
        let stream =
            ReaderStream::new(file).map(|value| value.map_err(Into::<anyhow::Error>::into));
        let stream = Box::new(stream);
        AudioStream {
            stream: Box::pin(stream),
        }
    }
}

impl Stream for AudioStream {
    type Item = anyhow::Result<Bytes>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}
