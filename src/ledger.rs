//! Ledger wallet-policy (BIP-388) registration helpers.
//!
//! Like Jade, a Ledger device must have a multisig wallet **registered** (the
//! user confirms the policy on-device, and the app receives an HMAC it caches)
//! before the device will sign that wallet's inputs. Ledger uses the
//! [BIP-388 wallet-policy] form rather than a raw descriptor:
//!
//! ```text
//! descriptor_template : wsh(sortedmulti(2,@0/**,@1/**,@2/**))
//! keys                : [ "[8c9b54d0/48'/1'/0'/2']tpub…",  … ]
//! ```
//!
//! The `@i` placeholders index into `keys` positionally; `/**` expands to the
//! `/<0;1>/*` receive+change wildcard. This is the P2WSH (`wsh`) shape that our
//! existing federations already use — the Taproot (`tr(NUMS,multi_a)`) template
//! is a later phase.
//!
//! We build the policy **server-side** (typed + testable) from the federation's
//! member signer rows and emit a JSON-friendly [`LedgerWalletPolicy`]. The
//! browser hands `descriptor_template` + `keys` straight to `ledger-bitcoin`'s
//! `WalletPolicy` and calls `registerWallet` / `signPsbt`.
//!
//! Mirrors [`crate::jade`]; see `plans/passport-prime-integration.md` and the
//! HW descriptor matrix for the wider external-signer picture.
//!
//! [BIP-388 wallet-policy]: https://github.com/bitcoin/bips/blob/master/bip-0388.mediawiki

use serde::Serialize;

use emvault::core::NetworkType;
use emvault::core::bitcoin::Network;
use emvault::xpub::ExternalSigner;

use crate::handlers::common::parse_device_type;
use crate::models::SignerRow;

/// A Ledger wallet-policy registration payload (JSON-friendly form of
/// `ledger-bitcoin`'s `WalletPolicy`), plus the device-facing `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerWalletPolicy {
    /// 1–64 ASCII chars; the policy name shown/stored on the device. The name
    /// is part of what the on-device HMAC commits to, so it is versioned per
    /// federation version (see [`ledger_reg_name`]).
    pub name: String,
    /// BIP-388 descriptor template, e.g. `wsh(sortedmulti(2,@0/**,@1/**))`.
    pub descriptor_template: String,
    /// Key-origin strings (`[fingerprint/derivation]xpub`) in federation order,
    /// referenced positionally by the `@i` placeholders in the template.
    pub keys: Vec<String>,
}

/// Errors building a Ledger wallet-policy registration.
#[derive(Debug, thiserror::Error)]
pub enum LedgerRegisterError {
    /// Threshold didn't fit a `u32` or was zero / greater than the signer count.
    #[error("invalid threshold {threshold} for {signers} signers")]
    BadThreshold {
        /// Requested threshold.
        threshold: i32,
        /// Number of signers.
        signers: usize,
    },
    /// No cosigners were supplied.
    #[error("federation has no onboarded signers to register")]
    NoSigners,
    /// Taproot policy assembly failed (signer parse or descriptor build).
    #[error("failed to build taproot policy: {0}")]
    Build(String),
}

/// Ledger's wallet-policy name limit, in bytes (BIP-388 / app constraint).
const LEDGER_NAME_MAX: usize = 64;

/// Device-safe Ledger wallet-policy name for a federation **version**:
/// `{label}-v{version}`, with `version` 1-indexed (so `version_index` `0` → `v1`).
///
/// Sanitized to ASCII alphanumerics and truncated so the whole name always fits
/// Ledger's 64-char limit. Versioning the name means a new federation version
/// registers under a **new** on-device policy (and a fresh HMAC) instead of
/// colliding with the prior version's registration.
#[must_use]
pub fn ledger_reg_name(label: &str, version_index: i32) -> String {
    let version = version_index.saturating_add(1);
    let version_str = version.to_string();
    // Reserve room for "-v" + the version digits so the total stays ≤ 64.
    let max_base = LEDGER_NAME_MAX.saturating_sub(2 + version_str.len());
    let base: String = label
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(max_base)
        .collect();
    let base = if base.is_empty() {
        "fed".to_string()
    } else {
        base
    };
    format!("{base}-v{version_str}")
}

