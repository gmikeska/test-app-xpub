//! Federation-migration handlers (Phase 3): start a migration that changes the
//! current version's roster and mints a **pending** successor version. No funds
//! move here — the migration transaction that sweeps the funds is Phase 4.
//!
//! - `GET  /federations/{id}/migrate`     — form: remove current members / add
//!                                           new ones / choose the next threshold.
//! - `POST /federations/{id}/migrations`  — compute the roster delta, build the
//!                                           successor descriptor, persist the
//!                                           migration + pending version.
//!
//! `{id}` is the **current (active)** version; only its members may start a
//! migration, and only one migration may be in flight per lineage. See
//! `emerald_multisignature/xpub_federation_migration.md` §5.1.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::Form;
use serde::Deserialize;
use uuid::Uuid;

use emvault::core::bitcoin::Amount;
use emvault::core::{Federation, FederationSnapshot, NetworkType};
use emvault::elements::descriptor::to_multipath_string as ct_to_multipath_string;
use emvault::elements::sync::KeychainKind;
use emvault::elements::{CtDescriptorBuilder, CtKeyMode, ElementsWollet};
use emvault::xpub::ExternalSigner;
use rand::RngCore;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db::{self, NewPendingMigration};
use crate::error::AppError;
use crate::handlers::federations::{BalanceView, CosignerView, FederationView, load_header};
use crate::handlers::new_federation::{parse_device_type, resolve_member_signers};
use emvault::core::build_federation;
use emvault::core::roster::{RosterAction, compute_roster_plan, validate_threshold};

// ---------------------------------------------------------------------------
// Member view (shared by the Federation tab's migrate form)
// ---------------------------------------------------------------------------

struct MemberView {
    user_id: Uuid,
    email: String,
}

// ---------------------------------------------------------------------------
// POST /federations/{id}/migrations
// ---------------------------------------------------------------------------

/// Form posted by `migration_new.html`. Repeated checkbox values deserialize
/// into `Vec`s via `serde_html_form`.
#[derive(Debug, Deserialize)]
pub struct MigrationForm {
    /// Threshold (`m`) for the next version.
    pub threshold: i32,
    /// Fee rate (sat/vB) for the migration sweep transaction.
    pub fee_rate: u64,
    /// User ids to add in the next version.
    #[serde(default)]
    pub add_ids: Vec<Uuid>,
    /// Current-member user ids to remove in the next version.
    #[serde(default)]
    pub remove_ids: Vec<Uuid>,
}

