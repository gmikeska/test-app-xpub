//! Thin `sqlx` query helpers wrapping the schema in `migrations/`.
//!
//! Every function takes a `&PgPool` (rather than a held connection) so
//! handlers don't have to manage acquisitions explicitly.

use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    FederationMigrationRow, FederationRow, MigrationChangeRow, ProposalRow, RejectionRow,
    SignatureRow, SignerRow, UserRow,
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
pub async fn find_user_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<UserRow>> {
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
pub async fn list_signers_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<SignerRow>> {
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
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signers WHERE user_id = $1")
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
pub async fn find_signer_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Option<SignerRow>> {
    sqlx::query_as::<_, SignerRow>(
        "SELECT id, user_id, label, descriptor_key, xpub, fingerprint, \
                derivation_path, device_type, network, created_at \
         FROM signers WHERE user_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Fetch the user's signer at a specific BIP-32 derivation path. Used by
/// federation-creation flows that need the *P2WSH* signer specifically (a
/// `m/48'/.../2'` xpub), not just "any" onboarded signer.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn find_signer_for_user_at_path(
    pool: &PgPool,
    user_id: Uuid,
    derivation_path: &str,
) -> sqlx::Result<Option<SignerRow>> {
    sqlx::query_as::<_, SignerRow>(
        "SELECT id, user_id, label, descriptor_key, xpub, fingerprint, \
                derivation_path, device_type, network, created_at \
         FROM signers WHERE user_id = $1 AND derivation_path = $2 \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(user_id)
    .bind(derivation_path)
    .fetch_optional(pool)
    .await
}

/// One row in the federation-creation form's user picker: the candidate
/// user plus a flag for whether they have a P2WSH signer on file at the
/// configured derivation path. The flag drives UI badges only; submission
/// validation runs separately on the server.
#[derive(Debug, Clone)]
pub struct UserPickerRow {
    /// The candidate user.
    pub user: UserRow,
    /// `true` iff the user has at least one `signers` row at the configured
    /// P2WSH derivation path.
    pub has_p2wsh_signer: bool,
}

/// Every registered user, paired with a `has_p2wsh_signer` flag computed
/// against `derivation_path`. Ordered by email for stable rendering.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn list_users_with_p2wsh_signer_status(
    pool: &PgPool,
    derivation_path: &str,
) -> sqlx::Result<Vec<UserPickerRow>> {
    #[derive(sqlx::FromRow)]
    struct Joined {
        id: Uuid,
        email: String,
        password_hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
        has_p2wsh_signer: bool,
    }

    let rows = sqlx::query_as::<_, Joined>(
        "SELECT u.id, u.email, u.password_hash, u.created_at, \
                EXISTS ( \
                  SELECT 1 FROM signers s \
                  WHERE s.user_id = u.id AND s.derivation_path = $1 \
                ) AS has_p2wsh_signer \
         FROM users u \
         ORDER BY u.email ASC",
    )
    .bind(derivation_path)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UserPickerRow {
            user: UserRow {
                id: r.id,
                email: r.email,
                password_hash: r.password_hash,
                created_at: r.created_at,
            },
            has_p2wsh_signer: r.has_p2wsh_signer,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// federations
// ---------------------------------------------------------------------------

/// Inputs to [`insert_federation_with_members`]. Mirrors the non-derived
/// columns of `federations` so callers can compute them once (descriptor,
/// snapshot, etc.) and pass a single struct rather than a long positional
/// argument list.
#[derive(Debug, Clone)]
pub struct NewFederation<'a> {
    /// Human-readable label.
    pub label: &'a str,
    /// `m` of an m-of-n federation.
    pub threshold: i32,
    /// `n` of an m-of-n federation.
    pub total_signers: i32,
    /// Bitcoin network string (matches BDK's `Network::to_string()`).
    pub network: &'a str,
    /// Multipath descriptor (`wsh(sortedmulti(m, ...))` with `/<0;1>/*`).
    pub descriptor: &'a str,
    /// Canonical `FederationSnapshot` JSON.
    pub snapshot_json: &'a JsonValue,
}

/// Atomically insert a federation row and one `federation_members` row per
/// member. Every member is recorded with `role = 'trustee'`. The order of
/// `members` determines `joined_at` order (oldest first).
///
/// # Errors
/// Propagates any underlying SQL error. Rolls back on any failure mid-way.
pub async fn insert_federation_with_members(
    pool: &PgPool,
    spec: &NewFederation<'_>,
    members: &[(Uuid, Uuid)],
) -> sqlx::Result<Uuid> {
    let mut tx = pool.begin().await?;

    let federation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO federations \
            (label, threshold, total_signers, network, descriptor, snapshot_json) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id",
    )
    .bind(spec.label)
    .bind(spec.threshold)
    .bind(spec.total_signers)
    .bind(spec.network)
    .bind(spec.descriptor)
    .bind(spec.snapshot_json)
    .fetch_one(&mut *tx)
    .await?;

    // A brand-new federation is v0 of a fresh lineage: lineage_id = its own id.
    // (version_index = 0 and status = 'active' come from column defaults.)
    sqlx::query("UPDATE federations SET lineage_id = id WHERE id = $1")
        .bind(federation_id)
        .execute(&mut *tx)
        .await?;

    for (user_id, signer_id) in members {
        sqlx::query(
            "INSERT INTO federation_members \
                (federation_id, user_id, signer_id, role) \
             VALUES ($1, $2, $3, 'trustee')",
        )
        .bind(federation_id)
        .bind(user_id)
        .bind(signer_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(federation_id)
}

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
                f.chain_tip_height, f.lineage_id, f.version_index, \
                f.predecessor_id, f.status, f.created_at \
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
pub async fn find_federation_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<FederationRow>> {
    sqlx::query_as::<_, FederationRow>(
        "SELECT id, label, threshold, total_signers, network, descriptor, \
                snapshot_json, bdk_changeset, chain_tip_height, lineage_id, \
                version_index, predecessor_id, status, created_at \
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
    sqlx::query("UPDATE federations SET chain_tip_height = $1 WHERE id = $2")
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
pub async fn find_proposal_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<ProposalRow>> {
    sqlx::query_as::<_, ProposalRow>(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM transaction_proposals WHERE id = $1",
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Sum of selected-UTXO amounts (sats) across every proposal whose status
/// is in `('proposed', 'signing', 'finalized')` — i.e. *in flight*, neither
/// cancelled nor broadcast. This is what the Balance card subtracts from
/// BDK's "spendable" to expose the federation's *post-reservation*
/// spendable balance.
///
/// Once a proposal flips to `broadcast`, BDK's own balance picks up the
/// on-chain spend (input UTXOs disappear, change reappears), so we stop
/// double-counting by excluding it here. `cancelled` proposals likewise
/// fall out, returning their reserved sats to spendable on the next
/// page load.
///
/// # Errors
/// Propagates any underlying SQL error.
pub async fn sum_inflight_inputs_for_federation(
    pool: &PgPool,
    federation_id: Uuid,
) -> sqlx::Result<u64> {
    // PostgreSQL widens `SUM(bigint)` to `numeric` to avoid overflow over
    // very large groups. sqlx-postgres won't auto-decode `numeric` into
    // `i64`, so we cast the COALESCE'd aggregate back to `bigint`. Our
    // amounts (capped at ~21M BTC = 2.1e15 sats) comfortably fit in i64.
    let sats: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM((coin_selection_json->>'total_input_sat')::bigint), 0)::bigint \
         FROM transaction_proposals \
         WHERE federation_id = $1 \
           AND status IN ('proposed', 'signing', 'finalized')",
    )
    .bind(federation_id)
    .fetch_one(pool)
    .await?;
    Ok(u64::try_from(sats).unwrap_or(0))
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

// ---------------------------------------------------------------------------
// Federation versions / lineage + migrations (Phase 1)
//
// A cohesive data layer consumed by Phases 2–4 (FederatedWallet adoption,
// migration flow, enactment). Grouped in a submodule marked `allow(dead_code)`
// until those phases wire it; re-exported so call sites stay `db::*`.
// ---------------------------------------------------------------------------
#[allow(unused_imports)] // handlers/wallet consume these from Phase 2 onward
pub use versioning::*;

mod versioning {
    #![allow(dead_code)]

    use super::{FederationMigrationRow, FederationRow, MigrationChangeRow, SignerRow};
    use sqlx::PgPool;
    use uuid::Uuid;

    const FEDERATION_COLS: &str = "id, label, threshold, total_signers, network, descriptor, \
     snapshot_json, bdk_changeset, chain_tip_height, lineage_id, version_index, \
     predecessor_id, status, created_at";

    /// All versions of a lineage, oldest first (`version_index` ascending).
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn load_lineage_versions(
        pool: &PgPool,
        lineage_id: Uuid,
    ) -> sqlx::Result<Vec<FederationRow>> {
        sqlx::query_as::<_, FederationRow>(&format!(
            "SELECT {FEDERATION_COLS} FROM federations \
         WHERE lineage_id = $1 ORDER BY version_index ASC"
        ))
        .bind(lineage_id)
        .fetch_all(pool)
        .await
    }

    /// The current (newest `active`) version of a lineage, if any.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn current_version_for_lineage(
        pool: &PgPool,
        lineage_id: Uuid,
    ) -> sqlx::Result<Option<FederationRow>> {
        sqlx::query_as::<_, FederationRow>(&format!(
            "SELECT {FEDERATION_COLS} FROM federations \
         WHERE lineage_id = $1 AND status = 'active' LIMIT 1"
        ))
        .bind(lineage_id)
        .fetch_optional(pool)
        .await
    }

    /// Lineage ids the user can *see*: those where they are a member of **any**
    /// version (visibility is lineage-wide; signing eligibility is per-version —
    /// see [`find_signer_for_user_in_version`]).
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn lineages_visible_to_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Uuid>> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT DISTINCT f.lineage_id \
         FROM federations f \
         JOIN federation_members m ON m.federation_id = f.id \
         WHERE m.user_id = $1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
    }

    /// The signer `user_id` contributes to **this specific** federation version, if
    /// any. Drives per-version signing eligibility: `Some` ⇒ the user is asked to
    /// sign proposals/migrations/relays spending this version; `None` ⇒ they are a
    /// non-member (or a member without a recorded signer) and are never asked.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn find_signer_for_user_in_version(
        pool: &PgPool,
        user_id: Uuid,
        federation_id: Uuid,
    ) -> sqlx::Result<Option<SignerRow>> {
        sqlx::query_as::<_, SignerRow>(
            "SELECT s.id, s.user_id, s.label, s.descriptor_key, s.xpub, s.fingerprint, \
                s.derivation_path, s.device_type, s.network, s.created_at \
         FROM signers s \
         JOIN federation_members m ON m.signer_id = s.id \
         WHERE m.federation_id = $1 AND m.user_id = $2 \
         LIMIT 1",
        )
        .bind(federation_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Set a federation version's lifecycle status (`pending` | `active` |
    /// `superseded` | `abandoned`).
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn set_federation_status(
        pool: &PgPool,
        federation_id: Uuid,
        status: &str,
    ) -> sqlx::Result<()> {
        sqlx::query("UPDATE federations SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(federation_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Federation migrations (the version-change record)
    // ---------------------------------------------------------------------------

    /// Inputs to [`insert_migration`].
    pub struct NewMigration {
        /// Lineage being migrated.
        pub lineage_id: Uuid,
        /// Current version this migration amends.
        pub base_version_id: Uuid,
        /// Member starting the migration.
        pub proposed_by: Uuid,
        /// Threshold (`m`) for the next version.
        pub next_threshold: i32,
        /// Optional note.
        pub description: Option<String>,
    }

    /// Open a new migration in `draft`. The partial unique index enforces at most
    /// one in-flight (`draft`/`proposed`) migration per lineage — a second
    /// concurrent open fails with a unique-violation.
    ///
    /// # Errors
    /// Propagates any underlying SQL error (including the one-in-flight violation).
    pub async fn insert_migration(pool: &PgPool, spec: &NewMigration) -> sqlx::Result<Uuid> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO federation_migrations \
            (lineage_id, base_version_id, proposed_by, next_threshold, description) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(spec.lineage_id)
        .bind(spec.base_version_id)
        .bind(spec.proposed_by)
        .bind(spec.next_threshold)
        .bind(spec.description.as_deref())
        .fetch_one(pool)
        .await
    }

    /// Look up a migration by id.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn find_migration_by_id(
        pool: &PgPool,
        id: Uuid,
    ) -> sqlx::Result<Option<FederationMigrationRow>> {
        sqlx::query_as::<_, FederationMigrationRow>(
            "SELECT id, lineage_id, base_version_id, target_version_id, proposed_by, \
                next_threshold, status, description, created_at, updated_at \
         FROM federation_migrations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// The in-flight (`draft`/`proposed`) migration for a lineage, if one is open.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn inflight_migration_for_lineage(
        pool: &PgPool,
        lineage_id: Uuid,
    ) -> sqlx::Result<Option<FederationMigrationRow>> {
        sqlx::query_as::<_, FederationMigrationRow>(
            "SELECT id, lineage_id, base_version_id, target_version_id, proposed_by, \
                next_threshold, status, description, created_at, updated_at \
         FROM federation_migrations \
         WHERE lineage_id = $1 AND status IN ('draft', 'proposed') LIMIT 1",
        )
        .bind(lineage_id)
        .fetch_optional(pool)
        .await
    }

    /// Move a migration to a new lifecycle status (`draft` | `proposed` |
    /// `enacted` | `cancelled`).
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn set_migration_status(
        pool: &PgPool,
        migration_id: Uuid,
        status: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE federation_migrations SET status = $1, updated_at = now() WHERE id = $2",
        )
        .bind(status)
        .bind(migration_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Record the pending successor version a migration mints (Phase 3).
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn set_migration_target_version(
        pool: &PgPool,
        migration_id: Uuid,
        target_version_id: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query(
        "UPDATE federation_migrations SET target_version_id = $1, updated_at = now() WHERE id = $2",
    )
    .bind(target_version_id)
    .bind(migration_id)
    .execute(pool)
    .await?;
        Ok(())
    }

    /// Add one roster-change row (`add` / `remove` / `keep`) to a migration.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn insert_migration_change(
        pool: &PgPool,
        migration_id: Uuid,
        user_id: Uuid,
        signer_id: Option<Uuid>,
        action: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO migration_changes (migration_id, user_id, signer_id, action) \
         VALUES ($1, $2, $3, $4)",
        )
        .bind(migration_id)
        .bind(user_id)
        .bind(signer_id)
        .bind(action)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// List a migration's roster changes.
    ///
    /// # Errors
    /// Propagates any underlying SQL error.
    pub async fn list_migration_changes(
        pool: &PgPool,
        migration_id: Uuid,
    ) -> sqlx::Result<Vec<MigrationChangeRow>> {
        sqlx::query_as::<_, MigrationChangeRow>(
            "SELECT migration_id, user_id, signer_id, action, role \
         FROM migration_changes WHERE migration_id = $1 ORDER BY action, user_id",
        )
        .bind(migration_id)
        .fetch_all(pool)
        .await
    }

    /// Atomically apply a version flip when a migration's transaction broadcasts:
    /// supersede the predecessor, activate the new version, and mark the migration
    /// `enacted`. The predecessor is superseded **before** the new version is
    /// activated so the "one active per lineage" unique index is never transiently
    /// violated.
    ///
    /// # Errors
    /// Propagates any underlying SQL error; rolls back on any failure mid-way.
    pub async fn enact_version_transition(
        pool: &PgPool,
        new_version_id: Uuid,
        predecessor_id: Uuid,
        migration_id: Uuid,
    ) -> sqlx::Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE federations SET status = 'superseded' WHERE id = $1")
            .bind(predecessor_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE federations SET status = 'active' WHERE id = $1")
            .bind(new_version_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE federation_migrations SET status = 'enacted', updated_at = now() WHERE id = $1",
        )
        .bind(migration_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
} // mod versioning

// ---------------------------------------------------------------------------
// Phase 1 DB-layer tests (need a reachable Postgres; gated behind `db-tests`).
//
//   DATABASE_URL=postgres://asterism:asterism@HOST:5432/asterism_xpub \
//     cargo test --features db-tests
//
// `#[sqlx::test]` provisions an isolated database per test and applies every
// `migrations/*.sql` first, so these also exercise that the new migrations
// apply cleanly on a fresh DB.
// ---------------------------------------------------------------------------
#[cfg(all(test, feature = "db-tests"))]
mod tests {
    #![allow(clippy::similar_names)]

    use super::{
        NewFederation, NewMigration, current_version_for_lineage, enact_version_transition,
        find_federation_by_id, find_migration_by_id, find_signer_for_user_in_version,
        insert_federation_with_members, insert_migration, lineages_visible_to_user,
        load_lineage_versions, set_migration_status, set_migration_target_version,
    };
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn mk_user(pool: &PgPool, email: &str) -> sqlx::Result<Uuid> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, password_hash) VALUES ($1, 'x') RETURNING id",
        )
        .bind(email)
        .fetch_one(pool)
        .await
    }

    async fn mk_signer(pool: &PgPool, user_id: Uuid, fingerprint: &str) -> sqlx::Result<Uuid> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO signers \
                (user_id, descriptor_key, xpub, fingerprint, derivation_path, device_type, network) \
             VALUES ($1, 'dk', 'xpub', $2, 'testpath', 'Trezor', 'testnet') RETURNING id",
        )
        .bind(user_id)
        .bind(fingerprint)
        .fetch_one(pool)
        .await
    }

    /// Create a v0 federation (its own lineage) with a single member, returning
    /// `(federation_id, lineage_id)` — which are equal for v0.
    async fn mk_v0(pool: &PgPool, owner: Uuid, signer: Uuid, label: &str) -> sqlx::Result<Uuid> {
        let snap = json!({});
        let spec = NewFederation {
            label,
            threshold: 1,
            total_signers: 1,
            network: "testnet",
            descriptor: "wsh(sortedmulti(1,...))",
            snapshot_json: &snap,
        };
        insert_federation_with_members(pool, &spec, &[(owner, signer)]).await
    }

    /// Insert a raw federation version row with explicit lineage/version/status.
    async fn mk_version(
        pool: &PgPool,
        lineage_id: Uuid,
        version_index: i32,
        status: &str,
        predecessor: Option<Uuid>,
    ) -> sqlx::Result<Uuid> {
        let snap = json!({});
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO federations \
                (label, threshold, total_signers, network, descriptor, snapshot_json, \
                 lineage_id, version_index, status, predecessor_id) \
             VALUES ('v', 1, 1, 'testnet', 'desc', $1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(snap)
        .bind(lineage_id)
        .bind(version_index)
        .bind(status)
        .bind(predecessor)
        .fetch_one(pool)
        .await
    }

    async fn mk_member(
        pool: &PgPool,
        federation_id: Uuid,
        user_id: Uuid,
        signer_id: Option<Uuid>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO federation_members (federation_id, user_id, signer_id, role) \
             VALUES ($1, $2, $3, 'trustee')",
        )
        .bind(federation_id)
        .bind(user_id)
        .bind(signer_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    fn is_unique_violation(err: &sqlx::Error) -> bool {
        err.as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
    }

    fn new_migration(lineage: Uuid, base: Uuid, by: Uuid) -> NewMigration {
        NewMigration {
            lineage_id: lineage,
            base_version_id: base,
            proposed_by: by,
            next_threshold: 1,
            description: None,
        }
    }

    #[sqlx::test]
    async fn new_federation_is_v0_active_of_its_own_lineage(pool: PgPool) -> sqlx::Result<()> {
        let user = mk_user(&pool, "a@example.com").await?;
        let signer = mk_signer(&pool, user, "fp0").await?;
        let fed = mk_v0(&pool, user, signer, "Treasury").await?;

        let row = find_federation_by_id(&pool, fed)
            .await?
            .expect("row exists");
        assert_eq!(row.lineage_id, fed, "v0 lineage_id equals its own id");
        assert_eq!(row.version_index, 0);
        assert_eq!(row.status, "active");
        assert!(row.predecessor_id.is_none());
        Ok(())
    }

    #[sqlx::test]
    async fn one_active_version_per_lineage_enforced(pool: PgPool) -> sqlx::Result<()> {
        let user = mk_user(&pool, "a@example.com").await?;
        let signer = mk_signer(&pool, user, "fp0").await?;
        let lineage = mk_v0(&pool, user, signer, "Treasury").await?;

        // A second `active` version in the same lineage is rejected.
        let err = mk_version(&pool, lineage, 1, "active", Some(lineage))
            .await
            .unwrap_err();
        assert!(
            is_unique_violation(&err),
            "two active versions must conflict: {err:?}"
        );

        // A `pending` successor is fine, and the lineage now has two versions.
        let _pending = mk_version(&pool, lineage, 1, "pending", Some(lineage)).await?;
        let versions = load_lineage_versions(&pool, lineage).await?;
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version_index, 0);
        assert_eq!(versions[1].version_index, 1);
        Ok(())
    }

    #[sqlx::test]
    async fn visibility_is_lineage_wide_but_signing_is_per_version(
        pool: PgPool,
    ) -> sqlx::Result<()> {
        // v0 = {alice}; v1 (pending) = {bob}. Alice removed, bob added.
        let alice = mk_user(&pool, "alice@example.com").await?;
        let alice_signer = mk_signer(&pool, alice, "fpa").await?;
        let bob = mk_user(&pool, "bob@example.com").await?;
        let bob_signer = mk_signer(&pool, bob, "fpb").await?;

        let lineage = mk_v0(&pool, alice, alice_signer, "Treasury").await?;
        let v1 = mk_version(&pool, lineage, 1, "pending", Some(lineage)).await?;
        mk_member(&pool, v1, bob, Some(bob_signer)).await?;

        // Req 7: a member of any version sees the whole lineage.
        assert!(
            lineages_visible_to_user(&pool, alice)
                .await?
                .contains(&lineage)
        );
        assert!(
            lineages_visible_to_user(&pool, bob)
                .await?
                .contains(&lineage)
        );

        // Req 6: alice (removed) can still sign v0, never v1.
        assert!(
            find_signer_for_user_in_version(&pool, alice, lineage)
                .await?
                .is_some()
        );
        assert!(
            find_signer_for_user_in_version(&pool, alice, v1)
                .await?
                .is_none()
        );
        // Req 7: bob (added) can sign v1, is never asked to sign historic v0.
        assert!(
            find_signer_for_user_in_version(&pool, bob, v1)
                .await?
                .is_some()
        );
        assert!(
            find_signer_for_user_in_version(&pool, bob, lineage)
                .await?
                .is_none()
        );
        Ok(())
    }

    #[sqlx::test]
    async fn one_inflight_migration_per_lineage(pool: PgPool) -> sqlx::Result<()> {
        let user = mk_user(&pool, "a@example.com").await?;
        let signer = mk_signer(&pool, user, "fp0").await?;
        let lineage = mk_v0(&pool, user, signer, "Treasury").await?;

        let first = insert_migration(&pool, &new_migration(lineage, lineage, user)).await?;
        // A second in-flight migration on the same lineage is rejected.
        let err = insert_migration(&pool, &new_migration(lineage, lineage, user))
            .await
            .unwrap_err();
        assert!(
            is_unique_violation(&err),
            "two in-flight migrations must conflict: {err:?}"
        );

        // Once the first is no longer in flight, a new one is allowed.
        set_migration_status(&pool, first, "enacted").await?;
        let _second = insert_migration(&pool, &new_migration(lineage, lineage, user)).await?;
        Ok(())
    }

    #[sqlx::test]
    async fn enact_transition_flips_versions_and_migration(pool: PgPool) -> sqlx::Result<()> {
        let user = mk_user(&pool, "a@example.com").await?;
        let signer = mk_signer(&pool, user, "fp0").await?;
        let lineage = mk_v0(&pool, user, signer, "Treasury").await?;
        let v1 = mk_version(&pool, lineage, 1, "pending", Some(lineage)).await?;

        let migration = insert_migration(&pool, &new_migration(lineage, lineage, user)).await?;
        set_migration_target_version(&pool, migration, v1).await?;
        set_migration_status(&pool, migration, "proposed").await?;

        enact_version_transition(&pool, v1, lineage, migration).await?;

        let current = current_version_for_lineage(&pool, lineage)
            .await?
            .expect("a current version");
        assert_eq!(current.id, v1, "the new version is now current");
        let old = find_federation_by_id(&pool, lineage)
            .await?
            .expect("v0 row");
        assert_eq!(old.status, "superseded");
        let mig = find_migration_by_id(&pool, migration)
            .await?
            .expect("migration row");
        assert_eq!(mig.status, "enacted");
        Ok(())
    }
}
