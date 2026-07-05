//! Federation-creation handlers.
//!
//! - `GET  /federations/new` — render a form letting any logged-in user
//!                              pick members (themselves + others), set a
//!                              threshold, and name the federation. Only
//!                              P2WSH (`wsh(sortedmulti)`) federations are
//!                              supported in this iteration.
//! - `POST /federations`     — validate, build the canonical multipath
//!                              descriptor via `emvault-core`, and persist
//!                              the federation + memberships atomically.
//!
//! The creator is always a member: the form's checkbox for the creator
//! is pre-checked and `disabled` client-side, and a hidden input keeps
//! their id in the submission. The POST handler re-enforces this
//! invariant server-side as defense-in-depth.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use emvault::core::NetworkType;
use emvault::xpub::{DeviceType, ExternalSigner};

use crate::AppState;
use crate::auth::AuthUser;
use crate::db::{self, NewFederation, UserPickerRow};
use crate::error::AppError;
use crate::models::SignerRow;

/// Hard cap on the federation label length. The label doubles as the
/// **FederatedWallet name**: each version registers on the Jade under
/// `{label}-v{version}`, and Jade caps a multisig registration name at 15 ASCII
/// chars. Reserving `-v` + up to 3 version digits leaves 10 for the label, so
/// versions v1..v999 always fit (see [`crate::jade::jade_reg_name`]).
const MAX_LABEL_LEN: usize = 10;

// ---------------------------------------------------------------------------
// GET /federations/new
// ---------------------------------------------------------------------------

/// Template rendered by [`new_federation_get`].
#[derive(Template, WebTemplate)]
#[template(path = "federation_new.html")]
struct NewFederationTemplate {
    /// Logged-in user's email (for the navbar).
    email: String,
    /// Logged-in user's id, surfaced into a hidden field so the creator
    /// is always submitted as a member even if the disabled checkbox is
    /// stripped client-side.
    creator_id: Uuid,
    /// BIP-48 derivation path the federation uses, displayed in the UI
    /// so the user can sanity-check what they're committing to.
    derivation_path: String,
    /// Bitcoin network label for the page header.
    network: String,
    /// One row per candidate user, oldest-by-email order.
    candidates: Vec<CandidateView>,
}

/// One row in the candidate-user picker.
#[derive(Debug, Clone, Serialize)]
struct CandidateView {
    /// User id (form value).
    id: Uuid,
    /// User email (display).
    email: String,
    /// `true` iff the user has an onboarded P2WSH signer at the
    /// configured derivation path. Drives a UI badge only — selection
    /// is gated server-side.
    has_p2wsh_signer: bool,
    /// `true` iff this row represents the logged-in user. Renders as
    /// pre-checked + disabled, with a hidden duplicate so the disabled
    /// checkbox still submits.
    is_creator: bool,
}

