//! Proposal lifecycle handlers.
//!
//! Routes:
//!
//! - `POST /federations/:id/proposals`                         — create.
//! - `GET  /federations/:id/proposals/:pid`                    — detail page.
//! - `GET  /federations/:id/proposals/:pid/sign-data`          — Trezor JSON.
//! - `POST /federations/:id/proposals/:pid/signatures`         — submit.
//! - `POST /federations/:id/proposals/:pid/rejections`         — advisory.
//! - `POST /federations/:id/proposals/:pid/cancel`             — proposer-only.
//! - `POST /federations/:id/proposals/:pid/broadcast`          — when finalized.
//!
//! Every route loads the proposal first and 404s on `federation_id`
//! mismatch. Membership-gated; only `federation_members` of the proposal's
//! federation may act on it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::AppError;
use crate::handlers::federations::{
    FederationView, format_btc_sats, format_timestamp, load_header, truncate_middle,
};
use crate::models::{ProposalRow, SignerRow};
use crate::wallet::TrezorSignRequest;

// ---------------------------------------------------------------------------
// POST /federations/:id/proposals
// ---------------------------------------------------------------------------

/// Form payload submitted by the Send tab.
#[derive(Debug, Deserialize)]
pub struct CreateProposalForm {
    /// Destination address (validated against the wallet's network).
    pub recipient_address: String,
    /// Amount as a decimal BTC value (e.g. `"0.0005"`).
    pub amount_btc: String,
    /// Fee rate in sat/vB (regtest default = 2).
    pub fee_rate_sat_vb: u64,
    /// Optional human-readable label.
    #[serde(default)]
    pub label: Option<String>,
}

