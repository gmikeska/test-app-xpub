//! Proposal lifecycle handlers.
//!
//! Routes:
//!
//! - `POST /federations/:id/proposals`                         — create.
//! - `GET  /federations/:id/proposals/:pid`                    — detail page.
//! - `GET  /federations/:id/proposals/:pid/sign-data`          — Trezor /
//!                                                              Jade JSON.
//! - `POST /federations/:id/proposals/:pid/signatures`         — submit
//!                                                              per-input
//!                                                              Trezor sigs.
//! - `POST /federations/:id/proposals/:pid/partial-psbt`       — submit a
//!                                                              full
//!                                                              partial PSBT
//!                                                              (Jade output).
//! - `POST /federations/:id/proposals/:pid/rejections`         — advisory.
//! - `POST /federations/:id/proposals/:pid/cancel`             — proposer-only.
//! - `POST /federations/:id/proposals/:pid/broadcast`          — when finalized.
//!
//! Every route loads the proposal first and 404s on `federation_id`
//! mismatch. Membership-gated; only `federation_members` of the proposal's
//! federation may act on it.

use emvault::core::bitcoin;
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
use crate::db::{self, FederationKind};
use crate::error::AppError;
use crate::handlers::federations::{
    FederationView, format_btc_sats, format_timestamp, truncate_middle,
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
    /// Amount as a decimal BTC value (e.g. `"0.0005"`). Optional: ignored for
    /// old-signer sends, which sweep the entire balance.
    #[serde(default)]
    pub amount_btc: Option<String>,
    /// When set (the "Send Max" checkbox posts `"true"`), sweep the **entire**
    /// balance to the recipient — fee deducted from the amount. Honored only
    /// for current signers; ignored otherwise.
    #[serde(default)]
    pub send_max: Option<String>,
    /// Fee rate in sat/vB (regtest default = 2).
    pub fee_rate_sat_vb: u64,
    /// Optional human-readable label.
    #[serde(default)]
    pub label: Option<String>,
}

/// `POST /federations/:id/proposals`
// Two full per-chain proposal-build branches (Bitcoin custody flow + Liquid
// PSET flow) live in one dispatch; splitting them would only scatter the
// closely-related validation.
#[allow(clippy::too_many_lines)]
pub async fn create(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    axum::extract::Query(cq): axum::extract::Query<crate::handlers::federations::ChainQuery>,
    axum::Form(form): axum::Form<CreateProposalForm>,
) -> Result<Response, AppError> {
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    if form.fee_rate_sat_vb == 0 {
        return Err(AppError::BadRequest(
            "fee_rate_sat_vb must be at least 1".to_string(),
        ));
    }
    // Dual-chain vaults are Bitcoin-networked; the active side is carried by the
    // `?chain=elements` toggle, not the row's network. Resolve from both so an
    // Elements-view send builds a PSET, not a PSBT.
    let kind = crate::handlers::federations::resolve_active_chain(&row, cq.chain.as_deref());

    let label_ref = form.label.as_deref().filter(|s| !s.trim().is_empty());
    let proposal = match kind {
        FederationKind::Bitcoin => {
            let fw = state.wallets.load_or_init(federation_id).await?;
            // Always sync before building a proposal so coin selection sees
            // the freshest UTXO set.
            fw.sync().await?;
            let address = fw.parse_address(form.recipient_address.trim())?;

            // Old signers (members of a previous version but not the current
            // one) may only move funds forward to the current federation, and
            // do so by sweeping the **entire** balance (the fee comes out of
            // the amount sent). Current signers send a chosen amount to any
            // address. Enforced server-side — the form is only UX.
            let status = crate::handlers::federations::current_signer_status(
                &state,
                row.lineage_id,
                user.id,
            )
            .await?;
            let send_max = matches!(form.send_max.as_deref(), Some("true"));
            let built = if status.is_current_signer {
                if send_max {
                    // "Send Max": sweep the entire balance to the chosen
                    // recipient, fee out of the swept amount. Reuses the proven
                    // drain builder.
                    fw.build_drain_tx(&address, form.fee_rate_sat_vb).await?
                } else {
                    let amount_str = form
                        .amount_btc
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| AppError::BadRequest("amount is required".to_string()))?;
                    let amount = parse_btc_amount(amount_str)?;
                    fw.build_proposal(&address, amount, form.fee_rate_sat_vb)
                        .await?
                }
            } else {
                if let Some(current_id) = status.current_version_id {
                    let current = state.wallets.load_or_init(current_id).await?;
                    if !current.is_address_mine(&address).await {
                        return Err(AppError::BadRequest(
                            "As a signer on a previous federation version, you can only send \
                             to the current federation."
                                .to_string(),
                        ));
                    }
                }
                // Sweep the whole balance forward; the fee comes out of the
                // swept amount.
                fw.build_migration_tx(&address, form.fee_rate_sat_vb)
                    .await?
            };
            db::insert_proposal(
                &state.db,
                federation_id,
                user.id,
                label_ref,
                &built.psbt_b64,
                &built.proposal_json,
                &built.coin_selection_json,
                "bitcoin",
            )
            .await?
        }
        FederationKind::Liquid => {
            let fw = state.elements_wallets.load_or_init(federation_id).await?;
            fw.sync().await?;
            let address = fw.parse_address(form.recipient_address.trim())?;
            // Liquid federations don't carry the versioned old-signer migration
            // flow, but do support Send-Max (drain) as well as an explicit amount.
            let send_max = matches!(form.send_max.as_deref(), Some("true"));
            let built = if send_max {
                fw.build_drain_proposal(&address, Some(form.fee_rate_sat_vb))
                    .await?
            } else {
                let amount_str = form
                    .amount_btc
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| AppError::BadRequest("amount is required".to_string()))?;
                let satoshi = parse_lbtc_amount_sat(amount_str)?;
                fw.build_proposal(&address, satoshi, Some(form.fee_rate_sat_vb))
                    .await?
            };
            db::insert_proposal(
                &state.db,
                federation_id,
                user.id,
                label_ref,
                &built.pset_b64,
                &built.proposal_json,
                &built.coin_selection_json,
                "elements",
            )
            .await?
        }
    };

    tracing::info!(
        federation_id = %federation_id,
        proposal_id = %proposal.id,
        proposer = %user.email,
        kind = ?kind,
        "created proposal"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{}",
        proposal.id
    ))
    .into_response())
}

