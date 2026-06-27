//! Trezor onboarding handlers.
//!
//! - `GET  /onboard`        — render the page that drives Trezor Connect.
//! - `POST /onboard/signer` — accept a descriptor-key string from the browser,
//!                            validate it via `ExternalSigner::from_descriptor_key`,
//!                            and persist a `signers` row.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};

use emvault::xpub::{DeviceType, ExternalSigner};

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::AppError;

/// Onboarding page template.
#[derive(Template, WebTemplate)]
#[template(path = "onboard.html")]
struct OnboardTemplate {
    /// Logged-in user's email (for the navbar).
    email: String,
    /// Trezor Connect `coin` token (e.g. `"test"`).
    trezor_coin: String,
    /// Trezor Connect manifest email.
    trezor_manifest_email: String,
    /// Trezor Connect manifest origin URL.
    trezor_manifest_app_url: String,
    /// BIP-48 derivation path to request from the device.
    derivation_path: String,
    /// Bitcoin network label rendered in the UI.
    network: String,
}

/// `GET /onboard`
///
/// Shown after login when the user has no signer on file. The page's JS
/// drives `TrezorConnect.getPublicKey` and POSTs the resulting descriptor
/// key to [`onboard_signer_post`].
pub async fn onboard_get(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Response, AppError> {
    if db::user_has_signer(&state.db, user.id).await? {
        return Ok(Redirect::to("/home").into_response());
    }
    Ok(OnboardTemplate {
        email: user.email,
        trezor_coin: state.config.trezor_coin.clone(),
        trezor_manifest_email: state.config.trezor_manifest_email.clone(),
        trezor_manifest_app_url: state.config.trezor_manifest_app_url.clone(),
        derivation_path: state.config.federation_derivation_path.clone(),
        network: state.config.network.to_string(),
    }
    .into_response())
}

/// JSON body posted by the browser after `TrezorConnect.getPublicKey`
/// resolves.
#[derive(Debug, Deserialize)]
pub struct OnboardSignerBody {
    /// BIP-380 descriptor key: `[<fingerprint>/<path>]<xpub>`.
    pub descriptor_key: String,
    /// Optional human-readable label.
    #[serde(default)]
    pub label: Option<String>,
}

/// Successful onboarding response.
#[derive(Debug, Serialize)]
pub struct OnboardSignerResponse {
    /// Always `"ok"`.
    pub status: &'static str,
    /// Where the browser should navigate next.
    pub redirect: &'static str,
    /// Signer fingerprint (echoed for confirmation).
    pub fingerprint: String,
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct OnboardSignerError {
    /// Always `"error"`.
    pub status: &'static str,
    /// Actionable message safe to render in the UI.
    pub message: String,
}

/// `POST /onboard/signer`
///
/// Validates `body.descriptor_key` by constructing an
/// [`ExternalSigner`] (which performs all the BIP-380 / BIP-32 checks),
/// then persists a `signers` row keyed to the logged-in user.
pub async fn onboard_signer_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(body): Json<OnboardSignerBody>,
) -> Result<Response, AppError> {
    let label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let signer = match ExternalSigner::from_descriptor_key(
        body.descriptor_key.trim(),
        state.config.network,
        DeviceType::Trezor,
        label.clone(),
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, user = %user.email, "rejected onboarding key");
            return Ok((
                StatusCode::BAD_REQUEST,
                Json(OnboardSignerError {
                    status: "error",
                    message: format!("Rejected descriptor key: {e}"),
                }),
            )
                .into_response());
        }
    };

    let fingerprint = signer.fingerprint_hex();
    let derivation_path = signer.derivation_path_with_master();
    let xpub = signer.xpub_string();
    let network = state.config.network.to_string();
    let device_type = format!("{:?}", signer.device_type());

    match db::insert_signer(
        &state.db,
        user.id,
        label.as_deref(),
        body.descriptor_key.trim(),
        &xpub,
        &fingerprint,
        &derivation_path,
        &device_type,
        &network,
    )
    .await
    {
        Ok(row) => {
            tracing::info!(
                user = %user.email,
                signer_id = %row.id,
                fingerprint = %fingerprint,
                "trezor onboarded"
            );
            Ok(Json(OnboardSignerResponse {
                status: "ok",
                redirect: "/home",
                fingerprint,
            })
            .into_response())
        }
        Err(sqlx::Error::Database(db_err)) if db_err.constraint().is_some() => {
            // Uniqueness violation: user already onboarded this fingerprint.
            tracing::info!(
                user = %user.email,
                fingerprint = %fingerprint,
                constraint = ?db_err.constraint(),
                "duplicate signer, ignoring"
            );
            Ok((
                StatusCode::CONFLICT,
                Json(OnboardSignerError {
                    status: "error",
                    message: "This Trezor is already onboarded for your account.".into(),
                }),
            )
                .into_response())
        }
        Err(e) => Err(AppError::Sqlx(e)),
    }
}

// ---------------------------------------------------------------------------
// Small extension shims — keep handler code expressive without polluting the
// public surface of `emvault-xpub` with stringly-typed accessors.
// ---------------------------------------------------------------------------

trait ExternalSignerStringExt {
    fn fingerprint_hex(&self) -> String;
    fn derivation_path_with_master(&self) -> String;
    fn xpub_string(&self) -> String;
}

impl ExternalSignerStringExt for ExternalSigner {
    fn fingerprint_hex(&self) -> String {
        use emvault::core::Signer as _;
        self.fingerprint().to_string()
    }

    fn derivation_path_with_master(&self) -> String {
        use emvault::core::Signer as _;
        // `DerivationPath::Display` emits child numbers separated by `/`
        // without the leading `m/`. We prefix it so the column matches the
        // canonical BIP-32 spelling.
        let body = self.derivation_path().to_string();
        if body.is_empty() {
            "m".to_string()
        } else {
            format!("m/{body}")
        }
    }

    fn xpub_string(&self) -> String {
        use emvault::core::Signer as _;
        self.xpub().to_string()
    }
}
