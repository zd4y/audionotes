use axum::async_trait;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use tokio::fs::File;

#[async_trait]
pub trait Whisper {
    async fn transcribe(
        &self,
        file: File,
        file_length: u64,
        language: &str,
    ) -> anyhow::Result<String>;
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
impl Whisper for WhisperApi {
    async fn transcribe(
        &self,
        file: File,
        file_length: u64,
        language: &str,
    ) -> anyhow::Result<String> {
        let file_part = Part::stream_with_length(file, file_length);
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

        Ok(res.text)
    }
}

#[derive(Deserialize)]
struct WhisperApiResponse {
    text: String,
}

#[derive(Clone)]
pub struct WhisperMock;

#[async_trait]
impl Whisper for WhisperMock {
    async fn transcribe(
        &self,
        file: File,
        _file_length: u64,
        language: &str,
    ) -> anyhow::Result<String> {
        tracing::info!("transcribe with language {}: {:?}", language, file);
        Ok("hello".to_string())
    }
}
