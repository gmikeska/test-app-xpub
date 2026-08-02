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
    /// Network identifier:
    ///
    /// - Bitcoin federations: `"bitcoin"`, `"testnet"`, `"signet"`, `"regtest"`
    ///   (matches `bdk_wallet::Network`'s display).
    /// - Liquid federations: `"liquid"`, `"liquidtestnet"`, `"elementsregtest"`.
    ///
    /// Use [`crate::db::FederationKind::from_network_str`] to discriminate.
    pub network: String,
    /// The Bitcoin `wsh(sortedmulti(...))` descriptor. (Legacy Liquid-only
    /// federations from before the dual-chain model stored a
    /// `ct(slip77(...), elwsh(sortedmulti(...)))` descriptor here instead.)
    pub descriptor: String,
    /// The Elements confidential descriptor `ct(slip77(mbk), elwsh(...))`,
    /// materialized at creation when every cosigner device is a Jade. `None`
    /// for Bitcoin-only federations. Its presence marks the federation as
    /// Elements-capable and drives the Bitcoin<->Elements toggle.
    pub elements_descriptor: Option<String>,
    /// Canonical `FederationSnapshot` JSON.
    pub snapshot_json: serde_json::Value,
    /// JSON-encoded `bdk_wallet::ChangeSet`. `None` until the federation's
    /// BDK wallet has been initialised at least once.
    ///
    /// Always `None` for Liquid federations — LWK has no `ChangeSet` shape;
    /// see [`Self::next_external_index`] / [`Self::next_internal_index`].
    pub bdk_changeset: Option<serde_json::Value>,
    /// Cached chain tip height (from the BDK / LWK local chain) for
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
    /// Reorg-reconciliation status for this version's migration sweep:
    /// `not_applicable` | `pending` | `in_progress` | `complete`. Set to
    /// `complete` (with [`Self::migration_sweep_txid`]) when a predecessor
    /// version's funds are swept forward; reverted to `pending` when a reorg
    /// evicts that sweep. Orthogonal to [`Self::status`].
    pub migration_status: String,
    /// Txid of the sweep that moved this version's funds forward, recorded when
    /// `migration_status` becomes `complete` and cleared (NULL) on a
    /// reorg-driven revert. See `db::set_migration_complete` /
    /// `db::reconcile_migration`.
    pub migration_sweep_txid: Option<String>,
    /// Block height at which [`Self::migration_sweep_txid`] was first observed
    /// **canonically confirmed** — the forward-latched, durable "was confirmed"
    /// witness for reorg-reconciliation. `None` until the sweep confirms (or
    /// when not `complete`); set once by `db::latch_migration_sweep_height` and
    /// cleared back to `None` alongside the txid on a reorg-driven revert. The
    /// reconcile predicate reverts `complete -> pending` iff this is `Some`
    /// (the sweep was confirmed) **and** the sweep is no longer canonically
    /// confirmed (confirmation lost) — see `db::reconcile_migration`.
    pub migration_sweep_confirmed_height: Option<i32>,
    /// 32-byte SLIP-77 master blinding key for Liquid federations. `None`
    /// for Bitcoin.
    pub master_blinding_key: Option<Vec<u8>>,
    /// Next address index to reveal on the external (`/0/*`) keychain.
    /// LWK has no `ChangeSet`-style persistence so we track this on the
    /// federation row directly. Ignored for Bitcoin federations (BDK
    /// round-trips this through `bdk_changeset`).
    pub next_external_index: i32,
    /// Next address index to reveal on the internal (`/1/*`, change)
    /// keychain. See [`Self::next_external_index`].
    pub next_internal_index: i32,
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
