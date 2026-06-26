//! Shared construction of federation artifacts from a signer set.
//!
//! Both federation **creation** (`handlers::new_federation`) and a migration's
//! **next-version** minting (`handlers::migrations`) must turn a set of
//! [`ExternalSigner`]s + a threshold into the exact same canonical multipath
//! descriptor and snapshot. Centralising it here guarantees v0 and vN can never
//! diverge in how they build the descriptor — which is what makes a migration's
//! pending version a faithful successor (Gate G3).

use asterism_core::descriptor::{to_multipath_string, KeyMode};
use asterism_core::{DescriptorBuilder, Federation, FederationSnapshot, NetworkType};
use asterism_xpub::ExternalSigner;

/// The artifacts needed to persist a federation version.
pub struct BuiltFederation {
    /// Canonical multipath `wsh(sortedmulti(...))/<0;1>/*` descriptor string.
    pub descriptor_string: String,
    /// Canonical `FederationSnapshot` JSON.
    pub snapshot_json: serde_json::Value,
}

/// Build the canonical descriptor + snapshot for a ranged P2WSH federation over
/// `signers` with the given `threshold`. The signer order is preserved
/// (`sortedmulti` canonicalises key order in the descriptor regardless).
///
/// # Errors
///
/// Returns a human-readable message if `asterism-core`'s [`DescriptorBuilder`]
/// or [`Federation::new`] rejects the inputs (duplicate xpub, network mismatch,
/// threshold out of range, snapshot serialisation failure).
pub fn build_federation(
    signers: Vec<ExternalSigner>,
    threshold: u32,
    network: NetworkType,
) -> Result<BuiltFederation, String> {
    let mut builder = DescriptorBuilder::new(threshold, network).key_mode(KeyMode::Ranged);
    for s in &signers {
        builder.add_signer(s).map_err(|e| e.to_string())?;
    }
    let descriptor = builder.build().map_err(|e| e.to_string())?;
    let descriptor_string = to_multipath_string(&descriptor);

    let federation = Federation::new(threshold, signers, network)
        .map_err(|e| format!("cannot construct federation: {e}"))?;

    let snapshot_json: serde_json::Value =
        serde_json::from_str(&FederationSnapshot::from_federation(&federation).to_canonical_json())
            .map_err(|e| format!("failed to serialise snapshot: {e}"))?;

    Ok(BuiltFederation {
        descriptor_string,
        snapshot_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_xpub::DeviceType;
    use bitcoin::bip32::{DerivationPath, Xpriv, Xpub};
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::Network;

    fn signer(seed: u8) -> ExternalSigner {
        let secp = Secp256k1::new();
        let xpriv = Xpriv::new_master(Network::Testnet, &[seed; 32]).unwrap();
        let path: DerivationPath = "m/48'/1'/0'/2'".parse().unwrap();
        let derived = xpriv.derive_priv(&secp, &path).unwrap();
        let xpub = Xpub::from_priv(&secp, &derived);
        let fp = xpriv.fingerprint(&secp);
        ExternalSigner::from_descriptor_key(
            &format!("[{fp}/48h/1h/0h/2h]{xpub}"),
            Network::Testnet,
            DeviceType::Trezor,
            None,
        )
        .unwrap()
    }

    #[test]
    fn builds_descriptor_and_snapshot() {
        let built = build_federation(
            vec![signer(1), signer(2), signer(3)],
            2,
            NetworkType::Bitcoin(Network::Testnet),
        )
        .unwrap();
        assert!(built.descriptor_string.starts_with("wsh(sortedmulti(2"));
        assert!(built.snapshot_json.is_object());
    }

    #[test]
    fn same_roster_yields_same_descriptor() {
        let net = NetworkType::Bitcoin(Network::Testnet);
        let a = build_federation(vec![signer(1), signer(2), signer(3)], 2, net).unwrap();
        // Different input order — sortedmulti must canonicalise to the same descriptor.
        let b = build_federation(vec![signer(3), signer(1), signer(2)], 2, net).unwrap();
        assert_eq!(a.descriptor_string, b.descriptor_string);
    }
}