/// `POST /federations/:id/proposals`
pub async fn create(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    axum::Form(form): axum::Form<CreateProposalForm>,
) -> Result<Response, AppError> {
    let _row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }

    let fw = state.wallets.load_or_init(federation_id).await?;
    // Always sync before building a proposal so coin selection sees the
    // freshest UTXO set.
    fw.sync().await?;

    let address = fw.parse_address(form.recipient_address.trim())?;
    let amount = parse_btc_amount(form.amount_btc.trim())?;
    if form.fee_rate_sat_vb == 0 {
        return Err(AppError::BadRequest(
            "fee_rate_sat_vb must be at least 1".to_string(),
        ));
    }

    let built = fw
        .build_proposal(&address, amount, form.fee_rate_sat_vb)
        .await?;

    let label_ref = form.label.as_deref().filter(|s| !s.trim().is_empty());
    let proposal = db::insert_proposal(
        &state.db,
        federation_id,
        user.id,
        label_ref,
        &built.psbt_b64,
        &built.proposal_json,
        &built.coin_selection_json,
    )
    .await?;

    tracing::info!(
        federation_id = %federation_id,
        proposal_id = %proposal.id,
        proposer = %user.email,
        "created proposal"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{}",
        proposal.id
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// GET /federations/:id/proposals/:pid
// ---------------------------------------------------------------------------

// Four bool flags drive Sign / Reject / Cancel / Broadcast button visibility
// on the proposal page. Bundling them in a sub-struct would obscure intent.
#[allow(clippy::struct_excessive_bools)]
#[derive(Template, WebTemplate)]
#[template(path = "proposal.html")]
struct ProposalTemplate {
    email: String,
    federation: FederationView,
    proposal: ProposalDetailView,
    cosigner_statuses: Vec<CosignerStatusView>,
    coin_selection: CoinSelectionView,
    is_proposer: bool,
    viewer_already_signed: bool,
    viewer_already_rejected: bool,
    /// `true` if the viewer is a federation member with a Trezor row of
    /// their own (i.e. they can sign).
    viewer_has_signer: bool,
}

#[derive(Debug, Serialize)]
struct ProposalDetailView {
    id: Uuid,
    label: String,
    status: String,
    /// Bootstrap-style colour class for the status badge.
    status_class: String,
    proposer_email: String,
    created_at: String,
    updated_at: String,
    recipient: String,
    recipient_amount_btc: String,
    fee_btc: String,
    total_output_btc: String,
    change_btc: String,
    input_count: u64,
    psbt_b64: String,
    finalized_tx_hex: Option<String>,
    txid: Option<String>,
    broadcast_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct CosignerStatusView {
    email: String,
    label: String,
    fingerprint: String,
    /// "signed" | "rejected" | "pending"
    state: String,
    when: Option<String>,
    reason: Option<String>,
    is_self: bool,
}

#[derive(Debug, Serialize, Default)]
struct CoinSelectionView {
    selected: Vec<CoinSelectionInput>,
    total_input_btc: String,
    outputs: Vec<CoinSelectionOutput>,
    fee_btc: String,
}

#[derive(Debug, Serialize)]
struct CoinSelectionInput {
    outpoint: String,
    outpoint_short: String,
    address: String,
    amount_btc: String,
    keychain: String,
    derivation_index: String,
}

#[derive(Debug, Serialize)]
struct CoinSelectionOutput {
    address: String,
    amount_btc: String,
    /// "recipient" | "change" | "external"
    kind: String,
}

/// `GET /federations/:id/proposals/:pid`
pub async fn detail(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let (federation, _cosigners) = load_header(&state, federation_id, user.id).await?;
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    let proposer = db::find_user_by_id(&state.db, proposal.proposed_by)
        .await?
        .map_or_else(|| "—".to_string(), |u| u.email);

    let signatures = db::list_signatures_for_proposal(&state.db, proposal_id).await?;
    let rejections = db::list_rejections_for_proposal(&state.db, proposal_id).await?;

    let signed_by_user: HashMap<Uuid, chrono::DateTime<chrono::Utc>> = signatures
        .iter()
        .map(|s| (s.user_id, s.signed_at))
        .collect();
    let rejected_by_user: HashMap<Uuid, (Option<String>, chrono::DateTime<chrono::Utc>)> =
        rejections
            .iter()
            .map(|r| (r.user_id, (r.reason.clone(), r.rejected_at)))
            .collect();

    let members = db::list_federation_members_with_signers(&state.db, federation_id).await?;
    let cosigner_statuses: Vec<CosignerStatusView> = members
        .iter()
        .map(|(u, s)| {
            let (state_label, when) = signed_by_user.get(&u.id).map_or_else(
                || {
                    rejected_by_user.get(&u.id).map_or_else(
                        || ("pending".to_string(), None),
                        |(_, ts)| ("rejected".to_string(), Some(format_timestamp(*ts))),
                    )
                },
                |ts| ("signed".to_string(), Some(format_timestamp(*ts))),
            );
            let reason = rejected_by_user.get(&u.id).and_then(|(r, _)| r.clone());
            CosignerStatusView {
                email: u.email.clone(),
                label: s
                    .as_ref()
                    .and_then(|sr| sr.label.clone())
                    .unwrap_or_else(|| "Trezor".to_string()),
                fingerprint: s
                    .as_ref()
                    .map_or_else(|| "—".to_string(), |sr| sr.fingerprint.clone()),
                state: state_label,
                when,
                reason,
                is_self: u.id == user.id,
            }
        })
        .collect();

    let viewer_already_signed = signed_by_user.contains_key(&user.id);
    let viewer_already_rejected = rejected_by_user.contains_key(&user.id);

    let viewer_signer_row = db::find_signer_for_user(&state.db, user.id).await?;
    let viewer_has_signer = viewer_signer_row.is_some();

    let detail_view = ProposalDetailView {
        id: proposal.id,
        label: proposal
            .label
            .clone()
            .unwrap_or_else(|| "(no label)".to_string()),
        status: proposal.status.clone(),
        status_class: status_class(&proposal.status).to_string(),
        proposer_email: proposer,
        created_at: format_timestamp(proposal.created_at),
        updated_at: format_timestamp(proposal.updated_at),
        recipient: proposal_recipient(&proposal),
        recipient_amount_btc: format_btc_sats(proposal_field_u64(
            &proposal,
            "recipient_amount_sat",
        )),
        fee_btc: format_btc_sats(proposal_field_u64(&proposal, "fee_sat")),
        total_output_btc: format_btc_sats(proposal_field_u64(&proposal, "total_output_sat")),
        change_btc: format_btc_sats(proposal_field_u64(&proposal, "change_sat")),
        input_count: proposal_field_u64(&proposal, "input_count"),
        psbt_b64: proposal.psbt_b64.clone(),
        finalized_tx_hex: proposal.finalized_tx_hex.clone(),
        txid: proposal.txid.clone(),
        broadcast_at: proposal.broadcast_at.map(format_timestamp),
    };

    let coin_selection = build_coin_selection_view(&proposal);

    Ok(ProposalTemplate {
        email: user.email,
        federation,
        proposal: detail_view,
        cosigner_statuses,
        coin_selection,
        is_proposer: proposal.proposed_by == user.id,
        viewer_already_signed,
        viewer_already_rejected,
        viewer_has_signer,
    }
    .into_response())
}

// ---------------------------------------------------------------------------
// GET /federations/:id/proposals/:pid/sign-data
// ---------------------------------------------------------------------------

/// Response shape for the JSON sign-data endpoint. The browser hands
/// `trezor` straight to `TrezorConnect.signTransaction`, and uses
/// `signer_fingerprint` + `signer_slots` (echoed via the wrapped struct) to
/// extract the per-input signatures from the Trezor result before `POST`ing
/// them back to `/signatures`.
#[derive(Debug, Serialize)]
pub struct SignDataResponse {
    pub psbt_b64: String,
    pub trezor: TrezorSignRequest,
}

/// `GET /federations/:id/proposals/:pid/sign-data`
pub async fn sign_data(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    require_status_in(&proposal, &["proposed", "signing"])?;

    let signer = db::find_signer_for_user(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("you have no Trezor onboarded".to_string()))?;

    let members = db::list_federation_members_with_signers(&state.db, federation_id).await?;
    let cosigners: Vec<SignerRow> = members.into_iter().filter_map(|(_, s)| s).collect();
    let total_signers = usize::try_from(row.total_signers).unwrap_or(0);
    if cosigners.len() != total_signers {
        return Err(AppError::BadRequest(format!(
            "federation has {} members but only {} have signers onboarded",
            row.total_signers,
            cosigners.len(),
        )));
    }
    let threshold = usize::try_from(row.threshold).unwrap_or(0);

    let fw = state.wallets.load_or_init(federation_id).await?;
    let trezor = fw
        .trezor_sign_request(
            &proposal.psbt_b64,
            &signer.fingerprint,
            &cosigners,
            threshold,
        )
        .await?;

    Ok(Json(SignDataResponse {
        psbt_b64: proposal.psbt_b64.clone(),
        trezor,
    })
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/:id/proposals/:pid/signatures
// ---------------------------------------------------------------------------

/// Body posted by the browser after `TrezorConnect.signTransaction` resolves.
///
/// One element per PSBT input. Each element is the DER-encoded ECDSA
/// signature for the signing Trezor's pubkey on that input. Empty string
/// for inputs the signer didn't contribute to (shouldn't happen in
/// practice).
#[derive(Debug, Deserialize)]
pub struct SubmitSignatures {
    pub signatures_hex: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitSignatureResponse {
    pub status: String,
    pub fully_signed: bool,
}

/// `POST /federations/:id/proposals/:pid/signatures`
pub async fn submit_signature(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SubmitSignatures>,
) -> Result<Response, AppError> {
    let _row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    require_status_in(&proposal, &["proposed", "signing"])?;

    let signer = db::find_signer_for_user(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::BadRequest("you have no Trezor onboarded".to_string()))?;

    let fw = state.wallets.load_or_init(federation_id).await?;

    // Inject Trezor's per-input signatures into a fresh PSBT cloned from
    // the canonical base. That's the "partial PSBT" we both archive
    // (`transaction_signatures.partial_psbt_b64`) and merge into base.
    let partial_b64 = fw.inject_trezor_signatures(
        &proposal.psbt_b64,
        &signer.fingerprint,
        &body.signatures_hex,
    )?;

    let merged = fw
        .merge_partial_signature(&proposal.psbt_b64, &partial_b64)
        .await?;

    let new_status = if merged.fully_signed {
        "finalized"
    } else {
        "signing"
    };

    let inserted =
        db::insert_signature(&state.db, proposal_id, signer.id, user.id, &partial_b64).await?;
    if inserted.is_none() {
        // Idempotent re-sign: the cosigner had already submitted. Don't
        // mutate the canonical PSBT; we already accepted their work.
        tracing::info!(
            %proposal_id,
            cosigner = %user.email,
            "ignoring duplicate signature submission"
        );
        return Ok(Json(SubmitSignatureResponse {
            status: proposal.status.clone(),
            fully_signed: proposal.status == "finalized",
        })
        .into_response());
    }

    db::update_proposal_psbt(&state.db, proposal_id, &merged.merged_psbt_b64, new_status).await?;
    if merged.fully_signed {
        // Extract finalize tx now so the Broadcast button has the hex
        // ready to send.
        let finalized = fw.finalize_and_extract(&merged.merged_psbt_b64).await?;
        db::finalize_proposal(
            &state.db,
            proposal_id,
            &finalized.tx_hex,
            &finalized.txid.to_string(),
        )
        .await?;
    }

    tracing::info!(
        %proposal_id,
        cosigner = %user.email,
        status = %new_status,
        fully_signed = merged.fully_signed,
        "accepted signature"
    );

    Ok(Json(SubmitSignatureResponse {
        status: new_status.to_string(),
        fully_signed: merged.fully_signed,
    })
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/:id/proposals/:pid/rejections
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RejectionForm {
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /federations/:id/proposals/:pid/rejections`
///
/// Advisory. Does NOT change `transaction_proposals.status`.
pub async fn submit_rejection(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
    axum::Form(form): axum::Form<RejectionForm>,
) -> Result<Response, AppError> {
    let _row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let _ = load_proposal_for_federation(&state, federation_id, proposal_id).await?;

    let reason = form
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    db::insert_rejection(&state.db, proposal_id, user.id, reason).await?;

    tracing::info!(
        %proposal_id,
        user = %user.email,
        "advisory rejection recorded"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/:id/proposals/:pid/cancel
// ---------------------------------------------------------------------------

/// `POST /federations/:id/proposals/:pid/cancel` — proposer-only.
pub async fn cancel(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let _row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    if proposal.proposed_by != user.id {
        return Err(AppError::Forbidden);
    }
    require_status_in(&proposal, &["proposed", "signing", "finalized"])?;

    db::cancel_proposal(&state.db, proposal_id).await?;
    tracing::info!(%proposal_id, "cancelled by proposer");

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/:id/proposals/:pid/broadcast
// ---------------------------------------------------------------------------

/// `POST /federations/:id/proposals/:pid/broadcast` — only when finalized.
pub async fn broadcast(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    let _row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    require_status_in(&proposal, &["finalized"])?;

    let tx_hex = proposal
        .finalized_tx_hex
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("no finalized tx on this proposal".to_string()))?;

    let fw = state.wallets.load_or_init(federation_id).await?;
    let txid = fw.broadcast_raw(tx_hex).await?;

    db::mark_proposal_broadcast(&state.db, proposal_id, &txid.to_string()).await?;
    tracing::info!(%proposal_id, %txid, "broadcast finalized proposal");

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn load_proposal_for_federation(
    state: &Arc<AppState>,
    federation_id: Uuid,
    proposal_id: Uuid,
) -> Result<ProposalRow, AppError> {
    let p = db::find_proposal_by_id(&state.db, proposal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("proposal {proposal_id}")))?;
    if p.federation_id != federation_id {
        return Err(AppError::NotFound(format!(
            "proposal {proposal_id} (wrong federation)"
        )));
    }
    Ok(p)
}

fn require_status_in(p: &ProposalRow, allowed: &[&str]) -> Result<(), AppError> {
    let allowed_set: HashSet<&str> = allowed.iter().copied().collect();
    if allowed_set.contains(p.status.as_str()) {
        return Ok(());
    }
    Err(AppError::BadRequest(format!(
        "proposal status is `{}`, expected one of {:?}",
        p.status, allowed,
    )))
}

fn proposal_recipient(p: &ProposalRow) -> String {
    p.proposal_json
        .get("recipient")
        .and_then(|v| v.as_str())
        .map_or_else(|| "—".to_string(), std::string::ToString::to_string)
}

fn proposal_field_u64(p: &ProposalRow, field: &str) -> u64 {
    p.proposal_json
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn status_class(status: &str) -> &'static str {
    match status {
        "proposed" => "status-proposed",
        "signing" => "status-signing",
        "finalized" => "status-finalized",
        "broadcast" => "status-broadcast",
        "cancelled" => "status-cancelled",
        _ => "status-unknown",
    }
}

fn build_coin_selection_view(p: &ProposalRow) -> CoinSelectionView {
    let cs = &p.coin_selection_json;

    let total_input_sat = cs
        .get("total_input_sat")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let fee_sat = cs
        .get("fee_sat")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let selected: Vec<CoinSelectionInput> = cs
        .get("selected")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let outpoint = entry
                .get("outpoint")
                .and_then(|v| v.as_str())
                .unwrap_or("—")
                .to_string();
            let outpoint_short = truncate_middle(&outpoint, 10, 8);
            let address = entry
                .get("address")
                .and_then(|v| v.as_str())
                .unwrap_or("—")
                .to_string();
            let amount_sat = entry
                .get("amount_sat")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let keychain = entry
                .get("keychain")
                .and_then(|v| v.as_str())
                .unwrap_or("—")
                .to_string();
            let derivation_index = entry
                .get("derivation_index")
                .map_or_else(|| "—".to_string(), std::string::ToString::to_string);
            CoinSelectionInput {
                outpoint,
                outpoint_short,
                address,
                amount_btc: format_btc_sats(amount_sat),
                keychain,
                derivation_index,
            }
        })
        .collect();

    let outputs: Vec<CoinSelectionOutput> = cs
        .get("outputs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| CoinSelectionOutput {
            address: entry
                .get("address")
                .and_then(|v| v.as_str())
                .unwrap_or("—")
                .to_string(),
            amount_btc: format_btc_sats(
                entry
                    .get("amount_sat")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            ),
            kind: entry
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("external")
                .to_string(),
        })
        .collect();

    CoinSelectionView {
        selected,
        total_input_btc: format_btc_sats(total_input_sat),
        outputs,
        fee_btc: format_btc_sats(fee_sat),
    }
}

fn parse_btc_amount(input: &str) -> Result<bitcoin::Amount, AppError> {
    bitcoin::Amount::from_str_in(input, bitcoin::Denomination::Bitcoin)
        .map_err(|e| AppError::BadRequest(format!("invalid BTC amount `{input}`: {e}")))
}