/// `POST /federations/{id}/migrations`
///
/// # Errors
/// - [`AppError::NotFound`] / [`AppError::BadRequest`] / [`AppError::Forbidden`]
///   as for [`migrate_get`].
/// - [`AppError::BadFederationInput`] for an invalid roster delta or threshold,
///   or if the successor descriptor can't be built.
/// - [`AppError::MissingMemberSigner`] if an added user lacks a P2WSH signer.
/// - Any underlying SQL error.
#[allow(clippy::too_many_lines)]
pub async fn migrate_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    Form(body): Form<MigrationForm>,
) -> Result<Response, AppError> {
    let federation = load_active_current(&state, federation_id).await?;
    let is_liquid = db::FederationKind::from_row(&federation).is_liquid();
    ensure_member(&state, federation_id, user.id).await?;
    ensure_no_inflight(&state, federation.lineage_id).await?;

    // Current roster + each member's existing signer (for `remove` change rows).
    let members = db::list_federation_members_with_signers(&state.db, federation_id).await?;
    let current_ids: Vec<Uuid> = members.iter().map(|(u, _)| u.id).collect();
    let current_signer: HashMap<Uuid, Option<Uuid>> = members
        .iter()
        .map(|(u, s)| (u.id, s.as_ref().map(|row| row.id)))
        .collect();

    // Roster arithmetic (pure, validated).
    let plan = compute_roster_plan(&current_ids, &body.add_ids, &body.remove_ids)
        .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
    // Convert the form's i32 threshold to u32 at the edge — core's roster API
    // speaks u32/NonZeroU32 (the i32 stays the storage shape, see `next_threshold`).
    let threshold_m = u32::try_from(body.threshold).map_err(|_| {
        AppError::BadFederationInput("threshold must be a positive integer".to_string())
    })?;
    let threshold = validate_threshold(threshold_m, plan.next_members.len())
        .map_err(|e| AppError::BadFederationInput(e.to_string()))?;

    // Resolve the next version's signers and build its descriptor/snapshot.
    let resolved = resolve_member_signers(
        &state.db,
        &plan.next_members,
        &state.config.federation_derivation_path,
    )
    .await?;
    let next_signer: HashMap<Uuid, Uuid> =
        resolved.iter().map(|(uid, row)| (*uid, row.id)).collect();

    let mut signers: Vec<ExternalSigner> = Vec::with_capacity(resolved.len());
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
        signers.push(s);
    }

    // A federation may hold funds on only one chain (e.g. Liquid funded,
    // Bitcoin empty). Migrating the *empty* chain has nothing to sweep, so it
    // becomes a pure version bump: mint the successor and enact the version flip
    // immediately — no sweep transaction, no proposal to sign.
    let has_funds = if is_liquid {
        let w = state.elements_wallets.load_or_init(federation_id).await?;
        w.sync().await?;
        w.lbtc_balance_sat().await? > 0
    } else {
        let w = state.wallets.load_or_init(federation_id).await?;
        w.sync().await?;
        w.balance().await.total().to_sat() > 0
    };
    if !has_funds {
        let (descriptor_string, snapshot_json) = if is_liquid {
            let elements_net = state.config.elements_network.ok_or_else(|| {
                AppError::BadFederationInput("ELEMENTS_NETWORK not configured.".into())
            })?;
            let mut mbk = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut mbk);
            let mut builder = CtDescriptorBuilder::new(threshold.get(), &mbk)
                .map_err(|e| AppError::BadFederationInput(e.to_string()))?
                .key_mode(CtKeyMode::Ranged);
            for s in &signers {
                builder
                    .add_signer(s)
                    .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
            }
            let ct_desc = builder
                .build()
                .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
            let descriptor_string = ct_to_multipath_string(&ct_desc);
            let net_type: NetworkType = elements_net.into();
            let federation_obj =
                Federation::new(threshold.get(), signers, net_type).map_err(|e| {
                    AppError::BadFederationInput(format!("Cannot construct federation: {e}"))
                })?;
            let snapshot_json: serde_json::Value = serde_json::from_str(
                &FederationSnapshot::from_federation(&federation_obj).to_canonical_json(),
            )
            .map_err(|e| {
                AppError::BadFederationInput(format!("Failed to serialise snapshot: {e}"))
            })?;
            (descriptor_string, snapshot_json)
        } else {
            let built = build_federation(
                signers,
                threshold.get(),
                NetworkType::Bitcoin(state.config.network),
            )
            .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
            (built.descriptor_string, built.snapshot_json)
        };

        let next_members: Vec<(Uuid, Uuid)> =
            resolved.iter().map(|(uid, row)| (*uid, row.id)).collect();
        let changes: Vec<(Uuid, Option<Uuid>, &str)> = plan
            .changes
            .iter()
            .map(|(uid, action)| {
                let signer_id = match action {
                    RosterAction::Keep | RosterAction::Add => next_signer.get(uid).copied(),
                    RosterAction::Remove => current_signer.get(uid).copied().flatten(),
                };
                (*uid, signer_id, action.as_str())
            })
            .collect();
        let spec = NewPendingMigration {
            lineage_id: federation.lineage_id,
            base_version_id: federation_id,
            proposed_by: user.id,
            next_threshold: body.threshold,
            description: None,
            label: &federation.label,
            network: &federation.network,
            descriptor: &descriptor_string,
            snapshot_json: &snapshot_json,
            version_index: federation.version_index + 1,
            next_members: &next_members,
            changes: &changes,
        };
        let (migration_id, pending_id) = db::create_pending_migration(&state.db, &spec).await?;
        // Nothing to move → enact the version flip now (base is left with its
        // default `not_applicable` migration status; no sweep to reconcile).
        db::enact_version_transition(&state.db, pending_id, federation_id, migration_id).await?;
        tracing::info!(
            %migration_id, %pending_id, lineage = %federation.lineage_id, proposer = %user.email,
            "federation migration enacted with no sweep (unfunded version bump)"
        );
        return Ok(Redirect::to(&format!("/federations/{pending_id}/federation")).into_response());
    }

    // Funded path: build the successor descriptor/snapshot + the migration sweep
    // BEFORE persisting anything, so a build failure leaves no dangling pending
    // version. The successor's destination is derived from its own descriptor —
    // its wallet recognises the inflow once it syncs (§5.2). Both chains produce
    // the same shape:
    // (descriptor, snapshot, sweep_b64, proposal_json, coin_selection_json).
    let (
        descriptor_string,
        snapshot_json,
        sweep_b64,
        sweep_proposal_json,
        sweep_coin_selection_json,
    ): (
        String,
        serde_json::Value,
        String,
        serde_json::Value,
        serde_json::Value,
    ) = if is_liquid {
        // --- Liquid: CT successor descriptor + drain (send-max) PSET ----------
        let elements_net = state.config.elements_network.ok_or_else(|| {
            AppError::BadFederationInput("ELEMENTS_NETWORK not configured.".into())
        })?;
        // Fresh SLIP-77 master blinding key, embedded in the `ct(...)` descriptor
        // (matches new-federation creation; per-version, server-side for dev).
        let mut mbk = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut mbk);
        let mut builder = CtDescriptorBuilder::new(threshold.get(), &mbk)
            .map_err(|e| AppError::BadFederationInput(e.to_string()))?
            .key_mode(CtKeyMode::Ranged);
        for s in &signers {
            builder
                .add_signer(s)
                .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
        }
        let ct_desc = builder
            .build()
            .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
        let descriptor_string = ct_to_multipath_string(&ct_desc);

        // Snapshot (a pure data carrier — Liquid signing is browser-side).
        let net_type: NetworkType = elements_net.into();
        let federation_obj = Federation::new(threshold.get(), signers, net_type).map_err(|e| {
            AppError::BadFederationInput(format!("Cannot construct federation: {e}"))
        })?;
        let snapshot_json: serde_json::Value = serde_json::from_str(
            &FederationSnapshot::from_federation(&federation_obj).to_canonical_json(),
        )
        .map_err(|e| AppError::BadFederationInput(format!("Failed to serialise snapshot: {e}")))?;

        // Successor's first external CT address = the sweep destination.
        let destination = ElementsWollet::from_descriptor_str(
            &descriptor_string,
            mbk,
            elements_net,
            elements_net.to_lwk(),
        )
        .and_then(|w| w.address(KeychainKind::External, 0))
        .map_err(|e| {
            AppError::BadFederationInput(format!("Cannot derive successor address: {e}"))
        })?;

        let current = state.elements_wallets.load_or_init(federation_id).await?;
        let built = current
            .build_migration_pset(&destination, Some(body.fee_rate))
            .await?;
        (
            descriptor_string,
            snapshot_json,
            built.pset_b64,
            built.proposal_json,
            built.coin_selection_json,
        )
    } else {
        // --- Bitcoin: wsh(sortedmulti) successor + BDK migration PSBT ---------
        let built = build_federation(
            signers,
            threshold.get(),
            NetworkType::Bitcoin(state.config.network),
        )
        .map_err(|e| AppError::BadFederationInput(e.to_string()))?;
        let destination =
            crate::wallet::first_external_address(state.config.network, &built.descriptor_string)?;
        let current_wallet = state.wallets.load_or_init(federation_id).await?;
        let built_tx = current_wallet
            .build_migration_tx(&destination, body.fee_rate)
            .await?;
        (
            built.descriptor_string,
            built.snapshot_json,
            built_tx.psbt_b64,
            built_tx.proposal_json,
            built_tx.coin_selection_json,
        )
    };

    // Persist: migration record + pending successor version (no funds moved yet).
    let next_members: Vec<(Uuid, Uuid)> =
        resolved.iter().map(|(uid, row)| (*uid, row.id)).collect();
    let changes: Vec<(Uuid, Option<Uuid>, &str)> = plan
        .changes
        .iter()
        .map(|(uid, action)| {
            let signer_id = match action {
                RosterAction::Keep | RosterAction::Add => next_signer.get(uid).copied(),
                RosterAction::Remove => current_signer.get(uid).copied().flatten(),
            };
            (*uid, signer_id, action.as_str())
        })
        .collect();

    let spec = NewPendingMigration {
        lineage_id: federation.lineage_id,
        base_version_id: federation_id,
        proposed_by: user.id,
        next_threshold: body.threshold,
        description: None,
        label: &federation.label,
        network: &federation.network,
        descriptor: &descriptor_string,
        snapshot_json: &snapshot_json,
        version_index: federation.version_index + 1,
        next_members: &next_members,
        changes: &changes,
    };
    let (migration_id, pending_id) = db::create_pending_migration(&state.db, &spec).await?;

    // The sweep is a `kind='migration'` proposal signed by the CURRENT
    // federation's members; the existing proposal UI drives signing → finalize
    // → broadcast, and broadcast enacts the version flip (consent-by-signing).
    let proposal_id = db::insert_migration_proposal(
        &state.db,
        federation_id,
        user.id,
        migration_id,
        &sweep_b64,
        &sweep_proposal_json,
        &sweep_coin_selection_json,
    )
    .await?;

    tracing::info!(
        %migration_id, %pending_id, %proposal_id, lineage = %federation.lineage_id,
        proposer = %user.email, "federation migration opened (pending version + sweep tx)"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/{id}/migrations/{mid}/cancel
// ---------------------------------------------------------------------------

/// `POST /federations/{id}/migrations/{mid}/cancel`
///
/// Cancel an in-flight migration before broadcast: abandon the pending version
/// and cancel its sweep proposal, freeing the lineage to start a new migration.
/// Members only.
///
/// # Errors
/// - [`AppError::Forbidden`] if the user isn't a member of the base federation.
/// - Any underlying SQL error.
pub async fn cancel_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path((federation_id, migration_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, AppError> {
    ensure_member(&state, federation_id, user.id).await?;
    db::cancel_migration(&state.db, migration_id).await?;
    tracing::info!(%migration_id, by = %user.email, "federation migration cancelled");
    Ok(Redirect::to(&format!("/federations/{federation_id}")).into_response())
}

// ---------------------------------------------------------------------------
// GET /federations/{id}/federation  — the merged "Federation" tab:
//   version history (req 6 & 7) + the migrate form (when eligible)
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "federation_manage.html")]
struct FederationManageTemplate {
    email: String,
    /// Page header (the version the user navigated to).
    federation: FederationView,
    cosigners: Vec<CosignerView>,
    balance: BalanceView,
    /// Every version of the lineage, with per-version balance / status / relay.
    versions: Vec<VersionView>,
    /// The migrate form — `Some` only when the viewer is a current signer of the
    /// active version and no migration is in flight.
    migrate: Option<MigrateFormView>,
    active_tab: &'static str,
}

// Four independent per-version UI flags (is_current / viewer_can_sign /
// relay_available / resweep_available) drive distinct badges and action buttons;
// folding them into an enum would obscure that they are orthogonal.
#[allow(clippy::struct_excessive_bools)]
struct VersionView {
    federation_id: Uuid,
    version_index: i32,
    status: String,
    is_current: bool,
    balance_btc: String,
    /// The viewer is a member of this version and can sign for it (req 6/7).
    viewer_can_sign: bool,
    /// Superseded version still holding funds → a relay can sweep them forward.
    relay_available: bool,
    /// Superseded version whose migration a censorship reorg reverted
    /// (`migration_status = 'pending'`) → a re-sweep can re-complete it. The
    /// funds are locked under the stranded unconfirmed sweep (so `balance` reads
    /// zero — this is deliberately **not** gated on balance, unlike relay).
    resweep_available: bool,
    /// Reorg-reconciliation status of this version's migration sweep
    /// (`not_applicable` | `pending` | `in_progress` | `complete`). Rendered as a
    /// badge that flips `complete → pending` when a reorg evicts the recorded
    /// sweep (the live-reorg demo's visible signal).
    migration_status: String,
}

/// The roster-change form embedded in the Federation tab. Targets the lineage's
/// **active** version (the form posts to `/federations/{active_id}/migrations`).
struct MigrateFormView {
    active_id: Uuid,
    threshold: i32,
    current_members: Vec<MemberView>,
    candidates: Vec<MemberView>,
}

/// `GET /federations/{id}/federation`
///
/// The Federation tab: the lineage's version history (visible to any member of
/// any version — req 7 — with per-version balances + relay on funded superseded
/// versions — req 6) **and** the migrate form (shown only to a current signer of
/// the active version when no migration is in flight).
///
/// # Errors
/// - [`AppError::NotFound`] if the federation doesn't exist.
/// - [`AppError::Forbidden`] if the viewer isn't a member of `{id}`.
/// - Any underlying SQL/wallet error.
#[allow(clippy::too_many_lines)]
pub async fn federation_manage(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    axum::extract::Query(cq): axum::extract::Query<crate::handlers::federations::ChainQuery>,
) -> Result<Response, AppError> {
    // Header + membership check for the page's version.
    let (header, cosigners) =
        load_header(&state, federation_id, user.id, cq.chain.as_deref()).await?;
    let lineage_id = header.lineage_id;

    // The tab renders for both chains; the migrate/relay/resweep *actions* are
    // Bitcoin-only for now (guarded in their handlers + hidden below for Liquid).
    let is_liquid = header.is_liquid;

    // Balance for the header (the page's version) — LWK for Liquid, BDK for BTC.
    let reserved = db::sum_inflight_inputs_for_federation(&state.db, federation_id).await?;
    let (federation, balance) = if is_liquid {
        let fw = state.elements_wallets.load_or_init(federation_id).await?;
        let sync = fw.sync().await?;
        let lbtc_sat = fw.lbtc_balance_sat().await?;
        (
            FederationView {
                tip_height: sync.tip_height,
                ..header
            },
            BalanceView::from_lbtc_sat(lbtc_sat, reserved),
        )
    } else {
        let fw = state.wallets.load_or_init(federation_id).await?;
        let sync = fw.sync().await?;
        (
            FederationView {
                tip_height: sync.tip_height,
                ..header
            },
            BalanceView::from_balance(&fw.balance().await, reserved),
        )
    };

    // Version history. Freshen all versions (best-effort — a node outage
    // shouldn't hide the history). BDK-only; the Liquid per-version balances are
    // read individually below.
    if !is_liquid && let Err(e) = state.wallets.sync_lineage(lineage_id).await {
        tracing::warn!(error = %e, %lineage_id, "lineage sync failed; rendering cached balances");
    }
    // Versions the viewer may see: a current signer sees all of them; an
    // old-only signer sees only the versions they're a member of.
    let versions = db::visible_versions_for_user(&state.db, lineage_id, user.id).await?;
    let current = db::current_version_for_lineage(&state.db, lineage_id).await?;
    let current_id = current.as_ref().map(|f| f.id);

    let mut version_views = Vec::with_capacity(versions.len());
    for v in &versions {
        // Per-version balance: LWK sat total for Liquid, BDK for Bitcoin.
        let (bal_sat, has_funds) = if is_liquid {
            let w = state.elements_wallets.load_or_init(v.id).await?;
            let s = w.lbtc_balance_sat().await?;
            (s, s > 0)
        } else {
            let w = state.wallets.load_or_init(v.id).await?;
            let b = w.balance().await.total();
            (b.to_sat(), b > Amount::ZERO)
        };
        let viewer_can_sign = db::find_signer_for_user_in_version(&state.db, user.id, v.id)
            .await?
            .is_some();
        let is_current = current_id == Some(v.id);
        version_views.push(VersionView {
            federation_id: v.id,
            version_index: v.version_index,
            status: v.status.clone(),
            is_current,
            balance_btc: format!("{:.8}", Amount::from_sat(bal_sat).to_btc()),
            viewer_can_sign,
            // Relay is only offered on a funded superseded version the viewer can
            // actually sign for. Liquid relay is not yet available (guarded), so
            // never offer the button for a Liquid federation.
            relay_available: !is_liquid && !is_current && has_funds && viewer_can_sign,
            // Re-sweep is offered on a superseded version whose migration a reorg
            // reverted (migration_status = 'pending'), for a viewer who can sign
            // it. Also Bitcoin-only for now.
            resweep_available: !is_liquid
                && !is_current
                && v.migration_status == "pending"
                && viewer_can_sign,
            migration_status: v.migration_status.clone(),
        });
    }

    // Migrate form: only a current signer of the active version, no in-flight
    // migration — and Bitcoin-only for now (Liquid migrate is guarded).
    let migrate = if let Some(c) = &current {
        let is_current_signer = db::find_signer_for_user_in_version(&state.db, user.id, c.id)
            .await?
            .is_some();
        let no_inflight = db::inflight_migration_for_lineage(&state.db, lineage_id)
            .await?
            .is_none();
        if is_current_signer && no_inflight {
            let members = db::list_federation_members_with_signers(&state.db, c.id).await?;
            let current_ids: HashSet<Uuid> = members.iter().map(|(u, _)| u.id).collect();
            let current_members = members
                .iter()
                .map(|(u, _)| MemberView {
                    user_id: u.id,
                    email: u.email.clone(),
                })
                .collect();
            let path = &state.config.federation_derivation_path;
            let candidates = db::list_users_with_p2wsh_signer_status(&state.db, path)
                .await?
                .into_iter()
                .filter(|row| row.has_p2wsh_signer && !current_ids.contains(&row.user.id))
                .map(|row| MemberView {
                    user_id: row.user.id,
                    email: row.user.email,
                })
                .collect();
            Some(MigrateFormView {
                active_id: c.id,
                threshold: c.threshold,
                current_members,
                candidates,
            })
        } else {
            None
        }
    } else {
        None
    };

    Ok(FederationManageTemplate {
        email: user.email,
        federation,
        cosigners,
        balance,
        versions: version_views,
        migrate,
        active_tab: "federation",
    }
    .into_response())
}

/// Redirect the retired `/lineage` and `/migrate` tabs to the merged
/// `/federation` tab (back-compat for bookmarks/links).
#[allow(clippy::unused_async)] // axum `Handler` requires an async fn
pub async fn redirect_to_federation(Path(federation_id): Path<Uuid>) -> Redirect {
    Redirect::to(&format!("/federations/{federation_id}/federation"))
}

// ---------------------------------------------------------------------------
// POST /federations/{id}/relay  — sweep a superseded version forward (req 6)
// ---------------------------------------------------------------------------

/// Form for [`relay_post`].
#[derive(Debug, Deserialize)]
pub struct RelayForm {
    /// Fee rate (sat/vB) for the relay sweep.
    pub fee_rate: u64,
}

/// `POST /federations/{id}/relay`
///
/// Sweep late inflows held by a **superseded** version `{id}` forward to the
/// lineage's current version. Persisted as a `kind='relay'` proposal on `{id}`,
/// so **that version's** members — including ones removed in later versions —
/// are the signers (requirement 6). Broadcasting it moves funds only; it does
/// **not** change versions.
///
/// # Errors
/// - [`AppError::NotFound`] / [`AppError::BadRequest`] (not a superseded version,
///   or the lineage has no current version).
/// - [`AppError::Forbidden`] if the viewer isn't a member of `{id}`.
/// - [`AppError::Wallet`] if the sweep can't be built (e.g. no spendable funds).
pub async fn relay_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    Form(body): Form<RelayForm>,
) -> Result<Response, AppError> {
    let source = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if source.status == "active" {
        return Err(AppError::BadRequest(
            "Relay applies to a superseded version; use Send on the current version.".to_owned(),
        ));
    }
    if db::FederationKind::from_row(&source).is_liquid() {
        return Err(AppError::BadRequest(
            "Liquid federation relay is not yet available in this build — it is in progress."
                .to_owned(),
        ));
    }
    ensure_member(&state, federation_id, user.id).await?;

    let current = db::current_version_for_lineage(&state.db, source.lineage_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("lineage has no current version".to_owned()))?;
    let destination = state
        .wallets
        .load_or_init(current.id)
        .await?
        .reveal_first_external()
        .await?;

    // Drain the historic version's funds to the current version (treasury-pays).
    let built = state
        .wallets
        .load_or_init(federation_id)
        .await?
        .build_migration_tx(&destination, body.fee_rate)
        .await?;

    let proposal_id = db::insert_relay_proposal(
        &state.db,
        federation_id,
        user.id,
        &built.psbt_b64,
        &built.proposal_json,
        &built.coin_selection_json,
    )
    .await?;

    tracing::info!(
        source = %federation_id, target = %current.id, %proposal_id,
        by = %user.email, "relay sweep proposed (superseded → current)"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /federations/{id}/resweep  — re-complete a censorship-reorged migration
// ---------------------------------------------------------------------------

/// Form for [`resweep_post`].
#[derive(Debug, Deserialize)]
pub struct ResweepForm {
    /// Fee rate (sat/vB) for the re-sweep `S'`. Must exceed the original sweep's
    /// rate so `S'` can RBF-displace the stranded old sweep in the mempool; the
    /// node rejects the broadcast otherwise.
    pub fee_rate: u64,
}

/// `POST /federations/{id}/resweep`
///
/// Re-complete a migration whose sweep a **censorship reorg** stripped of its
/// confirmation: the confirmation-loss predicate (migration 0008) reverted this
/// superseded base version `{id}` to `migration_status = 'pending'` while the
/// funds stayed preserved in the funding output `D`. The ordinary drain can't
/// re-sweep — the old sweep `S` sits unconfirmed-in-mempool spending `D`, so BDK
/// marks `D` spent. Build a re-sweep `S'`
/// ([`FederationWallet::build_migration_resweep`]) that force-respends `D` and
/// signals RBF to displace `S`, draining forward to the lineage's current
/// version. Persisted as a `kind = 'resweep'` proposal on `{id}` (so that
/// version's members are the signers), whose broadcast re-marks `{id}`
/// `complete` — no version flip (the flip already happened at the migration).
///
/// # Errors
/// - [`AppError::NotFound`] / [`AppError::BadRequest`] (not a reverted base
///   version, no enacted migration, or the lineage has no current version).
/// - [`AppError::Forbidden`] if the viewer isn't a member of `{id}`.
/// - [`AppError::Wallet`] if the re-sweep can't be built (nothing stranded).
pub async fn resweep_post(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
    Form(body): Form<ResweepForm>,
) -> Result<Response, AppError> {
    let source = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    // Only a base version a reorg reverted (migration_status = 'pending') is
    // re-sweepable. A healthy `complete` version, or one that never migrated
    // (`not_applicable`), has nothing to re-complete.
    if source.migration_status != "pending" {
        return Err(AppError::BadRequest(
            "Re-sweep applies only to a migration a reorg reverted (status pending).".to_owned(),
        ));
    }
    if db::FederationKind::from_row(&source).is_liquid() {
        return Err(AppError::BadRequest(
            "Liquid federation re-sweep is not yet available in this build — it is in progress."
                .to_owned(),
        ));
    }
    ensure_member(&state, federation_id, user.id).await?;

    let migration_id = db::enacted_migration_for_base(&state.db, federation_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest("no enacted migration to re-complete for this version".to_owned())
        })?;

    let current = db::current_version_for_lineage(&state.db, source.lineage_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("lineage has no current version".to_owned()))?;
    let destination = state
        .wallets
        .load_or_init(current.id)
        .await?
        .reveal_first_external()
        .await?;

    // Sync the reverted base so its wallet graph reflects the stranded
    // unconfirmed sweep, then build the RBF re-sweep S' that displaces it.
    let source_wallet = state.wallets.load_or_init(federation_id).await?;
    source_wallet.sync().await?;
    let (built, replaced) = source_wallet
        .build_migration_resweep(&destination, body.fee_rate)
        .await?;
    // The re-sweep builder transiently evicts the stranded sweep in-memory to
    // free the funding input; drop the cached wallet so no handler observes that
    // build-only state (the next load rebuilds from the persisted changeset).
    state.wallets.evict_cache(federation_id).await;

    let proposal_id = db::insert_resweep_proposal(
        &state.db,
        federation_id,
        user.id,
        migration_id,
        &built.psbt_b64,
        &built.proposal_json,
        &built.coin_selection_json,
    )
    .await?;

    tracing::info!(
        source = %federation_id, target = %current.id, %proposal_id, %migration_id,
        replaced_sweep = %replaced, by = %user.email,
        "migration re-sweep proposed (superseded base -> current, RBF-displacing stranded sweep)"
    );

    Ok(Redirect::to(&format!(
        "/federations/{federation_id}/proposals/{proposal_id}"
    ))
    .into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a federation and require it to be the current (`active`) version.
async fn load_active_current(
    state: &AppState,
    federation_id: Uuid,
) -> Result<crate::models::FederationRow, AppError> {
    let federation = db::find_federation_by_id(&state.db, federation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("federation {federation_id}")))?;
    if federation.status != "active" {
        return Err(AppError::BadRequest(
            "Only the current federation version can be migrated.".to_owned(),
        ));
    }
    Ok(federation)
}

/// Require `user_id` to be a current member of `federation_id`.
async fn ensure_member(
    state: &AppState,
    federation_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if db::user_is_federation_member(&state.db, federation_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Reject if a migration is already in flight for `lineage_id`.
async fn ensure_no_inflight(state: &AppState, lineage_id: Uuid) -> Result<(), AppError> {
    if db::inflight_migration_for_lineage(&state.db, lineage_id)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest(
            "A migration is already in progress for this federation.".to_owned(),
        ));
    }
    Ok(())
}
