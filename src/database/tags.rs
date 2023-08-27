use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbTag {
    pub user_id: i32,
    pub name: String,
    pub color: String,
}

pub async fn get_audio_tags(pool: &PgPool, audio_id: i32) -> sqlx::Result<Vec<DbTag>> {
    sqlx::query_as(
        "select t.user_id, t.name, t.color
            from tags t
         join audio_tags a
            on t.id = a.tag_id
         where a.audio_id = $1",
    )
    .bind(audio_id)
    .fetch_all(pool)
    .await
}
