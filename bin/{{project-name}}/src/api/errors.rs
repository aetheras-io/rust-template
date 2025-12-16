use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid input {0}")]
    Custom(String),

    #[error("Validation Error {0}")]
    Validations(#[from] validator::ValidationErrors),

    #[error("invalid input {0}")]
    InvalidInput(String),

    #[error("Database Error {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Unauthorized")]
    Unauthorized(String),

    #[error("Invalid IP")]
    InvalidIp,
}

impl Error {
    fn info(&self) -> (i16, StatusCode, Option<String>, Option<serde_json::Value>) {
        tracing::info!("api request error: {}", self);
        match self {
            Self::Custom(_) => (1000, StatusCode::INTERNAL_SERVER_ERROR, None, None),
            Self::Validations(e) => (1001, StatusCode::BAD_REQUEST, Some(e.to_string()), None),
            Self::InvalidInput(s) => (1009, StatusCode::BAD_REQUEST, Some(s.to_string()), None),
            Self::Sqlx(err) => match err {
                sqlx::Error::RowNotFound => (1002, StatusCode::NOT_FOUND, None, None),
                _ => (1002, StatusCode::INTERNAL_SERVER_ERROR, None, None),
            },
            Self::Unauthorized(s) => (1008, StatusCode::UNAUTHORIZED, Some(s.to_string()), None),
            Self::InvalidIp => (2007, StatusCode::FORBIDDEN, None, None),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let (code, status_code, context, data) = self.info();
        (status_code, Json(ErrorResponse::new(code, context, data))).into_response()
    }
}

// response structure
#[derive(Serialize)]
struct ErrorDetails {
    code: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    details: ErrorDetails,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(code: i16, context: Option<String>, data: Option<serde_json::Value>) -> Self {
        Self {
            details: ErrorDetails { code, context },
            data,
        }
    }
}
