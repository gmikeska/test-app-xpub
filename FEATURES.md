# test-app-xpub — Crate Integration Guide

> **How `test-app-xpub` consumes the EmVault crates** to build a self-custody,
> multi-party (m-of-n P2WSH) wallet. This is the *reference integration* for
> [`emvault-xpub`](https://github.com/gmikeska/emvault-xpub) +
> [`emvault-core`](https://github.com/gmikeska/emvault-core) (via the
> [`emvault`](https://github.com/gmikeska/emvault) facade): for each library
> capability, it shows the exact API the app calls, where (`src/file.rs::symbol`
> ↔ `emvault::…::symbol`), and the integration pattern + gotchas.
>
> **Scope:** how the app talks to the crates — *not* the UI, routes, templates,
> HTML/JS, auth, or DB schema. (The browser device drivers, Askama pages, and
> Postgres tables are app concerns; they appear only where they touch a crate
> boundary.) For the run/quick-start, see [`README.md`](README.md).

---

## 1. The integration contract

EmVault is **linked directly into the Axum binary** — no signing service, no
WASM, no proxy. The split of responsibility is the thing to internalize:

| The **crates** own | The **app** owns |
|---|---|
| Signer *identity* + key-material validation (`ExternalSigner`/`Signer`) | Capturing keys from the device (browser transport) |
| Descriptor / federation construction (`build_federation`, `DescriptorBuilder`) | Persisting the descriptor + snapshot (Postgres) |
| The PSBT pipeline (build / combine / finalize) | Orchestrating it; **device-specific PSBT shaping** |
| Chain-sync *drivers* (`chain_sync`) | Owning the `bdk_wallet::Wallet` + its `ChangeSet`, the RPC client, and the wallet **birthday** |
| Roster math for migrations (`roster`) | Driving the on-chain migration (proposals) |
| Env-parsing helpers (`emvault::config`) | App-specific config (`jade_network`, device coins) |

Two invariants fall out of this and explain almost every design choice below:

1. **The library never moves funds and has no persistence.** It transforms
   public-key material and PSBTs; the app builds, signs (via the browser),
   broadcasts, and stores everything.
2. **The `Signer` trait is *identity*, not signing capability.** Consumer
   hardware wallets can't sign server-side, so signing happens in the browser and
   the crate only ever holds/handles *public* data + the resulting PSBT. This is
   why adding a whole new device (Jade) required **zero** crate changes — see §3
   and §6.

---

## 2. The crate surface the app touches

| `emvault::…` API | Purpose | App call-site |
|---|---|---|
| `xpub::ExternalSigner::from_descriptor_key` | Validate + wrap a device XPUB | `handlers/onboard.rs`, `handlers/new_federation.rs` |
| `xpub::DeviceType` | Device-family tag (metadata) | `new_federation::parse_device_type` |
| `core::Signer` (trait) | `fingerprint()` / `xpub()` / `derivation_path()` | `handlers/onboard.rs` (persist), `wallet.rs` |
| `core::build_federation` | Build `wsh(sortedmulti(..))` descriptor + snapshot | `new_federation::*`, `migrations::migrate_post` |
| `core::NetworkType` | Network passed to the builders | both builders |
| `core::chain_sync::init_or_load_wallet` | Construct/load the BDK wallet from a descriptor | `WalletManager::load_or_init` |
| `core::chain_sync::emitter_sync` | Drive the bitcoind Emitter to tip | `FederationWallet::sync` + sweep paths |
| `core::psbt::build_spend` | Build an unsigned spend PSBT | `FederationWallet::build_proposal` |
| `core::psbt::combine_psbt` | Merge a cosigner partial into the base | `FederationWallet::merge_partial_signature` |
| `core::psbt::finalize_and_extract` | Finalize + extract the raw tx | `FederationWallet::finalize_and_extract` |
| `core::roster::{compute_roster_plan, validate_threshold, RosterAction}` | Roster-change arithmetic | `migrations::migrate_post` |
| `core::FederatedWallet` | Track funds across federation versions | `wallet.rs`, lineage views |
| `config::{require, optional, hex_decode, ConfigError}` | Env parsing | `config.rs` |

---

## 3. Onboarding a signer — `emvault::xpub::ExternalSigner`

```rust
let signer = ExternalSigner::from_descriptor_key(
    descriptor_key.trim(),   // "[<fp>/48'/1'/0'/2']<xpub>" captured in the browser
    config.network,          // bitcoin::Network — validated against the key
    device,                  // DeviceType (parse_device_type(body.device_type))
    label,
)?;                          // runs all BIP-380 / BIP-32 checks
```

`onboard_signer_post` (`handlers/onboard.rs`) hands the crate a **descriptor key**
and stores the resulting `Signer`'s `fingerprint()` / `xpub()` /
`derivation_path()` (via the `core::Signer` trait) on a `signers` row.

**Integration lesson — device-agnostic onboarding.** The crate validates a
*descriptor key*; it does not care which device produced it. Adding **Blockstream
Jade** alongside Trezor required **no change to `emvault-xpub`**: the browser's
Jade driver produces the same `[fp/path]xpub` string Trezor does, and only the
`DeviceType` (pure metadata, round-tripped through `parse_device_type`) differs.
That is the entire point of `emvault-xpub` — *"holds public-key material only; no
USB/HID/BLE drivers, no signing code."* Device comms live in the app's browser
layer, never the crate.

---

## 4. Building a federation — `emvault::core::build_federation`

```rust
let built = emvault::core::build_federation(
    external_signers,                    // Vec<ExternalSigner> (the members)
    threshold_u32,                       // m
    NetworkType::Bitcoin(config.network),
)?;                                      // -> { descriptor_string, snapshot_json }
```

`new_federation_post` resolves each member's `ExternalSigner` (from their stored
descriptor key) and calls `build_federation`, which produces the canonical
two-path `wsh(sortedmulti(m, …))` **multipath descriptor** plus a
`FederationSnapshot` JSON. The app persists both verbatim — **`descriptor_string`
is the single source of truth** for the BDK wallet (§5) and for the Jade multisig
registration (§6). Errors surface as `DescriptorError` (via `DescriptorBuilder`).
The same call builds the **successor** federation during migration (§7).

---

## 5. The BDK wallet & chain sync — `emvault::core::chain_sync` (app owns persistence)

`emvault-core` has **no database**. The app owns a `bdk_wallet::Wallet` per
federation, its serialized `ChangeSet` (JSON on `federations.bdk_changeset`), and
the `bitcoincore-rpc` client. The crate provides two pure drivers:

```rust
// Construct or load the wallet from the federation descriptor + stored changeset.
let loaded = chain_sync::init_or_load_wallet(network, descriptor, changeset)?;
//   -> LoadedWallet { wallet, changeset, fresh }

// Drive bdk_bitcoind_rpc::Emitter from the wallet's checkpoint to the node tip.
let result = chain_sync::emitter_sync(&mut wallet, &rpc)?;
//   -> SyncResult { tip_height, blocks_synced, new_mempool_txs, changeset }
```

The app's persistence pattern (`FederationWallet::sync`): drive `emitter_sync`,
`take_staged()` the delta, `merge()` it into the aggregate `ChangeSet`, then write
the merged blob to Postgres **after releasing the wallet mutex** (so DB I/O never
blocks other readers). This is BDK's recommended pattern for backends without a
native `WalletPersister`.

**Integration lesson — the app owns the wallet "birthday."** Because
`emitter_sync` starts at `wallet.latest_checkpoint().height()`, and a freshly
constructed wallet sits at **genesis (height 0)**, a naive first sync walks the
*entire* chain over RPC — fine on regtest, an effective hang on signet/mainnet
(~260k blocks). The crate can't know a federation's birthday, so **the app sets
it**: in `WalletManager::load_or_init`, fresh (or never-synced) wallets seed their
checkpoint at the current node tip via
`wallet.latest_checkpoint().insert(BlockId{ height, hash })` + `apply_update`
before the first sync. Inserting onto the existing genesis checkpoint keeps the
chain connected (no `CannotConnect`). This is the canonical "owning app supplies
the birthday" responsibility the no-persistence design implies.

---

## 6. The PSBT signing pipeline — `emvault::core::psbt` (device-agnostic)

The crate exposes three primitives the app strings together; **all are
signing-agnostic** — they accept any signed PSBT:

```rust
// 1. Build (FederationWallet::build_proposal)
let psbt = core::psbt::build_spend(&mut wallet, recipient_spk, amount, fee_rate)?;
// 2. Merge a cosigner partial (FederationWallet::merge_partial_signature)
let merged = core::psbt::combine_psbt(base, partial)?;   // Psbt::combine + probe finalize
// 3. Finalize + extract for broadcast (FederationWallet::finalize_and_extract)
let raw_tx = core::psbt::finalize_and_extract(&merged_psbt)?;
```

**The key property: both device flows converge on these same primitives.** This is
exactly why a second device dropped in with no core change — the *device-specific
PSBT shaping is the app's job*, while the crate stays device-blind:

```
                         build_spend  (unsigned PSBT)
                                │
        ┌───────────────────────┴────────────────────────┐
   Trezor (app-shaped)                              Jade (app-shaped)
   trezor_sign_request → device → per-input         build_jade_register → device
   DER sigs → inject_trezor_signatures (clone)      registerMultisig + signPsbt
                                │                    → full signed PSBT
        └───────────────────────┬────────────────────────┘
                          combine_psbt   →   finalize_and_extract   →   broadcast
```

- **Trezor** (`FederationWallet::trezor_sign_request` + `inject_trezor_signatures`,
  app-side): the app hand-builds Trezor Connect's `signTransaction` payload
  (sorted `multisig.pubkeys` matching `sortedmulti` order, `refTxs`,
  `version`/`locktime` echoed for the BIP-143 sighash, signer-slot map), receives
  per-input DER sigs, and injects them into a base-PSBT clone — *then* hands it to
  `combine_psbt`.
- **Jade** (`src/jade::build_jade_register`, app-side): the app derives Jade's
  multisig **registration object** (`variant`, `sorted`, `threshold`, per-member
  `{fingerprint, derivation_path, xpub}`) from the *same federation signer data*
  the descriptor came from, the browser registers + `signPsbt`s, and returns a
  **complete signed PSBT** that goes straight into `combine_psbt`.

`sign_data`/`submit_signature` (`handlers/proposals.rs`) pick the branch from the
member's stored `device_type`, so **mixed Trezor + Jade federations co-sign the
same proposal** — both partials `combine` cleanly, the threshold finalizes, and
any member broadcasts. (Cross-network note: the app maps `Network` → each device's
coin id — `AppConfig::jade_network` → Jade `"testnet"` for Signet, and `coin_name`
→ Trezor `"test"`; the crate is network-aware via `NetworkType`, the device coin
mapping is app-side.)

> The Trezor payload subtleties (`PAYTOWITNESS` for native-P2WSH change, the
> `version`/`locktime` NULLFAIL trap, slot ordering) are **app-side** device
> protocol, not crate behavior — kept here only because they're the must-not-
> regress part of the integration.

---

## 7. Funds across versions — `core::FederatedWallet` + `core::roster`

Federation roster changes are **versioned migrations**, and the crate supplies the
arithmetic while the app drives the on-chain move (library never moves funds):

```rust
// migrations::migrate_post
let plan = roster::compute_roster_plan(&current_ids, &add_ids, &remove_ids)?; // RosterAction per member
let threshold = roster::validate_threshold(m, plan.next_members.len())?;
let built = build_federation(next_signers, threshold.get(), NetworkType::Bitcoin(net))?; // successor descriptor
```

The app then builds the **sweep PSBT** (a BDK `build_tx().drain_wallet()` to the
successor's first address — app-side, via `FederationWallet::build_migration_tx`)
and opens a `migration`-kind proposal. Broadcasting that proposal (signed by the
*current* members through the §6 pipeline) enacts the version flip
(*consent-by-signing*). **Relay** sweeps (superseded-version members moving late
inflows forward) reuse the same drain + §6 pipeline. `core::FederatedWallet`
tracks balances/ownership across versions (`find_by_signer`, `current`, …) for the
lineage views and the `sync_lineage` fan-out.

---

## 8. Config helpers — `emvault::config`

`AppConfig::from_env` (`config.rs`) reuses the crate's env helpers —
`require` / `optional` / `hex_decode` / `ConfigError` — so env parsing matches the
library crates exactly (deduped in extraction). App-only additions sit alongside:
`federation_derivation_path` (the BIP-48 path fed to the builders), `trezor_coin`,
and `AppConfig::jade_network()` (the `Network` → Jade-firmware-id map from §6).

---

## 9. Division of responsibility (cheat sheet)

| Concern | EmVault crate | This app |
|---|---|---|
| Validate a device key | `ExternalSigner::from_descriptor_key` | capture it in the browser |
| Build the federation descriptor | `build_federation` / `DescriptorBuilder` | persist `descriptor_string` + snapshot |
| Build / merge / finalize a PSBT | `core::psbt::{build_spend, combine_psbt, finalize_and_extract}` | orchestrate; **shape the per-device signing** |
| Chain data | `chain_sync::{init_or_load_wallet, emitter_sync}` | own the `Wallet` + `ChangeSet` + RPC + **birthday** |
| Roster math | `roster::{compute_roster_plan, validate_threshold}` | drive the migration via proposals |
| Track versions | `FederatedWallet` | lineage views + sync fan-out |
| Device communication | *(none — by design)* | browser (`@trezor/connect`, `@emvault/jade`) |
| Persistence / moving funds | *(none — by design)* | Postgres + broadcast |

---

## 10. Where to start (integration entry points)

| I want to… | App call-site → crate symbol |
|---|---|
| Onboard a new device family | `handlers/onboard.rs` → `ExternalSigner::from_descriptor_key` (+ `DeviceType`); device comms are app/browser-side |
| Construct a federation descriptor | `new_federation::new_federation_post` → `build_federation` |
| Build/sign/finalize a spend | `FederationWallet::{build_proposal, merge_partial_signature, finalize_and_extract}` → `core::psbt::*` |
| Add a hardware-signing flow | shape it in the app (§6), converge on `combine_psbt`; **don't** touch the crate |
| Change chain-sync / wallet birthday | `WalletManager::load_or_init` + `FederationWallet::sync` → `chain_sync::*` |
| Change roster/migration logic | `migrations::migrate_post` → `core::roster::*` + `build_federation` |
| Parse a new env var | `AppConfig::from_env` → `emvault::config::{require, optional}` |

---

## 11. Relationship to the rest of EmVault

- Library crates: [`emvault-xpub`](https://github.com/gmikeska/emvault-xpub)
  (`ExternalSigner`, `DeviceType`) + [`emvault-core`](https://github.com/gmikeska/emvault-core)
  (`build_federation`, `roster`, `chain_sync`, `core::psbt`, `FederatedWallet`),
  consumed via the [`emvault`](https://github.com/gmikeska/emvault) facade.
- Browser device drivers (the app's, not the crates'): `@trezor/connect@9` and the
  vendored [`@emvault/jade`](https://github.com/gmikeska/emvault-jade) — see
  `emvault_design/jade-integration.md`.
- Contrasting integration — **server-side autonomous** signing with HSMs (no
  browser, no proposal lifecycle):
  [`test-app-pkcs11`](https://github.com/gmikeska/test-app-pkcs11) and its
  `FEATURES.md`. Same `emvault-core` descriptor/PSBT primitives, a different
  `Signer` backend.
