use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
pub struct User {
    pub email: String,
}

#[derive(Serialize)]
pub struct Audio {
    pub id: i32,
    pub transcription: Option<String>,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<Tag>,
}

#[derive(Serialize)]
pub struct Tag {
    pub name: String,
    pub color: Option<String>,
}

impl From<crate::database::DbTag> for Tag {
    fn from(db_tag: crate::database::DbTag) -> Self {
        Self {
            name: db_tag.name,
            color: db_tag.color,
        }
    }
}
