mod api_error;
mod audio_storage;
mod claims;
mod database;
mod models;
mod routes;
mod stt;

use std::{net::SocketAddr, sync::Arc, time::Duration};

pub use api_error::{ApiError, Result};
use audio_storage::AudioStorage;
use audio_storage::LocalAudioStorage;
pub use claims::Claims;
use stt::SpeechToText;
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

use crate::audio_storage::AzureAudioStorage;
use crate::stt::PicovoiceLeopard;

const MAX_BYTES_TO_SAVE: usize = 25 * 1_000_000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    if let Err(err) = dotenvy::dotenv() {
        tracing::warn!("failed to load .env: {err}")
    };

    tracing::info!("loading config");
    let config = Config::new().context("failed to load config")?;

    tracing::info!("connecting to database");
    let pool = PgPool::connect(&config.database_url)
        .await
        .context("failed to connect to database")?;

    tracing::info!("running migrations");
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let rand_rng = SystemRandom::new();
    let secret = config.jwt_secret.as_bytes();
    let keys = Keys {
        encoding: EncodingKey::from_secret(secret),
        decoding: DecodingKey::from_secret(secret),
    };

    let allowed_origin = config.allowed_origin.clone();

    tracing::info!("initializing storage");
    let storage: Box<dyn AudioStorage + Send + Sync> =
        if let Some(account) = &config.azure_storage_account {
            tracing::info!("using azure audio storage");
            let access_key = config.azure_storage_access_key.as_ref().unwrap();
            let container = config.azure_storage_container.as_ref().unwrap();
            Box::new(AzureAudioStorage::new(account, access_key, container))
        } else {
            tracing::info!("using local audio storage");
            Box::new(LocalAudioStorage::new().await?)
        };

    tracing::info!("initializing speech to text");
    let stt: Box<dyn SpeechToText + Send + Sync> =
        if let Some(ref openai_api_key) = config.openai_api_key {
            tracing::info!("using openai");
            Box::new(WhisperApi::new(openai_api_key.to_string()))
        } else {
            tracing::info!("using picovoice leopard");
            let access_key = config.picovoice_access_key.clone().unwrap();
            Box::new(
                PicovoiceLeopard::new_with_languages(&["es"], access_key)
                    .await
                    .context("failed to get PicovoiceLeopard")?,
            )
        };

    let app_state = Arc::new(AppStateInner {
        pool: pool.clone(),
        config,
        rand_rng,
        keys,
        stt,
        storage,
    }) as AppState;

    let app_state2 = Arc::clone(&app_state);

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

    tokio::spawn(async move {
        if let Err(err) = transcribe_old_failed(&app_state2).await {
            tracing::error!("failed transcribing old failed: {err}");
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    tracing::info!("listening on {addr}");
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
    storage: Box<dyn AudioStorage + Send + Sync>,
}

pub struct Config {
    database_url: String,
    jwt_secret: String,
    allowed_origin: String,
    smtp_from: String,
    smtp_username: String,
    smtp_password: String,
    smtp_relay: String,
    password_reset_link: String,
    azure_storage_account: Option<String>,
    azure_storage_access_key: Option<String>,
    azure_storage_container: Option<String>,
    openai_api_key: Option<String>,
    picovoice_access_key: Option<String>,
}

impl Config {
    fn new() -> anyhow::Result<Config> {
        let database_url = std::env::var("DATABASE_URL")?;
        let jwt_secret = std::env::var("JWT_SECRET")?;
        let allowed_origin = std::env::var("ALLOWED_ORIGIN")?;
        let smtp_from = std::env::var("SMTP_FROM")?;
        let smtp_username = std::env::var("SMTP_USERNAME")?;
        let smtp_password = std::env::var("SMTP_PASSWORD")?;
        let smtp_relay = std::env::var("SMTP_RELAY")?;
        let password_reset_link = std::env::var("PASSWORD_RESET_LINK")?;

        let azure_storage_account = std::env::var("AZURE_STORAGE_ACCOUNT").ok();
        let azure_storage_access_key = std::env::var("AZURE_STORAGE_ACCESS_KEY").ok();
        let azure_storage_container = std::env::var("AZURE_STORAGE_CONTAINER").ok();

        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        let picovoice_access_key = std::env::var("PICOVOICE_ACCESS_KEY").ok();

        Ok(Config {
            database_url,
            jwt_secret,
            allowed_origin,
            smtp_from,
            smtp_username,
            smtp_password,
            smtp_relay,
            password_reset_link,
            azure_storage_account,
            azure_storage_access_key,
            azure_storage_container,
            openai_api_key,
            picovoice_access_key,
        })
    }
}

pub struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

async fn transcribe_old_failed(state: &AppState) -> anyhow::Result<()> {
    let failed_transcriptions = database::get_failed_audio_transcriptions(&state.pool).await?;

    let ids = failed_transcriptions
        .iter()
        .map(|i| (i.id, i.audio_id))
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        tracing::info!(
            "retrying old failed transcriptions (id, audio_id): {:?}",
            ids
        );
    }

    for failed_transcription in failed_transcriptions {
        if let Err(err) = routes::audios::transcribe_and_update_retrying(
            state,
            failed_transcription.audio_id,
            &failed_transcription.language,
            Some(failed_transcription.id),
        )
        .await
        {
            tracing::error!("failed to transcribe and update retrying: {err}");
        };
        tokio::time::sleep(Duration::from_secs(60)).await;
    }

    Ok(())
}
