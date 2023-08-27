use axum::async_trait;
use tokio::fs::File;

#[async_trait]
pub trait Whisper {
    async fn transcribe(&self, file: File) -> anyhow::Result<String>;
}

#[derive(Clone)]
pub struct WhisperMock;

#[async_trait]
impl Whisper for WhisperMock {
    async fn transcribe(&self, file: File) -> anyhow::Result<String> {
        tracing::info!("transcribe: {:?}", file);
        Ok("hello".to_string())
    }
}