/// Canonical `[fingerprint/derivation]xpub` key-origin string for one cosigner,
/// built from the typed columns (not the stored `descriptor_key`) so the origin
/// format is deterministic regardless of how each device exported it. The
/// leading `m/` is stripped from the path, as BIP-388 keys expect.
fn key_origin(s: &SignerRow) -> String {
    let path = s
        .derivation_path
        .trim_start_matches("m/")
        .trim_start_matches('m');
    format!("[{}/{}]{}", s.fingerprint, path, s.xpub)
}

/// Build a [`LedgerWalletPolicy`] from a federation's member signer rows.
///
/// `label` and `version_index` name the on-device registration
/// (`{label}-v{version_index + 1}`, see [`ledger_reg_name`]). `cosigners` are the
/// federation members' [`SignerRow`]s (any device type — Ledger only needs the
/// public key-origin data). `threshold` is the federation's `m`. Produces a
/// `wsh(sortedmulti(m, @0/**, …))` policy.
///
/// # Errors
/// [`LedgerRegisterError`] if there are no signers or the threshold is out of
/// range for the signer count.
pub fn build_ledger_policy(
    label: &str,
    version_index: i32,
    threshold: i32,
    cosigners: &[SignerRow],
) -> Result<LedgerWalletPolicy, LedgerRegisterError> {
    if cosigners.is_empty() {
        return Err(LedgerRegisterError::NoSigners);
    }
    let n = cosigners.len();
    let threshold_u32 = u32::try_from(threshold)
        .ok()
        .filter(|m| *m >= 1 && usize::try_from(*m).is_ok_and(|m| m <= n));
    let Some(threshold_u32) = threshold_u32 else {
        return Err(LedgerRegisterError::BadThreshold {
            threshold,
            signers: n,
        });
    };

    let placeholders = (0..n)
        .map(|i| format!("@{i}/**"))
        .collect::<Vec<_>>()
        .join(",");
    let descriptor_template = format!("wsh(sortedmulti({threshold_u32},{placeholders}))");

    let keys = cosigners.iter().map(key_origin).collect();

    Ok(LedgerWalletPolicy {
        name: ledger_reg_name(label, version_index),
        descriptor_template,
        keys,
    })
}

