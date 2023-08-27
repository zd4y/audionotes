use std::collections::HashMap;

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

pub async fn get_audios_tags(
    pool: &PgPool,
    user_id: i32,
) -> sqlx::Result<HashMap<i32, Vec<DbTag>>> {
    let rows: Vec<(i32, String, String, i32)> = sqlx::query_as(
        "select t.user_id, t.name, t.color, a.audio_id
            from tags t
         join audio_tags a
            on t.id = a.tag_id
         where t.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut tags: HashMap<i32, Vec<DbTag>> = HashMap::new();

    for row in rows {
        let v = tags.entry(row.3).or_default();
        v.push(DbTag {
            user_id: row.0,
            name: row.1,
            color: row.2,
        })
    }

    Ok(tags)
}
