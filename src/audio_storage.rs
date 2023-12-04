use anyhow::Context;
use axum::{async_trait, extract::BodyStream, BoxError};
use azure_storage::StorageCredentials;
use azure_storage_blobs::{
    blob::{BlobBlockType, BlockList},
    prelude::{BlobClient, ClientBuilder},
};
use futures::{Stream, StreamExt, TryStreamExt};
use std::{
    io::{self, SeekFrom},
    path::{Path, PathBuf},
};
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
};
use tokio_util::{bytes::Bytes, io::StreamReader};

use crate::routes::audios::AUDIO_FILE_MIMETYPE;

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
    async fn get(&self, audio_id: i32) -> anyhow::Result<File> {
        let file = tokio::fs::File::open(self.get_path(audio_id)).await?;
        Ok(file)
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
        let storage_credentials = StorageCredentials::access_key(account, access_key);
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
    async fn get(&self, audio_id: i32) -> anyhow::Result<File> {
        let blob_client = self.get_client(audio_id);
        let mut stream = blob_client
            .get()
            .chunk_size(2u64 * 1024 * 1024)
            .into_stream();

        let file = tokio::task::spawn_blocking(tempfile::tempfile).await??;
        let mut file = File::from_std(file);
        let mut buf = BufWriter::new(&mut file);

        while let Some(value) = stream.next().await {
            let mut body = value?.data;
            while let Some(value) = body.next().await {
                let value = value?;
                buf.write_all(&value).await?;
            }
        }

        buf.flush().await?;
        file.seek(SeekFrom::Start(0)).await?;

        Ok(file)
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
    async fn get(&self, audio_id: i32) -> anyhow::Result<File> {
        tracing::info!("retrieving audio file {audio_id}");
        let file = tokio::task::spawn_blocking(tempfile::tempfile).await??;
        Ok(File::from_std(file))
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