/// Build a Ledger wallet policy for a **Taproot** (`tr(NUMS-xpub, multi_a)`)
/// federation, sourced from `emvault-core`'s `bip388_taproot_policy` so the
/// template + key order + NUMS xpub match the funded scriptPubKeys exactly
/// (single source of truth). `chaincode` is the federation's stored
/// `nums_chaincode`; `network` its Bitcoin network.
///
/// Produces `tr(@0/**, multi_a(m, @1/**, …))` with `@0` = the NUMS xpub and the
/// cosigners in descriptor order.
///
/// # Errors
/// [`LedgerRegisterError`] if there are no signers, the threshold is out of
/// range, a signer's descriptor key won't parse, or the core policy build fails.
pub fn build_ledger_taproot_policy(
    label: &str,
    version_index: i32,
    threshold: i32,
    cosigners: &[SignerRow],
    network: Network,
    chaincode: [u8; 32],
) -> Result<LedgerWalletPolicy, LedgerRegisterError> {
    if cosigners.is_empty() {
        return Err(LedgerRegisterError::NoSigners);
    }
    let n = cosigners.len();
    let threshold_u32 = u32::try_from(threshold)
        .ok()
        .filter(|m| *m >= 1 && usize::try_from(*m).is_ok_and(|m| m <= n))
        .ok_or(LedgerRegisterError::BadThreshold {
            threshold,
            signers: n,
        })?;

    // Parse each stored signer row into an `ExternalSigner`, then let core
    // assemble the policy with the identical setup used to build the funded
    // descriptor (single source of truth).
    let mut signers = Vec::with_capacity(n);
    for row in cosigners {
        let signer = ExternalSigner::from_descriptor_key(
            row.descriptor_key.trim(),
            network,
            parse_device_type(&row.device_type),
            row.label.clone(),
        )
        .map_err(|e| LedgerRegisterError::Build(e.to_string()))?;
        signers.push(signer);
    }

    let policy = emvault::core::bip388_taproot_policy(
        &signers,
        threshold_u32,
        NetworkType::Bitcoin(network),
        chaincode,
    )
    .map_err(|e| LedgerRegisterError::Build(e.to_string()))?;

    Ok(LedgerWalletPolicy {
        name: ledger_reg_name(label, version_index),
        descriptor_template: policy.template,
        keys: policy.keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn signer(fp: &str, xpub: &str) -> SignerRow {
        SignerRow {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            label: None,
            descriptor_key: format!("[{fp}/48'/1'/0'/2']{xpub}"),
            xpub: xpub.to_string(),
            fingerprint: fp.to_string(),
            derivation_path: "m/48'/1'/0'/2'".to_string(),
            device_type: "Ledger".to_string(),
            network: "signet".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn reg_name_is_versioned_and_device_safe() {
        assert_eq!(ledger_reg_name("Federation", 0), "Federation-v1");
        assert_eq!(ledger_reg_name("Federation", 1), "Federation-v2");
        // Non-alphanumerics are stripped (e.g. a legacy label with spaces).
        assert_eq!(ledger_reg_name("My Fed!", 1), "MyFed-v2");
        // Every result stays within Ledger's 1..64 ASCII limit.
        for (label, vi) in [("Federation", 0), ("Federation", 998), ("", 0)] {
            let name = ledger_reg_name(label, vi);
            assert!(
                !name.is_empty() && name.len() <= LEDGER_NAME_MAX,
                "{name:?} must be 1..={LEDGER_NAME_MAX}"
            );
            assert!(name.is_ascii());
        }
        // Empty/blank label falls back rather than producing a bare "-v1".
        assert_eq!(ledger_reg_name("", 0), "fed-v1");
    }

    #[test]
    fn builds_sorted_wsh_policy() {
        let cosigners = vec![
            signer("8c9b54d0", "tpubAAAA"),
            signer("11223344", "tpubBBBB"),
            signer("aabbccdd", "tpubCCCC"),
        ];
        let pol = build_ledger_policy("Federation", 0, 2, &cosigners).unwrap();
        assert_eq!(
            pol.descriptor_template,
            "wsh(sortedmulti(2,@0/**,@1/**,@2/**))"
        );
        assert_eq!(pol.keys.len(), 3);
        assert_eq!(pol.keys[0], "[8c9b54d0/48'/1'/0'/2']tpubAAAA");
        assert_eq!(pol.keys[2], "[aabbccdd/48'/1'/0'/2']tpubCCCC");
        assert_eq!(pol.name, "Federation-v1");
    }

    #[test]
    fn key_origin_strips_leading_m() {
        let s = signer("deadbeef", "tpubZZZZ");
        assert_eq!(key_origin(&s), "[deadbeef/48'/1'/0'/2']tpubZZZZ");
    }

    #[test]
    fn rejects_bad_threshold_and_empty() {
        let one = vec![signer("8c9b54d0", "tpubAAAA")];
        assert!(matches!(
            build_ledger_policy("Fed", 0, 0, &one),
            Err(LedgerRegisterError::BadThreshold { .. })
        ));
        assert!(matches!(
            build_ledger_policy("Fed", 0, 2, &one), // m > n
            Err(LedgerRegisterError::BadThreshold { .. })
        ));
        assert!(matches!(
            build_ledger_policy("Fed", 0, 1, &[]),
            Err(LedgerRegisterError::NoSigners)
        ));
    }
}
