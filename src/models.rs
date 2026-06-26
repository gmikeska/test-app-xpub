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
    /// JSON-encoded `bdk_wallet::ChangeSet`. `None` until the federation's
    /// BDK wallet has been initialised at least once.
    pub bdk_changeset: Option<serde_json::Value>,
    /// Cached chain tip height (from the BDK wallet's local chain) for
    /// display on the federation page. `None` before the first sync.
    pub chain_tip_height: Option<i32>,
    /// Lineage this version belongs to. All versions of one wallet share it;
    /// for a brand-new federation it equals the row's own `id` (v0).
    pub lineage_id: Uuid,
    /// Position within the lineage (`0` = oldest). The newest `active` version
    /// is "current".
    pub version_index: i32,
    /// The version this one succeeds (the migration's base). `None` for v0.
    pub predecessor_id: Option<Uuid>,
    /// Lifecycle: `pending` | `active` | `superseded` | `abandoned`.
    pub status: String,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// `federation_migrations` row — the version-change record (roster change that
/// mints version N+1). The signed sweep lives in `transaction_proposals`
/// (`kind = 'migration'`); this is the governance/record side.
#[allow(dead_code)] // consumed by the migration flow in Phases 3–4
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct FederationMigrationRow {
    /// Migration id.
    pub id: Uuid,
    /// Lineage being migrated.
    pub lineage_id: Uuid,
    /// Current version this migration amends.
    pub base_version_id: Uuid,
    /// Pending successor version (set once it is minted in Phase 3).
    pub target_version_id: Option<Uuid>,
    /// Member who started the migration.
    pub proposed_by: Uuid,
    /// Threshold (`m`) chosen for the next version.
    pub next_threshold: i32,
    /// Lifecycle: `draft` | `proposed` | `enacted` | `cancelled`.
    pub status: String,
    /// Optional free-form note.
    pub description: Option<String>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Most-recent-mutation timestamp.
    pub updated_at: DateTime<Utc>,
}

/// `migration_changes` row — one prospective member's roster action within a
/// migration (`add` / `remove` / `keep`).
#[allow(dead_code)] // consumed by the migration flow in Phases 3–4
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct MigrationChangeRow {
    /// Owning migration.
    pub migration_id: Uuid,
    /// The member this change concerns.
    pub user_id: Uuid,
    /// Signer the member contributes to the next version (for `add`/`keep`).
    pub signer_id: Option<Uuid>,
    /// `add` | `remove` | `keep`.
    pub action: String,
    /// Member role in the next version.
    pub role: String,
}

/// `transaction_proposals` row.
///
/// One outgoing transaction in flight against a federation. The `psbt_b64`
/// column carries the canonical PSBT and mutates as cosigner partials are
/// merged in; `proposal_json` and `coin_selection_json` are the cached
/// structural views the UI renders without having to re-deserialize the PSBT.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProposalRow {
    /// Proposal id.
    pub id: Uuid,
    /// Owning federation.
    pub federation_id: Uuid,
    /// User who created the proposal (and the only one who can `cancel` it).
    pub proposed_by: Uuid,
    /// Optional human-readable label (e.g. "Q3 payroll").
    pub label: Option<String>,
    /// Lifecycle state: `proposed` | `signing` | `finalized` | `broadcast` |
    /// `cancelled`. See `migrations/0003_proposals.sql` for the canonical
    /// description.
    pub status: String,
    /// Base64-encoded canonical PSBT.
    pub psbt_b64: String,
    /// Structural view (outputs, total, fee, `fee_rate`) of the unsigned tx.
    pub proposal_json: serde_json::Value,
    /// BDK's coin-selection result for this proposal: selected UTXOs +
    /// recipient/change split.
    pub coin_selection_json: serde_json::Value,
    /// Hex-encoded finalized raw transaction (populated when status
    /// transitions to `finalized`).
    pub finalized_tx_hex: Option<String>,
    /// Transaction id of the finalized tx (populated on `finalized` /
    /// `broadcast`).
    pub txid: Option<String>,
    /// Timestamp of successful `sendrawtransaction`.
    pub broadcast_at: Option<DateTime<Utc>>,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the most recent mutation (signature merge / cancel / etc.).
    pub updated_at: DateTime<Utc>,
}

/// `transaction_signatures` row.
///
/// One cosigner contribution to a proposal. A re-sign by the same cosigner
/// is treated as idempotent at the handler layer.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SignatureRow {
    /// Proposal this signature contributes to.
    pub proposal_id: Uuid,
    /// The `signers` row that produced the signature.
    pub signer_id: Uuid,
    /// The user who triggered the signing (always the owner of `signer_id`).
    pub user_id: Uuid,
    /// Base64-encoded partial PSBT containing this cosigner's `partial_sigs`.
    pub partial_psbt_b64: String,
    /// Timestamp.
    pub signed_at: DateTime<Utc>,
}

/// `transaction_rejections` row.
///
/// Advisory: rejections are recorded for audit but never auto-flip a
/// proposal's status. Only the proposer's `cancel` action can close a
/// proposal short of finalization.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RejectionRow {
    /// Proposal being rejected.
    pub proposal_id: Uuid,
    /// The user rejecting.
    pub user_id: Uuid,
    /// Optional free-form reason.
    pub reason: Option<String>,
    /// Timestamp.
    pub rejected_at: DateTime<Utc>,
}
