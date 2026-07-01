//! Per-federation BDK wallet management.
//!
//! Each federation gets one `bdk_wallet::Wallet` that lives in-process behind
//! a `tokio::sync::Mutex`. State is persisted as a JSON-encoded
//! `bdk_wallet::ChangeSet` on the `federations.bdk_changeset` column, and
//! chain data is sourced from the local Bitcoin Core node via
//! [`emvault::core::chain_sync::emitter_sync`].
//!
//! # Concurrency model
//!
//! - [`WalletManager`] owns an `Arc<HashMap<Uuid, Arc<FederationWallet>>>`
//!   behind an async mutex. The cache mutex is only held during lookup /
//!   insertion, never across BDK work.
//! - [`FederationWallet`] wraps the inner `Wallet` in its own async mutex so
//!   concurrent requests for the same federation serialize. Per-federation
//!   serialization is required because `Wallet` is single-owner mutable.
//! - DB persistence happens *after* the inner wallet lock is released so DB
//!   I/O doesn't block other readers of the same wallet.
//!
//! # Persistence model
//!
//! `Wallet::take_staged()` returns the delta accumulated since the last
//! `take_staged` / construction. We merge each delta into the aggregate
//! changeset stored on the row and write the merged blob back. This is what
//! `bdk_wallet`'s docs recommend for backends that don't implement
//! `WalletPersister` directly.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use emvault::config::{hex_decode, hex_encode};
use emvault::core::bdk_bitcoind_rpc::{Emitter, NO_EXPECTED_MEMPOOL_TXS};
use emvault::core::bdk_wallet::chain::{BlockId, ChainPosition, Merge};
use emvault::core::bdk_wallet::{
    self, AddressInfo, ChangeSet, KeychainKind, SignOptions, Update, Wallet,
};
use emvault::core::bitcoin::address::NetworkUnchecked;
use emvault::core::bitcoin::bip32::{ChildNumber, DerivationPath, Fingerprint, Xpub};
use emvault::core::bitcoin::consensus::Encodable;
use emvault::core::bitcoin::ecdsa::Signature as EcdsaSignature;
use emvault::core::bitcoin::sighash::EcdsaSighashType;
use emvault::core::bitcoin::{
    self, Address, Amount, FeeRate, Network, Psbt, PublicKey, ScriptBuf, Transaction, Txid,
};
use emvault::core::bitcoincore_rpc::{self, Auth, Client as RpcClient, RpcApi};
use emvault::core::chain_sync::{self, ChainSyncError, InitWalletError};
use emvault::core::error::PsbtError;
use emvault::core::psbt as core_psbt;
use serde::Serialize;
use sqlx::PgPool;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::db;
use crate::models::SignerRow;

