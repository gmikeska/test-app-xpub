//! Jade multisig-registration helpers.
//!
//! Blockstream Jade must have a multisig wallet **registered** (the user
//! confirms it on-device) before it will sign that wallet's inputs. The
//! `@emvault/jade` browser driver's `registerMultisig` forwards either a
//! Coldcard/Sparrow `multisig_file` string *or* a structured descriptor object
//! straight onto Jade's `register_multisig` CBOR params — it does **not** parse
//! descriptor strings. So we build the structured registration here, server-side
//! (typed + testable), from the federation's member signer rows.
//!
//! We emit a **JSON-friendly** shape ([`JadeRegister`]): `fingerprint` stays hex
//! and `derivation_path` stays a `m/...` string. The browser does the final
//! native conversion (hex → bytes, path → `u32[]`) using the driver's exported
//! `hexToBytes` / `pathToU32Array` helpers before calling `registerMultisig`,
//! because JSON cannot carry the CBOR byte-string / integer-array forms Jade
//! ultimately wants.
//!
//! See `emvault_design/jade-integration.md` §3 for the mapping rationale.

use serde::Serialize;

use crate::models::SignerRow;

/// One cosigner in a Jade multisig registration (JSON-friendly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JadeRegisterSigner {
    /// Master fingerprint, lowercase hex (browser converts to 4 bytes).
    pub fingerprint: String,
    /// Origin path to the account xpub, e.g. `m/48'/1'/0'/2'`
    /// (browser converts to a hardened `u32[]`).
    pub derivation_path: String,
    /// Account-level extended public key. Jade derives the `/0/*` and `/1/*`
    /// receive/change wildcards itself from the `variant`.
    pub xpub: String,
}

/// A Jade multisig registration payload (JSON-friendly form of Jade's
/// `descriptor` object), plus the device-facing wallet `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JadeRegister {
    /// 1–15 ASCII chars; the name shown/stored on the device.
    pub name: String,
    /// Jade script variant. P2WSH `sortedmulti` is `"wsh(multi(k))"` + `sorted`.
    pub variant: &'static str,
    /// BIP-67 lexicographic key sorting (`sortedmulti`).
    pub sorted: bool,
    /// Threshold `m` of the m-of-n.
    pub threshold: u32,
    /// One entry per federation member, in federation order (Jade re-sorts).
    pub signers: Vec<JadeRegisterSigner>,
}

/// Errors building a Jade registration.
#[derive(Debug, thiserror::Error)]
pub enum JadeRegisterError {
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
}

/// Device-safe (1–15 ASCII) Jade wallet name for a federation **version**:
/// `{label}-v{version}`, with `version` 1-indexed (so `version_index` `0` → `v1`).
///
/// The lineage `label` is capped at 10 chars at creation (see the federation
/// create handler); here it is additionally sanitized to ASCII alphanumerics
/// and truncated so the whole name always fits Jade's 15-char limit
/// (`label ≤10` + `-v` + up to 3 version digits = ≤15), even for legacy rows
/// whose label predates the cap. Versioning the name means a new federation
/// version registers under a **new** on-device name instead of overwriting the
/// prior version's descriptor — old registrations stay intact.
#[must_use]
pub fn jade_reg_name(label: &str, version_index: i32) -> String {
    let version = version_index.saturating_add(1);
    let version_str = version.to_string();
    // Reserve room for "-v" + the version digits so the total stays ≤ 15.
    let max_base = 15usize.saturating_sub(2 + version_str.len()).min(10);
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

/// Build a [`JadeRegister`] from a federation's member signer rows.
///
/// `label` and `version_index` name the on-device registration
/// (`{label}-v{version_index + 1}`, see [`jade_reg_name`]). `cosigners` are the
/// federation members' [`SignerRow`]s (any device type — Jade only needs the
/// public key-origin data). `threshold` is the federation's `m`. Produces a
/// `wsh(sortedmulti(m, ...))` registration.
///
/// # Errors
/// [`JadeRegisterError`] if there are no signers or the threshold is out of
/// range for the signer count.
pub fn build_jade_register(
    label: &str,
    version_index: i32,
    threshold: i32,
    cosigners: &[SignerRow],
) -> Result<JadeRegister, JadeRegisterError> {
    if cosigners.is_empty() {
        return Err(JadeRegisterError::NoSigners);
    }
    let n = cosigners.len();
    let threshold_u32 = u32::try_from(threshold)
        .ok()
        .filter(|m| *m >= 1 && usize::try_from(*m).is_ok_and(|m| m <= n));
    let Some(threshold_u32) = threshold_u32 else {
        return Err(JadeRegisterError::BadThreshold {
            threshold,
            signers: n,
        });
    };

    let signers = cosigners
        .iter()
        .map(|s| JadeRegisterSigner {
            fingerprint: s.fingerprint.clone(),
            derivation_path: s.derivation_path.clone(),
            xpub: s.xpub.clone(),
        })
        .collect();

    Ok(JadeRegister {
        name: jade_reg_name(label, version_index),
        variant: "wsh(multi(k))",
        sorted: true,
        threshold: threshold_u32,
        signers,
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
            device_type: "Jade".to_string(),
            network: "signet".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn reg_name_is_versioned_and_device_safe() {
        // 1-indexed version from the 0-indexed version_index.
        assert_eq!(jade_reg_name("Federation", 0), "Federation-v1");
        assert_eq!(jade_reg_name("Federation", 1), "Federation-v2");
        // Non-alphanumerics are stripped (e.g. a legacy label with spaces).
        assert_eq!(jade_reg_name("My Fed!", 1), "MyFed-v2");
        // Every result stays within Jade's 1..15 ASCII limit.
        for (label, vi) in [("Federation", 0), ("Federation", 998), ("", 0)] {
            let name = jade_reg_name(label, vi);
            assert!(!name.is_empty() && name.len() <= 15, "{name:?} must be 1..15");
            assert!(name.is_ascii());
        }
        // Empty/blank label falls back rather than producing a bare "-v1".
        assert_eq!(jade_reg_name("", 0), "fed-v1");
        // Stable across calls.
        assert_eq!(jade_reg_name("Federation", 2), jade_reg_name("Federation", 2));
    }

    #[test]
    fn builds_sorted_wsh_registration() {
        let cosigners = vec![
            signer("8c9b54d0", "tpubAAAA"),
            signer("11223344", "tpubBBBB"),
            signer("aabbccdd", "tpubCCCC"),
        ];
        let reg = build_jade_register("Federation", 0, 2, &cosigners).unwrap();
        assert_eq!(reg.variant, "wsh(multi(k))");
        assert!(reg.sorted);
        assert_eq!(reg.threshold, 2);
        assert_eq!(reg.signers.len(), 3);
        assert_eq!(reg.signers[0].fingerprint, "8c9b54d0");
        assert_eq!(reg.signers[0].derivation_path, "m/48'/1'/0'/2'");
        assert_eq!(reg.signers[2].xpub, "tpubCCCC");
        assert_eq!(reg.name, "Federation-v1");
    }

    #[test]
    fn rejects_bad_threshold_and_empty() {
        let one = vec![signer("8c9b54d0", "tpubAAAA")];
        assert!(matches!(
            build_jade_register("Fed", 0, 0, &one),
            Err(JadeRegisterError::BadThreshold { .. })
        ));
        assert!(matches!(
            build_jade_register("Fed", 0, 2, &one), // m > n
            Err(JadeRegisterError::BadThreshold { .. })
        ));
        assert!(matches!(
            build_jade_register("Fed", 0, 1, &[]),
            Err(JadeRegisterError::NoSigners)
        ));
    }
}
