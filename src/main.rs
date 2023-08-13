mod api_error;
mod database;
mod models;
mod routes;

pub use api_error::Result;

use anyhow::Context;
use axum::{routing::get, Router};
use sqlx::PgPool;

#[shuttle_runtime::main]
async fn axum(#[shuttle_shared_db::Postgres] pool: PgPool) -> shuttle_axum::ShuttleAxum {
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let audio_routes = Router::new().route(
        "/:user_id",
        get(routes::audios::all_audios_by).post(routes::audios::new_audio),
    );

    let user_routes = Router::new().route("/:id", get(routes::users::get_user));

    let api_routes = Router::new()
        .nest("/audios", audio_routes)
        .nest("/users", user_routes)
        .with_state(pool);

    let app = Router::new().nest("/api", api_routes);

    Ok(app.into())
}
