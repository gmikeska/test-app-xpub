//! Federation detail pages.
//!
//! The page is split across two tabs by URL:
//!
//! - `GET /federations/:id`          — redirects to `…/receive` (default).
//! - `GET /federations/:id/receive`  — addresses table + cosigner roster.
//! - `GET /federations/:id/send`     — proposal form + proposals list.
//!
//! Both tabs share a common header card (cosigners + descriptor + tip),
//! authored once in `_federation_layout.html` and reused via Askama
//! template inheritance.
//!
//! Auth-required + membership-gated: only `federation_members` of `:id`
//! get a 200; anyone else gets a 403.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db;
use crate::error::AppError;
use crate::models::{ProposalRow, SignerRow};
use crate::wallet::{REVEAL_COUNT, RevealedAddress};

// ---------------------------------------------------------------------------
// Shared view-models (header + cosigners)
// ---------------------------------------------------------------------------

/// View-model for the page header card. Shared by both tab body templates.
#[derive(Debug, Clone, Serialize)]
pub struct FederationView {
    pub id: Uuid,
    pub label: String,
    pub threshold: i32,
    pub total_signers: i32,
    pub network: String,
    pub descriptor: String,
    pub created_at: String,
    pub tip_height: u32,
}

/// View-model for one cosigner row.
#[derive(Debug, Clone, Serialize)]
pub struct CosignerView {
    pub email: String,
    pub label: String,
    pub fingerprint: String,
    pub derivation_path: String,
    pub device_type: String,
    pub xpub_truncated: String,
    /// `true` when this cosigner is the logged-in viewer (UI highlights it).
    pub is_self: bool,
}

/// View-model for one row in the addresses (Receive tab) table.
#[derive(Debug, Clone, Serialize)]
pub struct AddressView {
    pub index: u32,
    pub address: String,
    pub received_btc: String,
    pub unspent_btc: String,
}

/// View-model for the federation-balance card. Mirrors the breakdown BDK
/// exposes on [`bdk_wallet::Balance`] (`confirmed` / `trusted_pending` /
/// `untrusted_pending` / `immature`) plus the conventional `total` and
/// `spendable` rollups.
#[derive(Debug, Clone, Serialize)]
pub struct BalanceView {
    pub confirmed_btc: String,
    pub trusted_pending_btc: String,
    pub untrusted_pending_btc: String,
    pub immature_btc: String,
    /// `confirmed + trusted_pending` — what's safely spendable right now.
    pub spendable_btc: String,
    /// `confirmed + trusted_pending + untrusted_pending + immature`.
    pub total_btc: String,
    /// `true` iff any of the pending/immature buckets are non-zero. Lets
    /// the template omit the breakdown when everything is fully confirmed.
    pub has_pending: bool,
}

impl From<bdk_wallet::Balance> for BalanceView {
    fn from(b: bdk_wallet::Balance) -> Self {
        let has_pending = b.trusted_pending > bitcoin::Amount::ZERO
            || b.untrusted_pending > bitcoin::Amount::ZERO
            || b.immature > bitcoin::Amount::ZERO;
        Self {
            confirmed_btc: format_btc(b.confirmed),
            trusted_pending_btc: format_btc(b.trusted_pending),
            untrusted_pending_btc: format_btc(b.untrusted_pending),
            immature_btc: format_btc(b.immature),
            spendable_btc: format_btc(b.trusted_spendable()),
            total_btc: format_btc(b.total()),
            has_pending,
        }
    }
}