fn parse_lbtc_amount_sat(input: &str) -> Result<u64, AppError> {
    // L-BTC amounts use the same 8-decimal formatting as BTC. We piggy-back
    // on `bitcoin::Amount` for parsing; the sat count is what LWK wants.
    let amount = bitcoin::Amount::from_str_in(input, bitcoin::Denomination::Bitcoin)
        .map_err(|e| AppError::BadRequest(format!("invalid L-BTC amount `{input}`: {e}")))?;
    Ok(amount.to_sat())
}

// ---------------------------------------------------------------------------
// GET /federations/:id/max-spend  (Send-Max preview)
// ---------------------------------------------------------------------------

/// Query for the Send-Max preview: the recipient (its script type affects the
/// fee) and the fee rate.
#[derive(Debug, Deserialize)]
pub struct MaxSpendQuery {
    pub recipient_address: String,
    pub fee_rate_sat_vb: u64,
    /// `"elements"` selects the Liquid side of a dual-chain vault (from the
    /// same toggle the Send form posts on). Absent/other = Bitcoin.
    #[serde(default)]
    pub chain: Option<String>,
}

/// The exact net amount a Send-Max drain would deliver to the recipient
/// (balance − network fee), as both sats and a BTC string for the form field.
#[derive(Debug, Serialize)]
pub struct MaxSpendResponse {
    pub max_sat: u64,
    pub max_btc: String,
}

