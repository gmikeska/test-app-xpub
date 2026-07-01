# Vendored `@emeraldlabs/emvault-jade`

This directory is a **verbatim copy** of `emvault-jade/src/` (the
`@emeraldlabs/emvault-jade` browser driver — Bitcoin-only Jade WebSerial).

This app is a **no-build static app**: it imports the driver over HTTP from
`/vendor/emvault-jade/…`, so it vendors the source directly and **always will**.
npm publication is for **external distribution only** — it does not change how
this app consumes the driver.

- Source repo: `emvault-jade` (github.com/gmikeska/emvault-jade)
- Package: `@emeraldlabs/emvault-jade`
- Files: `index.js`, `jade-rpc.js`, `cbor.js` (runtime only; `.d.ts` not needed here)
- Last synced: 2026-07-01 from `emvault-jade/src/` (working tree)

**Do not edit here.** Change the source repo, then re-copy (or run
`scripts/check-vendor.sh`) to refresh. Drift is caught by that script.
