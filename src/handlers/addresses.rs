//! Address detail page.
//!
//! - `GET /federations/:id/addresses/:address` — render a QR code, summary
//!   stats, and a receipts table for one address belonging to the
//!   federation. Auth-required and membership-gated, same as the
//!   federation detail page.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use emvault::core::bdk_wallet::KeychainKind;
use emvault::core::bitcoin::{self, Txid};
use emvault::elements::lwk_wollet::Chain;
use qrcode::QrCode;
use qrcode::render::svg;
use serde::Serialize;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db::{self, FederationKind};
use crate::elements_wallet::REVEAL_COUNT as ELEMENTS_REVEAL_COUNT;
use crate::error::AppError;
use crate::wallet::{AddressReceipt, REVEAL_COUNT};

/// Address detail template.
#[derive(Template, WebTemplate)]
#[template(path = "address.html")]
struct AddressDetailTemplate {
    /// Logged-in user's email (for navbar).
    email: String,
    /// Federation header info.
    federation: FederationHeader,
    /// Address-level info.
    address: AddressInfoView,
    /// Receipts (incoming UTXOs) — the Bitcoin path. For Liquid these live
    /// per-asset in `asset_activities` instead.
    receipts: Vec<ReceiptView>,
    /// Liquid: one panel per asset with activity at this address (asset tabs).
    /// Empty for Bitcoin.
    asset_activities: Vec<AddressAssetActivityView>,
    /// `true` for Liquid federations, where LWK v1 exposes no per-address
    /// activity — the template hides the BTC-denominated receipt/balance rows
    /// and the `bitcoin-cli` funding hint.
    is_liquid: bool,
}

/// One asset's activity panel on the Liquid address-detail page (asset tab).
#[derive(Debug, Serialize)]
struct AddressAssetActivityView {
    asset_id: String,
    asset_label: String,
    is_policy: bool,
    /// Active tab (from `?asset=`, else the policy asset).
    is_active: bool,
    total_received_btc: String,
    unspent_btc: String,
    receipt_count: usize,
    receipts: Vec<ReceiptView>,
}

/// Lightweight federation header for the breadcrumb / "back" link.
#[derive(Debug, Serialize)]
struct FederationHeader {
    id: Uuid,
    label: String,
    network: String,
    tip_height: u32,
}

/// View-model for the selected address.
#[derive(Debug, Serialize)]
struct AddressInfoView {
    /// The bech32 address as a string. For Liquid this is the *confidential*
    /// address.
    address: String,
    /// Liquid only: the underlying *unconfidential* address (the `tex1…` /
    /// `ex1…` form electrs indexes by). `None` for Bitcoin.
    unconfidential: Option<String>,
    /// BIP-21 URI string (`bitcoin:<addr>`), used as the QR payload.
    qr_uri: String,
    /// Pre-rendered SVG markup, ready to drop inline.
    qr_svg: String,
    /// Derivation index if the wallet recognises this address; `None` for
    /// addresses outside our keychains (caught earlier in practice — we
    /// still 404 unknown addresses).
    derivation_index: Option<u32>,
    /// "external" / "change" / "—".
    keychain: String,
    /// Total amount ever received at this address, formatted BTC.
    total_received_btc: String,
    /// Current unspent amount, formatted BTC.
    unspent_btc: String,
    /// Number of receipts (i.e. distinct UTXOs ever paid in).
    receipt_count: usize,
}

/// View-model for one receipt row.
#[derive(Debug, Serialize)]
struct ReceiptView {
    txid: String,
    vout: u32,
    amount_btc: String,
    /// Friendly status: "1 conf", "12 confs", or "Mempool".
    status: String,
    /// Confirmation height, or "—" if mempool.
    height: String,
    /// `true` if the UTXO has been spent.
    is_spent: bool,
}

