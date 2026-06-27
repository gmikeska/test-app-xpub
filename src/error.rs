//! Application error type and Axum [`IntoResponse`] impl.
//!
//! Handlers return `Result<_, AppError>`; rendering / DB / session
//! failures bubble up here and become a `500` page (with a logged tracing
//! event). Handler-specific validation errors (e.g. bad password) are
//! handled inside the handler by re-rendering the form, not by raising
//! `AppError`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use asterism::core::DescriptorError;

use crate::wallet::WalletError;

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

    /// BDK / RPC / wallet error.
    #[error("wallet error: {0}")]
    Wallet(#[from] WalletError),

    /// Resource not found (404).
    #[error("not found: {0}")]
    NotFound(String),

    /// User isn't allowed to view this resource (403).
    #[error("forbidden")]
    Forbidden,

    /// Bad input from the user (400). Carries the user-visible reason.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Federation-creation form failed input validation (label empty,
    /// threshold out of range, too few members, etc.). 400 with the
    /// embedded message echoed to the user.
    #[error("invalid federation: {0}")]
    BadFederationInput(String),

    /// One or more picked users do not have a P2WSH signer on file at the
    /// configured derivation path, so they cannot contribute a key.
    /// Carries the offending emails so the response can name them.
    #[error("federation members missing onboarded hardware wallets: {}", .emails.join(", "))]
    MissingMemberSigner {
        /// Email addresses of users who lack a P2WSH signer.
        emails: Vec<String>,
    },

    /// `asterism-core`'s [`DescriptorBuilder`](asterism::core::DescriptorBuilder)
    /// rejected the assembled inputs — duplicate keys, network mismatch, etc.
    #[error("descriptor builder rejected federation: {0}")]
    DescriptorBuilderRejected(#[from] DescriptorError),
}

impl From<password_hash::Error> for AppError {
    fn from(e: password_hash::Error) -> Self {
        Self::PasswordHash(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            Self::NotFound(what) => {
                tracing::debug!(target = %what, "404");
                (StatusCode::NOT_FOUND, format!("Not found: {what}")).into_response()
            }
            Self::Forbidden => {
                tracing::debug!("403");
                (StatusCode::FORBIDDEN, "Forbidden").into_response()
            }
            Self::BadRequest(msg) => {
                tracing::debug!(reason = %msg, "400");
                (StatusCode::BAD_REQUEST, msg.clone()).into_response()
            }
            Self::BadFederationInput(msg) => {
                tracing::debug!(reason = %msg, "400 bad federation input");
                (StatusCode::BAD_REQUEST, msg.clone()).into_response()
            }
            Self::MissingMemberSigner { emails } => {
                tracing::debug!(?emails, "400 missing member signer");
                let list = emails.join(", ");
                (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "User(s) {list} have no hardware wallet onboarded. \
                         Have them log in and onboard a Trezor first."
                    ),
                )
                    .into_response()
            }
            Self::DescriptorBuilderRejected(e) => {
                tracing::debug!(error = %e, "400 descriptor rejected");
                (
                    StatusCode::BAD_REQUEST,
                    format!("Cannot build federation descriptor: {e}"),
                )
                    .into_response()
            }
            Self::Wallet(WalletError::NotFound(_)) => {
                tracing::debug!(error = %self, "404");
                (StatusCode::NOT_FOUND, "Federation not found").into_response()
            }
            Self::Wallet(WalletError::BadAddress {
                addr,
                network,
                reason,
            }) => {
                tracing::debug!(%addr, %network, %reason, "400 bad address");
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid address for {network}: {reason}"),
                )
                    .into_response()
            }
            // User-supplied form values that didn't pass validation in the
            // wallet layer → 400.
            Self::Wallet(
                WalletError::BadFeeRate { .. }
                | WalletError::CreateTx(_)
                | WalletError::BadPsbt(_)
                | WalletError::MergePsbt(_)
                | WalletError::Finalize(_)
                | WalletError::NotEnoughSignatures
                | WalletError::ExtractTx(_)
                | WalletError::UnknownCosigner(_)
                | WalletError::BadTrezorSignature { .. },
            ) => {
                tracing::debug!(error = %self, "400 wallet validation");
                (StatusCode::BAD_REQUEST, format!("{self}")).into_response()
            }
            // bitcoind rejected the broadcast — surface as 502 Bad Gateway
            // so the proposer can distinguish "your input is bad" (400) from
            // "upstream rejected" (502).
            Self::Wallet(WalletError::BroadcastRejected(reason)) => {
                tracing::warn!(%reason, "502 broadcast rejected");
                (
                    StatusCode::BAD_GATEWAY,
                    format!("bitcoind rejected broadcast: {reason}"),
                )
                    .into_response()
            }
            _ => {
                tracing::error!(error = %self, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}