/// Default reveal target: addresses `0..=REVEAL_COUNT-1` on the external
/// keychain are eagerly populated each time a federation page is rendered.
pub const REVEAL_COUNT: u32 = 20;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors raised by the wallet layer.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    /// Federation row not found.
    #[error("federation `{0}` not found")]
    NotFound(Uuid),

    /// The federation row's `network` value isn't a known [`bitcoin::Network`].
    #[error("federation `{id}` has unknown network `{network}`")]
    BadNetwork {
        /// Federation id.
        id: Uuid,
        /// The offending value as stored in the DB.
        network: String,
    },

    /// The stored aggregate changeset wouldn't deserialize.
    #[error("federation `{id}`: stored bdk_changeset is malformed: {source}")]
    DecodeChangeSet {
        /// Federation id.
        id: Uuid,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },

    /// `bdk_wallet::Wallet::load_wallet_no_persist` rejected the stored
    /// changeset (e.g. network mismatch with a previously-persisted record).
    #[error("federation `{id}`: failed to load persisted wallet: {source}")]
    LoadWallet {
        /// Federation id.
        id: Uuid,
        /// Underlying BDK load error.
        #[source]
        source: bdk_wallet::LoadError,
    },

    /// The stored changeset existed but didn't describe a wallet (empty /
    /// missing descriptor or network). Indicates a corrupt row.
    #[error("federation `{0}`: stored bdk_changeset is empty after merge")]
    EmptyChangeSet(Uuid),

    /// `bdk_wallet::Wallet::create*` rejected the descriptor.
    #[error("federation `{id}`: failed to construct wallet: {source}")]
    CreateWallet {
        /// Federation id.
        id: Uuid,
        /// Underlying descriptor parsing error.
        #[source]
        source: Box<bdk_wallet::descriptor::error::Error>,
    },

    /// Rebuilding a version's `Federation` from its members' stored signers
    /// failed (a member has no recorded signer, a `descriptor_key` no longer
    /// parses, or the federation parameters are invalid). Used by the
    /// lineage-scoped `BtcFederatedWallet` reconstruction.
    #[error("federation `{id}`: cannot reconstruct federation from members: {reason}")]
    ReconstructFederation {
        /// Federation (version) id.
        id: Uuid,
        /// Human-readable cause.
        reason: String,
    },

    /// Couldn't connect a freshly-emitted block to the wallet's local chain.
    /// This usually means bitcoind reorged below what we last persisted.
    #[error("federation `{id}`: applying block at height {height} failed: {source}")]
    ApplyBlock {
        /// Federation id.
        id: Uuid,
        /// The block height we tried to apply.
        height: u32,
        /// The underlying BDK error.
        #[source]
        source: bdk_wallet::chain::local_chain::ApplyHeaderError,
    },

    /// Bitcoin Core RPC error (mempool / `next_block` / etc.).
    #[error("bitcoind RPC error: {0}")]
    Rpc(#[from] bitcoincore_rpc::Error),

    /// Seeding a fresh federation wallet's birthday checkpoint at the node tip
    /// failed (so its first sync wouldn't have to walk the chain from genesis).
    #[error("failed to seed fresh wallet birthday at node tip: {0}")]
    SeedBirthday(String),

    /// Failed to construct the JSON-RPC client itself (only happens for
    /// cookie auth, which we don't use, but pass through anyway).
    #[error("failed to construct bitcoind RPC client: {0}")]
    RpcClientInit(bitcoincore_rpc::Error),

    /// Failed to JSON-encode the merged changeset for storage.
    #[error("failed to serialize wallet changeset: {0}")]
    EncodeChangeSet(#[source] serde_json::Error),

    /// Database error.
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// User-supplied address didn't parse / didn't match the federation's
    /// network. Surfaced as a 400 by the handler.
    #[error("address `{addr}` is not a valid `{network}` address: {reason}")]
    BadAddress {
        /// Raw input.
        addr: String,
        /// Expected network.
        network: Network,
        /// Human-readable parse reason.
        reason: String,
    },

    /// BDK rejected a [`bitcoin::FeeRate`] constructed from the user-supplied
    /// sat/vB value (e.g. zero, or > `u64::MAX/4` ceiling).
    #[error("invalid fee rate `{sat_per_vb}` sat/vB")]
    BadFeeRate {
        /// Raw input from the form.
        sat_per_vb: u64,
    },

    /// `Wallet::build_tx().finish()` failed (no spendable UTXOs, fee floor
    /// not met, etc.). Surfaced as a 400 by the handler.
    #[error("transaction construction failed: {0}")]
    CreateTx(String),

    /// PSBT base64 didn't parse. Surfaced as a 400.
    #[error("invalid PSBT base64: {0}")]
    BadPsbt(String),

    /// PSBT `combine` rejected a partial — e.g. txids didn't match.
    #[error("failed to merge partial PSBT into base: {0}")]
    MergePsbt(String),

    /// `Wallet::finalize_psbt` returned an error. Surfaced as a 400. Carries
    /// the BDK signer error's rendered message (stringified in
    /// [`emvault::core::psbt::finalize_and_extract`]).
    #[error("PSBT finalisation error: {0}")]
    Finalize(String),

    /// `Wallet::finalize_psbt` returned `Ok(false)` — finalize hit no error
    /// but couldn't satisfy every input (typically: threshold not yet met).
    #[error("PSBT cannot yet be finalized: missing signatures")]
    NotEnoughSignatures,

    /// `Psbt::extract_tx` failed (only happens for malformed PSBTs).
    #[error("failed to extract transaction: {0}")]
    ExtractTx(String),

    /// `sendrawtransaction` rejected the broadcast. Surfaced as 502.
    #[error("bitcoind rejected broadcast: {0}")]
    BroadcastRejected(String),

    /// The Trezor sign-request builder needed a cosigner xpub but couldn't
    /// match any of the PSBT's input `bip32_derivation` entries.
    #[error("no input bip32_derivation entry matches cosigner fingerprint `{0}`")]
    UnknownCosigner(String),

    /// A cosigner row's xpub failed to parse. Indicates DB corruption (the
    /// onboarding flow enforces validity at insert time).
    #[error("malformed cosigner xpub for signer `{id}`: {source}")]
    BadCosignerXpub {
        /// The offending signer id.
        id: Uuid,
        /// Underlying parse error.
        #[source]
        source: bitcoin::bip32::Error,
    },

    /// The cosigner's stored master fingerprint isn't a 4-byte hex string.
    #[error("malformed cosigner fingerprint for signer `{id}`: {source}")]
    BadCosignerFingerprint {
        /// The offending signer id.
        id: Uuid,
        /// Underlying parse error.
        #[source]
        source: bitcoin::hex::HexToArrayError,
    },

    /// Trezor returned a signature bytestring we couldn't parse as DER.
    #[error("failed to parse Trezor signature for input {input_index}: {reason}")]
    BadTrezorSignature {
        /// PSBT input the signature targets.
        input_index: usize,
        /// Human-readable parse reason.
        reason: String,
    },
}

impl WalletError {
    /// Map a core [`ChainSyncError`] back into the federation-tagged
    /// `WalletError` variants this app surfaces.
    fn from_chain_sync(id: Uuid, err: ChainSyncError) -> Self {
        match err {
            ChainSyncError::Rpc(source) => Self::Rpc(source),
            ChainSyncError::ApplyBlock { height, source } => {
                Self::ApplyBlock { id, height, source }
            }
        }
    }

    /// Map a core [`InitWalletError`] back into the federation-tagged
    /// `WalletError` variants this app surfaces.
    fn from_init_wallet(id: Uuid, err: InitWalletError) -> Self {
        match err {
            InitWalletError::Decode(source) => Self::DecodeChangeSet { id, source },
            InitWalletError::Load(source) => Self::LoadWallet { id, source },
            InitWalletError::EmptyChangeSet => Self::EmptyChangeSet(id),
            InitWalletError::Create(source) => Self::CreateWallet { id, source },
        }
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Cache + factory for [`FederationWallet`]s.
///
/// One [`WalletManager`] is built at startup and shared across handlers via
/// `Arc<AppState>`. It owns a single `bitcoincore_rpc::Client` (the client is
/// internally a thin wrapper around a JSON-RPC connection pool, so sharing it
/// is fine) and lazily instantiates per-federation [`FederationWallet`]s on
/// first access.
pub struct WalletManager {
    pool: PgPool,
    rpc: Arc<RpcClient>,
    cache: AsyncMutex<HashMap<Uuid, Arc<FederationWallet>>>,
}

impl WalletManager {
    /// Construct the manager using the connection details from [`AppConfig`].
    ///
    /// # Errors
    /// Returns [`WalletError::RpcClientInit`] if the RPC client setup fails
    /// (only realistic when using cookie auth, which we don't).
    pub fn new(pool: PgPool, config: &AppConfig) -> Result<Self, WalletError> {
        let auth = Auth::UserPass(
            config.bitcoin_rpc_user.clone(),
            config.bitcoin_rpc_password.clone(),
        );
        let rpc =
            RpcClient::new(&config.bitcoin_rpc_url, auth).map_err(WalletError::RpcClientInit)?;
        Ok(Self {
            pool,
            rpc: Arc::new(rpc),
            cache: AsyncMutex::new(HashMap::new()),
        })
    }

    /// Look up (or lazily instantiate) the wallet for federation `id`.
    ///
    /// On cache miss this reads the federation row, deserializes the stored
    /// `bdk_changeset` (or creates a fresh wallet from the descriptor if
    /// nothing has ever been persisted), wraps it in a [`FederationWallet`],
    /// and inserts it into the cache.
    ///
    /// # Errors
    /// See [`WalletError`].
    pub async fn load_or_init(
        &self,
        federation_id: Uuid,
    ) -> Result<Arc<FederationWallet>, WalletError> {
        {
            let cache = self.cache.lock().await;
            if let Some(fw) = cache.get(&federation_id) {
                return Ok(fw.clone());
            }
        }

        let row = db::find_federation_by_id(&self.pool, federation_id)
            .await?
            .ok_or(WalletError::NotFound(federation_id))?;
        let network = Network::from_str(&row.network).map_err(|_| WalletError::BadNetwork {
            id: federation_id,
            network: row.network.clone(),
        })?;

        // Init-or-load BDK construction lives in `emvault::core::chain_sync`
        // (E3b). On the fresh path core leaves the staged changeset intact so
        // we persist the initial changeset here, exactly as before.
        let loaded =
            chain_sync::init_or_load_wallet(network, row.descriptor.clone(), row.bdk_changeset)
                .map_err(|e| WalletError::from_init_wallet(federation_id, e))?;
        let mut wallet = loaded.wallet;
        let mut initial_changeset = if loaded.fresh {
            tracing::info!(federation_id = %federation_id, %network, "initializing fresh BDK wallet for federation");
            ChangeSet::default()
        } else {
            tracing::debug!(federation_id = %federation_id, "loading wallet from persisted changeset");
            loaded.changeset
        };

        // Birthday: a federation wallet still parked at genesis (a brand-new
        // federation, or one whose first sync never completed) can hold no funds
        // that predate "now", so jump its checkpoint to the current node tip.
        // Otherwise the first `emitter_sync` starts from height 0 and walks the
        // *entire* chain over RPC — a handful of blocks on regtest, but ~hundreds
        // of thousands on signet/mainnet (an effective hang). `height() == 0`
        // means the wallet has tracked nothing, so this is always safe; this also
        // heals federations created before this fix. Inserting onto the existing
        // genesis checkpoint keeps the chain connected (no `CannotConnect`).
        if loaded.fresh || wallet.latest_checkpoint().height() == 0 {
            let count = self.rpc.get_block_count().map_err(WalletError::Rpc)?;
            if count > 0 {
                let hash = self.rpc.get_block_hash(count).map_err(WalletError::Rpc)?;
                let height = u32::try_from(count).unwrap_or(u32::MAX);
                let birthday = wallet.latest_checkpoint().insert(BlockId { height, hash });
                wallet
                    .apply_update(Update {
                        chain: Some(birthday),
                        ..Default::default()
                    })
                    .map_err(|e| WalletError::SeedBirthday(e.to_string()))?;
                tracing::info!(
                    federation_id = %federation_id, birthday = height,
                    "seeded wallet birthday at node tip"
                );
            }
        }

        // Persist any staged delta — the fresh wallet's initial changeset and/or
        // the birthday checkpoint — and fold it into the aggregate we hand off.
        if let Some(delta) = wallet.take_staged() {
            initial_changeset.merge(delta);
            let json =
                serde_json::to_value(&initial_changeset).map_err(WalletError::EncodeChangeSet)?;
            let tip = wallet.latest_checkpoint().height();
            db::update_federation_changeset(
                &self.pool,
                federation_id,
                &json,
                i32::try_from(tip).unwrap_or(i32::MAX),
            )
            .await?;
        }

        let fw = Arc::new(FederationWallet {
            id: federation_id,
            network,
            inner: AsyncMutex::new(wallet),
            aggregate: AsyncMutex::new(initial_changeset),
            pool: self.pool.clone(),
            rpc: self.rpc.clone(),
        });

        let mut cache = self.cache.lock().await;
        Ok(cache.entry(federation_id).or_insert(fw).clone())
    }

    /// Sync **every** version's wallet in a lineage (sync fan-out), so historic
    /// (superseded) versions pick up late inflows for relay detection — not just
    /// the current version. Returns the per-version `(federation_id, summary)`.
    ///
    /// This is Phase 2's deferred fan-out, realised now that migrations create
    /// multi-version lineages (design §6.3).
    ///
    /// # Errors
    /// See [`WalletError`]; propagates the first sync/RPC/persistence error.
    #[allow(dead_code)] // wired into the lineage view / relay flow
    pub async fn sync_lineage(
        &self,
        lineage_id: Uuid,
    ) -> Result<Vec<(Uuid, SyncSummary)>, WalletError> {
        let versions = db::load_lineage_versions(&self.pool, lineage_id).await?;
        let mut summaries = Vec::with_capacity(versions.len());
        for row in &versions {
            let fw = self.load_or_init(row.id).await?;
            let summary = fw.sync().await?;
            summaries.push((row.id, summary));
        }
        Ok(summaries)
    }

    /// **Rescan** a federation's wallet from `from_height` (0 = genesis) to the
    /// node tip, rebuilding it **from the descriptor alone** — ignoring any
    /// persisted changeset and the birthday-bootstrap in [`Self::load_or_init`] —
    /// then **persist** the resulting changeset so the running app picks up any
    /// recovered funds, and evict the cache so the next load reflects them.
    ///
    /// This is the recovery counterpart to the birthday optimization: use it
    /// after a dev DB reset, or to rescue coins sent to a federation's addresses
    /// before it was tracked. A full from-zero scan on signet/mainnet is slow
    /// (one `getblock` RPC per block) — pass a `from_height` near the deposit if
    /// you know it.
    ///
    /// `RESCAN_GAP` external + internal addresses are pre-revealed so the scan
    /// can match deposits; raise it if funds were sent past that index.
    ///
    /// `on_progress(current_height, chain_tip)` is invoked periodically as the
    /// scan walks blocks (every [`PROGRESS_EVERY`] blocks, plus once at the end),
    /// so callers can render a live progress line. Pass `|_, _| {}` for none.
    ///
    /// # Errors
    /// See [`WalletError`] — `NotFound`, RPC, BDK, and persistence errors.
    pub async fn rescan<F: FnMut(u32, u32)>(
        &self,
        federation_id: Uuid,
        from_height: u32,
        mut on_progress: F,
    ) -> Result<RescanReport, WalletError> {
        let row = db::find_federation_by_id(&self.pool, federation_id)
            .await?
            .ok_or(WalletError::NotFound(federation_id))?;
        let network = Network::from_str(&row.network).map_err(|_| WalletError::BadNetwork {
            id: federation_id,
            network: row.network.clone(),
        })?;

        // Fresh wallet from the descriptor ALONE (changeset = None) — we want a
        // clean scan, not the birthday-truncated persisted state.
        let loaded = chain_sync::init_or_load_wallet(network, row.descriptor.clone(), None)
            .map_err(|e| WalletError::from_init_wallet(federation_id, e))?;
        let mut wallet = loaded.wallet;

        // Pre-reveal a window on both keychains: BDK only matches *revealed*
        // scripts during a sync, so unrevealed addresses would be invisible.
        let _ = wallet
            .reveal_addresses_to(KeychainKind::External, RESCAN_GAP - 1)
            .count();
        let _ = wallet
            .reveal_addresses_to(KeychainKind::Internal, RESCAN_GAP - 1)
            .count();

        // Start the emitter at `from_height` (0 = leave at genesis). Inserting
        // onto the existing genesis checkpoint keeps the chain connected.
        if from_height > 0 {
            let hash = self
                .rpc
                .get_block_hash(u64::from(from_height))
                .map_err(WalletError::Rpc)?;
            let cp = wallet.latest_checkpoint().insert(BlockId {
                height: from_height,
                hash,
            });
            wallet
                .apply_update(Update {
                    chain: Some(cp),
                    ..Default::default()
                })
                .map_err(|e| WalletError::SeedBirthday(e.to_string()))?;
        }

        // Walk every block from the checkpoint to the tip, driving the emitter
        // directly so we can report per-block progress. (`chain_sync::emitter_sync`
        // does the same walk but exposes no hook for a live progress line.)
        let cp = wallet.latest_checkpoint();
        let start = cp.height();
        let chain_tip = u32::try_from(self.rpc.get_block_count().map_err(WalletError::Rpc)?)
            .unwrap_or(u32::MAX);
        let mut emitter = Emitter::new(&*self.rpc, cp, start, NO_EXPECTED_MEMPOOL_TXS);
        let mut blocks_scanned: u32 = 0;
        while let Some(event) = emitter.next_block().map_err(WalletError::Rpc)? {
            let height = event.block_height();
            let connected_to = event.connected_to();
            wallet
                .apply_block_connected_to(&event.block, height, connected_to)
                .map_err(|source| WalletError::ApplyBlock {
                    id: federation_id,
                    height,
                    source,
                })?;
            blocks_scanned += 1;
            if blocks_scanned.is_multiple_of(PROGRESS_EVERY) {
                on_progress(height, chain_tip);
            }
        }
        let mempool = emitter.mempool().map_err(WalletError::Rpc)?;
        wallet.apply_unconfirmed_txs(mempool.update);
        let tip_height = wallet.latest_checkpoint().height();
        // Final tick so the progress line lands at 100% regardless of throttling.
        on_progress(tip_height, chain_tip.max(tip_height));

        // Report the funded addresses on both keychains.
        let balance = wallet.balance();
        let utxo_count = wallet.list_output().filter(|o| !o.is_spent).count();
        let receive_addresses = funded_addresses(&wallet, KeychainKind::External);
        let change_addresses = funded_addresses(&wallet, KeychainKind::Internal);

        // Persist the full rescanned changeset (overwrite) so the app sees it.
        if let Some(delta) = wallet.take_staged() {
            let json = serde_json::to_value(&delta).map_err(WalletError::EncodeChangeSet)?;
            db::update_federation_changeset(
                &self.pool,
                federation_id,
                &json,
                i32::try_from(tip_height).unwrap_or(i32::MAX),
            )
            .await?;
        }
        // Drop any cached (birthday-truncated) handle so the next load reloads
        // the rescanned, now-persisted state.
        self.cache.lock().await.remove(&federation_id);

        Ok(RescanReport {
            federation_id,
            label: row.label,
            from_height,
            tip_height,
            blocks_scanned,
            balance,
            utxo_count,
            receive_addresses,
            change_addresses,
        })
    }

    /// [`Self::rescan`] **every** federation version in the database (across all
    /// lineages), from `from_height`. Each federation is rescanned independently
    /// and its result collected, so one failure doesn't abort the rest — the
    /// returned `Vec` pairs each id with its `Ok(report)` or `Err`.
    ///
    /// # Errors
    /// Only the initial listing query can fail the whole call; per-federation
    /// failures are captured in the returned vector.
    pub async fn rescan_all(
        &self,
        from_height: u32,
    ) -> Result<Vec<(Uuid, Result<RescanReport, WalletError>)>, WalletError> {
        let ids = db::list_all_federation_ids(&self.pool).await?;
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            let res = self.rescan(id, from_height, |_, _| {}).await;
            results.push((id, res));
        }
        Ok(results)
    }

    /// Every federation id in the database (all versions, all lineages), ordered
    /// by creation. Lets callers drive [`Self::rescan`] per-federation with their
    /// own progress reporting instead of the batch [`Self::rescan_all`].
    ///
    /// # Errors
    /// Propagates the listing query error.
    pub async fn federation_ids(&self) -> Result<Vec<Uuid>, WalletError> {
        Ok(db::list_all_federation_ids(&self.pool).await?)
    }
}

/// Walk a wallet's outputs and return the funded addresses on `keychain`,
/// aggregated by derivation index into `(received, unspent)` totals.
fn funded_addresses(wallet: &Wallet, keychain: KeychainKind) -> Vec<RevealedAddress> {
    let mut seen = std::collections::BTreeMap::<u32, (Amount, Amount)>::new();
    for utxo in wallet.list_output() {
        if let Some((kc, idx)) = wallet.derivation_of_spk(utxo.txout.script_pubkey.clone())
            && kc == keychain
        {
            let entry = seen.entry(idx).or_insert((Amount::ZERO, Amount::ZERO));
            entry.0 += utxo.txout.value;
            if !utxo.is_spent {
                entry.1 += utxo.txout.value;
            }
        }
    }
    seen.into_iter()
        .map(|(index, (received, unspent))| {
            let info = wallet.peek_address(keychain, index);
            RevealedAddress {
                index,
                keychain: info.keychain,
                address: info.address.to_string(),
                received,
                unspent,
            }
        })
        .collect()
}

/// Number of addresses pre-revealed per keychain before a [`WalletManager::rescan`].
/// BDK only matches revealed scripts during a sync; raise this if a deposit
/// landed past this gap.
pub const RESCAN_GAP: u32 = 100;

/// How often (in blocks scanned) [`WalletManager::rescan`] fires its progress
/// callback. Coarse enough to keep RPC-bound scans from spamming the callback,
/// fine enough to feel live.
pub const PROGRESS_EVERY: u32 = 50;

/// Result of a [`WalletManager::rescan`].
#[derive(Debug, Clone)]
pub struct RescanReport {
    /// The federation that was rescanned.
    pub federation_id: Uuid,
    /// Its human-readable label.
    pub label: String,
    /// Height the scan started from (0 = genesis).
    pub from_height: u32,
    /// Node tip the scan reached.
    pub tip_height: u32,
    /// Blocks pulled in this scan.
    pub blocks_scanned: u32,
    /// Wallet balance after the scan.
    pub balance: bdk_wallet::Balance,
    /// Count of unspent outputs found.
    pub utxo_count: usize,
    /// Funded external (receive) addresses.
    pub receive_addresses: Vec<RevealedAddress>,
    /// Funded internal (change) addresses.
    pub change_addresses: Vec<RevealedAddress>,
}

/// Derive a federation descriptor's first external address **without**
/// persisting a wallet — used to route a migration sweep to a not-yet-persisted
/// successor version (so the sweep can be built and validated before the
/// pending version is committed).
///
/// # Errors
/// [`WalletError::CreateWallet`] if the descriptor is rejected.
pub fn first_external_address(network: Network, descriptor: &str) -> Result<Address, WalletError> {
    let wallet = Wallet::create_from_two_path_descriptor(descriptor.to_owned())
        .network(network)
        .create_wallet_no_persist()
        .map_err(|source| WalletError::CreateWallet {
            id: Uuid::nil(),
            source: Box::new(source),
        })?;
    Ok(wallet.peek_address(KeychainKind::External, 0).address)
}

// ---------------------------------------------------------------------------
// Per-federation wallet
// ---------------------------------------------------------------------------

/// A live BDK wallet bound to a single federation.
///
/// Use [`WalletManager::load_or_init`] to get one; never construct directly.
pub struct FederationWallet {
    /// Federation id (matches `federations.id`).
    pub id: Uuid,
    /// The wallet's network (cached from the row to avoid re-parsing on
    /// every request).
    pub network: Network,
    inner: AsyncMutex<Wallet>,
    /// Aggregate changeset persisted on the row. The wallet's *staged*
    /// changeset is the diff since the last sync; we merge each diff into
    /// this aggregate, then JSON-encode it back to the row.
    aggregate: AsyncMutex<ChangeSet>,
    pool: PgPool,
    rpc: Arc<RpcClient>,
}

/// Snapshot returned from [`FederationWallet::sync`].
#[derive(Debug, Clone, Copy)]
pub struct SyncSummary {
    /// Current chain tip the wallet is aware of, in blocks.
    pub tip_height: u32,
    /// Number of blocks pulled in this sync pass.
    pub new_blocks: u32,
    /// Number of mempool transactions ingested in this sync pass.
    pub new_mempool_txs: u32,
}

impl FederationWallet {
    /// Drive [`emvault::core::chain_sync::emitter_sync`] until the wallet
    /// matches bitcoind's tip, apply mempool transactions, and persist the
    /// resulting changeset.
    ///
    /// Cheap (and idempotent) when the wallet is already in sync — that's
    /// the common case after the first request for a given federation.
    ///
    /// # Errors
    /// See [`WalletError`]. RPC and DB errors propagate verbatim.
    pub async fn sync(&self) -> Result<SyncSummary, WalletError> {
        let (summary, delta) = {
            let mut wallet = self.inner.lock().await;
            // Pure-BDK emitter drive lives in `emvault::core::chain_sync` (E3b);
            // persistence (changeset merge + DB write) stays here.
            let result = chain_sync::emitter_sync(&mut wallet, &*self.rpc)
                .map_err(|e| WalletError::from_chain_sync(self.id, e))?;
            // Drop the BDK guard before falling out of the block so no
            // other request holds the wallet mutex across the DB await
            // below.
            drop(wallet);
            (
                SyncSummary {
                    tip_height: result.tip_height,
                    new_blocks: result.blocks_synced,
                    new_mempool_txs: result.new_mempool_txs,
                },
                result.changeset,
            )
        };

        if let Some(delta) = delta {
            let mut agg = self.aggregate.lock().await;
            agg.merge(delta);
            let json = serde_json::to_value(&*agg).map_err(WalletError::EncodeChangeSet)?;
            drop(agg);
            db::update_federation_changeset(
                &self.pool,
                self.id,
                &json,
                i32::try_from(summary.tip_height).unwrap_or(i32::MAX),
            )
            .await?;
        } else {
            db::update_federation_tip_only(
                &self.pool,
                self.id,
                i32::try_from(summary.tip_height).unwrap_or(i32::MAX),
            )
            .await?;
        }

        Ok(summary)
    }

    /// Reveal external-keychain addresses 0..n (idempotent), and return
    /// every address from 0 to `n - 1` as a view-model.
    ///
    /// We always reveal up to `REVEAL_COUNT - 1` so the wallet's address
    /// index doesn't fall behind chain scans — BDK only matches outputs
    /// against revealed scripts.
    ///
    /// # Errors
    /// Propagates persistence errors raised while writing the
    /// reveal-induced changeset to the DB.
    pub async fn reveal_addresses(
        &self,
        target_count: u32,
    ) -> Result<Vec<RevealedAddress>, WalletError> {
        if target_count == 0 {
            return Ok(Vec::new());
        }
        let target_index = target_count - 1;
        let (results, delta, tip) = {
            let mut wallet = self.inner.lock().await;
            // Force the reveal even if some indexes were already revealed.
            // `reveal_addresses_to` is idempotent and returns only the newly
            // revealed addresses; we then `peek_address` 0..target so the
            // caller always sees a contiguous list.
            let _newly: Vec<AddressInfo> = wallet
                .reveal_addresses_to(KeychainKind::External, target_index)
                .collect();

            let results: Vec<RevealedAddress> = (0..target_count)
                .map(|index| {
                    let info = wallet.peek_address(KeychainKind::External, index);
                    let spk = info.address.script_pubkey();
                    let mut received = Amount::ZERO;
                    let mut unspent = Amount::ZERO;
                    for utxo in wallet.list_output() {
                        if utxo.txout.script_pubkey == spk {
                            received += utxo.txout.value;
                            if !utxo.is_spent {
                                unspent += utxo.txout.value;
                            }
                        }
                    }
                    RevealedAddress {
                        index,
                        keychain: info.keychain,
                        address: info.address.to_string(),
                        received,
                        unspent,
                    }
                })
                .collect();

            let delta = wallet.take_staged();
            let tip = wallet.latest_checkpoint().height();
            // Drop the BDK guard before any DB await below.
            drop(wallet);
            (results, delta, tip)
        };

        if let Some(delta) = delta {
            let mut agg = self.aggregate.lock().await;
            agg.merge(delta);
            let json = serde_json::to_value(&*agg).map_err(WalletError::EncodeChangeSet)?;
            drop(agg);
            db::update_federation_changeset(
                &self.pool,
                self.id,
                &json,
                i32::try_from(tip).unwrap_or(i32::MAX),
            )
            .await?;
        }

        Ok(results)
    }

    /// List `Internal` (change) keychain addresses that have ever received
    /// funds, with their received/unspent totals. Read-only — change addresses
    /// are revealed by BDK as a side effect of building spends, so this never
    /// stages a changeset.
    pub async fn change_addresses(&self) -> Vec<RevealedAddress> {
        let wallet = self.inner.lock().await;
        // Aggregate by derivation index so multiple UTXOs at the same change
        // address collapse into one row: index -> (received, unspent).
        let mut seen = std::collections::BTreeMap::<u32, (Amount, Amount)>::new();
        for utxo in wallet.list_output() {
            if let Some((KeychainKind::Internal, idx)) =
                wallet.derivation_of_spk(utxo.txout.script_pubkey.clone())
            {
                let entry = seen.entry(idx).or_insert((Amount::ZERO, Amount::ZERO));
                entry.0 += utxo.txout.value;
                if !utxo.is_spent {
                    entry.1 += utxo.txout.value;
                }
            }
        }
        seen.into_iter()
            .map(|(index, (received, unspent))| {
                let info = wallet.peek_address(KeychainKind::Internal, index);
                RevealedAddress {
                    index,
                    keychain: info.keychain,
                    address: info.address.to_string(),
                    received,
                    unspent,
                }
            })
            .collect()
    }

    /// Resolve an externally-supplied address string into a checked
    /// [`Address`], rejecting anything that doesn't match this wallet's
    /// network. Used to make URL params safe to feed into BDK queries.
    pub fn parse_address(&self, raw: &str) -> Result<Address, WalletError> {
        let unchecked: Address<NetworkUnchecked> =
            raw.parse()
                .map_err(|e: bitcoin::address::ParseError| WalletError::BadAddress {
                    addr: raw.to_string(),
                    network: self.network,
                    reason: e.to_string(),
                })?;
        unchecked
            .require_network(self.network)
            .map_err(|e| WalletError::BadAddress {
                addr: raw.to_string(),
                network: self.network,
                reason: e.to_string(),
            })
    }

    /// Look up the keychain + derivation index BDK has assigned to the
    /// given address, if any. `None` for addresses not owned by the wallet.
    pub async fn locate_address(&self, address: &Address) -> Option<(KeychainKind, u32)> {
        let wallet = self.inner.lock().await;
        wallet.derivation_of_spk(address.script_pubkey())
    }

    /// Pull every wallet transaction that pays into `address`, plus a
    /// flag indicating whether the receiving outpoint has been spent.
    ///
    /// Returns oldest-first by chain position (confirmed first by
    /// ascending height, then unconfirmed last-seen ascending).
    pub async fn address_history(&self, address: &Address) -> Result<AddressActivity, WalletError> {
        let target_spk: ScriptBuf = address.script_pubkey();

        let wallet = self.inner.lock().await;
        let tip_height = wallet.latest_checkpoint().height();

        // Build a quick map outpoint -> is_spent over wallet outputs.
        // `list_output` covers both spent and unspent.
        let spent_status: std::collections::HashMap<_, _> = wallet
            .list_output()
            .map(|o| (o.outpoint, o.is_spent))
            .collect();

        let mut receipts: Vec<AddressReceipt> = Vec::new();
        let mut total_received = Amount::ZERO;
        let mut unspent = Amount::ZERO;

        for wtx in wallet.transactions() {
            let txid = wtx.tx_node.txid;
            let tx = wtx.tx_node.tx.as_ref();
            for (vout, txout) in tx.output.iter().enumerate() {
                if txout.script_pubkey != target_spk {
                    continue;
                }
                // A Bitcoin transaction can have at most 2^32 - 1 outputs;
                // `usize::try_into::<u32>` only fails on 32-bit platforms
                // with > 4 GiB indices, which is unreachable here.
                let vout32 = u32::try_from(vout).unwrap_or(u32::MAX);
                let outpoint = bitcoin::OutPoint::new(txid, vout32);
                let is_spent = spent_status.get(&outpoint).copied().unwrap_or(false);
                let (confirmation_height, confirmations) = match wtx.chain_position {
                    ChainPosition::Confirmed { anchor, .. } => {
                        let h = anchor.block_id.height;
                        let confs = tip_height.saturating_sub(h).saturating_add(1);
                        (Some(h), confs)
                    }
                    ChainPosition::Unconfirmed { .. } => (None, 0),
                };
                total_received += txout.value;
                if !is_spent {
                    unspent += txout.value;
                }
                receipts.push(AddressReceipt {
                    txid,
                    vout: vout32,
                    amount: txout.value,
                    confirmation_height,
                    confirmations,
                    is_spent,
                });
            }
        }
        // Done reading from the wallet; release the lock before sorting
        // and constructing the return value so other requests can proceed.
        drop(wallet);

        receipts.sort_by_key(|r| {
            // Confirmed first ordered by height; unconfirmed last.
            r.confirmation_height.unwrap_or(u32::MAX)
        });

        Ok(AddressActivity {
            tip_height,
            total_received,
            unspent,
            receipts,
        })
    }

    /// Return the wallet's current local-chain tip height.
    pub async fn tip_height(&self) -> u32 {
        self.inner.lock().await.latest_checkpoint().height()
    }

    /// Snapshot the wallet's current balance (confirmed + pending +
    /// immature). Returns BDK's [`bdk_wallet::Balance`] verbatim so callers
    /// can render whichever fields they want.
    pub async fn balance(&self) -> bdk_wallet::Balance {
        self.inner.lock().await.balance()
    }

    // -----------------------------------------------------------------
    // Proposal construction
    // -----------------------------------------------------------------

    /// Build an unsigned PSBT for a single-recipient send.
    ///
    /// Drives `Wallet::build_tx()` with BDK's default coin-selection
    /// algorithm, collects the structural view-models (`proposal_json` and
    /// `coin_selection_json`) the UI will render, then persists any staged
    /// changeset delta (which includes the newly-revealed change address)
    /// so subsequent syncs see the change script as ours.
    ///
    /// # Errors
    /// - [`WalletError::BadFeeRate`] if `fee_rate_sat_vb` is zero.
    /// - [`WalletError::CreateTx`] if BDK can't satisfy the spec (no
    ///   spendable UTXOs, dust output, etc.).
    /// - Persistence errors propagate as-is.
    pub async fn build_proposal(
        &self,
        recipient: &Address,
        amount: Amount,
        fee_rate_sat_vb: u64,
    ) -> Result<BuiltProposal, WalletError> {
        let fee_rate =
            FeeRate::from_sat_per_vb(fee_rate_sat_vb).ok_or(WalletError::BadFeeRate {
                sat_per_vb: fee_rate_sat_vb,
            })?;

        let (psbt, delta, tip) = {
            let mut wallet = self.inner.lock().await;
            let psbt =
                core_psbt::build_spend(&mut wallet, recipient.script_pubkey(), amount, fee_rate)
                    .map_err(|e| match e {
                        PsbtError::BuildFailed(s) => WalletError::CreateTx(s),
                        other => WalletError::CreateTx(other.to_string()),
                    })?;

            let delta = wallet.take_staged();
            let tip = wallet.latest_checkpoint().height();
            // Drop the BDK guard before awaiting on the changeset aggregate
            // or DB below.
            drop(wallet);
            (psbt, delta, tip)
        };

        // Persist the reveal-induced changeset (BDK may have revealed a
        // fresh internal address for change). Failing to persist here would
        // mean the next reload of the wallet wouldn't recognise the change
        // script as ours.
        if let Some(delta) = delta {
            let mut agg = self.aggregate.lock().await;
            agg.merge(delta);
            let json = serde_json::to_value(&*agg).map_err(WalletError::EncodeChangeSet)?;
            drop(agg);
            db::update_federation_changeset(
                &self.pool,
                self.id,
                &json,
                i32::try_from(tip).unwrap_or(i32::MAX),
            )
            .await?;
        }

        let (proposal_json, coin_selection_json) =
            self.proposal_view_models(&psbt, recipient).await;

        let psbt_b64 = psbt.to_string();

        Ok(BuiltProposal {
            psbt_b64,
            proposal_json,
            coin_selection_json,
        })
    }

    /// Reveal and return this wallet's first external (receive) address. Used to
    /// route a migration sweep into a successor version's wallet so it later
    /// recognises the inflow as its own once synced.
    ///
    /// # Errors
    /// Propagates persistence errors from writing the reveal-induced changeset.
    pub async fn reveal_first_external(&self) -> Result<Address, WalletError> {
        let (address, delta, tip) = {
            let mut wallet = self.inner.lock().await;
            let _ = wallet
                .reveal_addresses_to(KeychainKind::External, 0)
                .count();
            let address = wallet.peek_address(KeychainKind::External, 0).address;
            let delta = wallet.take_staged();
            let tip = wallet.latest_checkpoint().height();
            drop(wallet);
            (address, delta, tip)
        };
        self.persist_delta(delta, tip).await?;
        Ok(address)
    }

    /// `true` if `address` belongs to this wallet's descriptor (any keychain).
    /// Used to confirm a send destination is one of the current federation's own
    /// addresses (the restriction that old-version signers may only move funds
    /// forward to the current federation).
    pub async fn is_address_mine(&self, address: &Address) -> bool {
        let wallet = self.inner.lock().await;
        wallet.is_mine(address.script_pubkey())
    }

    /// Build the migration sweep PSBT: **drain** every UTXO of this (current)
    /// version to `destination` (the successor version's address), with the fee
    /// paid from the swept funds (treasury-pays). One input set → one output, no
    /// change. Mirrors [`build_proposal`](Self::build_proposal) but drains, and
    /// is the executor for the single-account `AccountForAccountSweep` plan.
    ///
    /// # Errors
    /// - [`WalletError::BadFeeRate`] if `fee_rate_sat_vb` is zero.
    /// - [`WalletError::CreateTx`] if BDK can't satisfy the drain (notably no
    ///   spendable UTXOs — an unfunded federation has nothing to sweep).
    /// - Persistence errors propagate as-is.
    pub async fn build_migration_tx(
        &self,
        destination: &Address,
        fee_rate_sat_vb: u64,
    ) -> Result<BuiltProposal, WalletError> {
        let fee_rate =
            FeeRate::from_sat_per_vb(fee_rate_sat_vb).ok_or(WalletError::BadFeeRate {
                sat_per_vb: fee_rate_sat_vb,
            })?;

        let (psbt, delta, tip) = {
            let mut wallet = self.inner.lock().await;
            let psbt = {
                let mut builder = wallet.build_tx();
                builder
                    .drain_wallet()
                    .drain_to(destination.script_pubkey())
                    .fee_rate(fee_rate);
                builder
                    .finish()
                    .map_err(|e| WalletError::CreateTx(e.to_string()))?
            };
            let delta = wallet.take_staged();
            let tip = wallet.latest_checkpoint().height();
            drop(wallet);
            (psbt, delta, tip)
        };

        self.persist_delta(delta, tip).await?;

        let (proposal_json, coin_selection_json) =
            self.proposal_view_models(&psbt, destination).await;
        Ok(BuiltProposal {
            psbt_b64: psbt.to_string(),
            proposal_json,
            coin_selection_json,
        })
    }

    /// Merge a staged changeset delta into the aggregate and persist it (no-op
    /// when `delta` is `None`). Shared by the address-reveal and sweep-build
    /// paths.
    async fn persist_delta(&self, delta: Option<ChangeSet>, tip: u32) -> Result<(), WalletError> {
        if let Some(delta) = delta {
            let mut agg = self.aggregate.lock().await;
            agg.merge(delta);
            let json = serde_json::to_value(&*agg).map_err(WalletError::EncodeChangeSet)?;
            drop(agg);
            db::update_federation_changeset(
                &self.pool,
                self.id,
                &json,
                i32::try_from(tip).unwrap_or(i32::MAX),
            )
            .await?;
        }
        Ok(())
    }

    /// Compute the cached `proposal_json` + `coin_selection_json` blobs that
    /// the UI renders. Walks each PSBT input via `Wallet::get_utxo` for amount
    /// and keychain info, and classifies each output via `Wallet::is_mine` to
    /// distinguish recipient from change.
    async fn proposal_view_models(
        &self,
        psbt: &Psbt,
        recipient: &Address,
    ) -> (serde_json::Value, serde_json::Value) {
        let wallet = self.inner.lock().await;

        let mut selected_inputs = Vec::with_capacity(psbt.unsigned_tx.input.len());
        let mut total_input_sat: u64 = 0;
        for txin in &psbt.unsigned_tx.input {
            let op = txin.previous_output;
            if let Some(utxo) = wallet.get_utxo(op) {
                let addr = Address::from_script(&utxo.txout.script_pubkey, self.network)
                    .map_or_else(
                        |_| utxo.txout.script_pubkey.to_hex_string(),
                        |a| a.to_string(),
                    );
                let amount_sat = utxo.txout.value.to_sat();
                total_input_sat = total_input_sat.saturating_add(amount_sat);
                selected_inputs.push(serde_json::json!({
                    "outpoint": format!("{}:{}", op.txid, op.vout),
                    "address": addr,
                    "amount_sat": amount_sat,
                    "keychain": match utxo.keychain {
                        KeychainKind::External => "external",
                        KeychainKind::Internal => "internal",
                    },
                    "derivation_index": utxo.derivation_index,
                }));
            } else {
                selected_inputs.push(serde_json::json!({
                    "outpoint": format!("{}:{}", op.txid, op.vout),
                    "address": serde_json::Value::Null,
                    "amount_sat": serde_json::Value::Null,
                    "keychain": serde_json::Value::Null,
                    "derivation_index": serde_json::Value::Null,
                }));
            }
        }

        let mut outputs = Vec::with_capacity(psbt.unsigned_tx.output.len());
        let mut total_output_sat: u64 = 0;
        let mut recipient_sat: u64 = 0;
        let mut change_sat: u64 = 0;
        let recipient_spk = recipient.script_pubkey();
        for txout in &psbt.unsigned_tx.output {
            let amount_sat = txout.value.to_sat();
            total_output_sat = total_output_sat.saturating_add(amount_sat);
            let is_mine = wallet.is_mine(txout.script_pubkey.clone());
            let is_recipient = txout.script_pubkey == recipient_spk;
            let addr = Address::from_script(&txout.script_pubkey, self.network)
                .map_or_else(|_| txout.script_pubkey.to_hex_string(), |a| a.to_string());
            let kind = if is_recipient {
                recipient_sat = recipient_sat.saturating_add(amount_sat);
                "recipient"
            } else if is_mine {
                change_sat = change_sat.saturating_add(amount_sat);
                "change"
            } else {
                "external"
            };
            outputs.push(serde_json::json!({
                "address": addr,
                "amount_sat": amount_sat,
                "kind": kind,
            }));
        }
        // Done querying the BDK wallet; release the lock before
        // constructing the JSON view-models so concurrent requests
        // aren't blocked on cheap serde work.
        drop(wallet);

        let fee_sat = total_input_sat.saturating_sub(total_output_sat);

        let proposal_json = serde_json::json!({
            "recipient": recipient.to_string(),
            "recipient_amount_sat": recipient_sat,
            "change_sat": change_sat,
            "total_output_sat": total_output_sat,
            "fee_sat": fee_sat,
            "input_count": selected_inputs.len(),
            "outputs": outputs.clone(),
        });

        let coin_selection_json = serde_json::json!({
            "selected": selected_inputs,
            "total_input_sat": total_input_sat,
            "outputs": outputs,
            "fee_sat": fee_sat,
        });

        (proposal_json, coin_selection_json)
    }

    // -----------------------------------------------------------------
    // Trezor sign request
    // -----------------------------------------------------------------

    /// Build the JSON payload that the browser hands to
    /// `TrezorConnect.signTransaction` for a P2WSH `sortedmulti` proposal.
    ///
    /// Per input:
    ///   - `script_type: "SPENDWITNESS"`
    ///   - `address_n`: BIP-32 path for the **signing Trezor**'s key on this
    ///     input (pulled from the PSBT's `bip32_derivation` map by master
    ///     fingerprint).
    ///   - `multisig.pubkeys[i]`: each cosigner's `HDNode` + the relative
    ///     derivation suffix (`[keychain, index]`), sorted lexicographically
    ///     by raw pubkey bytes at this derivation — i.e. matched to
    ///     `sortedmulti`'s on-chain script order.
    ///   - `multisig.signatures[i]`: all blank (`""`); Trezor populates its
    ///     slot.
    ///
    /// For each output, recipient outputs use `PAYTOADDRESS`; outputs whose
    /// script is one of our keychains (change) are emitted as
    /// `PAYTOWITNESS` + `multisig` (native P2WSH multisig — the firmware
    /// uses the presence of the `multisig` field to switch `PAYTOWITNESS`
    /// from P2WPKH to P2WSH internally), plus the signing-Trezor's
    /// relative derivation path so the firmware can verify it's our change.
    ///
    /// # Errors
    /// - [`WalletError::BadPsbt`] if `psbt_b64` doesn't parse.
    /// - [`WalletError::UnknownCosigner`] if the signing Trezor's
    ///   fingerprint isn't in the PSBT's `bip32_derivation` for some input.
    /// - [`WalletError::BadCosignerXpub`] /
    ///   [`WalletError::BadCosignerFingerprint`] on row decode errors.
    // The Trezor payload-builder is structurally one walk over PSBT inputs
    // + one walk over PSBT outputs + the refTxs fetch. Splitting it would
    // mean threading a half-dozen vectors through helper signatures.
    #[allow(clippy::too_many_lines)]
    pub async fn trezor_sign_request(
        &self,
        psbt_b64: &str,
        signing_fingerprint: &str,
        cosigners: &[SignerRow],
        threshold: usize,
    ) -> Result<TrezorSignRequest, WalletError> {
        let psbt = Psbt::from_str(psbt_b64).map_err(|e| WalletError::BadPsbt(e.to_string()))?;

        let signing_fp = Fingerprint::from_str(signing_fingerprint).map_err(|source| {
            WalletError::BadCosignerFingerprint {
                id: Uuid::nil(),
                source,
            }
        })?;

        let mut decoded: Vec<DecodedCosigner> = Vec::with_capacity(cosigners.len());
        for row in cosigners {
            let xpub = Xpub::from_str(&row.xpub)
                .map_err(|source| WalletError::BadCosignerXpub { id: row.id, source })?;
            let fp = Fingerprint::from_str(&row.fingerprint)
                .map_err(|source| WalletError::BadCosignerFingerprint { id: row.id, source })?;
            decoded.push(DecodedCosigner {
                xpub,
                fingerprint: fp,
            });
        }

        let secp = bitcoin::secp256k1::Secp256k1::verification_only();
        let threshold_u32 = u32::try_from(threshold).unwrap_or(u32::MAX);

        let mut inputs = Vec::with_capacity(psbt.unsigned_tx.input.len());
        let mut ref_txids: Vec<bitcoin::Txid> = Vec::new();
        let mut my_input_paths: Vec<DerivationPath> =
            Vec::with_capacity(psbt.unsigned_tx.input.len());

        for (txin, psbt_input) in psbt.unsigned_tx.input.iter().zip(psbt.inputs.iter()) {
            let op = txin.previous_output;
            ref_txids.push(op.txid);
            let amount = psbt_input
                .witness_utxo
                .as_ref()
                .map_or(0, |t| t.value.to_sat());

            // Locate the signing Trezor's full derivation path on this input.
            let mut signer_path: Option<DerivationPath> = None;
            for (fp, path) in psbt_input.bip32_derivation.values() {
                if *fp == signing_fp {
                    signer_path = Some(path.clone());
                    break;
                }
            }
            let signer_path =
                signer_path.ok_or_else(|| WalletError::UnknownCosigner(signing_fp.to_string()))?;
            my_input_paths.push(signer_path.clone());

            // The relative path inside the cosigner's xpub is the last two
            // components (keychain, index). For BIP-48 P2WSH this is `/0/N`
            // or `/1/N`.
            let signer_path_vec: Vec<ChildNumber> = signer_path.into();
            let relative: Vec<ChildNumber> = signer_path_vec
                .iter()
                .rev()
                .take(2)
                .rev()
                .copied()
                .collect();

            // Sort cosigners by the pubkey they derive at this `relative`
            // path — this matches `sortedmulti`'s on-chain script ordering.
            let mut entries: Vec<MultisigPubkeyEntry> = decoded
                .iter()
                .map(|c| {
                    let derived = c
                        .xpub
                        .derive_pub(&secp, &relative)
                        .expect("BIP-32 unhardened derivation cannot fail");
                    let pubkey_bytes = derived.to_pub().0.serialize();
                    MultisigPubkeyEntry {
                        sort_key: pubkey_bytes,
                        node: hd_node_from_xpub(&c.xpub),
                        address_n: relative_path_indices(&relative),
                    }
                })
                .collect();
            entries.sort_by_key(|e| e.sort_key);

            let pubkeys: Vec<TrezorMultisigPubkey> = entries
                .into_iter()
                .map(|e| TrezorMultisigPubkey {
                    node: e.node,
                    address_n: e.address_n,
                })
                .collect();
            let signatures: Vec<String> = (0..pubkeys.len()).map(|_| String::new()).collect();

            inputs.push(TrezorInput {
                address_n: derivation_path_to_indices(&signer_path_vec),
                prev_hash: op.txid.to_string(),
                prev_index: op.vout,
                amount: amount.to_string(),
                script_type: "SPENDWITNESS".into(),
                multisig: TrezorMultisig {
                    pubkeys,
                    signatures,
                    m: threshold_u32,
                },
                sequence: txin.sequence.0,
            });
        }

        // Outputs: walk the tx outputs once, classifying recipient vs change
        // via the PSBT output's `bip32_derivation` (BDK populates this for
        // every output that maps to one of the wallet's keychains).
        let mut outputs = Vec::with_capacity(psbt.unsigned_tx.output.len());
        for (txout, psbt_output) in psbt.unsigned_tx.output.iter().zip(psbt.outputs.iter()) {
            let amount = txout.value.to_sat();

            // If the signing Trezor's fingerprint appears in this output's
            // bip32_derivation map, the output is one of our keychains
            // (change). Otherwise it's the recipient.
            let signer_path = psbt_output
                .bip32_derivation
                .iter()
                .find_map(|(_pk, (fp, path))| {
                    if *fp == signing_fp {
                        Some(path.clone())
                    } else {
                        None
                    }
                });

            if let Some(path) = signer_path {
                let path_vec: Vec<ChildNumber> = path.into();
                let relative: Vec<ChildNumber> =
                    path_vec.iter().rev().take(2).rev().copied().collect();

                let mut entries: Vec<MultisigPubkeyEntry> = decoded
                    .iter()
                    .map(|c| {
                        let derived = c
                            .xpub
                            .derive_pub(&secp, &relative)
                            .expect("BIP-32 unhardened derivation cannot fail");
                        let pubkey_bytes = derived.to_pub().0.serialize();
                        MultisigPubkeyEntry {
                            sort_key: pubkey_bytes,
                            node: hd_node_from_xpub(&c.xpub),
                            address_n: relative_path_indices(&relative),
                        }
                    })
                    .collect();
                entries.sort_by_key(|e| e.sort_key);
                let pubkeys: Vec<TrezorMultisigPubkey> = entries
                    .into_iter()
                    .map(|e| TrezorMultisigPubkey {
                        node: e.node,
                        address_n: e.address_n,
                    })
                    .collect();
                let signatures: Vec<String> = (0..pubkeys.len()).map(|_| String::new()).collect();

                outputs.push(TrezorOutput::Change {
                    address_n: derivation_path_to_indices(&path_vec),
                    amount: amount.to_string(),
                    // Native P2WSH: `PAYTOWITNESS` + a `multisig` field is
                    // how Trezor whitelists this output as our change.
                    // `PAYTOMULTISIG` is the *legacy P2SH* multisig code
                    // path and triggers the "wrong derivation path for
                    // selected account" warning on the firmware screen.
                    script_type: "PAYTOWITNESS".into(),
                    multisig: TrezorMultisig {
                        pubkeys,
                        signatures,
                        m: threshold_u32,
                    },
                });
            } else {
                // Recipient output — render as a paying address.
                let address = Address::from_script(&txout.script_pubkey, self.network)
                    .map_or_else(|_| txout.script_pubkey.to_hex_string(), |a| a.to_string());
                outputs.push(TrezorOutput::External {
                    address,
                    amount: amount.to_string(),
                    script_type: "PAYTOADDRESS".into(),
                });
            }
        }

        // Fetch each previous transaction (refTxs) via the synchronous
        // bitcoincore_rpc client. Wrap in spawn_blocking so the executor
        // stays responsive on slow regtest nodes / mempool RPC stalls.
        let mut unique_txids: Vec<bitcoin::Txid> = Vec::new();
        let mut seen: std::collections::HashSet<bitcoin::Txid> = std::collections::HashSet::new();
        for txid in ref_txids {
            if seen.insert(txid) {
                unique_txids.push(txid);
            }
        }
        let rpc = self.rpc.clone();
        let raw_txs: Vec<Transaction> = tokio::task::spawn_blocking(move || {
            let mut out: Vec<Transaction> = Vec::with_capacity(unique_txids.len());
            for txid in unique_txids {
                let raw = rpc.get_raw_transaction(&txid, None)?;
                out.push(raw);
            }
            Ok::<_, bitcoincore_rpc::Error>(out)
        })
        .await
        .expect("get_raw_transaction join")
        .map_err(WalletError::Rpc)?;
        let ref_txs: Vec<TrezorRefTx> = raw_txs.iter().map(trezor_ref_tx_from).collect();

        // Compute the slot map: for each input, which index in the sorted
        // pubkey list does the signing Trezor occupy? The browser uses this
        // to extract Trezor's signature from `result.signatures[input][slot]`.
        let mut signer_slots: Vec<u32> = Vec::with_capacity(my_input_paths.len());
        for (idx, signer_path) in my_input_paths.iter().enumerate() {
            let signer_path_vec: Vec<ChildNumber> = signer_path.clone().into();
            let relative: Vec<ChildNumber> = signer_path_vec
                .iter()
                .rev()
                .take(2)
                .rev()
                .copied()
                .collect();
            let mut pubs: Vec<(Vec<u8>, Fingerprint)> = decoded
                .iter()
                .map(|c| {
                    let derived = c
                        .xpub
                        .derive_pub(&secp, &relative)
                        .expect("unhardened derive infallible");
                    (derived.to_pub().0.serialize().to_vec(), c.fingerprint)
                })
                .collect();
            pubs.sort_by(|a, b| a.0.cmp(&b.0));
            let slot = pubs
                .iter()
                .position(|(_, fp)| *fp == signing_fp)
                .ok_or_else(|| WalletError::UnknownCosigner(signing_fp.to_string()))?;
            let slot_u32 = u32::try_from(slot).unwrap_or(u32::MAX);
            signer_slots.push(slot_u32);
            tracing::debug!(input = idx, slot, "trezor signer slot");
        }

        // The unsigned tx's `version` is BDK's chosen tx version (defaults
        // to 2). `lock_time` carries BDK's anti-fee-sniping value (current
        // chain tip). Both must be echoed to Trezor so its BIP-143 sighash
        // hashes the same envelope bitcoind will eventually validate.
        let tx_version = u32::try_from(psbt.unsigned_tx.version.0).unwrap_or(2);
        let tx_lock_time = psbt.unsigned_tx.lock_time.to_consensus_u32();

        Ok(TrezorSignRequest {
            coin: coin_name(self.network).to_string(),
            inputs,
            outputs,
            ref_txs,
            version: tx_version,
            lock_time: tx_lock_time,
            signer_fingerprint: signing_fingerprint.to_string(),
            signer_slots,
        })
    }

    // -----------------------------------------------------------------
    // Signature merge / finalize / broadcast
    // -----------------------------------------------------------------

    /// Inject `signatures` (per input, DER-encoded ECDSA, no sighash byte)
    /// from the signing Trezor identified by `signing_fingerprint` into the
    /// PSBT's `partial_sigs`. Returns the resulting partial PSBT as base64.
    ///
    /// This is the server-side counterpart to the browser's
    /// `TrezorConnect.signTransaction` call: the browser ships back per-input
    /// signatures, and we slot them into a freshly-cloned base PSBT before
    /// running the canonical `combine + try-finalize` path.
    ///
    /// Method (not free function) on `FederationWallet` for API parity with
    /// the rest of the signing surface, even though it touches no internal
    /// state — keeps every signing-flow call site reading
    /// `wallet.<verb>(...)`.
    ///
    /// # Errors
    /// - [`WalletError::BadPsbt`] if the base PSBT doesn't parse.
    /// - [`WalletError::UnknownCosigner`] if an input's `bip32_derivation`
    ///   lacks the signing fingerprint.
    /// - [`WalletError::BadTrezorSignature`] if any signature isn't valid
    ///   DER.
    #[allow(clippy::unused_self)]
    pub fn inject_trezor_signatures(
        &self,
        base_psbt_b64: &str,
        signing_fingerprint: &str,
        signatures_hex: &[String],
    ) -> Result<String, WalletError> {
        let mut psbt =
            Psbt::from_str(base_psbt_b64).map_err(|e| WalletError::BadPsbt(e.to_string()))?;
        let signing_fp = Fingerprint::from_str(signing_fingerprint).map_err(|source| {
            WalletError::BadCosignerFingerprint {
                id: Uuid::nil(),
                source,
            }
        })?;

        if signatures_hex.len() != psbt.inputs.len() {
            return Err(WalletError::BadTrezorSignature {
                input_index: signatures_hex.len(),
                reason: format!(
                    "expected {} signatures, got {}",
                    psbt.inputs.len(),
                    signatures_hex.len()
                ),
            });
        }

        for (idx, sig_hex) in signatures_hex.iter().enumerate() {
            if sig_hex.is_empty() {
                continue;
            }
            let der = hex_decode(sig_hex).map_err(|reason| WalletError::BadTrezorSignature {
                input_index: idx,
                reason,
            })?;
            let secp_sig = bitcoin::secp256k1::ecdsa::Signature::from_der(&der).map_err(|e| {
                WalletError::BadTrezorSignature {
                    input_index: idx,
                    reason: e.to_string(),
                }
            })?;
            let ecdsa_sig = EcdsaSignature {
                signature: secp_sig,
                sighash_type: EcdsaSighashType::All,
            };

            // Find the signing Trezor's pubkey on this input.
            let psbt_input = &mut psbt.inputs[idx];
            let mut signer_pubkey: Option<PublicKey> = None;
            for (pk, (fp, _path)) in &psbt_input.bip32_derivation {
                if *fp == signing_fp {
                    signer_pubkey = Some(PublicKey::new(*pk));
                    break;
                }
            }
            let signer_pubkey = signer_pubkey
                .ok_or_else(|| WalletError::UnknownCosigner(signing_fp.to_string()))?;

            psbt_input.partial_sigs.insert(signer_pubkey, ecdsa_sig);
        }

        Ok(psbt.to_string())
    }

    /// Merge a cosigner's partial PSBT into the canonical base PSBT.
    /// Returns the merged PSBT (still as base64) and a flag indicating
    /// whether `Wallet::finalize_psbt` succeeds against the merged result.
    ///
    /// # Errors
    /// - [`WalletError::BadPsbt`] on parse failures.
    /// - [`WalletError::MergePsbt`] if `Psbt::combine` rejects the input
    ///   (e.g. txid mismatch).
    pub async fn merge_partial_signature(
        &self,
        base_psbt_b64: &str,
        partial_b64: &str,
    ) -> Result<MergedPsbt, WalletError> {
        let base =
            Psbt::from_str(base_psbt_b64).map_err(|e| WalletError::BadPsbt(e.to_string()))?;
        let partial =
            Psbt::from_str(partial_b64).map_err(|e| WalletError::BadPsbt(e.to_string()))?;
        let base = core_psbt::combine_psbt(base, partial).map_err(|e| match e {
            PsbtError::Bitcoin(s) => WalletError::MergePsbt(s),
            other => WalletError::MergePsbt(other.to_string()),
        })?;

        // Probe finalization on a clone so failure doesn't poison `base`.
        let wallet = self.inner.lock().await;
        let mut probe = base.clone();
        let fully_signed = wallet
            .finalize_psbt(&mut probe, SignOptions::default())
            .unwrap_or(false);
        drop(wallet);

        Ok(MergedPsbt {
            merged_psbt_b64: base.to_string(),
            fully_signed,
        })
    }

    /// Finalize a fully-signed PSBT and extract the raw transaction.
    ///
    /// # Errors
    /// - [`WalletError::BadPsbt`] on parse failures.
    /// - [`WalletError::Finalize`] if BDK's signer machinery errors.
    /// - [`WalletError::NotEnoughSignatures`] if finalize hits no error but
    ///   still can't satisfy every input (threshold not yet met).
    pub async fn finalize_and_extract(&self, psbt_b64: &str) -> Result<FinalizedTx, WalletError> {
        let psbt = Psbt::from_str(psbt_b64).map_err(|e| WalletError::BadPsbt(e.to_string()))?;
        let (tx, txid) = {
            let wallet = self.inner.lock().await;
            core_psbt::finalize_and_extract(&wallet, psbt).map_err(|e| match e {
                PsbtError::ThresholdNotMet => WalletError::NotEnoughSignatures,
                PsbtError::ExtractFailed(s) => WalletError::ExtractTx(s),
                PsbtError::FinalizationFailed(s) => WalletError::Finalize(s),
                other => WalletError::Finalize(other.to_string()),
            })?
        };
        let mut buf = Vec::new();
        tx.consensus_encode(&mut buf)
            .map_err(|e| WalletError::ExtractTx(e.to_string()))?;
        Ok(FinalizedTx {
            tx_hex: hex_encode(&buf),
            txid,
        })
    }

    /// Submit a finalized raw transaction (hex) to bitcoind.
    ///
    /// # Errors
    /// - [`WalletError::BroadcastRejected`] on RPC failure (mempool reject,
    ///   double-spend, fee-too-low, etc.).
    pub async fn broadcast_raw(&self, tx_hex: &str) -> Result<Txid, WalletError> {
        let bytes = hex_decode(tx_hex).map_err(WalletError::BroadcastRejected)?;
        let rpc = self.rpc.clone();
        // bitcoincore_rpc::Client is synchronous; spawn-blocking keeps the
        // executor responsive even on slow regtest nodes.
        let txid = tokio::task::spawn_blocking(move || rpc.send_raw_transaction(&bytes[..]))
            .await
            .expect("send_raw_transaction join")
            .map_err(|e| WalletError::BroadcastRejected(e.to_string()))?;
        Ok(txid)
    }
}

// ---------------------------------------------------------------------------
// View-model types
// ---------------------------------------------------------------------------

/// One row in the federation detail page's address table.
#[derive(Debug, Clone)]
pub struct RevealedAddress {
    /// Index on the external keychain.
    pub index: u32,
    /// Always [`KeychainKind::External`] today, but kept explicit so we can
    /// expand to change-address listings later without changing the type.
    pub keychain: KeychainKind,
    /// Address rendered in the user's network format.
    pub address: String,
    /// Cumulative amount the wallet has ever observed paying to this
    /// address (includes spent receipts).
    pub received: Amount,
    /// Amount currently sitting unspent at this address.
    pub unspent: Amount,
}

/// One incoming UTXO at the address detail page's target address.
#[derive(Debug, Clone)]
pub struct AddressReceipt {
    /// Transaction id of the payer.
    pub txid: Txid,
    /// Output index the address appears at.
    pub vout: u32,
    /// Amount in satoshis.
    pub amount: Amount,
    /// Confirmation height, or `None` if still in the mempool.
    pub confirmation_height: Option<u32>,
    /// Confirmation count (0 = unconfirmed).
    pub confirmations: u32,
    /// `true` if this UTXO has subsequently been spent by another wallet tx.
    pub is_spent: bool,
}

/// Summary of all receipts paying into a given address.
#[derive(Debug, Clone)]
pub struct AddressActivity {
    /// Chain tip at the time of the query — confirmations are relative to
    /// this height.
    pub tip_height: u32,
    /// Total amount ever received at this address.
    pub total_received: Amount,
    /// Current unspent amount at this address.
    pub unspent: Amount,
    /// All receiving txouts, oldest confirmed first.
    pub receipts: Vec<AddressReceipt>,
}

// ---------------------------------------------------------------------------
// Proposal-side view-model types
// ---------------------------------------------------------------------------

/// Output of [`FederationWallet::build_proposal`]. Everything callers need
/// to row-insert a proposal.
#[derive(Debug, Clone)]
pub struct BuiltProposal {
    /// Base64-encoded unsigned PSBT (canonical form).
    pub psbt_b64: String,
    /// Structural view: outputs, total, fee, `fee_rate`. Stored in the
    /// `proposal_json` column.
    pub proposal_json: serde_json::Value,
    /// Coin-selection breakdown. Stored in `coin_selection_json`.
    pub coin_selection_json: serde_json::Value,
}

/// Output of [`FederationWallet::merge_partial_signature`].
#[derive(Debug, Clone)]
pub struct MergedPsbt {
    /// Merged base PSBT (base64) — what we persist back to `psbt_b64`.
    pub merged_psbt_b64: String,
    /// `true` iff a finalize probe against the merged PSBT succeeded.
    /// Lets the handler flip status to `finalized` in the same DB
    /// transaction as the signature insert.
    pub fully_signed: bool,
}

/// Output of [`FederationWallet::finalize_and_extract`].
#[derive(Debug, Clone)]
pub struct FinalizedTx {
    /// Hex-encoded raw transaction (consensus-serialized).
    pub tx_hex: String,
    /// Transaction id.
    pub txid: Txid,
}

// ---------------------------------------------------------------------------
// Trezor sign-request payload
// ---------------------------------------------------------------------------

/// The JSON the browser hands to `TrezorConnect.signTransaction`.
///
/// `signer_fingerprint` + `signer_slots` are *out-of-band* metadata the
/// browser uses to extract the right signature from Trezor's
/// `result.signatures[i][slot]` and POST it back — Trezor itself ignores
/// these fields.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorSignRequest {
    /// Trezor's coin identifier ("regtest" / "testnet" / "btc").
    pub coin: String,
    /// One entry per PSBT input.
    pub inputs: Vec<TrezorInput>,
    /// One entry per PSBT output.
    pub outputs: Vec<TrezorOutput>,
    /// Previous transactions in Trezor's `RefTx` format. Required by Connect
    /// for some firmware versions even with native segwit.
    #[serde(rename = "refTxs")]
    pub ref_txs: Vec<TrezorRefTx>,
    /// `nVersion` of the unsigned tx. BDK builds version-2 tx by default;
    /// Trezor Connect defaults to version 1 if we don't pass it. Without
    /// matching, the BIP-143 sighash mismatches and bitcoind rejects the
    /// broadcast with NULLFAIL.
    pub version: u32,
    /// `nLockTime` of the unsigned tx. BDK sets this to the current chain
    /// tip for anti-fee-sniping; Trezor Connect defaults to 0 if we don't
    /// pass it. Same NULLFAIL failure mode as `version` if mismatched.
    #[serde(rename = "locktime")]
    pub lock_time: u32,
    /// Hex master fingerprint of the signing device. Echoed for browser
    /// convenience; not consumed by Trezor itself.
    pub signer_fingerprint: String,
    /// Per-input slot index in the sorted multisig pubkeys array. The
    /// browser uses this to extract the signer's signature.
    pub signer_slots: Vec<u32>,
}

/// One input in [`TrezorSignRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct TrezorInput {
    /// Full BIP-32 path of the signing Trezor's key for this input.
    pub address_n: Vec<u32>,
    /// Funding txid (hex, big-endian — Trezor's native form).
    pub prev_hash: String,
    /// Funding output index.
    pub prev_index: u32,
    /// Funding amount in satoshis, as a string (Trezor's protocol convention).
    pub amount: String,
    /// `"SPENDWITNESS"` for native P2WSH multisig.
    pub script_type: String,
    /// `sortedmulti` multisig payload (pubkeys + blank signatures + m).
    pub multisig: TrezorMultisig,
    /// nSequence (echoed unchanged from the PSBT input).
    pub sequence: u32,
}

