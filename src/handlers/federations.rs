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

use emvault::core::bdk_wallet;
use emvault::core::bitcoin;
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
use crate::models::{FederationRow, ProposalRow, SignerRow};
use crate::wallet::{REVEAL_COUNT, RevealedAddress};

// ---------------------------------------------------------------------------
// Shared view-models (header + cosigners)
// ---------------------------------------------------------------------------

/// View-model for the page header card. Shared by both tab body templates.
#[derive(Debug, Clone, Serialize)]
pub struct FederationView {
    pub id: Uuid,
    /// Lineage this version belongs to (for current-version lookups).
    pub lineage_id: Uuid,
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
/// `spendable` rollups, and a `reserved` row covering sats locked up in
/// in-flight proposals (status `proposed` / `signing` / `finalized`).
#[derive(Debug, Clone, Serialize)]
pub struct BalanceView {
    pub confirmed_btc: String,
    pub trusted_pending_btc: String,
    pub untrusted_pending_btc: String,
    pub immature_btc: String,
    /// `confirmed + trusted_pending - reserved` — money the user can
    /// commit to a *new* proposal right now without double-spending an
    /// in-flight one.
    pub spendable_btc: String,
    /// `confirmed + trusted_pending + untrusted_pending + immature`. The
    /// on-chain reality, ignoring the proposal queue.
    pub total_btc: String,
    /// Sum of selected-UTXO amounts across in-flight proposals.
    pub reserved_btc: String,
    /// `true` iff any of the pending/immature buckets are non-zero. Lets
    /// the template omit the breakdown when everything is fully confirmed.
    pub has_pending: bool,
    /// `true` iff `reserved_sat > 0`. Lets the template hide the row
    /// when no proposals are in flight.
    pub has_reserved: bool,
}

impl BalanceView {
    /// Build a [`BalanceView`] from BDK's on-chain balance and the
    /// in-flight reservation total (sats) returned by
    /// [`crate::db::sum_inflight_inputs_for_federation`].
    pub fn from_balance(balance: &bdk_wallet::Balance, reserved_sat: u64) -> Self {
        let has_pending = balance.trusted_pending > bitcoin::Amount::ZERO
            || balance.untrusted_pending > bitcoin::Amount::ZERO
            || balance.immature > bitcoin::Amount::ZERO;
        let reserved = bitcoin::Amount::from_sat(reserved_sat);
        // BDK's trusted_spendable is the "spend right now" baseline.
        // Subtract reserved; `Amount::checked_sub` keeps us safe if a
        // race ever puts reserved > spendable.
        let spendable = balance
            .trusted_spendable()
            .checked_sub(reserved)
            .unwrap_or(bitcoin::Amount::ZERO);
        Self {
            confirmed_btc: format_btc(balance.confirmed),
            trusted_pending_btc: format_btc(balance.trusted_pending),
            untrusted_pending_btc: format_btc(balance.untrusted_pending),
            immature_btc: format_btc(balance.immature),
            spendable_btc: format_btc(spendable),
            total_btc: format_btc(balance.total()),
            reserved_btc: format_btc(reserved),
            has_pending,
            has_reserved: reserved_sat > 0,
        }
    }
}

/// View-model for one row in the proposals (Send tab) table.
#[derive(Debug, Clone, Serialize)]
pub struct ProposalView {
    pub id: Uuid,
    /// The proposal's own federation **version** id — used for the detail link
    /// (the list aggregates across versions, so this is not the page's id).
    pub federation_id: Uuid,
    pub label: String,
    pub status: String,
    pub recipient: String,
    pub amount_btc: String,
    pub fee_btc: String,
    pub created_at: String,
    /// Label of the version this proposal belongs to, e.g. `"v1 (current)"`.
    pub version_label: String,
    /// `true` if the viewer is eligible to sign this proposal (a member of its
    /// version with a Trezor onboarded). A current signer sees every version's
    /// proposals but may only act on the ones this is `true` for.
    pub viewer_can_sign: bool,
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
    /// One panel per version the viewer is entitled to (newest first); the
    /// panel for the page's `federation_id` is `selected` by default.
    federation_groups: Vec<FederationGroupView>,
    active_tab: &'static str,
}

/// A version's address panel in the version-tabbed receive view.
struct FederationGroupView {
    /// `version_index` — stable id for the tab/panel.
    version: i32,
    /// This version's `federation_id` — for address-detail links.
    federation_id: Uuid,
    /// Tab label, e.g. `"v1 (current)"` / `"v2"`.
    label: String,
    /// The default-open panel (the version the user navigated to).
    selected: bool,
    /// Revealed external (receive) addresses for this version.
    addresses: Vec<AddressView>,
    /// Internal (change) addresses for this version that have held funds.
    change_addresses: Vec<AddressView>,
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
    /// `Some(addr)` when the viewer is an **old** signer (a member of a previous
    /// version but not the current one): the recipient is locked to the current
    /// federation's address. `None` for current signers (free recipient).
    locked_recipient: Option<String>,
    active_tab: &'static str,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

/// `GET /federations/:id` — 303 redirect to the default tab (Receive).
///
/// Keeps existing bookmarks working after the tab split.
#[allow(clippy::unused_async)] // axum `Handler` requires an async fn
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

