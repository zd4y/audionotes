use axum::async_trait;

#[async_trait]
pub trait Whisper {
    async fn transcribe(&self, file_path: &str) -> anyhow::Result<String>;
}

#[derive(Clone)]
pub struct WhisperMock;

#[async_trait]
impl Whisper for WhisperMock {
    async fn transcribe(&self, file_path: &str) -> anyhow::Result<String> {
        tracing::info!("transcribe: {:?}", file_path);
        Ok("hello".to_string())
    }
}
