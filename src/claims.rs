use anyhow::Context;
use axum::{
    async_trait,
    extract::{FromRequestParts, TypedHeader},
    headers::{authorization::Bearer, Authorization},
    http::request::Parts,
    Extension, RequestPartsExt,
};
use jsonwebtoken::{decode, Validation};
use serde::{Deserialize, Serialize};

use crate::{ApiError, AppState};

#[derive(Deserialize, Serialize)]
pub struct Claims {
    pub user_id: i32,
    pub email: String,
    pub exp: i64,
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| ApiError::Unauthorized)?;

        let Extension(state) = parts
            .extract::<Extension<AppState>>()
            .await
            .context("failed to get AppState in Claims FromRequestParts")?;

        // Decode the user data
        let token_data =
            decode::<Claims>(bearer.token(), &state.keys.decoding, &Validation::default())
                .map_err(|_| ApiError::Unauthorized)?;

        Ok(token_data.claims)
    }
}