    // Show addresses for every version the viewer may see (newest first), as
    // tabs; default to the version they navigated to (`federation_id`). A current
    // signer sees all versions (view-only on historic ones); an old-only signer
    // sees only theirs. The header card reflects the navigated version.
    let visible = db::visible_versions_for_user(&state.db, federation.lineage_id, user.id).await?;

    let mut federation_groups = Vec::with_capacity(visible.len());
    let mut header_tip = federation.tip_height;
    let mut header_balance: Option<BalanceView> = None;
    for v in visible.iter().rev() {
        let vw = state.wallets.load_or_init(v.id).await?;
        let sync = vw.sync().await?;
        let addresses = vw
            .reveal_addresses(REVEAL_COUNT)
            .await?
            .into_iter()
            .map(AddressView::from)
            .collect();
        let change_addresses = vw
            .change_addresses()
            .await
            .into_iter()
            .map(AddressView::from)
            .collect();
        let label = if v.status == "active" {
            format!("v{} (current)", v.version_index + 1)
        } else {
            format!("v{}", v.version_index + 1)
        };
        if v.id == federation_id {
            header_tip = sync.tip_height;
            let reserved = db::sum_inflight_inputs_for_federation(&state.db, federation_id).await?;
            header_balance = Some(BalanceView::from_balance(&vw.balance().await, reserved));
        }
        federation_groups.push(FederationGroupView {
            version: v.version_index,
            federation_id: v.id,
            label,
            selected: v.id == federation_id,
            addresses,
            change_addresses,
        });
    }

    let balance = header_balance.ok_or_else(|| AppError::Forbidden)?;
    let federation = FederationView {
        tip_height: header_tip,
        ..federation
    };

