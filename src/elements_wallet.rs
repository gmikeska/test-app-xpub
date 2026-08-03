//! Per-federation LWK wallet management for Liquid (Elements) federations.
//!
//! Mirrors [`crate::wallet::WalletManager`] / [`crate::wallet::FederationWallet`]
//! shape but routes everything through [`lwk_wollet::Wollet`] and PSETs
//! instead of BDK `Wallet` and PSBTs. The receive / send / proposal /
//! address-detail handlers branch on
//! [`crate::db::FederationKind`] before deciding which manager to consume.
//!
//! # Concurrency model
//!
//! - [`LwkWalletManager`] owns a `HashMap<Uuid, Arc<LiquidFederationWallet>>`
//!   behind an async mutex. The cache mutex is only held during lookup /
//!   insertion.
//! - [`LiquidFederationWallet`] wraps the inner [`lwk_wollet::Wollet`] in
//!   its own async mutex. LWK's wallet is `Send` but not `Sync`, and most
//!   of its mutators (`apply_update`, `set_next_index`) require `&mut self`.
//! - LWK has no `ChangeSet`-style persistence. We persist the next reveal
//!   index per keychain on the federation row directly via
//!   [`crate::db::update_federation_indices`].
//!
//! # Sync model
//!
//! Sync / broadcast route through the backend selected by
//! [`crate::config::AppConfig::elements_chain_backend`]:
//!
//! - `Esplora` / `Waterfalls` — async [`lwk_wollet::asyncr::EsploraClient`]
//!   against `ELEMENTS_ESPLORA_URL` (Waterfalls flips the descriptor-endpoint
//!   fast path on the same client).
//! - `Electrum` — LWK's blocking `ElectrumClient` against
//!   `ELEMENTS_ELECTRUM_URL`, run inside [`tokio::task::spawn_blocking`] over a
//!   released wollet-state snapshot so it never stalls the async runtime.
//!
//! When the endpoint for the active backend is unset, [`LiquidFederationWallet::sync`]
//! is a no-op and the UI surfaces a "no Liquid indexer configured" warning.

#![allow(clippy::module_name_repetitions)]

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use emvault::elements::ElementsNetwork;
use emvault::elements::elements::pset::PartiallySignedTransaction as Pset;
use emvault::elements::lwk_wollet::asyncr::{EsploraClient, EsploraClientBuilder};
use emvault::elements::lwk_wollet::blocking::BlockchainBackend;
use emvault::elements::lwk_wollet::elements::{
    Address, AddressParams, OutPoint, Transaction, Txid, encode::serialize_hex,
};
use emvault::elements::lwk_wollet::{
    Chain, ElectrumClient, ElectrumUrl, TxBuilder, Update, Wollet, WolletBuilder, WolletDescriptor,
};
use sqlx::PgPool;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::config::{AppConfig, ElementsChainBackend};
use crate::db;

/// Default reveal target: addresses 0..=REVEAL_COUNT-1 on the external
/// keychain are populated each time a federation page is rendered.
pub const REVEAL_COUNT: u32 = 20;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors raised by the Liquid wallet layer.
#[derive(Debug, thiserror::Error)]
pub enum ElementsWalletError {
    /// Federation row not found or wrong kind.
    #[error("federation `{0}` not found or not a Liquid federation")]
    NotFound(Uuid),

    /// Server is missing `ELEMENTS_NETWORK` configuration so a Liquid
    /// federation is unusable.
    #[error("server has no ELEMENTS_NETWORK configured; Liquid federations are disabled")]
    NetworkUnconfigured,

    /// The federations row's `network` doesn't match server config.
    #[error("federation `{id}`: network `{stored}` does not match server `ELEMENTS_NETWORK`")]
    NetworkMismatch {
        /// Federation id.
        id: Uuid,
        /// Network stored in the row.
        stored: String,
    },

    /// The stored CT descriptor failed to parse via
    /// [`lwk_wollet::WolletDescriptor::from_str`].
    #[error("federation `{id}`: failed to parse CT descriptor: {reason}")]
    BadDescriptor {
        /// Federation id.
        id: Uuid,
        /// Underlying parse error.
        reason: String,
    },

    /// `WolletBuilder::build` rejected the descriptor / network combo.
    #[error("federation `{id}`: failed to build LWK wallet: {reason}")]
    BuildWollet {
        /// Federation id.
        id: Uuid,
        /// Underlying error.
        reason: String,
    },

    /// `Wollet::address` errored (typically: requested index outside the
    /// descriptor's keyspace).
    #[error("LWK address derivation failed: {0}")]
    AddressDerive(String),

    /// `Wollet::balance` errored.
    #[error("LWK balance query failed: {0}")]
    Balance(String),

    /// `Wollet::utxos` errored.
    #[error("LWK UTXO query failed: {0}")]
    Utxos(String),

    /// `Wollet::transactions` errored.
    #[error("LWK transactions query failed: {0}")]
    Transactions(String),

    /// `Wollet::finalize` errored.
    #[error("LWK PSET finalize failed: {0}")]
    Finalize(String),

    /// `TxBuilder::finish` errored (insufficient funds, etc.).
    #[error("LWK PSET construction failed: {0}")]
    BuildPset(String),

    /// PSET base64 didn't parse.
    #[error("invalid PSET base64: {0}")]
    BadPset(String),

    /// `Pset::merge` rejected the partial.
    #[error("failed to merge partial PSET into base: {0}")]
    MergePset(String),

