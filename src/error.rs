//! Application error type and Axum [`IntoResponse`] impl.
//!
//! Handlers return `Result<_, AppError>`; rendering / DB / session
//! failures bubble up here and become a `500` page (with a logged tracing
//! event). Handler-specific validation errors (e.g. bad password) are
//! handled inside the handler by re-rendering the form, not by raising
//! `AppError`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Top-level error type for the web app.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Database error.
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// Session-store error.
    #[error("session error: {0}")]
    Session(#[from] tower_sessions::session::Error),

    /// Template rendering error.
    #[error("template render error: {0}")]
    Render(#[from] askama::Error),

    /// Password hashing / verification error.
    #[error("password hashing error: {0}")]
    PasswordHash(String),
}

impl From<password_hash::Error> for AppError {
    fn from(e: password_hash::Error) -> Self {
        Self::PasswordHash(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self, "request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
        )
            .into_response()
    }
}
