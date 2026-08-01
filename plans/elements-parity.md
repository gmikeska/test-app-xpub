# Elements Parity & Backend Matrix — Republish Gate

**Status:** active · **Started:** 2026-08-01 · **Owner:** Maggie (prep) + Greg (commits)

## Goal

Bring **Elements/Liquid to full feature parity with Bitcoin** across the whole
suite (crate + both test apps), and prove **every Elements feature works on all
four chain backends — RPC, Esplora, Waterfalls, Electrum — before the next
crates.io republish**.

Two shapes of work:
- **(A) Feature parity** — every Bitcoin feature must exist for Liquid.
- **(B) Backend matrix** — every Elements feature green on RPC / Esplora /
  Waterfalls / Electrum.

## Definition of Done (republish gate)

1. Every feature in **Matrix A** is `both` (Bitcoin + Liquid), or explicitly
   waived with a written reason.
2. Every Elements op in **Matrix B** is green on all four backends, exercised by
   an automated, backend-parametrized e2e (the `fund → receive → send → migrate →
   resweep` loop run once per backend).
3. `emvault-elements` closes the crate-level gaps in **Matrix C** that back the
   above (roster/migration-planning, reorg reconcile surface, output verify).
4. `cargo fmt` + `cargo clippy --all-features -- -D warnings -W clippy::pedantic
   -W rust-2018-idioms` clean across touched crates/apps.

## Scope

- **Crate:** `emvault-elements` (+ `emvault-core` seams it reuses).
- **Apps:** `test-app-xpub` (external-signer / Trezor+Jade) and
  `test-app-pkcs11` (HSM). pkcs11 is the fuller reference; xpub is the laggard.

---

## Matrix A — xpub app feature parity (feature × chain)

