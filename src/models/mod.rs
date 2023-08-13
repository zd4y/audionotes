use serde::Serialize;

#[derive(Serialize)]
pub struct User {
    pub id: u32,
    pub username: String,
}

#[derive(Serialize)]
pub struct Audio {
    pub id: u32,
    pub transcription: Option<String>,
}