/// One output in [`TrezorSignRequest`].
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TrezorOutput {
    /// Payment to an arbitrary external address.
    External {
        /// Destination address in canonical form.
        address: String,
        /// Amount in satoshis (string).
        amount: String,
        /// `"PAYTOADDRESS"`.
        script_type: String,
    },
    /// Change output paying back into the federation.
    Change {
        /// Full BIP-32 path of the signing Trezor's change key.
        address_n: Vec<u32>,
        /// Amount in satoshis (string).
        amount: String,
        /// `"PAYTOWITNESS"` for native P2WSH multisig change. The
        /// firmware reads the presence of `multisig` and produces a
        /// P2WSH `script_pubkey` (rather than P2WPKH) internally.
        script_type: String,
        /// Same shape as input multisig at the change index.
        multisig: TrezorMultisig,
    },
}

/// `multisig` field shared by inputs and change outputs.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorMultisig {
    /// Sorted pubkeys (matching `sortedmulti` on-chain order).
    pub pubkeys: Vec<TrezorMultisigPubkey>,
    /// One placeholder per pubkey — Trezor fills its own slot.
    pub signatures: Vec<String>,
    /// Threshold.
    pub m: u32,
}

/// One entry in `multisig.pubkeys`.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorMultisigPubkey {
    /// `HDNode` form of the cosigner's xpub.
    pub node: TrezorHdNode,
    /// Relative derivation from the xpub for this input/output.
    pub address_n: Vec<u32>,
}