Mechanism: handlers dispatch on `FederationKind` (`src/db.rs:260-292`) between
the BDK manager `state.wallets` and the LWK manager `state.elements_wallets`. A
handler that calls `state.wallets.load_or_init(...)` with **no** kind branch is
structurally Bitcoin-only (a `ct(...)` descriptor can't load into BDK).

| # | Feature | BTC | Liquid | Status | Evidence (xpub) |
|---|---|---|---|---|---|
| 1 | Onboard / device reg | ✅ | ✅ | **full** | `onboard.rs:141`; xpub reused both chains |
| 2 | Create federation | ✅ | ✅ | **full** | `new_federation.rs:247-278`, `build_liquid_federation:337` |
| 3 | Receive addresses | ✅ | ⚠️ | **partial** | `federations.rs:266-290`; Liquid per-addr received/unspent hardcoded 0 (`:276`), change history empty (`:289`) |
| 4 | Address detail | ✅ | ❌ | **missing** | `addresses.rs:111-115` hard `BadRequest("not available for Liquid")`; QR hardcodes `bitcoin:` `:137` |
| 5 | Balance / sync | ✅ | ✅ | **full** | `federations.rs:356-386` (BDK vs LWK) |
| 6 | Send / build proposal | ✅ | ⚠️ | **partial** | `proposals.rs:96-186`; Liquid ignores `send_max`, always explicit amount (`:164`) |
| 7 | Max-spend / drain | ✅ | ❌ | **missing** | `proposals.rs:249` BDK-only; UI hides Max for Liquid (`federation_send.html:28`) |
| 8 | Sign-data | ✅ | ✅ | **full** | `proposals.rs:516-547`; PSET in `psbt_b64` for Liquid |
| 9 | Submit signature (`/signatures`) | ✅ | n/a | **ok** | `proposals.rs:597` BDK-only, but Liquid uses `/partial-psbt` instead |
| 10 | Submit partial (`/partial-psbt`) | ✅ | ✅ | **full** | `proposals.rs:714-728` (merge_partial_pset), finalize `:750-768` |
| 11 | Rejections | ✅ | ✅ | **full** | `proposals.rs:800` (DB-only) |
| 12 | Cancel proposal | ✅ | ✅ | **full** | `proposals.rs:838` (DB-only) |
| 13 | Finalize (on threshold) | ✅ | ✅ | **full** | `proposals.rs:750-768` |
| 14 | Broadcast | ✅ | ✅ | **full** | `proposals.rs:889-897` (elements broadcast_raw) |
| 15 | **Migration — migrate** | ✅ | ❌ | **missing** | `migrations.rs:137` hardcodes `NetworkType::Bitcoin`, `:147` BDK, p2wsh-only candidate `:376`; **no kind guard** |
| 16 | Migration cancel | ✅ | ❌ | **missing** | `migrations.rs:221` (only reachable BTC-side) |
| 17 | Federation tab (migrations list + lineage) | ✅ | ❌ **broken** | **missing** | `migrations.rs:309,320,331` BDK unconditional; tab shown for ALL feds (`_federation_layout.html:104`) → Liquid user hits raw BDK error |
| 18 | Relay | ✅ | ❌ | **missing** | `migrations.rs:462,470` BDK-only |
| 19 | Resweep | ✅ | ❌ | **missing** | `migrations.rs:558,565` BDK-only |

**Client signing (corrected):** deployed `static/` scripts sign **both PSBT and
PSET**. Jade Liquid signing exists — `static/proposal-sign-jade-liquid.js:284`
`jade.signPset` → `/partial-psbt`. **Trezor cannot sign Liquid** (intentional;
`proposal.html:133`). (The `client/` Vite sources are stale — ignore; `static/`
is deployed.)

### Ranked xpub gaps
1. **Whole migration/re-key subsystem is Bitcoin-only** (migrate, federation/
   lineage tab, relay, resweep) — and unguarded, so Liquid users get raw errors
   rather than a disabled UI. *Largest surface.*
2. **Max-spend / Send-Max missing for Liquid.**
3. **Address-detail page disabled for Liquid.**
4. ~~Trezor-Liquid signing absent~~ — **waived**: Trezor hardware doesn't support
   the Liquid network. Jade-only is correct; app already shows the notice.
5. Receive per-address analytics degraded for Liquid (LWK v1 limitation).

---

## Matrix B — Elements backend matrix

Backends: **RPC** (elementsd JSON-RPC, in-crate block-scan) · **Electrum /
Esplora / Waterfalls** (LWK `NodelessSync`, feature-gated). Esplora & Waterfalls
share one client/feature (`esplora`), differing only by constructor.

| Layer | RPC | Esplora | Waterfalls | Electrum |
|---|---|---|---|---|
| `emvault-elements` crate (sync, broadcast, tip, utxo) | ✅ | ✅ | ✅ | ✅ |
| `test-app-pkcs11` (wired + dispatched) | ✅ | ✅ | ✅ | ✅ |
| **`test-app-xpub`** | ❌ **not wired** | ✅ | ✅ | ✅ |

- Crate evidence: `nodeless.rs:321/337/371` constructors, `rpc.rs` / `sync/`
  block-scan, reorg on both families (`scan.rs:122`, `nodeless.rs:156`).
- pkcs11 evidence: `config.rs:105` 4-way enum, `elements_ingest.rs:54-67`
  sync dispatch, `elements_wallet.rs:95-138` `broadcast_via_backend`.
- **xpub gap:** `config.rs` `ElementsChainBackend` = Esplora/Waterfalls/Electrum
  only ("no elementsd-RPC path in this app"). Add `Rpc`.
- **Crate-wide gap (all backends):** no fee estimation — fee rate is always
  caller-supplied (`rpc.rs:197`, `spend.rs:41`). Decide whether that's in scope.

**DoD test:** every Matrix-A Liquid feature run once per backend, both apps.

**⚠️ Waterfalls testability blocker:** waterfalls is Blockstream QuickSync
(enterprise tier, no regtest/signet) and our local electrs-esplora does not serve
the waterfalls descriptor endpoint. So rpc/electrum/esplora are testable on the
regtest stack today; **waterfalls needs a dedicated server** — either the OSS
`waterfalls` daemon pointed at our regtest esplora/electrs, or the Blockstream
enterprise endpoint (was out of credits earlier). Decision needed before the
waterfalls column of the DoD can go green.
(Also fixed in passing: dev `.env` `ELEMENTS_ESPLORA_URL` pointed at the testnet4
electrs `:3111`; corrected to the regtest esplora REST `:3112`.)

---

## Matrix C — crate parity (`emvault-core` Bitcoin vs `emvault-elements`)

| Capability | Core (BTC) | Elements | Parity |
|---|---|---|---|
| CT descriptor build | `DescriptorBuilder` | `CtDescriptorBuilder` (`ct(slip77,…)`) | ✅ |
| Federation object | `Federation` | reuses core `Federation` | ✅ |
| Versioned federated wallet | `BtcFederatedWallet` | `ElementsFederatedWallet` (+per-version blinding) | ✅ |
| PSBT/PSET build→sign→finalize→extract | `psbt.rs` | `pset.rs` (+blind, stricter newtypes) | ✅ |
| Signing coordinator | `SigningCoordinator` | `ElementsSigningCoordinator` | ✅ |
| **Roster / version planning** | `roster.rs` (`compute_roster_plan`, `historic_versions_at_risk`) | **none** | ❌ **gap** |
| **Migration/sweep algorithm layer** | `migration.rs` (`SweepAlgorithm`, `MigrationPlan`, batched sweeps, fee est) | only concrete PSET builders in `spend.rs`; batched logic lives in *test* code | ❌ **gap** |
| Reorg reconcile surface | `chain_sync::SyncResult{evicted_txids, reorg_rebuilt}` + phantom-UTXO heal | `reorg_to` + store rollback; **no evicted_txids/reorg_rebuilt** | ⚠️ **partial** |
| **Output verification** | `verify.rs` (`verify_psbt_outputs`, `MultisigPolicy`) | only `validate_blinding` (CT structure) | ❌ **gap** |
| Recovery template / snapshot | `recovery.rs`, `snapshot.rs` | none (core doesn't capture SLIP-77) | ⚠️ gap |
| Fee estimation | size-based in migration planning | none | ⚠️ gap |

---

## Reference — what pkcs11 already has (port sources for xpub)

pkcs11 Liquid is nearly complete and is the **port baseline**:
- 4-backend dispatch: `config.rs:105`, `elements_ingest.rs:54`,
  `elements_wallet.rs:95` (`broadcast_via_backend`).
- Migration `--elements` + completion stamping: `examples/federation_migration.rs:868,1485-1547`.
- Post-migration descriptor self-heal on load: `elements_wallet.rs:389-433` +
  `db::update_elements_wallet_descriptor` (`db.rs:400`).
- Send/Send-Max/max-spend for Liquid: `elements_wallet.rs:942-1104`,
  `handlers/elements_wallet.rs:306-455`.
- Stores + RPC source: `elements_sync.rs`.

**Net-new for BOTH apps (nobody has it):** runtime **Elements migration-reorg
reconciliation** — `db::reconcile_migration` is table-generic (`db.rs:626`) but
only the Bitcoin sync calls it (`wallet.rs:1131`); no Elements caller anywhere.

---

## Workstream (ordered; each step = a commit checkpoint Greg lands)

0. **[this doc]** audit + matrices + DoD. ← *commit 1*
1. **Backend-matrix e2e harness** — DONE (v1): `test-app-pkcs11/scripts/
   elements_backend_matrix.py` drives the running app through `fund → sync →
   balance → max-spend → signed send → node-receives` per backend (swaps
   `ELEMENTS_CHAIN_BACKEND`, restarts, asserts deltas). **Proven green live
   2026-08-01: rpc ✅ · electrum ✅ · esplora ✅.** Waterfalls skips pending a
   server (see caveat). Grows to cover migrate/resweep as those land. ← *commit*
2. **Crate: roster + migration-planning for Elements** (`emvault-elements`) —
   DONE. `roster` is chain-agnostic → **reused directly** via `pub use
   emvault_core::roster` (no duplication). New `migration.rs`: Elements-typed
   `ElementsSweepAlgorithm` + `ElementsMigrationPlan`/`SweepOutput`/
   `SweepTransaction`/`AccountUtxoSet` + `ElementsAccountForAccountSweep` and
   `…BatchedSweep` (mirrors core's arithmetic; fees in u64 sat via the proven
   `estimate_elements_fee_sat`). 5 unit tests + build + fmt + pedantic clippy
   green. Planning-only (consumer builds the PSET via `build_migration_pset`).
   Uncommitted (Greg commits). ← *commit* — msg: `Add Elements roster reuse + migration-planning layer (SweepAlgorithm/MigrationPlan)` ← next up: step 3 wires this into the xpub app.
3. **xpub: Liquid migration subsystem** — in sub-steps:
   - **3a DONE (staged):** `FederationKind` guard across `migrations.rs`. The
     migrate/relay/resweep POSTs now decline Liquid cleanly (was: raw BDK error
     loading a `ct(...)` descriptor into BDK); the Federation tab (`federation_manage`)
     renders Liquid via LWK (`elements_wallets` balances + version history) and hides
     the Bitcoin-only migrate form + relay/resweep buttons. build+fmt+pedantic-clippy
     green; compile-verified (not yet live-tested against a running xpub Liquid fed —
     needs the xpub app + browser-signer setup). msg: `Guard xpub migration handlers for FederationKind; render Federation tab for Liquid`
   - **3b next:** Liquid Phase-3 migrate — build the CT successor descriptor
     (`CtDescriptorBuilder`) + persist the pending version (mirror `migrate_post`).
   - **3c next:** Liquid Phase-4 sweep — add a drain-PSET builder to the xpub
     `LiquidFederationWallet` (LWK `TxBuilder` drain, not the fixed-amount
     `build_proposal`), open the migration proposal, browser Jade `signPset` →
     finalize → broadcast → version flip. ← *commit(s)*
4. **xpub: max-spend + Send-Max for Liquid**; **address-detail for Liquid**. ← *commit*
5. **xpub: wire the RPC Elements backend** (`ElementsChainBackend::Rpc`), so xpub
   has the full 4-backend matrix. ← *commit*
6. **Crate: reorg reconcile surface + Elements migration-reconciliation** (net-new)
   — emit an eviction/reconcile signal and call `reconcile_migration` from the
   Elements sync path in both apps. ← *commit*
7. **Crate: output verification for PSET** (`verify` analogue). ← *commit*
8. **Crate: fee estimation** per Elements backend (RPC `estimatesmartfee`; LWK/
   Esplora fee endpoints) — replaces caller-supplied-only fee rate. ← *commit*
9. **Crate: recovery template + federation snapshot** for Elements (capturing the
   per-version SLIP-77 blinding keys). ← *commit*
10. **Full DoD sweep:** run the Matrix-B harness green on all four backends, both
    apps; fmt+clippy; then republish. ← *republish*

## Resolved decisions (Greg, 2026-08-01)
- **Fee estimation** across Elements backends — **in scope** for republish (add
  estimation per backend; RPC `estimatesmartfee`, LWK/Esplora fee endpoints).
- **Trezor-Liquid signing** — **not applicable**: Trezor hardware does not support
  the Liquid network. Jade-only for Liquid is a hardware fact, not a gap; the app
  already renders the "use a Jade" notice (`proposal.html:133`). No work needed.
- **Recovery template / federation snapshot** for Elements — **in scope** (must
  capture the per-version SLIP-77 blinding keys, which core's snapshot doesn't).
- **Multi-asset** (non-L-BTC) send/UI — **out of scope for now.**