/// `GET /federations/:id/addresses/:address`
pub async fn show(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, address_raw)): Path<(Uuid, String)>,
    axum::extract::Query(cq): axum::extract::Query<crate::handlers::federations::ChainQuery>,
) -> Result<Response, AppError> {
    let row = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;

    // View access: a member of this version, OR a current signer of the lineage
    // (who may view historic versions' addresses read-only — mirrors the
    // version-visibility rule for the address tabs).
    if !db::user_is_federation_member(&state.db, federation_id, user.id).await? {
        let status =
            crate::handlers::federations::current_signer_status(&state, row.lineage_id, user.id)
                .await?;
        if !status.is_current_signer {
            return Err(AppError::Forbidden);
        }
    }

    // Liquid federations render through a dedicated helper: confidential +
    // unconfidential address, derivation index, QR, and real per-address
    // received/unspent + receipts reconstructed from LWK's index-stamped
    // wallet outputs — full parity with the Bitcoin path.
    if crate::handlers::federations::resolve_active_chain(&row, cq.chain.as_deref())
        == FederationKind::Liquid
    {
        return render_liquid_address_detail(&state, user.email, row, &address_raw, cq.asset).await;
    }

    let fw = state.wallets.load_or_init(federation_id).await?;
    fw.sync().await?;
    // Make sure indices 0..REVEAL_COUNT are revealed so URL-deep-linked
    // addresses resolve even on a fresh wallet load.
    let _ = fw.reveal_addresses(REVEAL_COUNT).await?;

    let address = fw.parse_address(&address_raw)?;
    let derivation = fw.locate_address(&address).await;
    if derivation.is_none() {
        // The address parses for this network but isn't one we own — treat
        // as 404 rather than dump confusing empty data.
        return Err(AppError::NotFound(format!(
            "address {address_raw} for federation {federation_id}",
        )));
    }
    let (keychain, index) = derivation.map_or((None, None), |(k, i)| (Some(k), Some(i)));
    let keychain_str = keychain.map_or_else(|| "—".to_string(), keychain_label);

    let activity = fw.address_history(&address).await?;

    let qr_uri = format!("bitcoin:{address}");
    let qr_svg = QrCode::new(qr_uri.as_bytes())
        .map_err(|e| AppError::BadRequest(format!("Failed to encode QR: {e}")))?
        .render::<svg::Color<'_>>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(svg::Color("#0b0d12"))
        .light_color(svg::Color("#f4f6fb"))
        .build();

    let receipts = activity
        .receipts
        .iter()
        .cloned()
        .map(ReceiptView::from)
        .collect();

    Ok(AddressDetailTemplate {
        email: user.email,
        federation: FederationHeader {
            id: row.id,
            label: row.label,
            network: row.network,
            tip_height: activity.tip_height,
        },
        address: AddressInfoView {
            address: address.to_string(),
            unconfidential: None,
            qr_uri,
            qr_svg,
            derivation_index: index,
            keychain: keychain_str,
            total_received_btc: format_btc(activity.total_received),
            unspent_btc: format_btc(activity.unspent),
            receipt_count: activity.receipts.len(),
        },
        receipts,
        asset_activities: Vec::new(),
        is_liquid: false,
    }
    .into_response())
}