/// `GET /federations/:id/max-spend` — compute (without creating a proposal or
/// persisting anything) the net amount a "Send Max" drain to `recipient_address`
/// would send at the given fee rate. Drives the Send tab's **Max** button.
pub async fn max_spend(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<MaxSpendQuery>,
) -> Result<Json<MaxSpendResponse>, AppError> {
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    if q.fee_rate_sat_vb == 0 {
        return Err(AppError::BadRequest(
            "fee_rate_sat_vb must be at least 1".to_string(),
        ));
    }
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    let max_sat = if crate::handlers::federations::resolve_active_chain(&row, q.chain.as_deref())
        .is_liquid()
    {
        let fw = state.elements_wallets.load_or_init(federation_id).await?;
        fw.sync().await?;
        let address = fw.parse_address(q.recipient_address.trim())?;
        fw.compute_drain_amount(&address, q.fee_rate_sat_vb).await?
    } else {
        let fw = state.wallets.load_or_init(federation_id).await?;
        fw.sync().await?;
        let address = fw.parse_address(q.recipient_address.trim())?;
        fw.compute_drain_amount(&address, q.fee_rate_sat_vb)
            .await?
            .to_sat()
    };
    Ok(Json(MaxSpendResponse {
        max_sat,
        max_btc: format!("{:.8}", bitcoin::Amount::from_sat(max_sat).to_btc()),
    }))
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
    /// `true` if the viewer is a federation member with a signer row of
    /// their own (i.e. they can sign).
    viewer_has_signer: bool,
    /// Lower-case device family of the viewer's signer (`"trezor"`,
    /// `"jade"`), or empty string if the viewer has no signer. Drives
    /// per-device script + button selection in `proposal.html`.
    viewer_device_type: String,
    /// `true` if the viewer is a member of this proposal's federation version.
    /// A current signer of the lineage who is *not* a member may view the
    /// proposal read-only, so member-only actions (reject, broadcast) are gated
    /// on this rather than on mere visibility.
    viewer_is_member: bool,
    /// Where the breadcrumb "back" link points. For a member it's this version's
    /// Send tab; for a view-only current signer it's the version they can act on
    /// (the current one), since the historic version's Send tab is member-gated.
    back_href: String,
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
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;

    // View access: a member of this version, OR a current signer of the lineage
    // (who may view a historic version's proposal read-only — mirrors the
    // version-visibility rule for addresses and the Send-tab proposals list).
    let viewer_is_member = db::user_is_federation_member(&state.db, federation_id, user.id).await?;
    let status =
        crate::handlers::federations::current_signer_status(&state, row.lineage_id, user.id)
            .await?;
    if !viewer_is_member && !status.is_current_signer {
        return Err(AppError::Forbidden);
    }

    let back_href = if viewer_is_member {
        format!("/federations/{federation_id}/send")
    } else if let Some(current_id) = status.current_version_id {
        format!("/federations/{current_id}/send")
    } else {
        "/home".to_string()
    };

    // The proposal's own chain drives the header (so a dual-chain vault's page
    // loads the right — Bitcoin vs Liquid — sign script and labels).
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    let proposal_chain = if proposal.is_liquid() {
        Some("elements")
    } else {
        None
    };
    let (federation, _cosigners) =
        crate::handlers::federations::build_header_views(&state, row, user.id, proposal_chain)
            .await?;
    let proposer = db::find_user_by_id(&state.db, proposal.proposed_by)
        .await?
        .map_or_else(|| "—".to_string(), |u| u.email);

    let (cosigner_statuses, viewer_already_signed, viewer_already_rejected) =
        load_cosigner_statuses(&state, federation_id, proposal_id, user.id).await?;

    let viewer_signer_row =
        db::find_signer_for_user_in_version(&state.db, user.id, federation_id).await?;
    let viewer_has_signer = viewer_signer_row.is_some();
    let viewer_device_type = viewer_signer_row
        .as_ref()
        .map(|s| s.device_type.to_lowercase())
        .unwrap_or_default();

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
        viewer_device_type,
        viewer_is_member,
        back_href,
    }
    .into_response())
}

// ---------------------------------------------------------------------------
// GET /federations/:id/proposals/:pid/sign-data
// ---------------------------------------------------------------------------

/// Response shape for the JSON sign-data endpoint.
///
/// Trezor consumers hand `trezor` straight to
/// `TrezorConnect.signTransaction` and use `signer_fingerprint` +
/// `signer_slots` (echoed via the wrapped struct) to extract the per-input
/// signatures from the Trezor result before `POST`ing them back to
/// `/signatures`.
///
/// Jade consumers ignore `trezor` entirely and instead use `psbt_b64`,
/// `descriptor`, and `network` to drive `jade.registerMultisig` +
/// `jade.signPsbt`, then POST the resulting partial PSBT to
/// `/partial-psbt`.
#[derive(Debug, Serialize)]
pub struct SignDataResponse {
    /// Canonical base PSBT or PSET (base64) that the device should sign.
    /// PSBT for Bitcoin federations, PSET for Liquid federations.
    pub psbt_b64: String,
    /// Multipath descriptor for this federation. `wsh(sortedmulti(...))`
    /// for Bitcoin, `ct(slip77(...), elwsh(sortedmulti(...)))` for Liquid.
    /// Jade uses this to (idempotently) `registerMultisig` /
    /// `registerLiquidMultisig` before signing.
    pub descriptor: String,
    /// Network label. For Bitcoin this matches `federations.network`
    /// (`"bitcoin"`/`"testnet"`/`"regtest"`); for Liquid it's
    /// `"liquid"` / `"liquidtestnet"` / `"elementsregtest"`.
    pub network: String,
    /// Federation kind tag the browser uses to pick between the
    /// PSBT and PSET signing flows.
    pub federation_kind: String,
    /// Trezor-shaped JSON. Populated only for Bitcoin federations; on
    /// Liquid the browser doesn't have a Trezor sign path to invoke.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trezor: Option<TrezorSignRequest>,
    /// Ledger BIP-388 wallet policy (`{name, descriptor_template, keys}`).
    /// Populated only when the viewer's signer is a Ledger on a Bitcoin
    /// federation; the browser builds a `WalletPolicy` from it, registers it
    /// on-device, then signs. `None` for other devices / Liquid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger: Option<crate::ledger::LedgerWalletPolicy>,
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

