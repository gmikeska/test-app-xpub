# Changelog

All notable changes to `test-app-xpub` (the xpub / external-signer reference app)
are documented here. This is the app's first CHANGELOG; the `0.2.0` entry
summarizes the current cycle (which tracks the emvault suite **0.8.0** / Bitcoin
Taproot release). Earlier history lives in git.

## [0.3.0] - 2026-08-21

Tracks emvault suite **0.9.0** (asset-aware Elements migration).

### Added
- **Full Liquid asset support in the Elements UI** (parity with the pkcs11 app):
  - **Holdings by asset** panel — the policy asset is labelled `L-BTC` and listed
    first; issued assets follow, keyed by id.
  - **Per-asset receive tabs** nested under the federation-version tabs (first-4-char
    asset labels, `L-BTC` for the policy asset), plus a receive→address `?asset=`
    deep-link that preselects the matching asset tab on the Address-detail page.
  - **Asset send** — a send-form asset dropdown (defaults to L-BTC) with a per-asset
    Max; the chosen asset rides the existing proposal → cosign → finalize flow
    (`build_proposal` is now asset-aware).
- **Asset-aware federation migration** — the Elements migration sweep
  (`build_migration_pset`) carries L-BTC **and** every issued asset to the successor
  in a single PSET, and the proposal detail lists the swept assets (`assets_swept`).

### Fixed
- **Chain-scoped in-flight/reserved balance** — the reserved figure is now computed
  per chain, so a pending Elements sweep no longer shows as "Reserved" on the
  Bitcoin page (and vice-versa).

### Tested
- `tests/taproot_federation.rs` — taproot descriptor / NUMS chaincode / P2TR address
  derivation (no hardware, no node).
- `tests/elements_asset_e2e.rs` — `SoftwareSigner`-signed asset receive / send /
  asset-aware migration end-to-end (`RPC_LIVE`-gated).
- Updated the reorg e2e helpers for the `script_type` / `nums_chaincode` fields.

### Changed
- Bumped to **emvault 0.9**.

## [0.2.0] - 2026-08-16

Tracks emvault suite **0.8.0** (Bitcoin Taproot).

### Added
- **Ledger hardware support** for onboarding and signing (SegWit `wsh` first).
- **Ledger Taproot federations (xpub-NUMS).** Ledger-only taproot vaults that emit
  a `tr(NUMS-xpub, multi_a)` BIP-388 policy in sign-data
  (`build_ledger_taproot_policy`), backed by new `script_type` / `nums_chaincode`
  columns (Random or Custom chaincode) and a taproot `tapScriptSig` merge in
  `ledger.js`; migration is guarded for taproot federations. Proven end-to-end on
  testnet4 (two-Ledger 2-of-2 script-path spend).
- HW taproot-descriptor matrix + Passport Prime integration design docs.

### Changed
- Bumped the `emvault` suite dependency `0.7 → 0.8`.
- **Elements:** full send tab + dual-chain (Bitcoin + Liquid) migration with
  chain-filtered proposals and Jade-only sign-data.
- Styling refinements.
