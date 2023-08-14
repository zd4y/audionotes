use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use data_encoding::BASE64URL;
use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use ring::rand::{self, Random, SecureRandom};
use serde::Deserialize;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;

use crate::{api_error::ApiError, database, models::User, AppState};

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
pub struct PasswordPayload {
    email: String,
    username: String,
}

pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(payload): Json<PasswordPayload>,
) -> crate::Result<(StatusCode, &'static str)> {
    let user = database::find_user_by_email(&state.pool, &payload.email).await?;
    let user = match user {
        Some(user) => user,
        None => return Err(ApiError::NotFound),
    };

    if user.username != payload.username {
        return Err(ApiError::NotFound);
    }

    let token = generate_token(&state.rand_rng).map_err(|_| ApiError::InternalServerError)?;

    // TODO: Save token to database

    tokio::spawn(async move {
        match send_email(&state.secret_store, &token, &user.email).await {
            Ok(()) => {}
            Err(err) => tracing::error!("error sending email: {}", err),
        };
    });

    Ok((StatusCode::ACCEPTED, "Email sent"))
}

fn generate_token(rng: &dyn SecureRandom) -> anyhow::Result<String> {
    let random: Random<[u8; 48]> = rand::generate(rng)?;

    Ok(BASE64URL.encode(&random.expose()))
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