/// Render the address-detail page for a Liquid federation.
///
/// LWK v1 offers no per-address activity, so this shows the address, its
/// external-keychain derivation index, the unconfidential address, a QR, and
/// real per-address received/unspent + receipts (grouped from LWK's
/// index-stamped wallet outputs). Addresses we don't own (or beyond the reveal
/// window) 404, mirroring the Bitcoin path.
async fn render_liquid_address_detail(
    state: &AppState,
    email: String,
    row: crate::models::FederationRow,
    address_raw: &str,
    selected_asset: Option<String>,
) -> Result<Response, AppError> {
    let fw = state.elements_wallets.load_or_init(row.id).await?;
    let summary = fw.sync().await?;
    let address = fw.parse_address(address_raw)?;
    let addr_str = address.to_string();
    // Match against the revealed external addresses to recover the index.
    let revealed = fw.reveal_addresses(ELEMENTS_REVEAL_COUNT).await?;
    let Some(index) = revealed
        .iter()
        .find(|r| r.address == addr_str)
        .map(|r| r.index)
    else {
        return Err(AppError::NotFound(format!(
            "address {address_raw} for federation {}",
            row.id,
        )));
    };

    // Real per-address, **per-asset** activity: LWK stamps every wallet output
    // with its derivation index + asset, so we attribute confidential amounts
    // to (address, asset).
    let tip = summary.tip_height;
    let mut asset_activities: Vec<AddressAssetActivityView> = fw
        .address_activity_by_asset(Chain::External, index)
        .await?
        .into_iter()
        .map(|a| {
            let is_active = match selected_asset.as_deref() {
                Some(sel) => sel == a.asset_id,
                None => a.is_policy,
            };
            let mut receipts: Vec<ReceiptView> = a
                .receipts
                .iter()
                .map(|r| liquid_receipt_view(r, tip))
                .collect();
            receipts.sort_by(|x, y| y.height.cmp(&x.height));
            AddressAssetActivityView {
                asset_label: if a.is_policy {
                    "L-BTC".to_string()
                } else {
                    a.asset_id.chars().take(4).collect()
                },
                is_policy: a.is_policy,
                is_active,
                total_received_btc: format_sat(a.received_sat),
                unspent_btc: format_sat(a.unspent_sat),
                receipt_count: a.receipts.len(),
                receipts,
                asset_id: a.asset_id,
            }
        })
        .collect();
    // Guarantee one active tab even if `?asset=` names an asset with no
    // activity here (or the policy asset has none).
    let none_active = !asset_activities.iter().any(|v| v.is_active);
    if let (true, Some(first)) = (none_active, asset_activities.first_mut()) {
        first.is_active = true;
    }
    // Header totals mirror the policy-asset panel (hidden in the Liquid view,
    // which shows the per-asset panels instead).
    let (hdr_recv, hdr_unspent, hdr_count) =
        asset_activities.iter().find(|a| a.is_policy).map_or_else(
            || ("0.00000000".to_string(), "0.00000000".to_string(), 0),
            |a| {
                (
                    a.total_received_btc.clone(),
                    a.unspent_btc.clone(),
                    a.receipt_count,
                )
            },
        );

    // The unconfidential address is the scriptPubKey-bearing form electrs
    // indexes by (drop the blinding key).
    let unconfidential = address.to_unconfidential().to_string();

    // Bare confidential address as the QR payload; there is no universally
    // honoured `liquidnetwork:` BIP-21 analogue, and every Liquid wallet
    // accepts the raw address.
    let qr_uri = addr_str.clone();
    let qr_svg = QrCode::new(qr_uri.as_bytes())
        .map_err(|e| AppError::BadRequest(format!("Failed to encode QR: {e}")))?
        .render::<svg::Color<'_>>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(svg::Color("#0b0d12"))
        .light_color(svg::Color("#f4f6fb"))
        .build();

    Ok(AddressDetailTemplate {
        email,
        federation: FederationHeader {
            id: row.id,
            label: row.label,
            network: row.network,
            tip_height: tip,
        },
        address: AddressInfoView {
            address: addr_str,
            unconfidential: Some(unconfidential),
            qr_uri,
            qr_svg,
            derivation_index: Some(index),
            keychain: "external".to_string(),
            total_received_btc: hdr_recv,
            unspent_btc: hdr_unspent,
            receipt_count: hdr_count,
        },
        receipts: Vec::new(),
        asset_activities,
        is_liquid: true,
    }
    .into_response())
}

/// Format an L-BTC amount in sats as a fixed 8-decimal string (exact integer
/// math, no float rounding).
fn format_sat(sats: u64) -> String {
    format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000)
}

/// Build a receipt row from a Liquid per-address output, computing a friendly
/// confirmation status against the current chain tip.
fn liquid_receipt_view(r: &crate::elements_wallet::LiquidReceipt, tip: u32) -> ReceiptView {
    let (status, height) = r.height.map_or_else(
        || ("Mempool".to_string(), "—".to_string()),
        |h| {
            let confs = tip.saturating_sub(h).saturating_add(1);
            let plural = if confs == 1 { "conf" } else { "confs" };
            (format!("{confs} {plural} (h={h})"), h.to_string())
        },
    );
    ReceiptView {
        txid: r.txid.to_string(),
        vout: r.vout,
        amount_btc: format_sat(r.amount_sat),
        status,
        height,
        is_spent: r.is_spent,
    }
}

fn keychain_label(k: KeychainKind) -> String {
    match k {
        KeychainKind::External => "external".to_string(),
        KeychainKind::Internal => "change".to_string(),
    }
}

impl From<AddressReceipt> for ReceiptView {
    fn from(r: AddressReceipt) -> Self {
        let status = r.confirmation_height.map_or_else(
            || "Mempool".to_string(),
            |h| {
                let confs = r.confirmations;
                let plural = if confs == 1 { "conf" } else { "confs" };
                format!("{confs} {plural} (h={h})")
            },
        );
        let height = r
            .confirmation_height
            .map_or_else(|| "—".to_string(), |h| h.to_string());
        Self {
            txid: format_txid(&r.txid),
            vout: r.vout,
            amount_btc: format_btc(r.amount),
            status,
            height,
            is_spent: r.is_spent,
        }
    }
}

fn format_btc(amount: bitcoin::Amount) -> String {
    format!("{:.8}", amount.to_btc())
}

fn format_txid(txid: &Txid) -> String {
    txid.to_string()
}