    let signer = db::find_signer_for_user_in_version(&state.db, user.id, federation_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("you have no signer onboarded".to_string()))?;

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
    // Chain is the proposal's own — a dual-chain federation carries both PSBT
    // and PSET proposals, and its `network` is always Bitcoin.
    let kind = proposal_kind(&proposal);

    // Only a Trezor needs the Trezor-format request, which fetches each input's
    // previous transaction via bitcoind RPC. A Jade signs the PSBT directly (its
    // script consumes only psbt/descriptor/network), so building it for a Jade is
    // both wasted work and an outright failure on electrum/esplora backends whose
    // RPC node doesn't hold the input txs (e.g. a testnet4-via-electrum vault with
    // a regtest RPC node → RPC error -5). PSET (Liquid) never uses it either.
    let trezor = match kind {
        FederationKind::Bitcoin if signer.device_type.eq_ignore_ascii_case("trezor") => {
            let fw = state.wallets.load_or_init(federation_id).await?;
            Some(
                fw.trezor_sign_request(
                    &proposal.psbt_b64,
                    &signer.fingerprint,
                    &cosigners,
                    threshold,
                )
                .await?,
            )
        }
        FederationKind::Bitcoin | FederationKind::Liquid => None,
    };

    // A Liquid proposal must hand the Jade-Liquid signer the *Elements* network
    // and the confidential (`ct(...)`) descriptor — not the federation's Bitcoin
    // network / `wsh(...)` descriptor. Dual-chain vaults are Bitcoin-networked
    // and keep the ct descriptor in `elements_descriptor`; legacy Liquid feds
    // keep it in `descriptor` with a Liquid `network`.
    let (network, descriptor) = match kind {
        FederationKind::Bitcoin => (row.network.clone(), row.descriptor.clone()),
        FederationKind::Liquid => {
            let net = state
                .config
                .elements_network
                .map_or_else(|| row.network.clone(), |n| n.to_string());
            let ct = row
                .elements_descriptor
                .clone()
                .unwrap_or_else(|| row.descriptor.clone());
            (net, ct)
        }
    };

    // Ledger needs the BIP-388 wallet policy to register + sign. SegWit
    // (Bitcoin) only for now; the Taproot/Liquid policies are later phases.
    let ledger = match kind {
        FederationKind::Bitcoin if signer.device_type.eq_ignore_ascii_case("ledger") => Some(
            crate::ledger::build_ledger_policy(
                &row.label,
                row.version_index,
                row.threshold,
                &cosigners,
            )
            .map_err(|e| AppError::BadRequest(e.to_string()))?,
        ),
        FederationKind::Bitcoin | FederationKind::Liquid => None,
    };