/// `HDNode` form Trezor expects (matches the protobuf `HDNodeType`).
#[derive(Debug, Clone, Serialize)]
pub struct TrezorHdNode {
    /// BIP-32 depth.
    pub depth: u32,
    /// BIP-32 parent fingerprint (u32 form).
    pub fingerprint: u32,
    /// BIP-32 child number of this xpub.
    pub child_num: u32,
    /// Chain code (32 bytes, hex-encoded).
    pub chain_code: String,
    /// Compressed pubkey (33 bytes, hex-encoded).
    pub public_key: String,
}

/// One entry in `refTxs`.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorRefTx {
    /// Txid (hex, big-endian).
    pub hash: String,
    /// Tx version.
    pub version: u32,
    /// nLockTime.
    pub lock_time: u32,
    /// Stripped input list (no witness).
    pub inputs: Vec<TrezorRefInput>,
    /// Output list with amount + `script_pubkey`.
    pub bin_outputs: Vec<TrezorRefOutput>,
}

/// One input in a `refTxs` entry.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorRefInput {
    /// Prev txid (hex).
    pub prev_hash: String,
    /// Prev output index.
    pub prev_index: u32,
    /// scriptSig hex (empty for segwit).
    pub script_sig: String,
    /// nSequence.
    pub sequence: u32,
}

