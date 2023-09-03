use std::collections::HashMap;

use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub struct DbTag {
    pub id: i32,
    pub user_id: i32,
    pub name: String,
    pub color: Option<String>,
}

pub async fn get_all_tags(pool: &PgPool, user_id: i32) -> sqlx::Result<Vec<DbTag>> {
    sqlx::query_as("select id, user_id, name, color from tags where user_id = $1 order by id")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get_audio_tags(pool: &PgPool, audio_id: i32) -> sqlx::Result<Vec<DbTag>> {
    sqlx::query_as(
        "select t.id, t.user_id, t.name, t.color
            from tags t
         join audio_tags a
            on t.id = a.tag_id
         where a.audio_id = $1
         order by t.id",
    )
    .bind(audio_id)
    .fetch_all(pool)
    .await
}

pub async fn get_audios_tags(
    pool: &PgPool,
    user_id: i32,
) -> sqlx::Result<HashMap<i32, Vec<DbTag>>> {
    let rows: Vec<(i32, i32, String, Option<String>, i32)> = sqlx::query_as(
        "select t.id, t.user_id, t.name, t.color, a.audio_id
            from tags t
         join audio_tags a
            on t.id = a.tag_id
         where t.user_id = $1
         order by a.audio_id, t.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut tags: HashMap<i32, Vec<DbTag>> = HashMap::new();

    for row in rows {
        let v = tags.entry(row.4).or_default();
        v.push(DbTag {
            id: row.0,
            user_id: row.1,
            name: row.2,
            color: row.3,
        })
    }

    Ok(tags)
}

pub async fn get_or_create_tag(
    pool: &PgPool,
    user_id: i32,
    tag_name: &str,
    tag_color: Option<String>,
) -> sqlx::Result<DbTag> {
    let color_is_some = tag_color.is_some();
    let query = format!(
        "insert into tags (user_id, name{})
         values ($1, $2{})
         on conflict (user_id, name) do update
            set name = EXCLUDED.name{}
         returning id, user_id, name, color",
        if color_is_some { ", color" } else { "" },
        if color_is_some { ", $3" } else { "" },
        if color_is_some {
            ", color = EXCLUDED.color"
        } else {
            ""
        }
    );
    let query = sqlx::query_as(&query).bind(user_id).bind(tag_name);

    if let Some(color) = tag_color {
        query.bind(color)
    } else {
        query
    }
    .fetch_one(pool)
    .await
}

pub async fn tag_audio(pool: &PgPool, tag_id: i32, audio_id: i32) -> sqlx::Result<()> {
    sqlx::query("insert into audio_tags (tag_id, audio_id) values ($1, $2) on conflict (tag_id, audio_id) do nothing")
        .bind(tag_id)
        .bind(audio_id)
        .execute(pool)
        .await?;
    Ok(())
}
