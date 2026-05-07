//! Typed error → response mapping.
//!
//! Handlers return `Result<T, AppError>`; `IntoResponse` makes the status
//! code explicit at the call site instead of leaking `anyhow` into HTTP.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found")]
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".into()),
        };
        // Log at warn — these are client-side problems, not server faults.
        tracing::warn!(error = %self, "request failed");
        (status, msg).into_response()
    }
}
