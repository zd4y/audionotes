mod api_error;
mod database;
mod models;
mod routes;

use std::{ops::Deref, sync::Arc};

pub use api_error::Result;

use anyhow::Context;
use axum::{
    extract::FromRef,
    routing::{get, put},
    Router,
};
use ring::rand::SystemRandom;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;

use routes::{audios::*, users::*};

const UPLOADS_DIRECTORY: &str = "uploads";

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_shared_db::Postgres] pool: PgPool,
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_axum::ShuttleAxum {
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    if !std::path::Path::new(UPLOADS_DIRECTORY).exists() {
        tokio::fs::create_dir(UPLOADS_DIRECTORY)
            .await
            .context("failed to create the uploads directory")?;
    }

    let rand_rng = SystemRandom::new();

    let app_state = AppState(Arc::new(AppStateInner {
        pool,
        secret_store,
        rand_rng,
    }));

    let user_routes = Router::new()
        .route(
            "/reset-password",
            put(password_reset).post(request_password_reset),
        )
        .route("/:user_id", get(get_user))
        .route("/:user_id/audios", get(all_audios_by).post(new_audio))
        .route("/:user_id/audios/:audio_id", get(get_audio_by))
        .route("/:user_id/audios/:audio_id/file", get(get_audio_file_by));

    let api_routes = Router::new()
        .nest("/users", user_routes)
        .with_state(app_state);

    let app = Router::new().nest("/api", api_routes);

    Ok(app.into())
}

#[derive(Clone)]
pub struct AppState(Arc<AppStateInner>);

pub struct AppStateInner {
    pool: PgPool,
    secret_store: SecretStore,
    rand_rng: SystemRandom,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.0.pool.clone()
    }
}

impl Deref for AppState {
    type Target = AppStateInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
