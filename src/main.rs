mod api_error;
mod claims;
mod database;
mod models;
mod routes;
mod stt;

use std::{sync::Arc, net::SocketAddr};

pub use api_error::{ApiError, Result};
pub use claims::Claims;
pub use stt::SpeechToText;
use stt::WhisperApi;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

use anyhow::Context;
use axum::{
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    routing::{delete, get, post, put},
    Extension, Router,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use ring::rand::SystemRandom;
use sqlx::PgPool;

use routes::{audios::*, ping, users::*};

const UPLOADS_DIRECTORY: &str = "uploads";
const MAX_BYTES_TO_SAVE: usize = 25 * 1_000_000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::new()?;

    let pool = PgPool::connect(&config.database_url).await?;

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
    let secret = config.jwt_secret.as_bytes();
    let keys = Keys {
        encoding: EncodingKey::from_secret(secret),
        decoding: DecodingKey::from_secret(secret),
    };

    let openai_api_key = config.openai_api_key.clone().unwrap();
    let allowed_origin = config.allowed_origin.clone();

    let app_state = Arc::new(AppStateInner {
        pool: pool.clone(),
        config,
        rand_rng,
        keys,
        stt: Box::new(WhisperApi::new(openai_api_key)),
    }) as AppState;

    let audio_routes = Router::new()
        .route("/", get(all_audios).post(new_audio))
        .route("/:audio_id", get(get_audio))
        .route("/:audio_id/file", get(get_audio_file))
        .route("/:audio_id", delete(delete_audio))
        .route("/:audio_id/tags", put(tag_audio))
        .route("/tags", get(all_tags));

    let user_routes = Router::new()
        .route("/", get(get_user))
        .route("/authorize", post(authorize))
        .route("/reset-password", put(password_reset))
        .route("/request-reset-password", put(request_password_reset));

    let api_routes = Router::new()
        .route("/ping", get(ping))
        .nest("/user", user_routes)
        .nest("/audios", audio_routes)
        .layer(Extension(app_state))
        .layer(Extension(pool))
        .layer(RequestBodyLimitLayer::new(MAX_BYTES_TO_SAVE));

    let app = Router::new().nest("/api", api_routes).layer(
        CorsLayer::new()
            .allow_origin(allowed_origin.parse::<HeaderValue>().unwrap())
            .allow_headers([CONTENT_TYPE, AUTHORIZATION])
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE]),
    );

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    tracing::debug!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

pub type AppState = Arc<AppStateInner>;

pub struct AppStateInner {
    pool: PgPool,
    config: Config,
    rand_rng: SystemRandom,
    keys: Keys,
    stt: Box<dyn SpeechToText + Send + Sync>,
}

pub struct Config {
    database_url: String,
    jwt_secret: String,
    allowed_origin: String,
    openai_api_key: Option<String>,
    smtp_from: String,
    smtp_username: String,
    smtp_password: String,
    smtp_relay: String,
    password_reset_link: String,
}

impl Config {
    fn new() -> anyhow::Result<Config> {
        let database_url = std::env::var("DATABASE_URL")?;
        let jwt_secret = std::env::var("JWT_SECRET")?;
        let allowed_origin = std::env::var("ALLOWED_ORIGIN")?;
        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        let smtp_from = std::env::var("SMTP_FROM")?;
        let smtp_username = std::env::var("SMTP_USERNAME")?;
        let smtp_password = std::env::var("SMTP_PASSWORD")?;
        let smtp_relay = std::env::var("SMTP_RELAY")?;
        let password_reset_link = std::env::var("PASSWORD_RESET_LINK")?;
        Ok(Config {
            database_url,
            jwt_secret,
            allowed_origin,
            openai_api_key,
            smtp_from,
            smtp_username,
            smtp_password,
            smtp_relay,
            password_reset_link,
        })
    }
}

pub struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}
