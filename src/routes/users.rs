use anyhow::Context;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{http::StatusCode, Extension, Json};
use chrono::{Duration, Utc};
use data_encoding::BASE64URL;
use jsonwebtoken::{encode, Header};
use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};

use crate::{database, models::User, ApiError, AppState, Claims, Config};

const TOKEN_BYTES: usize = 48;

#[derive(Deserialize)]
pub struct AuthPayload {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct AuthBody {
    access_token: String,
    token_type: String,
}

pub async fn authorize(
    Extension(state): Extension<AppState>,
    Json(payload): Json<AuthPayload>,
) -> crate::Result<Json<AuthBody>> {
    if payload.email.is_empty() || payload.password.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let user = match database::find_user_by_email(&state.pool, &payload.email).await? {
        Some(user) => user,
        None => return Err(ApiError::Unauthorized),
    };

    let password_hash = match user.password {
        Some(password) => password,
        None => return Err(ApiError::Unauthorized),
    };

    let parsed_hash =
        PasswordHash::new(&password_hash).map_err(|_| ApiError::InternalServerError)?;
    let password_correct = Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_ok();

    if !password_correct {
        return Err(ApiError::Unauthorized);
    };

    let expiration_date = Utc::now() + Duration::days(180);
    let claims = Claims {
        user_id: user.id,
        email: user.email,
        language: user.language,
        exp: expiration_date.timestamp(),
    };

    let token = encode(&Header::default(), &claims, &state.keys.encoding)
        .context("failed encoding jwt token")?;

    Ok(Json(AuthBody {
        access_token: token,
        token_type: String::from("Bearer"),
    }))
}

pub async fn get_user(claims: Claims) -> (StatusCode, Json<User>) {
    (
        StatusCode::OK,
        Json(User {
            email: claims.email,
            language: claims.language,
        }),
    )
}

#[derive(Deserialize)]
pub struct PasswordResetPayload {
    user_id: i32,
    token: String,
    new_password: String,
}

pub async fn password_reset(
    Extension(state): Extension<AppState>,
    Json(payload): Json<PasswordResetPayload>,
) -> crate::Result<StatusCode> {
    if payload.new_password.is_empty() {
        return Err(ApiError::BadRequest);
    }
    let entropy = zxcvbn::zxcvbn(&payload.new_password, &[])
        .context("failed to check password with zxcvbn")?;
    if entropy.score() <= 2 {
        let feedback = entropy.feedback().clone().unwrap();
        return Err(ApiError::WeakPassword(feedback));
    }
    let db_tokens = database::get_user_tokens(&state.pool, payload.user_id).await?;

    let mut matched_token = None;

    let payload_token = payload.token.as_bytes();
    let argon2 = Argon2::default();
    let now = Utc::now();

    for db_token in db_tokens {
        if now >= db_token.expires_at {
            database::delete_token(&state.pool, db_token.user_id, db_token.token).await?;
            continue;
        }

        let parsed_hash =
            PasswordHash::new(&db_token.token).map_err(|_| ApiError::InternalServerError)?;
        let token_correct = argon2.verify_password(payload_token, &parsed_hash).is_ok();

        if token_correct {
            matched_token = Some(db_token);
            break;
        }
    }

    let user = match database::get_user(&state.pool, payload.user_id).await? {
        Some(user) => user,
        None => return Err(ApiError::NotFound),
    };

    if matched_token.is_some() {
        let new_password_hash = hash(&payload.new_password)?;
        database::update_user_password(&state.pool, payload.user_id, new_password_hash).await?;
        database::delete_user_tokens(&state.pool, payload.user_id).await?;

        tokio::spawn(async move {
            let email_body = "Your password has been updated successfully.";
            let subject = "Password updated";
            match send_email(&state.config, subject, email_body.to_string(), &user.email).await {
                Ok(()) => {}
                Err(err) => tracing::error!(?err, "error sending email"),
            };
        });
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize)]
pub struct RequestPasswordResetPayload {
    email: String,
}

pub async fn request_password_reset(
    Extension(state): Extension<AppState>,
    Json(payload): Json<RequestPasswordResetPayload>,
) -> crate::Result<(StatusCode, &'static str)> {
    let response = Ok((
        StatusCode::ACCEPTED,
        "If that email address is in our database, we will send you an email to reset your password."
    ));

    let token = generate_token(&state.rand_rng)?;
    let token_hash = hash(&token)?;

    let link = &state.config.password_reset_link;

    let user = match database::find_user_by_email(&state.pool, &payload.email).await? {
        Some(user) => user,
        None => return response,
    };

    database::insert_token(&state.pool, user.id, token_hash).await?;

    let email_body = format!(
        r#"
Follow this link for resetting your password: {}?token={}&user_id={}

If you didn't initialize any password reset, you can safely ignore this message."#,
        link, token, user.id
    );

    tokio::spawn(async move {
        let subject = "Password reset link";
        match send_email(&state.config, subject, email_body, &user.email).await {
            Ok(()) => {}
            Err(err) => tracing::error!(?err, "error sending email"),
        };
    });

    response
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
    config: &Config,
    subject: &str,
    body: String,
    user_email: &str,
) -> anyhow::Result<()> {
    let to_mbox = match user_email.parse() {
        Ok(to) => to,
        Err(_) => {
            anyhow::bail!("failed parsing user email {}", user_email);
        }
    };

    let email = Message::builder()
        .from(config.smtp_from.parse().unwrap())
        .to(to_mbox)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .unwrap();

    let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_relay)
            .unwrap()
            .credentials(creds)
            .build();

    match mailer.send(email).await {
        Ok(_) => {}
        Err(err) => anyhow::bail!("mailer.send(email) error: {}", err),
    };

    Ok(())
}
