//! Taproot federation coverage — no hardware, no node, no database.
//!
//! The app gates taproot *creation* to all-Ledger signers, and its only software
//! signer (`TestExternalSigner::sign_for_test`) is P2WSH-only, so an in-app
//! taproot **spend** needs a real Ledger and can't run in CI. What we prove here
//! is everything up to (but not including) hardware signing — the taproot build
//! path the handler calls (`emvault::core::build_federation_taproot_with`):
//!
//!   * it emits a `tr(NUMS, multi_a(m, …))` multipath descriptor,
//!   * with a 32-byte NUMS chain code,
//!   * a **custom** chain code reproduces the same descriptor (import round-trip)
//!     while a different one changes the internal key (so the scriptPubKeys move),
//!   * and the descriptor derives real P2TR (bech32m) addresses.
//!
//! Descriptor-algebra edge cases (signer rotation, threshold change, ranged vs
//! fixed) are covered by emvault-core's own unit tests; this file is the
//! app-level guard that the wiring the handler depends on stays correct.

use std::str::FromStr;

use emvault::core::bitcoin::Network;
use emvault::core::bitcoin::bip32::DerivationPath;
use emvault::core::miniscript::{Descriptor, DescriptorPublicKey};
use emvault::core::{NetworkType, NumsChaincode, build_federation_taproot_with};
use emvault::xpub::DeviceType;
use emvault::xpub::signer::ExternalSigner;
use emvault::xpub::test_utils::TestExternalSigner;

const MNEMONICS: [&str; 3] = [
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    "legal winner thank year wave sausage worth useful legal winner thank yellow",
    "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
];

const FED_PATH: &str = "m/48'/1'/0'/2'";

/// Three deterministic external (XPUB) signers on the federation path.
fn signers() -> Vec<ExternalSigner> {
    let path: DerivationPath = FED_PATH.parse().expect("federation path");
    MNEMONICS
        .iter()
        .enumerate()
        .map(|(i, m)| {
            TestExternalSigner::from_mnemonic(
                m,
                "", // empty passphrase → deterministic xpubs (reproducibility matters here)
                &path,
                Network::Regtest,
                DeviceType::Ledger,
                Some(format!("ledger-{}", i + 1)),
            )
            .expect("build test signer")
            .external_signer()
            .clone()
        })
        .collect()
}

fn net() -> NetworkType {
    NetworkType::Bitcoin(Network::Regtest)
}

#[test]
fn taproot_build_emits_tr_multi_a_with_nums_chaincode() {
    let (built, chaincode) =
        build_federation_taproot_with(signers(), 2, net(), NumsChaincode::Random)
            .expect("build 2-of-3 taproot federation");

    let d = &built.descriptor_string;
    assert!(
        d.starts_with("tr("),
        "taproot descriptor must start with tr(: {d}"
    );
    assert!(
        d.contains("multi_a(2,"),
        "taproot federation must use multi_a with the threshold: {d}"
    );
    // Multipath, ranged: the `<0;1>/*` receive/change split with a wildcard.
    assert!(
        d.contains("/<0;1>/*"),
        "expected a ranged multipath descriptor (/<0;1>/*): {d}"
    );
    assert_eq!(chaincode.len(), 32, "NUMS chain code must be 32 bytes");
    // A NUMS internal key with a *non-zero* chain code (Random must not degenerate).
    assert_ne!(chaincode, [0u8; 32], "NUMS chain code must not be all-zero");
}

#[test]
fn taproot_custom_chaincode_is_reproducible_and_binding() {
    let cc = [0x11u8; 32];

    let (a, ra) = build_federation_taproot_with(signers(), 2, net(), NumsChaincode::Custom(cc))
        .expect("build A");
    let (b, rb) = build_federation_taproot_with(signers(), 2, net(), NumsChaincode::Custom(cc))
        .expect("build B");

    // Same signers + same custom chain code ⇒ byte-identical descriptor: this is
    // what lets a device re-derive/verify an imported taproot vault.
    assert_eq!(ra, cc, "resolved chain code must echo the custom input");
    assert_eq!(rb, cc);
    assert_eq!(
        a.descriptor_string, b.descriptor_string,
        "custom NUMS chain code must reproduce the same descriptor"
    );

    // A different chain code moves the NUMS internal key ⇒ a different descriptor
    // (and therefore different scriptPubKeys / addresses).
    let (c, _) =
        build_federation_taproot_with(signers(), 2, net(), NumsChaincode::Custom([0x22u8; 32]))
            .expect("build C");
    assert_ne!(
        a.descriptor_string, c.descriptor_string,
        "a different NUMS chain code must change the descriptor"
    );
}

#[test]
fn taproot_descriptor_derives_p2tr_addresses() {
    let (built, _) = build_federation_taproot_with(signers(), 2, net(), NumsChaincode::Random)
        .expect("build taproot federation");

    // Split the `<0;1>` multipath into its external/internal single descriptors and
    // derive the first receive address; a taproot output is bech32m (`bcrt1p…`).
    let desc = Descriptor::<DescriptorPublicKey>::from_str(&built.descriptor_string)
        .expect("parse multipath taproot descriptor");
    let singles = desc
        .into_single_descriptors()
        .expect("split multipath descriptor");
    assert_eq!(singles.len(), 2, "expected external + change descriptors");

    let external = &singles[0];
    let addr0 = external
        .at_derivation_index(0)
        .expect("derive index 0")
        .address(Network::Regtest)
        .expect("taproot address");
    assert!(
        addr0.to_string().starts_with("bcrt1p"),
        "taproot receive address must be P2TR (bcrt1p…): {addr0}"
    );

    // Distinct indices ⇒ distinct addresses (ranged derivation actually ranges).
    let addr1 = external
        .at_derivation_index(1)
        .expect("derive index 1")
        .address(Network::Regtest)
        .expect("taproot address");
    assert_ne!(
        addr0, addr1,
        "consecutive indices must derive distinct addresses"
    );
}