/// One output in a `refTxs` entry.
#[derive(Debug, Clone, Serialize)]
pub struct TrezorRefOutput {
    /// Amount in satoshis (string).
    pub amount: String,
    /// scriptPubKey hex.
    pub script_pubkey: String,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct DecodedCosigner {
    xpub: Xpub,
    fingerprint: Fingerprint,
}

struct MultisigPubkeyEntry {
    sort_key: [u8; 33],
    node: TrezorHdNode,
    address_n: Vec<u32>,
}

const fn coin_name(network: Network) -> &'static str {
    // `bitcoin::Network` is `#[non_exhaustive]`; any future variant (e.g.
    // a Testnet4-like value) falls through to regtest/dev territory rather
    // than mainnet, which is the safer default for this dev app.
    #[allow(clippy::match_wildcard_for_single_variants, clippy::match_same_arms)]
    match network {
        Network::Bitcoin => "btc",
        // Trezor Connect has no `"signet"` coin ("coin not found"). Signet
        // shares testnet's coin params + xpub/address versions, and the
        // BIP-143 sighash Trezor signs is network-magic-independent, so we sign
        // it as testnet — the same `"test"` coin onboarding uses.
        Network::Testnet | Network::Signet => "test",
        Network::Regtest => "regtest",
        _ => "regtest",
    }
}

