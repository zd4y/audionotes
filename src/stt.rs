use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::Context;
use axum::async_trait;
use futures::StreamExt;
use leopard::LeopardBuilder;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use tempfile::TempDir;
use tokio::{fs::File, io::BufWriter, process::Command};
use tokio_util::io::StreamReader;
use tracing::instrument;

use crate::audio_storage::{stream_to_file, AudioStream, AUDIO_FILE_EXTENSION};

#[async_trait]
pub trait SpeechToText {
    async fn transcribe(&self, file: AudioStream, language: &str) -> anyhow::Result<String>;
}

#[derive(Debug, Clone)]
pub struct WhisperApi {
    client: Client,
    openai_api_key: String,
}

#[derive(Debug, Clone)]
pub struct PicovoiceLeopard<'a> {
    access_key: String,
    models_folder: &'a Path,
    library_path: PathBuf,
}

#[derive(Clone)]
pub struct SpeechToTextMock;

impl WhisperApi {
    pub fn new(openai_api_key: String) -> Self {
        let client = Client::new();
        Self {
            client,
            openai_api_key,
        }
    }
}

#[async_trait]
impl SpeechToText for WhisperApi {
    #[instrument]
    async fn transcribe(&self, stream: AudioStream, language: &str) -> anyhow::Result<String> {
        // TODO: use reqwest::Body::wrap_stream instead
        // The reason I am currently doing this is that Pageable<GetBlobResponse, azure_core::Error>
        // is not Sync, so I can't make AudioStream Sync, and that means I can't pass it to wrap_stream
        let bytes = stream.into_bytes().await?;
        let length = bytes.len().try_into()?;
        let body = reqwest::Body::from(bytes);
        let file_part = Part::stream_with_length(body, length)
            .file_name(format!("audio{}", AUDIO_FILE_EXTENSION));
        let form = Form::new()
            .part("file", file_part)
            .text("model", "whisper-1")
            .text("language", language.to_string());

        let res: WhisperApiResponse = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.openai_api_key)
            .multipart(form)
            .send()
            .await?
            .json()
            .await?;

        if let Some(text) = res.text {
            return Ok(text);
        }

        if let Some(error) = res.error {
            return Err(anyhow::anyhow!(
                "error returned from whisper api: {}",
                error
            ));
        }

        return Err(anyhow::anyhow!("whisper api did not return text nor error"));
    }
}

impl<'a> PicovoiceLeopard<'a> {
    #[instrument]
    pub async fn new_with_languages(
        languages: &'a [&'a str],
        access_key: String,
    ) -> anyhow::Result<PicovoiceLeopard<'a>> {
        let models_folder = Path::new("picovoice_leopard_models");
        if !models_folder.exists() {
            tokio::fs::create_dir(models_folder).await?;
        }

        for language in languages {
            if !models_folder.join(language).is_file() {
                PicovoiceLeopard::download_model(models_folder, language).await?;
            }
        }

        let current_dir = std::env::current_dir().context("failed to get current dir")?;
        let library_path = current_dir.join("picovoice_leopard_lib.so");
        if !library_path.exists() {
            PicovoiceLeopard::download_library(&library_path).await?;
        }

        Ok(PicovoiceLeopard {
            access_key,
            models_folder,
            library_path,
        })
    }

    #[instrument]
    async fn download_model(folder: &Path, language: &str) -> anyhow::Result<()> {
        let base_url = "https://github.com/Picovoice/leopard/raw/master/lib/common/leopard_params";
        let url = if language == "en" {
            format!("{base_url}.pv")
        } else {
            format!("{base_url}_{language}.pv")
        };

        tracing::info!("fetching picovoice leopard model for language: {language}");
        let stream = reqwest::get(url).await?.bytes_stream();
        let path = folder.join(language);
        stream_to_file(&path, stream).await?;

        Ok(())
    }

    #[instrument]
    async fn download_library(path: &Path) -> anyhow::Result<()> {
        let url =
            "https://github.com/Picovoice/leopard/raw/master/lib/linux/x86_64/libpv_leopard.so";
        tracing::info!("fetching picovoice library");
        let stream = reqwest::get(url).await?.bytes_stream();
        stream_to_file(path, stream).await?;
        Ok(())
    }

    #[instrument]
    async fn get_model_path(&self, language: &str) -> anyhow::Result<PathBuf> {
        let path = self.models_folder.join(language);
        if !path.exists() {
            PicovoiceLeopard::download_model(self.models_folder, language).await?;
        }
        Ok(path)
    }
}

#[async_trait]
impl<'a> SpeechToText for PicovoiceLeopard<'a> {
    #[instrument]
    async fn transcribe(&self, stream: AudioStream, language: &str) -> anyhow::Result<String> {
        let model_path = self.get_model_path(language).await?;

        let tmpdir = tokio::task::spawn_blocking(TempDir::new).await??;
        let path = tmpdir.path().join(format!("audio{}", AUDIO_FILE_EXTENSION));
        let mut file = File::create(&path)
            .await
            .context("failed to create file in tmpdir")?;
        let mut writer = BufWriter::new(&mut file);

        let stream = stream.map(|v| v.map_err(|err| io::Error::new(io::ErrorKind::Other, err)));
        let mut reader = StreamReader::new(stream);

        tokio::io::copy(&mut reader, &mut writer).await?;

        // The MediaRecorderAPI in the frontend produces raw headerless audio,
        // trying to transcribe those audios with this api produces an error.
        // To fix this, repackage the files with ffmpeg.
        // See https://stackoverflow.com/a/40117749
        let new_path = tmpdir
            .path()
            .join(format!("new_audio{}", AUDIO_FILE_EXTENSION));
        let exit_status = Command::new("ffmpeg")
            .arg("-i")
            .arg(&path)
            .arg("-acodec")
            .arg("copy")
            .arg(&new_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .context("failed executing ffmpeg")?;
        if !exit_status.success() {
            anyhow::bail!("ffmpeg exited with non-successful exit status: {exit_status}");
        }

        let access_key = self.access_key.clone();
        let library_path = self.library_path.to_owned();
        let transcript = tokio::task::spawn_blocking(move || {
            let leopard = LeopardBuilder::new()
                .access_key(&access_key)
                .model_path(model_path)
                .library_path(library_path)
                .init()
                .context("failed LeopardBuilder init")?;
            let transcript = leopard
                .process_file(&new_path)
                .context("failed to process file")?;

            Ok::<_, anyhow::Error>(transcript)
        })
        .await??;

        tokio::task::spawn_blocking(move || tmpdir.close())
            .await?
            .context("failed to delete tmpdir")?;

        Ok(transcript.transcript)
    }
}

#[derive(Deserialize)]
struct WhisperApiResponse {
    text: Option<String>,
    error: Option<serde_json::Value>,
}

#[async_trait]
impl SpeechToText for SpeechToTextMock {
    async fn transcribe(&self, _stream: AudioStream, language: &str) -> anyhow::Result<String> {
        tracing::info!("transcribe with language {}", language);
        Ok("hello".to_string())
    }
}