/// View-model for one row in the proposals (Send tab) table.
#[derive(Debug, Clone, Serialize)]
pub struct ProposalView {
    pub id: Uuid,
    pub label: String,
    pub status: String,
    pub recipient: String,
    pub amount_btc: String,
    pub fee_btc: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// Receive tab — addresses table.
#[derive(Template, WebTemplate)]
#[template(path = "federation_receive.html")]
struct ReceiveTemplate {
    email: String,
    federation: FederationView,
    cosigners: Vec<CosignerView>,
    balance: BalanceView,
    addresses: Vec<AddressView>,
    active_tab: &'static str,
}

/// Send tab — proposal form + proposals table.
#[derive(Template, WebTemplate)]
#[template(path = "federation_send.html")]
struct SendTemplate {
    email: String,
    federation: FederationView,
    cosigners: Vec<CosignerView>,
    balance: BalanceView,
    proposals: Vec<ProposalView>,
    active_tab: &'static str,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

/// `GET /federations/:id` — 303 redirect to the default tab (Receive).
///
/// Keeps existing bookmarks working after the tab split.
pub async fn redirect_to_default(Path(id): Path<Uuid>) -> Redirect {
    Redirect::to(&format!("/federations/{id}/receive"))
}

/// `GET /federations/:id/receive`
pub async fn receive(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (federation, cosigners) = load_header(&state, federation_id, user.id).await?;

    let fw = state.wallets.load_or_init(federation_id).await?;
    let sync = fw.sync().await?;
    tracing::debug!(
        federation_id = %federation_id,
        tip = sync.tip_height,
        new_blocks = sync.new_blocks,
        new_mempool_txs = sync.new_mempool_txs,
        "synced federation wallet",
    );

    let addresses_raw = fw.reveal_addresses(REVEAL_COUNT).await?;
    let addresses = addresses_raw.into_iter().map(AddressView::from).collect();
    let balance: BalanceView = fw.balance().await.into();

    let federation = FederationView {
        tip_height: sync.tip_height,
        ..federation
    };

    Ok(ReceiveTemplate {
        email: user.email,
        federation,
        cosigners,
        balance,
        addresses,
        active_tab: "receive",
    }
    .into_response())
}

/// `GET /federations/:id/send`
pub async fn send(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (federation, cosigners) = load_header(&state, federation_id, user.id).await?;

    // We still drive a sync on the Send tab so balances reflected in BDK's
    // coin-selection are fresh.
    let fw = state.wallets.load_or_init(federation_id).await?;
    let sync = fw.sync().await?;
    let balance: BalanceView = fw.balance().await.into();
    let federation = FederationView {
        tip_height: sync.tip_height,
        ..federation
    };

    let proposal_rows = db::list_proposals_for_federation(&state.db, federation_id).await?;
    let proposals: Vec<ProposalView> = proposal_rows.into_iter().map(ProposalView::from).collect();

    Ok(SendTemplate {
        email: user.email,
        federation,
        cosigners,
        balance,
        proposals,
        active_tab: "send",
    }
    .into_response())
}

// ---------------------------------------------------------------------------
// Header helpers
// ---------------------------------------------------------------------------

/// Load the federation row, enforce membership, and build the shared
/// header view-models (federation card + cosigner roster).
///
/// Returns the page header data without a fresh `tip_height`: callers fill
/// that in from a subsequent `sync()` call (so the Send tab doesn't pay for
/// the extra address-reveal work the Receive tab does).
pub async fn load_header(
    state: &Arc<AppState>,
    federation_id: Uuid,
    user_id: Uuid,
) -> Result<(FederationView, Vec<CosignerView>), AppError> {
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;

    if !db::user_is_federation_member(&state.db, federation_id, user_id).await? {
        return Err(AppError::Forbidden);
    }

    let members = db::list_federation_members_with_signers(&state.db, federation_id).await?;
    let cosigners = members
        .into_iter()
        .map(|(u, s)| build_cosigner_view(&u.email, s, user_id == u.id))
        .collect::<Vec<_>>();

    let federation = FederationView {
        id: row.id,
        label: row.label,
        threshold: row.threshold,
        total_signers: row.total_signers,
        network: row.network,
        descriptor: row.descriptor,
        created_at: row.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        // Caller overrides with the post-sync tip.
        tip_height: row
            .chain_tip_height
            .and_then(|h| u32::try_from(h).ok())
            .unwrap_or(0),
    };

    Ok((federation, cosigners))
}

fn build_cosigner_view(email: &str, signer: Option<SignerRow>, is_self: bool) -> CosignerView {
    match signer {
        Some(s) => CosignerView {
            email: email.to_string(),
            label: s.label.unwrap_or_else(|| "Trezor".to_string()),
            fingerprint: s.fingerprint,
            derivation_path: s.derivation_path,
            device_type: s.device_type,
            xpub_truncated: truncate_middle(&s.xpub, 14, 12),
            is_self,
        },
        None => CosignerView {
            email: email.to_string(),
            label: "—".into(),
            fingerprint: "—".into(),
            derivation_path: "—".into(),
            device_type: "—".into(),
            xpub_truncated: "—".into(),
            is_self,
        },
    }
}

impl From<RevealedAddress> for AddressView {
    fn from(r: RevealedAddress) -> Self {
        Self {
            index: r.index,
            address: r.address,
            received_btc: format_btc(r.received),
            unspent_btc: format_btc(r.unspent),
        }
    }
}

impl From<ProposalRow> for ProposalView {
    fn from(r: ProposalRow) -> Self {
        let recipient = r
            .proposal_json
            .get("recipient")
            .and_then(|v| v.as_str()).map_or_else(|| "—".to_string(), std::string::ToString::to_string);
        let amount_sats = r
            .proposal_json
            .get("recipient_amount_sat")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let fee_sats = r
            .proposal_json
            .get("fee_sat")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        Self {
            id: r.id,
            label: r.label.unwrap_or_else(|| "(no label)".to_string()),
            status: r.status,
            recipient,
            amount_btc: format_btc_sats(amount_sats),
            fee_btc: format_btc_sats(fee_sats),
            created_at: format_timestamp(r.created_at),
        }
    }
}

pub fn format_btc(amount: bitcoin::Amount) -> String {
    format!("{:.8}", amount.to_btc())
}

pub fn format_btc_sats(sats: u64) -> String {
    format_btc(bitcoin::Amount::from_sat(sats))
}

pub fn format_timestamp(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M UTC").to_string()
}

pub fn truncate_middle(s: &str, head: usize, tail: usize) -> String {
    if s.chars().count() <= head + tail + 3 {
        return s.to_string();
    }
    let head_part: String = s.chars().take(head).collect();
    let tail_part: String = s
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head_part}…{tail_part}")
}
