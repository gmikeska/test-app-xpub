# Changelog

All notable changes to `test-app-xpub` (the xpub / external-signer reference app)
are documented here. This is the app's first CHANGELOG; the `0.2.0` entry
summarizes the current cycle (which tracks the emvault suite **0.8.0** / Bitcoin
Taproot release). Earlier history lives in git.

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