/// `GET /federations/new`
///
/// # Errors
/// Returns any underlying SQL error from looking up candidate users.
pub async fn new_federation_get(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Response, AppError> {
    let path = &state.config.federation_derivation_path;
    let candidates: Vec<CandidateView> = db::list_users_with_p2wsh_signer_status(&state.db, path)
        .await?
        .into_iter()
        .map(
            |UserPickerRow {
                 user: u,
                 has_p2wsh_signer,
             }| CandidateView {
                is_creator: u.id == user.id,
                id: u.id,
                email: u.email,
                has_p2wsh_signer,
            },
        )
        .collect();

    Ok(NewFederationTemplate {
        creator_id: user.id,
        email: user.email,
        derivation_path: path.clone(),
        network: state.config.network.to_string(),
        candidates,
    }
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations
// ---------------------------------------------------------------------------

/// Form body posted by `federation_new.html`. Uses
/// [`axum_extra::extract::Form`] (backed by `serde_html_form`) so the
/// repeated `member_ids` checkbox values deserialize into a `Vec`.
#[derive(Debug, Deserialize)]
pub struct NewFederationForm {
    /// User-supplied label for the federation.
    pub label: String,
    /// Threshold `m` (signatures required) of an m-of-n federation.
    pub threshold: i32,
    /// User ids of the chosen members. Empty when no checkbox is
    /// selected; `#[serde(default)]` is required because `serde_html_form`
    /// would otherwise fail to deserialize a missing field into a `Vec`.
    #[serde(default)]
    pub member_ids: Vec<Uuid>,
}

/// `POST /federations`
///
/// # Errors
/// - [`AppError::BadFederationInput`] on invalid label, threshold, or
///   member count.
/// - [`AppError::MissingMemberSigner`] if any picked user lacks a P2WSH
///   signer at the configured derivation path.
/// - [`AppError::DescriptorBuilderRejected`] if `emvault-core`'s
///   [`DescriptorBuilder`] rejects the inputs (duplicate xpub, network
///   mismatch, etc.).
/// - Any underlying SQL error.
#[allow(clippy::too_many_lines)]
pub async fn new_federation_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Form(body): Form<NewFederationForm>,
) -> Result<Response, AppError> {
    let label = sanitise_label(&body.label)?;

    let member_ids = dedupe_and_force_include_creator(body.member_ids, user.id);
    let n = i32::try_from(member_ids.len())
        .map_err(|_| AppError::BadFederationInput("Too many members selected.".into()))?;
    if n < 1 {
        return Err(AppError::BadFederationInput(
            "Pick at least one member.".into(),
        ));
    }
    if body.threshold < 1 || body.threshold > n {
        return Err(AppError::BadFederationInput(format!(
            "Threshold must be between 1 and {n} (got {}).",
            body.threshold,
        )));
    }

    let resolved = resolve_member_signers(
        &state.db,
        &member_ids,
        &state.config.federation_derivation_path,
    )
    .await?;

    let mut external_signers: Vec<ExternalSigner> = Vec::with_capacity(resolved.len());
    for (_, row) in &resolved {
        let s = ExternalSigner::from_descriptor_key(
            row.descriptor_key.trim(),
            state.config.network,
            parse_device_type(&row.device_type),
            row.label.clone(),
        )
        .map_err(|e| {
            AppError::BadFederationInput(format!(
                "Stored signer for user {uid} is no longer parseable: {e}",
                uid = row.user_id,
            ))
        })?;
        external_signers.push(s);
    }

    let network_type = NetworkType::Bitcoin(state.config.network);
    let threshold_u32 = u32::try_from(body.threshold).map_err(|_| {
        AppError::BadFederationInput(format!("Threshold {} out of range.", body.threshold))
    })?;

    let built = emvault::core::build_federation(external_signers, threshold_u32, network_type)
        .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
    let descriptor_string = built.descriptor_string;
    let snapshot_json = built.snapshot_json;

    let network_str = state.config.network.to_string();
    let spec = NewFederation {
        label: &label,
        threshold: body.threshold,
        total_signers: n,
        network: &network_str,
        descriptor: &descriptor_string,
        snapshot_json: &snapshot_json,
    };
    let members: Vec<(Uuid, Uuid)> = resolved.iter().map(|(uid, row)| (*uid, row.id)).collect();
    let federation_id = db::insert_federation_with_members(&state.db, &spec, &members).await?;

    tracing::info!(
        federation_id = %federation_id,
        creator = %user.email,
        label = %label,
        threshold = body.threshold,
        total_signers = n,
        "federation created"
    );

    Ok(Redirect::to(&format!("/federations/{federation_id}")).into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitise_label(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadFederationInput("Label is required.".into()));
    }
    if trimmed.chars().count() > MAX_LABEL_LEN {
        return Err(AppError::BadFederationInput(format!(
            "Label must be at most {MAX_LABEL_LEN} characters.",
        )));
    }
    // The label becomes the Jade registration name `{label}-v{version}`, which
    // must be device- and descriptor-filename-safe. Restrict to ASCII
    // alphanumerics so the on-device name is unambiguous and portable.
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::BadFederationInput(
            "Label must use only letters and digits (A–Z, a–z, 0–9).".into(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Insert the creator into `ids` if missing, then sort + dedupe so the
/// resulting `Vec` represents a canonical member set regardless of how
/// the browser submitted the form.
fn dedupe_and_force_include_creator(mut ids: Vec<Uuid>, creator: Uuid) -> Vec<Uuid> {
    if !ids.contains(&creator) {
        ids.push(creator);
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Look up every picked user's P2WSH signer at the configured derivation
/// path. Collects all missing emails into a single
/// [`AppError::MissingMemberSigner`] so the user sees the full list at
/// once instead of re-submitting once per missing member.
pub(crate) async fn resolve_member_signers(
    pool: &sqlx::PgPool,
    member_ids: &[Uuid],
    derivation_path: &str,
) -> Result<Vec<(Uuid, SignerRow)>, AppError> {
    let mut resolved: Vec<(Uuid, SignerRow)> = Vec::with_capacity(member_ids.len());
    let mut missing: Vec<String> = Vec::new();
    for uid in member_ids {
        if let Some(row) = db::find_signer_for_user_at_path(pool, *uid, derivation_path).await? {
            resolved.push((*uid, row));
        } else {
            let email = db::find_user_by_id(pool, *uid)
                .await?
                .map_or_else(|| uid.to_string(), |u| u.email);
            missing.push(email);
        }
    }
    if missing.is_empty() {
        Ok(resolved)
    } else {
        Err(AppError::MissingMemberSigner { emails: missing })
    }
}

/// Map the `signers.device_type` column back into the typed
/// [`DeviceType`] that [`ExternalSigner::from_descriptor_key`] expects.
///
/// The column is populated by [`crate::handlers::onboard`] via
/// `format!("{:?}", signer.device_type())`, so the round-trip is:
/// `DeviceType::Trezor` → `"Trezor"` → `DeviceType::Trezor`. Any value
/// we don't recognise falls through to [`DeviceType::Generic`] rather
/// than failing the federation build.
pub(crate) fn parse_device_type(s: &str) -> DeviceType {
    match s {
        "Trezor" => DeviceType::Trezor,
        "Jade" => DeviceType::Jade,
        "PassportPrime" => DeviceType::PassportPrime,
        "Ledger" => DeviceType::Ledger,
        "Coldcard" => DeviceType::Coldcard,
        _ => DeviceType::Generic,
    }
}
