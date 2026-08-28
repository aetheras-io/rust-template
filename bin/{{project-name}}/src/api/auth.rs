use crate::api::{errors::Error, state::AppState};

use atb_types::{DateTime, Duration, Utc, Uuid};
use axum::{Json, Router, extract::State, routing::post};
use axum_client_ip::ClientIp;
use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use validator::Validate;

pub fn routes() -> Router<crate::api::state::AppState> {
    Router::new().route("/login", post(login_by_username))
}

#[derive(Serialize)]
pub struct LoginOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_expires_at: Option<DateTime>,
}

#[derive(Deserialize, Validate)]
pub struct LoginInput {
    #[validate(length(min = 1))]
    pub username: String,
    #[validate(length(min = 1))]
    pub password: String,
    #[serde(default)]
    pub device_id: Option<Uuid>,
}

#[instrument(
    skip(state, req),
    fields(username = %req.username)
)]
pub async fn login_by_username(
    State(state): State<AppState>,
    ClientIp(_ip): ClientIp,
    Json(req): Json<LoginInput>,
) -> Result<Json<LoginOutput>, Error> {
    req.validate()?;
    tracing::warn!(username = %req.username, "fake login");
    let id = &atb_fixtures_utils::ALICE;
    let output = issue_user_tokens(&state, id).await?;
    Ok(Json(output))
}

async fn issue_user_tokens(state: &AppState, user_id: &Uuid) -> Result<LoginOutput, Error> {
    let (token, _, _) = state
        .jwt_encoder
        .claims_encoded(user_id, vec![], Duration::days(30), None::<()>)
        .map_err(Error::TokenIssue)?;
    let refresh_token = generate_refresh_token();
    let refresh_expires_at = Utc::now() + Duration::days(30);
    Ok(LoginOutput {
        token: Some(token),
        refresh_token: Some(refresh_token),
        refresh_expires_at: Some(refresh_expires_at),
    })
}

fn generate_refresh_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let mut bytes = [0u8; 32];
    rng.fill_bytes(&mut bytes);
    general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
