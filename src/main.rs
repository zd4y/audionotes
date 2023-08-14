mod api_error;
mod database;
mod models;
mod routes;

pub use api_error::Result;

use anyhow::Context;
use axum::{routing::get, Router};
use sqlx::PgPool;

use routes::{audios::*, users::*};

const UPLOADS_DIRECTORY: &str = "uploads";

#[shuttle_runtime::main]
async fn axum(#[shuttle_shared_db::Postgres] pool: PgPool) -> shuttle_axum::ShuttleAxum {
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    if !std::path::Path::new(UPLOADS_DIRECTORY).exists() {
        tokio::fs::create_dir(UPLOADS_DIRECTORY)
            .await
            .context("failed to create the uploads directory")?;
    }

    let user_routes = Router::new()
        .route("/:user_id", get(get_user))
        .route("/:user_id/audios", get(all_audios_by).post(new_audio))
        .route("/:user_id/audios/:audio_id", get(get_audio_by))
        .route("/:user_id/audios/:audio_id/file", get(get_audio_file_by));

    let api_routes = Router::new().nest("/users", user_routes).with_state(pool);

    let app = Router::new().nest("/api", api_routes);

    Ok(app.into())
}
