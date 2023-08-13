use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
}

#[derive(Serialize)]
pub struct Audio {
    pub id: i64,
    pub transcription: Option<String>,
    pub created_at: DateTime<Utc>,
}
