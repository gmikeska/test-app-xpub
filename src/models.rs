//! Row structs mirroring the `migrations/0001_init.sql` schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// `users` row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserRow {
    /// User id.
    pub id: Uuid,
    /// Login email.
    pub email: String,
    /// Argon2id-encoded password hash (PHC string).
    pub password_hash: String,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// `signers` row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SignerRow {
    /// Signer id.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// The literal descriptor-key string the device exported.
    pub descriptor_key: String,
    /// Extended public key (xpub/tpub).
    pub xpub: String,
    /// Master fingerprint (hex, lowercase).
    pub fingerprint: String,
    /// Full derivation path including the leading `m/`.
    pub derivation_path: String,
    /// Device family, e.g. `"Trezor"`.
    pub device_type: String,
    /// Bitcoin network, e.g. `"testnet"`.
    pub network: String,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// `federations` row.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FederationRow {
    /// Federation id.
    pub id: Uuid,
    /// Human-readable label.
    pub label: String,
    /// `m` value of the m-of-n federation.
    pub threshold: i32,
    /// `n` value (total signers).
    pub total_signers: i32,
    /// Bitcoin network the federation operates on.
    pub network: String,
    /// `wsh(sortedmulti(...))` descriptor.
    pub descriptor: String,
    /// Canonical `FederationSnapshot` JSON.
    pub snapshot_json: serde_json::Value,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
}
