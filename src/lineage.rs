//! Lineage-scoped wallet view (Phase 2).
//!
//! Adopts asterism-core's [`BtcFederatedWallet`]: a wallet becomes a **lineage**
//! of federation **versions**, each paired with its own watch-only `bdk` wallet.
//! This is the version-aware **read/query** surface (per-version + aggregate
//! balances, UTXOs, signer membership) backing requirements 1, 6 & 7.
//!
//! The operational per-version wallet (sync / build / sign) stays
//! [`crate::wallet::FederationWallet`]; existing handlers keep using it against
//! the current version, so a single-version lineage behaves exactly as before.
//! Reconstruction (DB rows → `BtcFederatedWallet`) is driven by
//! [`crate::wallet::WalletManager::load_lineage`]. See
//! `emerald_multisignature/xpub_federation_migration.md` §4, §6.
//!
//! Consumed by the lineage/visibility/relay handlers from Phase 5 onward; the
//! items here are scaffolding until then.

#![allow(dead_code)]

use asterism_core::{BtcFederatedWallet, Federation, FederatedWallet, NetworkType};
use asterism_xpub::ExternalSigner;
use bdk_wallet::{ChangeSet, Wallet};
use bitcoin::{Amount, Network};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::handlers::new_federation::parse_device_type;
use crate::models::{SignerRow, UserRow};
use crate::wallet::WalletError;

/// A lineage's federation versions stacked into one [`BtcFederatedWallet`],
/// with the version `federation_id`s tracked alongside (index-aligned, oldest
/// first) so callers can map between the trait's positional view and DB ids.
pub struct LineageWallet {
    lineage_id: Uuid,
    inner: BtcFederatedWallet<ExternalSigner>,
    /// `federations.id` per version, aligned with `inner`'s order (oldest = 0).
    version_ids: Vec<Uuid>,
}

impl LineageWallet {
    /// Stack pre-reconstructed `(federation_id, federation, wallet)` versions,
    /// oldest first, into a lineage wallet. Offline-constructible (no DB) so the
    /// stacking and accessors are unit-testable.
    ///
    /// # Errors
    ///
    /// - [`WalletError::NotFound`] if `versions` is empty (a lineage always has
    ///   at least v0).
    /// - [`WalletError::ReconstructFederation`] if a version's network is
    ///   non-Bitcoin or mismatches the lineage (surfaced from
    ///   `BtcFederatedWallet`).
    pub fn from_versions(
        lineage_id: Uuid,
        versions: Vec<(Uuid, Federation<ExternalSigner>, Wallet)>,
    ) -> Result<Self, WalletError> {
        let mut versions = versions.into_iter();
        let (first_id, first_fed, first_wallet) =
            versions.next().ok_or(WalletError::NotFound(lineage_id))?;
        let mut inner = BtcFederatedWallet::new(first_fed, first_wallet).map_err(|e| {
            WalletError::ReconstructFederation {
                id: first_id,
                reason: e.to_string(),
            }
        })?;
        let mut version_ids = vec![first_id];
        for (id, federation, wallet) in versions {
            inner =
                inner
                    .with_federation(federation, wallet)
                    .map_err(|e| WalletError::ReconstructFederation {
                        id,
                        reason: e.to_string(),
                    })?;
            version_ids.push(id);
        }
        Ok(Self {
            lineage_id,
            inner,
            version_ids,
        })
    }

    /// The lineage this wallet represents.
    #[must_use]
    pub fn lineage_id(&self) -> Uuid {
        self.lineage_id
    }

    /// Number of federation versions in the lineage.
    #[must_use]
    pub fn version_count(&self) -> usize {
        self.inner.federation_count()
    }

    /// The current (newest) version's `federation_id`.
    #[must_use]
    pub fn current_version_id(&self) -> Uuid {
        *self
            .version_ids
            .last()
            .expect("LineageWallet always has at least one version")
    }

    /// The `federation_id` at stack position `index` (0 = oldest).
    #[must_use]
    pub fn version_id_at(&self, index: usize) -> Option<Uuid> {
        self.version_ids.get(index).copied()
    }

