mod api_error;
mod claims;
mod database;
mod models;
mod routes;
mod whisper;

use std::{ops::Deref, sync::Arc};

pub use api_error::{ApiError, Result};
pub use claims::Claims;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
pub use whisper::Whisper;
use whisper::WhisperMock;

use anyhow::Context;
use axum::{
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    routing::{get, post, put},
    Extension, Router,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use ring::rand::SystemRandom;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;

use routes::{audios::*, users::*};

const UPLOADS_DIRECTORY: &str = "uploads";
const MAX_BYTES_TO_SAVE: usize = 25 * 1_000_000;

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
    let secret = secret_store.get("jwt_secret").unwrap();
    let secret = secret.as_bytes();
    let keys = Keys {
        encoding: EncodingKey::from_secret(secret),
        decoding: DecodingKey::from_secret(secret),
    };

    let allowed_origin = secret_store.get("allowed_origin").unwrap();

    let app_state = AppStateW(Arc::new(AppStateInner {
        pool: pool.clone(),
        secret_store,
        rand_rng,
        keys,
        whisper: WhisperMock,
    }));

    let audio_routes = Router::new()
        .route("/", get(all_audios).post(new_audio))
        .route("/:audio_id", get(get_audio))
        .route("/:audio_id/file", get(get_audio_file));

    let user_routes = Router::new()
        .route("/", get(get_user))
        .route("/authorize", post(authorize))
        .route("/reset-password", put(password_reset))
        .route("/request-reset-password", put(request_password_reset));

    let api_routes = Router::new()
        .nest("/user", user_routes)
        .nest("/audios", audio_routes)
        .layer(Extension(app_state))
        .layer(Extension(pool))
        .layer(RequestBodyLimitLayer::new(MAX_BYTES_TO_SAVE));

    let app = Router::new().nest("/api", api_routes).layer(
        CorsLayer::new()
            .allow_origin(allowed_origin.parse::<HeaderValue>().unwrap())
            .allow_headers([CONTENT_TYPE, AUTHORIZATION])
            .allow_methods([Method::GET, Method::POST, Method::PUT]),
    );

    Ok(app.into())
}

pub type AppState = AppStateW<WhisperMock>;

#[derive(Clone)]
pub struct AppStateW<W: Whisper>(Arc<AppStateInner<W>>);

pub struct AppStateInner<W>
where
    W: Whisper,
{
    pool: PgPool,
    secret_store: SecretStore,
    rand_rng: SystemRandom,
    keys: Keys,
    whisper: W,
}

impl Deref for AppState {
    type Target = AppStateInner<WhisperMock>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}
