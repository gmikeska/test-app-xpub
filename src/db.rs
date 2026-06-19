//! Thin `sqlx` query helpers wrapping the schema in `migrations/`.
//!
//! Every function takes a `&PgPool` (rather than a held connection) so
//! handlers don't have to manage acquisitions explicitly.

use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    FederationRow, ProposalRow, RejectionRow, SignatureRow, SignerRow, UserRow,
};

/// Run all `migrations/*.sql` against `pool`.
///
/// # Errors
/// Propagates [`sqlx::migrate::MigrateError`] verbatim.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

// ---------------------------------------------------------------------------
// users
// ---------------------------------------------------------------------------

/// Insert a user if no row already exists with the same email.
///
/// Returns `true` if a row was inserted.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn upsert_user_if_absent(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) \
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(email)
    .bind(password_hash)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Look up a user by email (case-insensitive on the indexed column).
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_user_by_email(
    pool: &PgPool,
    email: &str,
) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, created_at \
         FROM users WHERE lower(email) = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// Look up a user by id.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_user_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<UserRow>> {
    sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, created_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

// ---------------------------------------------------------------------------
// signers
// ---------------------------------------------------------------------------

/// Insert a freshly onboarded signer.
///
/// # Errors
/// Propagates any underlying SQL error (notably a `(user_id, fingerprint)`
/// uniqueness violation if the user re-onboards the same device).
#[allow(clippy::too_many_arguments)]
pub async fn insert_signer(
    pool: &PgPool,
    user_id: Uuid,
    label: Option<&str>,
    descriptor_key: &str,
    xpub: &str,
    fingerprint: &str,
    derivation_path: &str,
    device_type: &str,
    network: &str,
) -> sqlx::Result<SignerRow> {
    sqlx::query_as::<_, SignerRow>(
        "INSERT INTO signers \
            (user_id, label, descriptor_key, xpub, fingerprint, \
             derivation_path, device_type, network) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, user_id, label, descriptor_key, xpub, fingerprint, \
                   derivation_path, device_type, network, created_at",
    )
    .bind(user_id)
    .bind(label)
    .bind(descriptor_key)
    .bind(xpub)
    .bind(fingerprint)
    .bind(derivation_path)
    .bind(device_type)
    .bind(network)
    .fetch_one(pool)
    .await
}