    /// Stack position of a version by `federation_id`.
    #[must_use]
    pub fn index_of(&self, federation_id: Uuid) -> Option<usize> {
        self.version_ids.iter().position(|id| *id == federation_id)
    }

    /// Total confirmed balance across all versions.
    #[must_use]
    pub fn total_balance(&self) -> Amount {
        self.inner.total_balance()
    }

    /// Confirmed balance of the version at stack position `index`.
    #[must_use]
    pub fn balance_at(&self, index: usize) -> Option<Amount> {
        self.inner.balance_at(index)
    }

    /// The underlying [`FederatedWallet`] for advanced reads (`find_by_signer`,
    /// `signer_is_current`, per-version UTXOs).
    #[must_use]
    pub fn federated(&self) -> &BtcFederatedWallet<ExternalSigner> {
        &self.inner
    }
}

/// Reconstruct a version's [`Federation`] from its members' stored signers.
///
/// Mirrors the federation-construction path in
/// [`crate::handlers::new_federation`]: each member's `descriptor_key` is parsed
/// back into an [`ExternalSigner`]. The bdk wallet is built separately from the
/// row's descriptor (see [`build_version_wallet`]); this only recovers the
/// signer set so membership queries (`find_by_signer` / `signer_is_current`)
/// work across versions.
///
/// # Errors
///
/// [`WalletError::ReconstructFederation`] if a member has no recorded signer,
/// a `descriptor_key` no longer parses, the threshold is out of range, or
/// `Federation::new` rejects the parameters.
pub fn reconstruct_federation(
    federation_id: Uuid,
    threshold: i32,
    network: Network,
    members: &[(UserRow, Option<SignerRow>)],
) -> Result<Federation<ExternalSigner>, WalletError> {
    let reconstruct_err = |reason: String| WalletError::ReconstructFederation {
        id: federation_id,
        reason,
    };

    let mut signers = Vec::with_capacity(members.len());
    for (_user, signer) in members {
        let signer = signer
            .as_ref()
            .ok_or_else(|| reconstruct_err("a member has no recorded signer".to_owned()))?;
        let external = ExternalSigner::from_descriptor_key(
            signer.descriptor_key.trim(),
            network,
            parse_device_type(&signer.device_type),
            signer.label.clone(),
        )
        .map_err(|e| reconstruct_err(format!("signer {}: {e}", signer.id)))?;
        signers.push(external);
    }

    let threshold = u32::try_from(threshold)
        .map_err(|_| reconstruct_err(format!("threshold {threshold} out of range")))?;

    Federation::new(threshold, signers, NetworkType::Bitcoin(network))
        .map_err(|e| reconstruct_err(e.to_string()))
}

