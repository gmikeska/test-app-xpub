//! Thin `sqlx` query helpers wrapping the schema in `migrations/0001_init.sql`.
//!
//! Every function takes a `&PgPool` (rather than a held connection) so
//! handlers don't have to manage acquisitions explicitly.

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{FederationRow, SignerRow, UserRow};

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
                f.descriptor, f.snapshot_json, f.created_at \
         FROM federations f \
         JOIN federation_members m ON m.federation_id = f.id \
         WHERE m.user_id = $1 \
         ORDER BY f.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}
