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
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
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
