use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use data_encoding::BASE64URL;
use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use ring::rand::SecureRandom;
use serde::Deserialize;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;

use crate::{api_error::ApiError, database, models::User, AppState};

const TOKEN_BYTES: usize = 48;
const PASSWORD_RESET_LINK: &str = "http://127.0.0.1/";

pub async fn get_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<i32>,
) -> crate::Result<(StatusCode, Json<User>)> {
    let user = database::get_user(&pool, user_id).await?;

    match user {
        Some(user) => Ok((
            StatusCode::OK,
            Json(User {
                id: user.id,
                username: user.username,
            }),
        )),
        None => Err(ApiError::NotFound),
    }
}

#[derive(Deserialize)]
pub struct PasswordResetPayload {
    user_id: i32,
    token: String,
    new_password: String,
}

pub async fn password_reset(
    State(pool): State<PgPool>,
    Json(payload): Json<PasswordResetPayload>,
) -> crate::Result<StatusCode> {
    let db_tokens = database::get_user_tokens(&pool, payload.user_id).await?;

    let mut matched_token = None;

    for db_token in db_tokens {
        let parsed_hash =
            PasswordHash::new(&db_token.token).map_err(|_| ApiError::InternalServerError)?;
        let token_correct = Argon2::default()
            .verify_password(payload.token.as_bytes(), &parsed_hash)
            .is_ok();

        if token_correct {
            matched_token = Some(db_token);
        }
    }

    match matched_token {
        Some(token) => {
            if Utc::now() >= token.expires_at {
                database::delete_token(&pool, token.user_id, token.token).await?;
                return Err(ApiError::NotFound);
            }
            let new_password_hash =
                hash(&payload.new_password).map_err(|_| ApiError::InternalServerError)?;
            database::update_user_password(&pool, payload.user_id, new_password_hash).await?;
            database::delete_user_tokens(&pool, payload.user_id).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(ApiError::NotFound),
    }
}

#[derive(Deserialize)]
pub struct RequestPasswordResetPayload {
    email: String,
    username: String,
}

pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(payload): Json<RequestPasswordResetPayload>,
) -> crate::Result<(StatusCode, &'static str)> {
    // TODO: Consider returning a generic message when there is an error instead of using `?`

    let user = database::find_user_by_email(&state.pool, &payload.email).await?;
    let user = match user {
        Some(user) => user,
        None => return Err(ApiError::NotFound),
    };

    if user.username != payload.username {
        return Err(ApiError::NotFound);
    }

    let token = generate_token(&state.rand_rng).map_err(|_| ApiError::InternalServerError)?;
    let token_hash = hash(&token).map_err(|_| ApiError::InternalServerError)?;
    database::insert_token(&state.pool, user.id, token_hash).await?;

    tokio::spawn(async move {
        match send_email(&state.secret_store, &token, &user.email).await {
            Ok(()) => {}
            Err(err) => tracing::error!("error sending email: {}", err),
        };
    });

    Ok((StatusCode::ACCEPTED, "If that email address is in our database, we will send you an email to reset your password."))
}

fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| anyhow::anyhow!("failed hashing password"))?
        .to_string())
}

fn generate_token(rng: &dyn SecureRandom) -> anyhow::Result<String> {
    let mut random = [0u8; TOKEN_BYTES];
    rng.fill(&mut random)?;

    Ok(BASE64URL.encode(&random))
}

async fn send_email(
    secret_store: &SecretStore,
    token: &str,
    user_email: &str,
) -> anyhow::Result<()> {
    let to_mbox = match user_email.parse() {
        Ok(to) => to,
        Err(_) => {
            anyhow::bail!("failed parsing user email {}", user_email);
        }
    };

    let email = Message::builder()
        .from(secret_store.get("smtp_from").unwrap().parse().unwrap())
        .to(to_mbox)
        .subject("Password reset link")
        .header(ContentType::TEXT_PLAIN)
        .body(format!(
            "Follow this link for resetting your password: {}?token={}\n\nIf you didn't initialize any password reset, you can safely ignore this message.",
            PASSWORD_RESET_LINK, token
        ))
        .unwrap();

    let creds = Credentials::new(
        secret_store.get("smtp_username").unwrap(),
        secret_store.get("smtp_password").unwrap(),
    );

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::relay(&secret_store.get("smtp_relay").unwrap())
            .unwrap()
            .credentials(creds)
            .build();

    match mailer.send(email).await {
        Ok(_) => {}
        Err(err) => anyhow::bail!("mailer.send(email) error: {}", err),
    };

    Ok(())
}
