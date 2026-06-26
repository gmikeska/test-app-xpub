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

use asterism_core::NetworkType;
use asterism_xpub::ExternalSigner;

use crate::AppState;
use crate::auth::AuthUser;
use crate::db::{self, NewPendingMigration};
use crate::error::AppError;
use crate::federation_build::build_federation;
use crate::handlers::new_federation::{parse_device_type, resolve_member_signers};
use crate::roster::{RosterAction, compute_roster_plan, validate_threshold};

// ---------------------------------------------------------------------------
// GET /federations/{id}/migrate
// ---------------------------------------------------------------------------

#[derive(Template, WebTemplate)]
#[template(path = "migration_new.html")]
struct MigrationNewTemplate {
    email: String,
    federation_id: Uuid,
    label: String,
    network: String,
    threshold: i32,
    current_members: Vec<MemberView>,
    candidates: Vec<MemberView>,
}

struct MemberView {
    user_id: Uuid,
    email: String,
}

/// `GET /federations/{id}/migrate`
///
/// # Errors
/// - [`AppError::NotFound`] if the federation doesn't exist.
/// - [`AppError::BadRequest`] if it isn't the current (active) version or a
///   migration is already in flight for its lineage.
/// - [`AppError::Forbidden`] if the user isn't a current member.
/// - Any underlying SQL error.
pub async fn migrate_get(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(federation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let federation = load_active_current(&state, federation_id).await?;
    ensure_member(&state, federation_id, user.id).await?;
    ensure_no_inflight(&state, federation.lineage_id).await?;

    let members = db::list_federation_members_with_signers(&state.db, federation_id).await?;
    let current_ids: HashSet<Uuid> = members.iter().map(|(u, _)| u.id).collect();
    let current_members: Vec<MemberView> = members
        .iter()
        .map(|(u, _)| MemberView {
            user_id: u.id,
            email: u.email.clone(),
        })
        .collect();

    // Addable candidates: users with a P2WSH signer who aren't already members.
    let path = &state.config.federation_derivation_path;
    let candidates: Vec<MemberView> = db::list_users_with_p2wsh_signer_status(&state.db, path)
        .await?
        .into_iter()
        .filter(|row| row.has_p2wsh_signer && !current_ids.contains(&row.user.id))
        .map(|row| MemberView {
            user_id: row.user.id,
            email: row.user.email,
        })
        .collect();

    Ok(MigrationNewTemplate {
        email: user.email,
        federation_id,
        label: federation.label,
        network: federation.network,
        threshold: federation.threshold,
        current_members,
        candidates,
    }
    .into_response())
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
    let threshold = validate_threshold(body.threshold, plan.next_members.len())
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

    let built = build_federation(
        signers,
        threshold,
        NetworkType::Bitcoin(state.config.network),
    )
    .map_err(AppError::BadFederationInput)?;

    // Persist: migration record + pending successor version (no funds touched).
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
        descriptor: &built.descriptor_string,
        snapshot_json: &built.snapshot_json,
        version_index: federation.version_index + 1,
        next_members: &next_members,
        changes: &changes,
    };
    let (migration_id, pending_id) = db::create_pending_migration(&state.db, &spec).await?;

    tracing::info!(
        %migration_id, %pending_id, lineage = %federation.lineage_id,
        proposer = %user.email, "federation migration opened (pending version minted)"
    );

    Ok(Redirect::to(&format!("/federations/{pending_id}")).into_response())
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