    /// `EsploraClient::full_scan` errored.
    #[error("LWK Esplora sync failed: {0}")]
    Sync(String),

    /// `EsploraClient::broadcast` errored.
    #[error("Esplora rejected broadcast: {0}")]
    BroadcastRejected(String),

    /// User-supplied address didn't parse / didn't match this network.
    #[error("address `{addr}` is not a valid `{network}` address: {reason}")]
    BadAddress {
        /// Raw input.
        addr: String,
        /// Expected network.
        network: ElementsNetwork,
        /// Human-readable parse reason.
        reason: String,
    },

    /// User-supplied amount couldn't be parsed.
    #[error("invalid L-BTC amount `{input}`: {reason}")]
    BadAmount {
        /// Raw input.
        input: String,
        /// Underlying parse reason.
        reason: String,
    },

    /// No Esplora endpoint configured but a network operation was requested.
    #[error("ELEMENTS_ESPLORA_URL is not configured; sync and broadcast are unavailable")]
    NoEsploraEndpoint,

    /// Database error.
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Cache + factory for [`LiquidFederationWallet`]s.
///
/// Mirrors [`crate::wallet::WalletManager`] for the Liquid pipeline. One
/// instance is built at startup and shared via `Arc<AppState>`.
pub struct LwkWalletManager {
    pool: PgPool,
    network: Option<ElementsNetwork>,
    /// Chain backend selected via `ELEMENTS_CHAIN_BACKEND`. Determines whether
    /// sync / broadcast go through Esplora (optionally Waterfalls) or a
    /// blocking LWK Electrum client.
    backend: ElementsChainBackend,
    esplora_url: Option<String>,
    electrum_url: Option<String>,
    cache: AsyncMutex<HashMap<Uuid, Arc<LiquidFederationWallet>>>,
}

impl LwkWalletManager {
    /// Construct the manager from [`AppConfig`].
    #[must_use]
    pub fn new(pool: PgPool, config: &AppConfig) -> Self {
        Self {
            pool,
            network: config.elements_network,
            backend: config.elements_chain_backend,
            esplora_url: config.elements_esplora_url.clone(),
            electrum_url: config.elements_electrum_url.clone(),
            cache: AsyncMutex::new(HashMap::new()),
        }
    }

    /// `true` iff the server has any Elements network configured. The
    /// federation-creation form gates the "Liquid" radio on this.
    #[must_use]
    pub fn elements_enabled(&self) -> bool {
        self.network.is_some()
    }

    /// The configured Elements network, if any.
    #[must_use]
    pub fn network(&self) -> Option<ElementsNetwork> {
        self.network
    }

    /// `true` iff sync / broadcast are available.
    #[must_use]
    pub fn has_esplora(&self) -> bool {
        self.esplora_url.is_some()
    }

    /// The configured Esplora endpoint, if any.
    #[must_use]
    pub fn esplora_url(&self) -> Option<&str> {
        self.esplora_url.as_deref()
    }

