use anyhow::Context;
use axum::async_trait;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use std::path::Path;

#[async_trait]
pub trait SpeechToText {
    async fn transcribe(&self, file_path: &Path, language: &str) -> anyhow::Result<String>;
}

#[derive(Clone)]
pub struct WhisperApi {
    client: Client,
    openai_api_key: String,
}

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
    async fn transcribe(&self, file_path: &Path, language: &str) -> anyhow::Result<String> {
        let file_name = file_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .context("failed to get audio filename")?;

        let file = tokio::fs::File::open(file_path).await?;
        let file_length = file.metadata().await?.len();

        let file_part =
            Part::stream_with_length(file, file_length).file_name(file_name.to_string());
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

#[derive(Deserialize)]
struct WhisperApiResponse {
    text: Option<String>,
    error: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct SpeechToTextMock;

#[async_trait]
impl SpeechToText for SpeechToTextMock {
    async fn transcribe(&self, file_path: &Path, language: &str) -> anyhow::Result<String> {
        tracing::info!("transcribe with language {}: {:?}", language, file_path);
        Ok("hello".to_string())
    }
}
