use crate::opts::Decoder;

use atb_types::prelude::{Claims as ClaimsInner, NoCustom, jwt::HEADER_RS256};
use axum::{
    Json, RequestPartsExt,
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use serde::{Serialize, de::DeserializeOwned};

pub struct Claims<T = NoCustom>(ClaimsInner<T>);

impl<T> Claims<T> {
    /// Convert to the internal `T`
    pub fn into_inner(self) -> ClaimsInner<T> {
        self.0
    }
}

impl<T> std::ops::Deref for Claims<T> {
    type Target = ClaimsInner<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, T> FromRequestParts<S> for Claims<T>
where
    S: Send + Sync,
    Decoder: FromRef<S>,
    T: Serialize + DeserializeOwned,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::InvalidToken)?;
        let decoder = Decoder::from_ref(state);
        let claims = ClaimsInner::<T>::decode_custom(bearer.token(), &HEADER_RS256, &decoder.0)
            .map_err(|_| AuthError::InvalidToken)?;
        if claims.issuer() != "tt" || !validate_expiry_custom(&claims) {
            return Err(AuthError::InvalidToken);
        }
        Ok(Claims(claims))
    }
}

//#TODO: This shouldn't be needed, but ClaimsInner doesn't seem to validate customs correctly
fn validate_expiry_custom<T>(claims: &ClaimsInner<T>) -> bool
where
    T: Serialize + DeserializeOwned,
{
    const LEEWAY: i64 = 0;
    let now = atb_types::Utc::now().timestamp();
    claims.expiry() > now - LEEWAY
}

#[derive(Debug)]
pub enum AuthError {
    // WrongCredentials,
    // MissingCredentials,
    // TokenCreation,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            // AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
            // AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
            // AuthError::TokenCreation => (StatusCode::INTERNAL_SERVER_ERROR, "Token creation error"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
        };
        let body = Json(serde_json::json!({
            "error": error_message,
        }));
        (status, body).into_response()
    }
}