    /// Look up (or lazily instantiate) the wallet for federation `id`.
    /// The caller is responsible for confirming the federation kind is
    /// `Liquid` before calling this.
    ///
    /// # Errors
    /// See [`ElementsWalletError`].
    pub async fn load_or_init(
        &self,
        federation_id: Uuid,
    ) -> Result<Arc<LiquidFederationWallet>, ElementsWalletError> {
        {
            let cache = self.cache.lock().await;
            if let Some(fw) = cache.get(&federation_id) {
                return Ok(fw.clone());
            }
        }

        let row = db::find_federation_by_id(&self.pool, federation_id)
            .await?
            .ok_or(ElementsWalletError::NotFound(federation_id))?;

        let server_network = self
            .network
            .ok_or(ElementsWalletError::NetworkUnconfigured)?;

        // Dual-chain federations are Bitcoin-networked and carry the Elements
        // confidential descriptor in `elements_descriptor`; the Elements
        // network is the server's `ELEMENTS_NETWORK`. Legacy Liquid-only
        // federations instead stored the `ct(...)` descriptor in `descriptor`
        // with a Liquid `network` — those still get the network match check.
        let ct_descriptor_str = if let Some(ed) = row.elements_descriptor.as_deref() {
            ed
        } else {
            let row_network = parse_row_network(&row.network).ok_or_else(|| {
                ElementsWalletError::NetworkMismatch {
                    id: federation_id,
                    stored: row.network.clone(),
                }
            })?;
            if row_network != server_network {
                return Err(ElementsWalletError::NetworkMismatch {
                    id: federation_id,
                    stored: row.network.clone(),
                });
            }
            &row.descriptor
        };

        let descriptor = WolletDescriptor::from_str(ct_descriptor_str).map_err(|e| {
            ElementsWalletError::BadDescriptor {
                id: federation_id,
                reason: e.to_string(),
            }
        })?;

        let wollet = WolletBuilder::new(server_network.to_lwk(), descriptor)
            .build()
            .map_err(|e| ElementsWalletError::BuildWollet {
                id: federation_id,
                reason: e.to_string(),
            })?;

        // Restore the persisted reveal indices so addresses we showed in
        // earlier sessions don't get re-derived as fresh ones.
        let next_external = u32::try_from(row.next_external_index).unwrap_or(0);
        let next_internal = u32::try_from(row.next_internal_index).unwrap_or(0);

        let fw = Arc::new(LiquidFederationWallet {
            id: federation_id,
            network: server_network,
            backend: self.backend,
            esplora_url: self.esplora_url.clone(),
            electrum_url: self.electrum_url.clone(),
            inner: AsyncMutex::new(wollet),
            indices: AsyncMutex::new(KeychainIndices {
                next_external,
                next_internal,
            }),
            pool: self.pool.clone(),
        });

        let mut cache = self.cache.lock().await;
        Ok(cache.entry(federation_id).or_insert(fw).clone())
    }
}

fn parse_row_network(s: &str) -> Option<ElementsNetwork> {
    match s {
        "liquid" => Some(ElementsNetwork::Liquid),
        "liquidtestnet" | "liquid_testnet" => Some(ElementsNetwork::LiquidTestnet),
        "elementsregtest" | "elements_regtest" => Some(ElementsNetwork::ElementsRegtest),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Per-federation wallet
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct KeychainIndices {
    next_external: u32,
    next_internal: u32,
}

/// A live LWK wallet bound to a single Liquid federation.
///
/// Use [`LwkWalletManager::load_or_init`] to get one; never construct
/// directly.
pub struct LiquidFederationWallet {
    /// Federation id (matches `federations.id`).
    pub id: Uuid,
    /// The wallet's Elements network.
    pub network: ElementsNetwork,
    /// Selected chain backend (Esplora / Waterfalls / Electrum).
    backend: ElementsChainBackend,
    esplora_url: Option<String>,
    electrum_url: Option<String>,
    inner: AsyncMutex<Wollet>,
    indices: AsyncMutex<KeychainIndices>,
    pool: PgPool,
}

/// Snapshot returned from [`LiquidFederationWallet::sync`].
#[derive(Debug, Clone, Copy)]
pub struct LiquidSyncSummary {
    /// Current chain tip after sync. `0` when no Esplora endpoint is
    /// configured (sync was a no-op).
    pub tip_height: u32,
    /// `true` iff a wallet update was applied during this call.
    pub applied_update: bool,
}

impl LiquidFederationWallet {
    /// Drive `lwk_wollet::asyncr::EsploraClient::full_scan` against the
    /// configured endpoint. No-op (returns `applied_update = false`) when
    /// no endpoint is configured.
    ///
    /// # Errors
    /// See [`ElementsWalletError`].
    pub async fn sync(&self) -> Result<LiquidSyncSummary, ElementsWalletError> {
        // Compute the wallet update via the configured chain backend, or a
        // no-op tip read when no endpoint is configured for that backend.
        // Handlers surface the no-op case as a UI warning, not a hard error.
        let update = match self.backend {
            ElementsChainBackend::Esplora | ElementsChainBackend::Waterfalls => {
                let Some(esplora_url) = self.esplora_url.clone() else {
                    return self.cached_tip_summary().await;
                };
                self.full_scan_esplora(&esplora_url).await?
            }
            ElementsChainBackend::Electrum => {
                let Some(electrum_url) = self.electrum_url.clone() else {
                    return self.cached_tip_summary().await;
                };
                self.full_scan_electrum(&electrum_url).await?
            }
        };

        let (tip, applied) = if let Some(update) = update {
            let mut wollet = self.inner.lock().await;
            wollet
                .apply_update(update)
                .map_err(|e| ElementsWalletError::Sync(e.to_string()))?;
            (wollet.tip().height(), true)
        } else {
            let wollet = self.inner.lock().await;
            (wollet.tip().height(), false)
        };

        Ok(LiquidSyncSummary {
            tip_height: tip,
            applied_update: applied,
        })
    }

    /// Read the cached wallet tip without contacting any indexer. Returned
    /// when no endpoint is configured for the active backend (fresh loads
    /// report height `0`).
    async fn cached_tip_summary(&self) -> Result<LiquidSyncSummary, ElementsWalletError> {
        let tip = self.inner.lock().await.tip().height();
        Ok(LiquidSyncSummary {
            tip_height: tip,
            applied_update: false,
        })
    }

    /// Async Esplora full-scan. When the backend is [`ElementsChainBackend::Waterfalls`]
    /// the client negotiates the descriptor endpoint (fewer round-trips).
    ///
    /// The wollet lock is held only to clone the scan state, then released
    /// before the (potentially slow) HTTP round-trip.
    async fn full_scan_esplora(
        &self,
        esplora_url: &str,
    ) -> Result<Option<Update>, ElementsWalletError> {
        let mut client = EsploraClientBuilder::new(esplora_url, self.network.to_lwk())
            .waterfalls(matches!(self.backend, ElementsChainBackend::Waterfalls))
            .build()
            .map_err(|e| ElementsWalletError::Sync(e.to_string()))?;
        let wollet = self.inner.lock().await;
        client
            .full_scan(&wollet)
            .await
            .map_err(|e| ElementsWalletError::Sync(e.to_string()))
    }

    /// Electrum full-scan. LWK's `ElectrumClient` is a blocking client, so we
    /// snapshot the wollet state (releasing the async lock) and run the scan
    /// on a blocking thread via [`tokio::task::spawn_blocking`], mirroring the
    /// Esplora flow.
    async fn full_scan_electrum(
        &self,
        electrum_url: &str,
    ) -> Result<Option<Update>, ElementsWalletError> {
        // `WolletConciseState` is owned + `Send`, letting us release the async
        // lock before the blocking network round-trip.
        let state = { self.inner.lock().await.state() };
        let electrum_url = electrum_url.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Update>, String> {
            let url = ElectrumUrl::from_str(&electrum_url).map_err(|e| e.to_string())?;
            let mut client = ElectrumClient::new(&url).map_err(|e| e.to_string())?;
            client.full_scan(&state).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| ElementsWalletError::Sync(format!("electrum scan task failed: {e}")))?
        .map_err(ElementsWalletError::Sync)
    }

    /// Reveal external-keychain addresses 0..n (idempotent), and return
    /// every address from 0 to `n - 1` as a view-model.
    ///
    /// LWK doesn't track "revealed" indices internally — every
    /// `Wollet::address(Some(idx))` call returns a freshly-derived
    /// address. We track the high-water mark on `federations.next_external_index`
    /// so the UI can show a stable list across restarts.
    ///
    /// # Errors
    /// Propagates LWK and DB errors.
    pub async fn reveal_addresses(
        &self,
        target_count: u32,
    ) -> Result<Vec<RevealedLiquidAddress>, ElementsWalletError> {
        if target_count == 0 {
            return Ok(Vec::new());
        }

        let wollet = self.inner.lock().await;
        let mut results = Vec::with_capacity(target_count as usize);
        for index in 0..target_count {
            let info = wollet
                .address(Some(index))
                .map_err(|e| ElementsWalletError::AddressDerive(e.to_string()))?;
            results.push(RevealedLiquidAddress {
                index: info.index(),
                address: info.address().to_string(),
            });
        }
        drop(wollet);

        // Persist the high-water mark.
        let mut indices = self.indices.lock().await;
        let new_external = indices.next_external.max(target_count);
        if new_external != indices.next_external {
            indices.next_external = new_external;
            let next_internal = indices.next_internal;
            drop(indices);
            db::update_federation_indices(
                &self.pool,
                self.id,
                i32::try_from(new_external).unwrap_or(i32::MAX),
                i32::try_from(next_internal).unwrap_or(i32::MAX),
                None,
            )
            .await?;
        }

        Ok(results)
    }

    /// Snapshot the wallet's L-BTC balance (sats). Multi-asset balances
    /// are out of scope for v1: we only render the policy-asset row.
    ///
    /// # Errors
    /// Propagates LWK errors.
    pub async fn lbtc_balance_sat(&self) -> Result<u64, ElementsWalletError> {
        let wollet = self.inner.lock().await;
        let policy = wollet.policy_asset();
        let bal = wollet
            .balance()
            .map_err(|e| ElementsWalletError::Balance(e.to_string()))?;
        Ok(bal.get(&policy).copied().unwrap_or(0))
    }

    /// Per-address L-BTC activity for a revealed address, reconstructed from
    /// the synced wallet's tx history + current UTXO set.
    ///
    /// LWK stamps every wallet-owned output with its `wildcard_index` and
    /// `ext_int` chain, so — unlike the wallet-level balance — we *can*
    /// attribute confidential amounts to a specific address: walk every
    /// wallet output across all transactions, keep the ones on `(chain, index)`
    /// carrying the policy (L-BTC) asset, and mark each spent/unspent by
    /// membership in the live UTXO set. This is the Elements analog of the
    /// Bitcoin address-history path.
    ///
    /// # Errors
    /// Propagates LWK `utxos` / `transactions` errors.
    pub async fn address_activity(
        &self,
        chain: Chain,
        index: u32,
    ) -> Result<LiquidAddressActivity, ElementsWalletError> {
        let wollet = self.inner.lock().await;
        let policy = wollet.policy_asset();
        let unspent: HashSet<OutPoint> = wollet
            .utxos()
            .map_err(|e| ElementsWalletError::Utxos(e.to_string()))?
            .iter()
            .map(|u| u.outpoint)
            .collect();
        let txs = wollet
            .transactions()
            .map_err(|e| ElementsWalletError::Transactions(e.to_string()))?;

        let mut activity = LiquidAddressActivity::default();
        for tx in &txs {
            for out in tx.outputs.iter().flatten() {
                if out.ext_int != chain
                    || out.wildcard_index != index
                    || out.unblinded.asset != policy
                {
                    continue;
                }
                let is_spent = !unspent.contains(&out.outpoint);
                activity.received_sat = activity.received_sat.saturating_add(out.unblinded.value);
                if !is_spent {
                    activity.unspent_sat = activity.unspent_sat.saturating_add(out.unblinded.value);
                }
                activity.receipts.push(LiquidReceipt {
                    txid: out.outpoint.txid,
                    vout: out.outpoint.vout,
                    amount_sat: out.unblinded.value,
                    height: out.height,
                    is_spent,
                });
            }
        }
        Ok(activity)
    }

    /// Per-address L-BTC totals for the whole wallet in a single tx-history
    /// pass, keyed by `(is_external, index)` → `(received_sat, unspent_sat)`.
    /// Cheaper than calling [`address_activity`](Self::address_activity) once
    /// per address when rendering the full receive table.
    ///
    /// # Errors
    /// Propagates LWK `utxos` / `transactions` errors.
    pub async fn address_activity_map(
        &self,
    ) -> Result<HashMap<(bool, u32), (u64, u64)>, ElementsWalletError> {
        let wollet = self.inner.lock().await;
        let policy = wollet.policy_asset();
        let unspent: HashSet<OutPoint> = wollet
            .utxos()
            .map_err(|e| ElementsWalletError::Utxos(e.to_string()))?
            .iter()
            .map(|u| u.outpoint)
            .collect();
        let txs = wollet
            .transactions()
            .map_err(|e| ElementsWalletError::Transactions(e.to_string()))?;

        let mut map: HashMap<(bool, u32), (u64, u64)> = HashMap::new();
        for tx in &txs {
            for out in tx.outputs.iter().flatten() {
                if out.unblinded.asset != policy {
                    continue;
                }
                let key = (matches!(out.ext_int, Chain::External), out.wildcard_index);
                let entry = map.entry(key).or_default();
                entry.0 = entry.0.saturating_add(out.unblinded.value);
                if unspent.contains(&out.outpoint) {
                    entry.1 = entry.1.saturating_add(out.unblinded.value);
                }
            }
        }
        Ok(map)
    }

    /// Current chain tip height the wallet is aware of.
    /// Fetch the current chain-tip height **directly from the configured
    /// backend** (esplora/waterfalls or electrum) without a full wallet scan —
    /// a cheap live read for header/confirmation display. Falls back to the
    /// cached wollet tip when no endpoint is configured for the backend.
    ///
    /// # Errors
    /// [`ElementsWalletError::Sync`] if the backend can't be reached.
    pub async fn tip_height(&self) -> Result<u32, ElementsWalletError> {
        match self.backend {
            ElementsChainBackend::Esplora | ElementsChainBackend::Waterfalls => {
                let Some(esplora_url) = self.esplora_url.clone() else {
                    return Ok(self.inner.lock().await.tip().height());
                };
                let mut client = EsploraClientBuilder::new(&esplora_url, self.network.to_lwk())
                    .waterfalls(matches!(self.backend, ElementsChainBackend::Waterfalls))
                    .build()
                    .map_err(|e| ElementsWalletError::Sync(e.to_string()))?;
                let header = client
                    .tip()
                    .await
                    .map_err(|e| ElementsWalletError::Sync(e.to_string()))?;
                Ok(header.height)
            }
            ElementsChainBackend::Electrum => {
                let Some(electrum_url) = self.electrum_url.clone() else {
                    return Ok(self.inner.lock().await.tip().height());
                };
                tokio::task::spawn_blocking(move || -> Result<u32, String> {
                    let url = ElectrumUrl::from_str(&electrum_url).map_err(|e| e.to_string())?;
                    let mut client = ElectrumClient::new(&url).map_err(|e| e.to_string())?;
                    Ok(client.tip().map_err(|e| e.to_string())?.height)
                })
                .await
                .map_err(|e| ElementsWalletError::Sync(format!("electrum tip task failed: {e}")))?
                .map_err(ElementsWalletError::Sync)
            }
        }
    }

    /// Resolve a free-form Liquid address string. Only confidential
    /// addresses for this network are accepted.
    ///
    /// # Errors
    /// Returns [`ElementsWalletError::BadAddress`] on parse / network mismatch.
    pub fn parse_address(&self, raw: &str) -> Result<Address, ElementsWalletError> {
        let addr = Address::from_str(raw).map_err(|e| ElementsWalletError::BadAddress {
            addr: raw.to_string(),
            network: self.network,
            reason: e.to_string(),
        })?;
        if !is_address_for_network(&addr, self.network) {
            return Err(ElementsWalletError::BadAddress {
                addr: raw.to_string(),
                network: self.network,
                reason: "address is for a different Elements network".into(),
            });
        }
        Ok(addr)
    }

    /// Build an unsigned PSET for a single L-BTC recipient.
    ///
    /// # Errors
    /// - [`ElementsWalletError::BuildPset`] for any LWK error during
    ///   coin selection / blinding.
    /// - DB errors propagate.
    pub async fn build_proposal(
        &self,
        recipient: &Address,
        satoshi: u64,
        fee_rate_sat_vb: Option<u64>,
    ) -> Result<BuiltLiquidProposal, ElementsWalletError> {
        let (pset_b64, fee_sat, input_count) = {
            let wollet = self.inner.lock().await;
            // sats/vB → sats/kvB; LWK takes f32 sats/kvB. The cast is
            // safe in practice because fee rates fit in 24 bits long
            // before precision loss matters.
            let lwk_fee_rate = fee_rate_sat_vb.map(|sat_vb| {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let v = (sat_vb as f64 * 1000.0) as f32;
                v
            });

            let mut builder = TxBuilder::new(self.network.to_lwk())
                .add_lbtc_recipient(recipient, satoshi)
                .map_err(|e| ElementsWalletError::BuildPset(e.to_string()))?;
            builder = builder.fee_rate(lwk_fee_rate);
            let pset = builder
                .finish(&wollet)
                .map_err(|e| ElementsWalletError::BuildPset(e.to_string()))?;
            let input_count = pset.inputs().len();
            // The fee output is the explicit, script-less output. Input amounts
            // are blinded, so change stays undisclosed; Amount + Fee + Inputs are
            // the reliably-derivable view fields.
            let fee_sat = pset
                .outputs()
                .iter()
                .find(|o| o.script_pubkey.is_empty())
                .and_then(|o| o.amount)
                .unwrap_or(0);
            drop(wollet);
            (pset.to_string(), fee_sat, input_count)
        };

        let proposal_json = serde_json::json!({
            "recipient": recipient.to_string(),
            "recipient_amount_sat": satoshi,
            "asset": "L-BTC",
            "fee_rate_sat_vb": fee_rate_sat_vb,
            "fee_sat": fee_sat,
            "input_count": input_count,
        });
        let coin_selection_json = serde_json::json!({
            "selected": [],
            "outputs": [{ "address": recipient.to_string(), "amount_sat": satoshi }],
            "fee_sat": fee_sat,
            "total_input_sat": serde_json::Value::Null,
            "note": "LWK-driven coin selection (Liquid input amounts are blinded).",
        });

        Ok(BuiltLiquidProposal {
            pset_b64,
            proposal_json,
            coin_selection_json,
        })
    }

    /// Build a **drain** (send-max) PSET sweeping this federation's entire L-BTC
    /// balance to `destination` — the migration sweep from the current Liquid
    /// federation to its successor. Same shape as [`Self::build_proposal`] but
    /// uses LWK's `drain_lbtc_wallet`, so the full balance minus the mining fee
    /// lands at the successor's address. The successor recognises the inflow once
    /// it syncs its own CT descriptor. Signing / finalize / broadcast reuse the
    /// regular Liquid proposal path.
    ///
    /// # Errors
    /// [`ElementsWalletError::BuildPset`] if LWK cannot build the drain (e.g. no
    /// spendable balance, or fee exceeds value).
    pub async fn build_migration_pset(
        &self,
        destination: &Address,
        fee_rate_sat_vb: Option<u64>,
    ) -> Result<BuiltLiquidProposal, ElementsWalletError> {
        let (pset_b64, drained_sat, fee_sat, input_count) = {
            let wollet = self.inner.lock().await;
            let policy = wollet.policy_asset();
            let balance = wollet
                .balance()
                .map_err(|e| ElementsWalletError::Balance(e.to_string()))?
                .get(&policy)
                .copied()
                .unwrap_or(0);
            let lwk_fee_rate = fee_rate_sat_vb.map(|sat_vb| {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let v = (sat_vb as f64 * 1000.0) as f32;
                v
            });
            let pset = TxBuilder::new(self.network.to_lwk())
                .drain_lbtc_wallet()
                .drain_lbtc_to(destination.clone())
                .fee_rate(lwk_fee_rate)
                .finish(&wollet)
                .map_err(|e| ElementsWalletError::BuildPset(e.to_string()))?;
            let input_count = pset.inputs().len();
            // The fee output is the explicit, script-less output; everything else
            // is swept to the destination, so drained = balance − fee.
            let fee_sat = pset
                .outputs()
                .iter()
                .find(|o| o.script_pubkey.is_empty())
                .and_then(|o| o.amount)
                .unwrap_or(0);
            drop(wollet);
            (
                pset.to_string(),
                balance.saturating_sub(fee_sat),
                fee_sat,
                input_count,
            )
        };

        // A drain that selected no inputs means the wallet holds no spendable
        // L-BTC — surface it rather than persist a confusing zero-value sweep.
        if input_count == 0 {
            return Err(ElementsWalletError::BuildPset(
                "migration sweep selected no inputs — the wallet has no spendable L-BTC \
                 (has the deposit confirmed and synced?)"
                    .to_string(),
            ));
        }

        let proposal_json = serde_json::json!({
            "recipient": destination.to_string(),
            "recipient_amount_sat": drained_sat,
            "kind": "migration",
            "asset": "L-BTC",
            "fee_rate_sat_vb": fee_rate_sat_vb,
            "fee_sat": fee_sat,
            "input_count": input_count,
        });
        let coin_selection_json = serde_json::json!({
            "selected": [],
            "outputs": [{ "address": destination.to_string(), "amount_sat": drained_sat }],
            "fee_sat": fee_sat,
            "total_input_sat": drained_sat + fee_sat,
            "note": "LWK-driven drain sweep.",
        });

        Ok(BuiltLiquidProposal {
            pset_b64,
            proposal_json,
            coin_selection_json,
        })
    }

    /// Preview a full-balance drain to `recipient`: the net L-BTC (sats) a
    /// send-max would deliver (balance − mining fee), without signing or
    /// broadcasting. Drives the Send page's **Max** button. Uses the same LWK
    /// `drain_lbtc` builder as [`Self::build_migration_pset`] so the previewed
    /// amount matches the sweep that follows.
    ///
    /// # Errors
    /// [`ElementsWalletError::BuildPset`] if LWK cannot build the drain (e.g. no
    /// spendable balance, or the fee would exceed the value).
    pub async fn compute_drain_amount(
        &self,
        recipient: &Address,
        fee_rate_sat_vb: u64,
    ) -> Result<u64, ElementsWalletError> {
        let balance = self.lbtc_balance_sat().await?;
        let fee_sat = {
            let wollet = self.inner.lock().await;
            let lwk_fee_rate = {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let v = (fee_rate_sat_vb as f64 * 1000.0) as f32;
                Some(v)
            };
            let pset = TxBuilder::new(self.network.to_lwk())
                .drain_lbtc_wallet()
                .drain_lbtc_to(recipient.clone())
                .fee_rate(lwk_fee_rate)
                .finish(&wollet)
                .map_err(|e| ElementsWalletError::BuildPset(e.to_string()))?;
            drop(wollet);
            // The fee output is the explicit, script-less output; the rest is
            // swept to the recipient, so drained = balance − fee.
            pset.outputs()
                .iter()
                .find(|o| o.script_pubkey.is_empty())
                .and_then(|o| o.amount)
                .unwrap_or(0)
        };
        Ok(balance.saturating_sub(fee_sat))
    }

    /// Build a **Send-Max** proposal: drain the whole L-BTC balance to
    /// `recipient` (fee out of the swept amount). Same drain as
    /// [`Self::build_migration_pset`] but labelled as a regular send, so the
    /// Send page's Max button produces a normal (non-migration) proposal.
    ///
    /// # Errors
    /// [`ElementsWalletError::BuildPset`] if LWK cannot build the drain.
    pub async fn build_drain_proposal(
        &self,
        recipient: &Address,
        fee_rate_sat_vb: Option<u64>,
    ) -> Result<BuiltLiquidProposal, ElementsWalletError> {
        let (pset_b64, drained_sat, fee_sat, input_count) = {
            let wollet = self.inner.lock().await;
            let policy = wollet.policy_asset();
            let balance = wollet
                .balance()
                .map_err(|e| ElementsWalletError::Balance(e.to_string()))?
                .get(&policy)
                .copied()
                .unwrap_or(0);
            let lwk_fee_rate = fee_rate_sat_vb.map(|sat_vb| {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let v = (sat_vb as f64 * 1000.0) as f32;
                v
            });
            let pset = TxBuilder::new(self.network.to_lwk())
                .drain_lbtc_wallet()
                .drain_lbtc_to(recipient.clone())
                .fee_rate(lwk_fee_rate)
                .finish(&wollet)
                .map_err(|e| ElementsWalletError::BuildPset(e.to_string()))?;
            let input_count = pset.inputs().len();
            // The fee output is the explicit, script-less output; everything else
            // is swept to the recipient, so drained = balance − fee.
            let fee_sat = pset
                .outputs()
                .iter()
                .find(|o| o.script_pubkey.is_empty())
                .and_then(|o| o.amount)
                .unwrap_or(0);
            drop(wollet);
            (
                pset.to_string(),
                balance.saturating_sub(fee_sat),
                fee_sat,
                input_count,
            )
        };

        // A drain that selected no inputs means the wallet holds no spendable
        // L-BTC — surface it rather than persist a confusing zero-value send.
        if input_count == 0 {
            return Err(ElementsWalletError::BuildPset(
                "send-max selected no inputs — the wallet has no spendable L-BTC \
                 (has the deposit confirmed and synced?)"
                    .to_string(),
            ));
        }

        let proposal_json = serde_json::json!({
            "recipient": recipient.to_string(),
            "recipient_amount_sat": drained_sat,
            "send_max": true,
            "asset": "L-BTC",
            "fee_rate_sat_vb": fee_rate_sat_vb,
            "fee_sat": fee_sat,
            "input_count": input_count,
        });
        let coin_selection_json = serde_json::json!({
            "selected": [],
            "outputs": [{ "address": recipient.to_string(), "amount_sat": drained_sat }],
            "fee_sat": fee_sat,
            "total_input_sat": drained_sat + fee_sat,
            "note": "LWK-driven send-max drain.",
        });

        Ok(BuiltLiquidProposal {
            pset_b64,
            proposal_json,
            coin_selection_json,
        })
    }

    /// Merge a cosigner's partial PSET into the canonical base PSET.
    /// Returns the merged PSET (still as base64) and a flag indicating
    /// whether [`Wollet::finalize`] succeeds against the merged result.
    ///
    /// # Errors
    /// - [`ElementsWalletError::BadPset`] on parse failures.
    /// - [`ElementsWalletError::MergePset`] if [`Pset::merge`] rejects.
    pub async fn merge_partial_pset(
        &self,
        base_pset_b64: &str,
        partial_b64: &str,
    ) -> Result<MergedLiquidPset, ElementsWalletError> {
        let mut base = Pset::from_str(base_pset_b64)
            .map_err(|e| ElementsWalletError::BadPset(e.to_string()))?;
        let partial =
            Pset::from_str(partial_b64).map_err(|e| ElementsWalletError::BadPset(e.to_string()))?;
        base.merge(partial)
            .map_err(|e| ElementsWalletError::MergePset(e.to_string()))?;

        // Probe finalize on a clone.
        let wollet = self.inner.lock().await;
        let mut probe = base.clone();
        let fully_signed = wollet.finalize(&mut probe).is_ok();
        drop(wollet);

        Ok(MergedLiquidPset {
            merged_pset_b64: base.to_string(),
            fully_signed,
        })
    }

    /// Finalize a fully-signed PSET and extract the raw transaction.
    ///
    /// # Errors
    /// - [`ElementsWalletError::BadPset`] on parse failures.
    /// - [`ElementsWalletError::Finalize`] if LWK rejects the PSET.
    pub async fn finalize_and_extract(
        &self,
        pset_b64: &str,
    ) -> Result<FinalizedLiquidTx, ElementsWalletError> {
        let mut pset =
            Pset::from_str(pset_b64).map_err(|e| ElementsWalletError::BadPset(e.to_string()))?;
        let wollet = self.inner.lock().await;
        let tx: Transaction = wollet
            .finalize(&mut pset)
            .map_err(|e| ElementsWalletError::Finalize(e.to_string()))?;
        drop(wollet);
        let txid = tx.txid().to_string();
        let tx_hex = serialize_hex(&tx);
        Ok(FinalizedLiquidTx { tx_hex, txid })
    }

    /// Submit a hex-encoded raw Elements transaction via the configured
    /// Esplora endpoint.
    ///
    /// # Errors
    /// - [`ElementsWalletError::NoEsploraEndpoint`] if no endpoint is
    ///   configured.
    /// - [`ElementsWalletError::BroadcastRejected`] on RPC failure.
    pub async fn broadcast_raw(&self, tx_hex: &str) -> Result<String, ElementsWalletError> {
        let bytes = hex_decode(tx_hex)
            .map_err(|e| ElementsWalletError::BroadcastRejected(format!("invalid hex: {e}")))?;

        match self.backend {
            ElementsChainBackend::Esplora | ElementsChainBackend::Waterfalls => {
                let esplora_url = self
                    .esplora_url
                    .as_deref()
                    .ok_or(ElementsWalletError::NoEsploraEndpoint)?;
                let tx: Transaction =
                    emvault::elements::lwk_wollet::elements::encode::deserialize(&bytes)
                        .map_err(|e| ElementsWalletError::BroadcastRejected(e.to_string()))?;
                let client = EsploraClient::new(self.network.to_lwk(), esplora_url);
                let txid = client
                    .broadcast(&tx)
                    .await
                    .map_err(|e| ElementsWalletError::BroadcastRejected(e.to_string()))?;
                Ok(txid.to_string())
            }
            ElementsChainBackend::Electrum => {
                let electrum_url = self
                    .electrum_url
                    .clone()
                    .ok_or(ElementsWalletError::NoEsploraEndpoint)?;
                // Blocking Electrum client — broadcast on a blocking thread,
                // mirroring the sync path.
                tokio::task::spawn_blocking(move || -> Result<String, String> {
                    let tx: Transaction =
                        emvault::elements::lwk_wollet::elements::encode::deserialize(&bytes)
                            .map_err(|e| e.to_string())?;
                    let url = ElectrumUrl::from_str(&electrum_url).map_err(|e| e.to_string())?;
                    let client = ElectrumClient::new(&url).map_err(|e| e.to_string())?;
                    client
                        .broadcast(&tx)
                        .map(|t| t.to_string())
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| {
                    ElementsWalletError::BroadcastRejected(format!(
                        "electrum broadcast task failed: {e}"
                    ))
                })?
                .map_err(ElementsWalletError::BroadcastRejected)
            }
        }
    }
}

fn is_address_for_network(addr: &Address, network: ElementsNetwork) -> bool {
    let want: &AddressParams = network.address_params();
    std::ptr::eq(addr.params, want) || addr.params == want
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err(format!("odd-length hex string ({} chars)", hex.len()));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = nibble(bytes[i])?;
        let lo = nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn nibble(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("not a hex digit: 0x{c:02x}")),
    }
}

// ---------------------------------------------------------------------------
// View-model types
// ---------------------------------------------------------------------------

/// One row in the federation detail page's address table (Liquid variant).
#[derive(Debug, Clone)]
pub struct RevealedLiquidAddress {
    /// Index on the external keychain.
    pub index: u32,
    /// Confidential address rendered in the user's network format.
    pub address: String,
}

/// Per-address L-BTC activity, the Elements analog of the Bitcoin
/// address-history view. Produced by
/// [`LiquidFederationWallet::address_activity`].
#[derive(Debug, Clone, Default)]
pub struct LiquidAddressActivity {
    /// Total L-BTC (sats) ever received at this address, across all history.
    pub received_sat: u64,
    /// L-BTC (sats) still unspent at this address.
    pub unspent_sat: u64,
    /// One entry per wallet-owned output paid to this address.
    pub receipts: Vec<LiquidReceipt>,
}

/// A single output paid to a watched Liquid address.
#[derive(Debug, Clone)]
pub struct LiquidReceipt {
    /// Funding transaction id.
    pub txid: Txid,
    /// Output index within the funding transaction.
    pub vout: u32,
    /// Unblinded L-BTC amount (sats).
    pub amount_sat: u64,
    /// Confirmation height, or `None` if still in the mempool.
    pub height: Option<u32>,
    /// `true` once the output has been spent.
    pub is_spent: bool,
}

/// Output of [`LiquidFederationWallet::build_proposal`].
#[derive(Debug, Clone)]
pub struct BuiltLiquidProposal {
    /// Base64-encoded unsigned PSET.
    pub pset_b64: String,
    /// Structural view (recipient, amount, asset). Stored in
    /// `proposal_json`.
    pub proposal_json: serde_json::Value,
    /// Coin selection breakdown. LWK doesn't expose its internal coin
    /// selection in v1; we persist a placeholder shape so the column
    /// stays non-null and the UI doesn't crash on it.
    pub coin_selection_json: serde_json::Value,
}

/// Output of [`LiquidFederationWallet::merge_partial_pset`].
#[derive(Debug, Clone)]
pub struct MergedLiquidPset {
    /// Merged base PSET (base64).
    pub merged_pset_b64: String,
    /// `true` iff a finalize probe against the merged PSET succeeded.
    pub fully_signed: bool,
}

/// Output of [`LiquidFederationWallet::finalize_and_extract`].
#[derive(Debug, Clone)]
pub struct FinalizedLiquidTx {
    /// Hex-encoded raw Elements transaction.
    pub tx_hex: String,
    /// Transaction id.
    pub txid: String,
}