/// Build a **watch-only** bdk wallet for a version: from its persisted
/// `bdk_changeset` if present, else fresh from its descriptor. Read-only — it
/// does not persist (unlike [`crate::wallet::WalletManager::load_or_init`],
/// whose fresh-init branch this mirrors).
///
/// # Errors
///
/// [`WalletError::DecodeChangeSet`] / [`WalletError::LoadWallet`] /
/// [`WalletError::EmptyChangeSet`] for a malformed or unusable changeset, or
/// [`WalletError::CreateWallet`] if the descriptor is rejected.
pub fn build_version_wallet(
    federation_id: Uuid,
    network: Network,
    descriptor: &str,
    changeset: Option<JsonValue>,
) -> Result<Wallet, WalletError> {
    if let Some(json) = changeset {
        let aggregate: ChangeSet =
            serde_json::from_value(json).map_err(|source| WalletError::DecodeChangeSet {
                id: federation_id,
                source,
            })?;
        Wallet::load()
            .check_network(network)
            .load_wallet_no_persist(aggregate)
            .map_err(|source| WalletError::LoadWallet {
                id: federation_id,
                source,
            })?
            .ok_or(WalletError::EmptyChangeSet(federation_id))
    } else {
        Wallet::create_from_two_path_descriptor(descriptor.to_owned())
            .network(network)
            .create_wallet_no_persist()
            .map_err(|source| WalletError::CreateWallet {
                id: federation_id,
                source: Box::new(source),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_core::Signer;
    use asterism_xpub::DeviceType;
    use bitcoin::bip32::{DerivationPath, Xpriv, Xpub};
    use bitcoin::secp256k1::Secp256k1;

    /// A deterministic, valid testnet descriptor-key string for a seed byte —
    /// the same shape a hardware wallet exports (`[fp/48h/1h/0h/2h]tpub…`).
    fn descriptor_key(seed: u8) -> String {
        let secp = Secp256k1::new();
        let xpriv = Xpriv::new_master(Network::Testnet, &[seed; 32]).unwrap();
        let path: DerivationPath = "m/48'/1'/0'/2'".parse().unwrap();
        let derived = xpriv.derive_priv(&secp, &path).unwrap();
        let xpub = Xpub::from_priv(&secp, &derived);
        let fp = xpriv.fingerprint(&secp);
        format!("[{fp}/48h/1h/0h/2h]{xpub}")
    }

    fn signer(seed: u8) -> ExternalSigner {
        ExternalSigner::from_descriptor_key(
            &descriptor_key(seed),
            Network::Testnet,
            DeviceType::Trezor,
            None,
        )
        .expect("valid descriptor key")
    }

    fn federation(signers: Vec<ExternalSigner>) -> Federation<ExternalSigner> {
        Federation::new(2, signers, NetworkType::Bitcoin(Network::Testnet))
            .expect("valid federation")
    }

    fn watch_wallet(fed: &Federation<ExternalSigner>) -> Wallet {
        Wallet::create_single(fed.descriptor().to_string())
            .network(Network::Testnet)
            .create_wallet_no_persist()
            .expect("valid watch-only wallet")
    }

    #[test]
    fn stacks_versions_and_maps_ids() {
        let (s1, s2, s3, s4) = (signer(1), signer(2), signer(3), signer(4));
        // v0 = {s1,s2,s3}; v1 = {s1,s3,s4} (remove s2, add s4).
        let v0 = federation(vec![s1.clone(), s2.clone(), s3.clone()]);
        let v1 = federation(vec![s1.clone(), s3.clone(), s4.clone()]);
        let (w0, w1) = (watch_wallet(&v0), watch_wallet(&v1));
        let (id0, id1, lineage) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());

        let lw = LineageWallet::from_versions(lineage, vec![(id0, v0, w0), (id1, v1, w1)]).unwrap();

        assert_eq!(lw.version_count(), 2);
        assert_eq!(lw.current_version_id(), id1);
        assert_eq!(lw.version_id_at(0), Some(id0));
        assert_eq!(lw.index_of(id1), Some(1));
        assert_eq!(lw.total_balance(), Amount::ZERO);
        assert_eq!(lw.balance_at(0), Some(Amount::ZERO));
    }

    #[test]
    fn membership_is_per_version_signing_is_current_only() {
        // Encodes requirements 6 & 7 at the wallet layer (complementing the
        // Phase-1 DB-layer test): s2 removed at v0→v1, s4 added at v1.
        let (s1, s2, s3, s4) = (signer(1), signer(2), signer(3), signer(4));
        let v0 = federation(vec![s1.clone(), s2.clone(), s3.clone()]);
        let v1 = federation(vec![s1.clone(), s3.clone(), s4.clone()]);
        let (w0, w1) = (watch_wallet(&v0), watch_wallet(&v1));
        let lw = LineageWallet::from_versions(
            Uuid::new_v4(),
            vec![(Uuid::new_v4(), v0, w0), (Uuid::new_v4(), v1, w1)],
        )
        .unwrap();
        let fed = lw.federated();

        // s2 (removed) belongs to v0 only and is not current; s4 (added) is current.
        assert_eq!(fed.find_by_signer(&s2.id()).len(), 1);
        assert!(!fed.signer_is_current(&s2.id()));
        assert!(fed.signer_is_current(&s4.id()));
        // s1 (kept) is in both versions and current.
        assert_eq!(fed.find_by_signer(&s1.id()).len(), 2);
        assert!(fed.signer_is_current(&s1.id()));
    }
}