fn derivation_path_to_indices(path: &[ChildNumber]) -> Vec<u32> {
    path.iter().map(|c| u32::from(*c)).collect()
}

fn relative_path_indices(path: &[ChildNumber]) -> Vec<u32> {
    path.iter().map(|c| u32::from(*c)).collect()
}

fn hd_node_from_xpub(xpub: &Xpub) -> TrezorHdNode {
    let parent_fp_bytes: [u8; 4] = xpub.parent_fingerprint.to_bytes();
    let parent_fp = u32::from_be_bytes(parent_fp_bytes);
    let child_num = u32::from(xpub.child_number);
    let chain_code_hex = hex_encode(xpub.chain_code.as_bytes());
    let pubkey_bytes = xpub.public_key.serialize();
    let pubkey_hex = hex_encode(&pubkey_bytes);
    TrezorHdNode {
        depth: u32::from(xpub.depth),
        fingerprint: parent_fp,
        child_num,
        chain_code: chain_code_hex,
        public_key: pubkey_hex,
    }
}

fn trezor_ref_tx_from(tx: &Transaction) -> TrezorRefTx {
    let inputs = tx
        .input
        .iter()
        .map(|txin| TrezorRefInput {
            prev_hash: txin.previous_output.txid.to_string(),
            prev_index: txin.previous_output.vout,
            script_sig: hex_encode(txin.script_sig.as_bytes()),
            sequence: txin.sequence.0,
        })
        .collect();
    let bin_outputs = tx
        .output
        .iter()
        .map(|txout| TrezorRefOutput {
            amount: txout.value.to_sat().to_string(),
            script_pubkey: hex_encode(txout.script_pubkey.as_bytes()),
        })
        .collect();
    // `Version` wraps an `i32` but only positive standard values (1, 2)
    // appear on the wire; any non-standard negative version would already
    // have failed consensus on the upstream tx.
    let version_u32 = u32::try_from(tx.version.0).unwrap_or(2);
    TrezorRefTx {
        hash: tx.compute_txid().to_string(),
        version: version_u32,
        lock_time: tx.lock_time.to_consensus_u32(),
        inputs,
        bin_outputs,
    }
}

// `hex_encode` / `hex_decode` now live in `emvault::config` (imported above) —
// deduplicated in extraction phase E5b.
