use axum::{http::StatusCode, Json};

use crate::models::Audio;

pub async fn all_audios() -> (StatusCode, Json<Vec<Audio>>) {
    (
        StatusCode::OK,
        Json(vec![
            Audio {
                id: 1,
                transcription: Some("Hello".to_string()),
            },
            Audio {
                id: 2,
                transcription: Some("Bye".to_string()),
            },
        ]),
    )
}

pub async fn new_audio() -> (StatusCode, Json<Audio>) {
    (
        StatusCode::CREATED,
        Json(Audio {
            id: 1,
            transcription: None,
        }),
    )
}