/// Fetch every signer onboarded by `user_id`, oldest first.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_signers_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> sqlx::Result<Vec<SignerRow>> {
    sqlx::query_as::<_, SignerRow>(
        "SELECT id, user_id, label, descriptor_key, xpub, fingerprint, \
                derivation_path, device_type, network, created_at \
         FROM signers WHERE user_id = $1 ORDER BY created_at ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// `true` iff `user_id` has at least one row in `signers`.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn user_has_signer(pool: &PgPool, user_id: Uuid) -> sqlx::Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM signers WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// Fetch the user's primary (oldest) signer row. Used by signing flows that
/// assume a single onboarded Trezor per user.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_signer_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> sqlx::Result<Option<SignerRow>> {
    sqlx::query_as::<_, SignerRow>(
        "SELECT id, user_id, label, descriptor_key, xpub, fingerprint, \
                derivation_path, device_type, network, created_at \
         FROM signers WHERE user_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

// ---------------------------------------------------------------------------
// federations
// ---------------------------------------------------------------------------

/// Federations `user_id` is a member of, most recently created first.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_federations_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> sqlx::Result<Vec<FederationRow>> {
    sqlx::query_as::<_, FederationRow>(
        "SELECT f.id, f.label, f.threshold, f.total_signers, f.network, \
                f.descriptor, f.snapshot_json, f.bdk_changeset, \
                f.chain_tip_height, f.created_at \
         FROM federations f \
         JOIN federation_members m ON m.federation_id = f.id \
         WHERE m.user_id = $1 \
         ORDER BY f.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Look up a federation by id. `Ok(None)` if no such row exists.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_federation_by_id(
    pool: &PgPool,
    id: Uuid,
) -> sqlx::Result<Option<FederationRow>> {
    sqlx::query_as::<_, FederationRow>(
        "SELECT id, label, threshold, total_signers, network, descriptor, \
                snapshot_json, bdk_changeset, chain_tip_height, created_at \
         FROM federations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// `true` iff `user_id` has a `federation_members` row for `federation_id`.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn user_is_federation_member(
    pool: &PgPool,
    federation_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM federation_members \
         WHERE federation_id = $1 AND user_id = $2",
    )
    .bind(federation_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Cosigner roster for the federation detail page: every member's `users`
/// row paired with the [`SignerRow`] they contributed (if any — the
/// `signer_id` column is nullable to allow for post-creation rotation
/// flows).
///
/// Members are ordered by `joined_at` so the federation's listing is stable.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_federation_members_with_signers(
    pool: &PgPool,
    federation_id: Uuid,
) -> sqlx::Result<Vec<(UserRow, Option<SignerRow>)>> {
    // Sqlx doesn't have a great way to fetch a `LEFT JOIN`-shaped tuple
    // with `query_as`, so we fetch each side separately with a single
    // intermediate row struct.
    #[derive(sqlx::FromRow)]
    struct Joined {
        u_id: Uuid,
        u_email: String,
        u_password_hash: String,
        u_created_at: chrono::DateTime<chrono::Utc>,
        s_id: Option<Uuid>,
        s_user_id: Option<Uuid>,
        s_label: Option<String>,
        s_descriptor_key: Option<String>,
        s_xpub: Option<String>,
        s_fingerprint: Option<String>,
        s_derivation_path: Option<String>,
        s_device_type: Option<String>,
        s_network: Option<String>,
        s_created_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let rows = sqlx::query_as::<_, Joined>(
        "SELECT u.id              AS u_id, \
                u.email           AS u_email, \
                u.password_hash   AS u_password_hash, \
                u.created_at      AS u_created_at, \
                s.id              AS s_id, \
                s.user_id         AS s_user_id, \
                s.label           AS s_label, \
                s.descriptor_key  AS s_descriptor_key, \
                s.xpub            AS s_xpub, \
                s.fingerprint     AS s_fingerprint, \
                s.derivation_path AS s_derivation_path, \
                s.device_type     AS s_device_type, \
                s.network         AS s_network, \
                s.created_at      AS s_created_at \
         FROM federation_members m \
         JOIN users   u ON u.id = m.user_id \
         LEFT JOIN signers s ON s.id = m.signer_id \
         WHERE m.federation_id = $1 \
         ORDER BY m.joined_at ASC",
    )
    .bind(federation_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let user = UserRow {
                id: r.u_id,
                email: r.u_email,
                password_hash: r.u_password_hash,
                created_at: r.u_created_at,
            };
            let signer = match (
                r.s_id,
                r.s_user_id,
                r.s_descriptor_key,
                r.s_xpub,
                r.s_fingerprint,
                r.s_derivation_path,
                r.s_device_type,
                r.s_network,
                r.s_created_at,
            ) {
                (
                    Some(id),
                    Some(user_id),
                    Some(descriptor_key),
                    Some(xpub),
                    Some(fingerprint),
                    Some(derivation_path),
                    Some(device_type),
                    Some(network),
                    Some(created_at),
                ) => Some(SignerRow {
                    id,
                    user_id,
                    label: r.s_label,
                    descriptor_key,
                    xpub,
                    fingerprint,
                    derivation_path,
                    device_type,
                    network,
                    created_at,
                }),
                _ => None,
            };
            (user, signer)
        })
        .collect())
}

/// Persist the merged BDK changeset and the current chain tip onto the
/// federation row.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn update_federation_changeset(
    pool: &PgPool,
    federation_id: Uuid,
    changeset: &JsonValue,
    tip_height: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE federations \
         SET bdk_changeset = $1, chain_tip_height = $2 \
         WHERE id = $3",
    )
    .bind(changeset)
    .bind(tip_height)
    .bind(federation_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update only the cached `chain_tip_height` (used when a sync produced no
/// changeset delta but still observed a higher tip).
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn update_federation_tip_only(
    pool: &PgPool,
    federation_id: Uuid,
    tip_height: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE federations SET chain_tip_height = $1 WHERE id = $2",
    )
    .bind(tip_height)
    .bind(federation_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// transaction_proposals
// ---------------------------------------------------------------------------

const PROPOSAL_COLUMNS: &str = "id, federation_id, proposed_by, label, status, \
    psbt_b64, proposal_json, coin_selection_json, finalized_tx_hex, txid, \
    broadcast_at, created_at, updated_at";

/// Create a new proposal row.
///
/// # Errors
/// Propagates any underlying SQL error.
#[allow(clippy::too_many_arguments)]
pub async fn insert_proposal(
    pool: &PgPool,
    federation_id: Uuid,
    proposed_by: Uuid,
    label: Option<&str>,
    psbt_b64: &str,
    proposal_json: &JsonValue,
    coin_selection_json: &JsonValue,
) -> sqlx::Result<ProposalRow> {
    sqlx::query_as::<_, ProposalRow>(
        "INSERT INTO transaction_proposals \
            (federation_id, proposed_by, label, psbt_b64, proposal_json, \
             coin_selection_json) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, federation_id, proposed_by, label, status, psbt_b64, \
                   proposal_json, coin_selection_json, finalized_tx_hex, \
                   txid, broadcast_at, created_at, updated_at",
    )
    .bind(federation_id)
    .bind(proposed_by)
    .bind(label)
    .bind(psbt_b64)
    .bind(proposal_json)
    .bind(coin_selection_json)
    .fetch_one(pool)
    .await
}

/// Look up a proposal by id.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_proposal_by_id(
    pool: &PgPool,
    id: Uuid,
) -> sqlx::Result<Option<ProposalRow>> {
    sqlx::query_as::<_, ProposalRow>(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM transaction_proposals WHERE id = $1",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// All proposals for a federation, newest first.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_proposals_for_federation(
    pool: &PgPool,
    federation_id: Uuid,
) -> sqlx::Result<Vec<ProposalRow>> {
    sqlx::query_as::<_, ProposalRow>(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM transaction_proposals \
         WHERE federation_id = $1 ORDER BY created_at DESC",
    ))
    .bind(federation_id)
    .fetch_all(pool)
    .await
}

/// Persist a merged PSBT + new status. Called after each signature merge.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn update_proposal_psbt(
    pool: &PgPool,
    proposal_id: Uuid,
    psbt_b64: &str,
    status: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE transaction_proposals \
         SET psbt_b64 = $1, status = $2, updated_at = now() \
         WHERE id = $3",
    )
    .bind(psbt_b64)
    .bind(status)
    .bind(proposal_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Move a proposal into the `finalized` state with its raw-tx encoding.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn finalize_proposal(
    pool: &PgPool,
    proposal_id: Uuid,
    finalized_tx_hex: &str,
    txid: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE transaction_proposals \
         SET status = 'finalized', finalized_tx_hex = $1, txid = $2, updated_at = now() \
         WHERE id = $3",
    )
    .bind(finalized_tx_hex)
    .bind(txid)
    .bind(proposal_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Move a proposal into the `broadcast` state.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn mark_proposal_broadcast(
    pool: &PgPool,
    proposal_id: Uuid,
    txid: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE transaction_proposals \
         SET status = 'broadcast', txid = $1, broadcast_at = now(), updated_at = now() \
         WHERE id = $2",
    )
    .bind(txid)
    .bind(proposal_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Move a proposal into the `cancelled` state.
///
/// Caller (handler) MUST confirm the requesting user is the proposer.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn cancel_proposal(pool: &PgPool, proposal_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE transaction_proposals \
         SET status = 'cancelled', updated_at = now() \
         WHERE id = $1",
    )
    .bind(proposal_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// transaction_signatures
// ---------------------------------------------------------------------------

/// Insert a cosigner contribution. Re-signing by the same cosigner is a
/// no-op (returns `Ok(None)`).
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn insert_signature(
    pool: &PgPool,
    proposal_id: Uuid,
    signer_id: Uuid,
    user_id: Uuid,
    partial_psbt_b64: &str,
) -> sqlx::Result<Option<SignatureRow>> {
    sqlx::query_as::<_, SignatureRow>(
        "INSERT INTO transaction_signatures \
            (proposal_id, signer_id, user_id, partial_psbt_b64) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (proposal_id, signer_id) DO NOTHING \
         RETURNING proposal_id, signer_id, user_id, partial_psbt_b64, signed_at",
    )
    .bind(proposal_id)
    .bind(signer_id)
    .bind(user_id)
    .bind(partial_psbt_b64)
    .fetch_optional(pool)
    .await
}

/// All cosigner contributions for a proposal, oldest first.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_signatures_for_proposal(
    pool: &PgPool,
    proposal_id: Uuid,
) -> sqlx::Result<Vec<SignatureRow>> {
    sqlx::query_as::<_, SignatureRow>(
        "SELECT proposal_id, signer_id, user_id, partial_psbt_b64, signed_at \
         FROM transaction_signatures \
         WHERE proposal_id = $1 ORDER BY signed_at ASC",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// transaction_rejections
// ---------------------------------------------------------------------------

/// Insert or update an advisory rejection. Idempotent on `(proposal_id,
/// user_id)`; a second call by the same user overwrites the reason.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn insert_rejection(
    pool: &PgPool,
    proposal_id: Uuid,
    user_id: Uuid,
    reason: Option<&str>,
) -> sqlx::Result<RejectionRow> {
    sqlx::query_as::<_, RejectionRow>(
        "INSERT INTO transaction_rejections (proposal_id, user_id, reason) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (proposal_id, user_id) DO UPDATE \
            SET reason = EXCLUDED.reason, rejected_at = now() \
         RETURNING proposal_id, user_id, reason, rejected_at",
    )
    .bind(proposal_id)
    .bind(user_id)
    .bind(reason)
    .fetch_one(pool)
    .await
}

/// All rejections for a proposal, oldest first.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_rejections_for_proposal(
    pool: &PgPool,
    proposal_id: Uuid,
) -> sqlx::Result<Vec<RejectionRow>> {
    sqlx::query_as::<_, RejectionRow>(
        "SELECT proposal_id, user_id, reason, rejected_at \
         FROM transaction_rejections \
         WHERE proposal_id = $1 ORDER BY rejected_at ASC",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await
}