    Ok(Json(SignDataResponse {
        psbt_b64: proposal.psbt_b64.clone(),
        descriptor,
        network,
        federation_kind: match kind {
            FederationKind::Bitcoin => "bitcoin".to_string(),
            FederationKind::Liquid => "liquid".to_string(),
        },
        trezor,
        ledger,
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
    /// Trezor: one DER-encoded ECDSA signature per PSBT input.
    #[serde(default)]
    pub signatures_hex: Option<Vec<String>>,
    /// Jade: the full signed PSBT (base64) — Jade returns a complete partial
    /// PSBT rather than per-input signatures, so it merges directly.
    #[serde(default)]
    pub signed_psbt_b64: Option<String>,
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

    let signer = db::find_signer_for_user_in_version(&state.db, user.id, federation_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("you have no signer onboarded".to_string()))?;

    let fw = state.wallets.load_or_init(federation_id).await?;

    // Produce the cosigner's "partial PSBT" — the thing we both archive
    // (`transaction_signatures.partial_psbt_b64`) and merge into the base.
    //
    // - **Jade** returns a complete signed PSBT, which *is* the partial.
    // - **Trezor** returns per-input sigs we inject into a fresh clone of base.
    let partial_b64 = if signer.device_type == "Jade" {
        body.signed_psbt_b64.clone().ok_or_else(|| {
            AppError::BadRequest("Jade signature submission missing `signed_psbt_b64`".to_string())
        })?
    } else {
        let signatures_hex = body.signatures_hex.as_deref().ok_or_else(|| {
            AppError::BadRequest("Trezor signature submission missing `signatures_hex`".to_string())
        })?;
        fw.inject_trezor_signatures(&proposal.psbt_b64, &signer.fingerprint, signatures_hex)?
    };

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
// POST /federations/:id/proposals/:pid/partial-psbt
// ---------------------------------------------------------------------------

/// Body posted by the browser after a Jade-style `signPsbt` resolves.
///
/// Unlike the Trezor flow (which ships back per-input DER signatures that
/// the server slots into a PSBT shell), Jade returns a *complete* partial
/// PSBT with its `partial_sigs` already populated. The handler can merge
/// it directly via `Psbt::combine`.
#[derive(Debug, Deserialize)]
pub struct SubmitPartialPsbt {
    /// Base64-encoded partial PSBT containing only the cosigner's
    /// contributions on top of the canonical base.
    pub partial_psbt_b64: String,
}

/// `POST /federations/:id/proposals/:pid/partial-psbt`
///
/// Generic device-agnostic counterpart to [`submit_signature`]. Accepts a
/// full partial PSBT and merges it via [`crate::wallet::FederationWallet::merge_partial_signature`].
pub async fn submit_partial_psbt(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, proposal_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SubmitPartialPsbt>,
) -> Result<Response, AppError> {
    // Existence guard (the proposal's own `chain` drives PSBT-vs-PSET below).
    db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        return Err(AppError::Forbidden);
    }
    let proposal = load_proposal_for_federation(&state, federation_id, proposal_id).await?;
    require_status_in(&proposal, &["proposed", "signing"])?;

    let signer = db::find_signer_for_user_in_version(&state.db, user.id, federation_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("you have no signer onboarded".to_string()))?;

    let kind = proposal_kind(&proposal);
    let partial_b64 = body.partial_psbt_b64.trim().to_string();

    let (merged_b64, fully_signed) = match kind {
        FederationKind::Bitcoin => {
            let fw = state.wallets.load_or_init(federation_id).await?;
            let merged = fw
                .merge_partial_signature(&proposal.psbt_b64, &partial_b64)
                .await?;
            (merged.merged_psbt_b64, merged.fully_signed)
        }
        FederationKind::Liquid => {
            let fw = state.elements_wallets.load_or_init(federation_id).await?;
            let merged = fw
                .merge_partial_pset(&proposal.psbt_b64, &partial_b64)
                .await?;
            (merged.merged_pset_b64, merged.fully_signed)
        }
    };

    let new_status = if fully_signed { "finalized" } else { "signing" };

    let inserted =
        db::insert_signature(&state.db, proposal_id, signer.id, user.id, &partial_b64).await?;
    if inserted.is_none() {
        tracing::info!(
            %proposal_id,
            cosigner = %user.email,
            "ignoring duplicate partial-PSBT submission"
        );
        return Ok(Json(SubmitSignatureResponse {
            status: proposal.status.clone(),
            fully_signed: proposal.status == "finalized",
        })
        .into_response());
    }

    db::update_proposal_psbt(&state.db, proposal_id, &merged_b64, new_status).await?;
    if fully_signed {
        match kind {
            FederationKind::Bitcoin => {
                let fw = state.wallets.load_or_init(federation_id).await?;
                let finalized = fw.finalize_and_extract(&merged_b64).await?;
                db::finalize_proposal(
                    &state.db,
                    proposal_id,
                    &finalized.tx_hex,
                    &finalized.txid.to_string(),
                )
                .await?;
            }
            FederationKind::Liquid => {
                let fw = state.elements_wallets.load_or_init(federation_id).await?;
                let finalized = fw.finalize_and_extract(&merged_b64).await?;
                db::finalize_proposal(&state.db, proposal_id, &finalized.tx_hex, &finalized.txid)
                    .await?;
            }
        }
    }

    tracing::info!(
        %proposal_id,
        cosigner = %user.email,
        status = %new_status,
        fully_signed,
        kind = ?kind,
        "accepted partial-PSBT submission"
    );

    Ok(Json(SubmitSignatureResponse {
        status: new_status.to_string(),
        fully_signed,
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
    // Existence guard (the proposal's own `chain` drives the broadcast path).
    db::find_federation_by_id(&state.db, federation_id)
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

    let kind = proposal_kind(&proposal);
    let txid = match kind {
        FederationKind::Bitcoin => {
            let fw = state.wallets.load_or_init(federation_id).await?;
            fw.broadcast_raw(tx_hex).await?.to_string()
        }
        FederationKind::Liquid => {
            let fw = state.elements_wallets.load_or_init(federation_id).await?;
            fw.broadcast_raw(tx_hex).await?
        }
    };

    db::mark_proposal_broadcast(&state.db, proposal_id, &txid).await?;
    tracing::info!(%proposal_id, %txid, kind = ?kind, "broadcast finalized proposal");

    // Consent-by-signing: broadcasting a migration transaction enacts the
    // version flip (pending → active, predecessor → superseded). Idempotent —
    // the query returns `None` once the migration is `enacted`.
    if let Some(enact) = db::migration_enactment_for_proposal(&state.db, proposal_id).await? {
        db::enact_version_transition(
            &state.db,
            enact.target_version_id,
            enact.base_version_id,
            enact.migration_id,
        )
        .await?;
        // Bind the sweep txid to the predecessor (base) version and flip its
        // reorg-reconciliation status to `complete`. If a later reorg evicts
        // this sweep from the wallet graph, `WalletManager::sync_lineage`'s
        // reconcile reverts the version to `pending` and clears the txid,
        // re-opening it for a re-sweep. This is the app-side custody guard that
        // a broadcast-then-reorged migration is not silently treated as final.
        db::set_migration_complete(&state.db, enact.base_version_id, &txid).await?;
        tracing::info!(
            %proposal_id, migration = %enact.migration_id,
            new_version = %enact.target_version_id, "migration enacted: version flipped"
        );
    }

    // Funds-preserving re-completion: broadcasting a `kind = 'resweep'` (`S'`)
    // for a base version a censorship reorg reverted to `pending` re-marks that
    // version `complete`, recording `S'` as the sweep. No version flip — the
    // flip already happened at the original migration; this only re-closes the
    // reorg-reconciliation column so the badge flips `pending -> complete` again.
    // Idempotent: the query returns `None` once the base is no longer `pending`.
    if let Some(base_version_id) =
        db::resweep_recompletion_for_proposal(&state.db, proposal_id).await?
    {
        db::set_migration_complete(&state.db, base_version_id, &txid).await?;
        tracing::info!(
            %proposal_id, base_version = %base_version_id,
            "migration re-sweep broadcast: base version re-marked complete"
        );
    }

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the cosigner status rows for a proposal, plus the viewer's own
/// `(already_signed, already_rejected)` flags. Factored out of `detail` to keep
/// that handler within size limits.
async fn load_cosigner_statuses(
    state: &Arc<AppState>,
    federation_id: Uuid,
    proposal_id: Uuid,
    viewer_id: Uuid,
) -> Result<(Vec<CosignerStatusView>, bool, bool), AppError> {
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
                is_self: u.id == viewer_id,
            }
        })
        .collect();

    let viewer_already_signed = signed_by_user.contains_key(&viewer_id);
    let viewer_already_rejected = rejected_by_user.contains_key(&viewer_id);
    Ok((
        cosigner_statuses,
        viewer_already_signed,
        viewer_already_rejected,
    ))
}

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

/// The chain a proposal lives on, as a [`FederationKind`]. Dual-chain
/// federations carry both PSBT (Bitcoin) and PSET (Elements) proposals, so the
/// PSBT-vs-PSET path keys off the proposal's own `chain`, not the federation
/// network (which is always Bitcoin for a dual-chain vault).
fn proposal_kind(p: &ProposalRow) -> FederationKind {
    if p.is_liquid() {
        FederationKind::Liquid
    } else {
        FederationKind::Bitcoin
    }
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