    Ok(ReceiveTemplate {
        email: user.email,
        federation,
        cosigners,
        balance,
        federation_groups,
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
    let reserved_sat = db::sum_inflight_inputs_for_federation(&state.db, federation_id).await?;
    let balance = BalanceView::from_balance(&fw.balance().await, reserved_sat);
    let federation = FederationView {
        tip_height: sync.tip_height,
        ..federation
    };

    // Proposals are lineage-scoped by the same visibility split as the address
    // tabs: a current signer sees every version's proposals; an old-only signer
    // sees only their versions'. Each row links to its own version and is tagged
    // with whether the viewer may actually sign it.
    let visible = db::visible_versions_for_user(&state.db, federation.lineage_id, user.id).await?;
    let mut version_meta: std::collections::HashMap<Uuid, (String, bool)> =
        std::collections::HashMap::with_capacity(visible.len());
    let mut version_ids: Vec<Uuid> = Vec::with_capacity(visible.len());
    for v in &visible {
        let label = if v.status == "active" {
            format!("v{} (current)", v.version_index + 1)
        } else {
            format!("v{}", v.version_index + 1)
        };
        let can_sign = db::find_signer_for_user_in_version(&state.db, user.id, v.id)
            .await?
            .is_some();
        version_meta.insert(v.id, (label, can_sign));
        version_ids.push(v.id);
    }
    let proposal_rows = db::list_proposals_for_federations(&state.db, &version_ids).await?;
    let proposals: Vec<ProposalView> = proposal_rows
        .into_iter()
        .map(|r| {
            let (version_label, viewer_can_sign) = version_meta
                .get(&r.federation_id)
                .cloned()
                .unwrap_or_else(|| ("—".to_string(), false));
            ProposalView::build(r, version_label, viewer_can_sign)
        })
        .collect();

    // Old signers (members of a previous version, not the current one) may only
    // send to the current federation: pre-fill + lock the recipient. Current
    // signers send anywhere.
    let status = current_signer_status(&state, federation.lineage_id, user.id).await?;
    let locked_recipient = match (status.is_current_signer, status.current_version_id) {
        (false, Some(current_id)) => Some(
            state
                .wallets
                .load_or_init(current_id)
                .await?
                .reveal_first_external()
                .await?
                .to_string(),
        ),
        _ => None,
    };

    Ok(SendTemplate {
        email: user.email,
        federation,
        cosigners,
        balance,
        proposals,
        locked_recipient,
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

    build_header_views(state, row, user_id).await
}

/// Build the shared header view-models (federation card + cosigner roster) from
/// an already-loaded row, **without** a membership gate. Callers that allow a
/// broader (view-only) audience — e.g. a current signer of the lineage viewing a
/// historic version's proposal — do their own access check first.
pub(crate) async fn build_header_views(
    state: &Arc<AppState>,
    row: FederationRow,
    user_id: Uuid,
) -> Result<(FederationView, Vec<CosignerView>), AppError> {
    let members = db::list_federation_members_with_signers(&state.db, row.id).await?;
    let cosigners = members
        .into_iter()
        .map(|(u, s)| build_cosigner_view(&u.email, s, user_id == u.id))
        .collect::<Vec<_>>();

    let federation = FederationView {
        id: row.id,
        lineage_id: row.lineage_id,
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

/// A viewer's signing relationship to a lineage's **current** version.
pub(crate) struct CurrentSignerStatus {
    /// The lineage's current active version, if one exists.
    pub current_version_id: Option<Uuid>,
    /// Whether the viewer is a signer on that current version.
    pub is_current_signer: bool,
}

/// Determine whether `user_id` is a **current** signer of `lineage_id` (a signer
/// on its active version). Old-only signers (members of a previous version but
/// not the current one) are restricted to sending to the current federation;
/// current signers may send anywhere. With no active version, nothing is
/// restricted.
///
/// # Errors
/// Propagates any underlying SQL error.
pub(crate) async fn current_signer_status(
    state: &AppState,
    lineage_id: Uuid,
    user_id: Uuid,
) -> Result<CurrentSignerStatus, AppError> {
    let current = db::current_version_for_lineage(&state.db, lineage_id).await?;
    let is_current_signer = match &current {
        Some(c) => db::find_signer_for_user_in_version(&state.db, user_id, c.id)
            .await?
            .is_some(),
        None => true,
    };
    Ok(CurrentSignerStatus {
        current_version_id: current.map(|c| c.id),
        is_current_signer,
    })
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

impl ProposalView {
    /// Build a row from a proposal plus the resolved version context (which
    /// version it belongs to and whether the viewer may sign it).
    fn build(r: ProposalRow, version_label: String, viewer_can_sign: bool) -> Self {
        let recipient = r
            .proposal_json
            .get("recipient")
            .and_then(|v| v.as_str())
            .map_or_else(|| "—".to_string(), std::string::ToString::to_string);
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
            federation_id: r.federation_id,
            label: r.label.unwrap_or_else(|| "(no label)".to_string()),
            status: r.status,
            recipient,
            amount_btc: format_btc_sats(amount_sats),
            fee_btc: format_btc_sats(fee_sats),
            created_at: format_timestamp(r.created_at),
            version_label,
            viewer_can_sign,
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
