# test-app-xpub

Server-rendered Axum web app that exercises
[`emvault-xpub`](https://github.com/gmikeska/emvault-xpub/) and
[`emvault-core`](https://github.com/gmikeska/emvault-core/) end-to-end against a local Bitcoin
Core node (`regtest` / `testnet` / `signet` / `mainnet`, selected via `BITCOIN_NETWORK`):

1. **User auth.** Email + password login (Argon2id, signed
   cookie-backed sessions stored in Postgres).
2. **Hardware-wallet onboarding (Trezor or Jade).** On first login the
   user is sent to `/onboard` and picks their device. **Trezor** uses
   `@trezor/connect@9`; **Blockstream Jade** uses the vendored
   [`@emvault/jade`](https://github.com/gmikeska/emvault-jade) driver over
   Web Serial (USB). Either way the browser derives an XPUB at
   `m/48'/1'/0'/2'`, assembles a BIP-380 descriptor key
   `[<root_fingerprint>/48'/1'/0'/2']<xpub>`, and POSTs it (with
   `device_type`) to the server, which validates it via
   [`emvault::xpub::ExternalSigner`] and persists the result. Signing later
   auto-routes by the stored device type, so **Trezor and Jade members can
   co-sign the same federation**.
3. **Federation membership.** `/home` lists every federation the user
   participates in (label, policy, network, creation date). Clicking a
   federation opens the detail page.
4. **Federation detail.** A header card (descriptor, threshold, members,
   tip height) and balance card sit above two tabs:
   - **Receive.** Address table backed by a per-federation BDK wallet
     (revealed lazily, persisted via `ChangeSet`). Clicking an address
     opens a detail page with a QR code and the on-chain receipt
     history for that script.
   - **Send.** A proposal form and a table of every proposal for the
     federation with status badges.
5. **Candidate sends + Trezor multisig signing.** Each proposal page
   walks a 2-of-3 (or any m-of-n) P2WSH multisig through:
   1. `Wallet::build_tx` produces an unsigned PSBT plus a cached
      `coin_selection_json` (selected UTXOs + outputs + fee).
   2. The server hands the browser a Trezor-shaped JSON payload
      (`inputs`, `outputs`, `refTxs`, `version`, `locktime`,
      `multisig.pubkeys` with cosigner `HDNode`s, sorted to match
      `sortedmulti`).
   3. The user signs in their browser via
      `TrezorConnect.signTransaction`; the partial PSBT is POSTed back,
      merged into the canonical PSBT (`Psbt::combine`), and recorded as
      a `transaction_signatures` row.
   4. Once the merged PSBT finalizes, the proposer (or any member) can
      hit **Broadcast** to push the extracted raw transaction to
      bitcoind via `bitcoincore-rpc`.
6. **Federation creation + migration (versioned lineage).** Federations
   are created **from the UI** (`GET /federations/new` → `POST
   /federations`). A federation is a **lineage of versions**: changing the
   roster/threshold mints a new version and drives a
   `roster`-planned **migration** that sweeps funds from the old version to
   the new one (`POST /federations/{id}/migrations`), with a **relay**
   helper (`POST …/relay`) for fee-bumping the funding hop. The federation
   management page is `GET /federations/{id}/federation`.
7. **Reorg reconciliation.** The app tracks each migration sweep's txid and
   confirmed height (migrations `0007`/`0008`); on chain sync, if a reorg
   strips the sweep's confirmation it reverts the migration `complete →
   pending` (funds preserved on the old version), and a funds-preserving
   **re-sweep** (`POST /federations/{id}/resweep`, migration `0009`)
   RBF-displaces the stuck sweep to re-complete the migration.

The EmVault Rust library is linked **directly** into the Axum binary —
there is no separate signing service, no WASM, no proxy. Trezor only
talks to the browser; the backend never sees the device.

## Crate integration guide

For a developer-oriented walkthrough of **how this app consumes the EmVault
crates** — `ExternalSigner` onboarding, `build_federation`, the `chain_sync`
wallet/birthday, the device-agnostic `core::psbt` signing pipeline (Trezor *and*
Jade), `roster`-driven migration, and `emvault::config` — see
**[`FEATURES.md`](FEATURES.md)**.

`FEATURES.md` is the **reference integration** for
[`emvault-xpub`](https://github.com/gmikeska/emvault-xpub) +
[`emvault-core`](https://github.com/gmikeska/emvault-core): for each library
capability it shows the exact API the app calls and where
(`src/file.rs::symbol` ↔ `emvault::…::symbol`), so AI/human developers can learn
*how to integrate the crates*. It deliberately covers the app↔crate boundary, not
the UI, routes, templates, or DB schema. This README is the quick-start;
`FEATURES.md` is the deep integration reference.

## Prerequisites

- **PostgreSQL** with a database `emvault_xpub` reachable via
  `postgres://emvault:emvault@127.0.0.1:5432/emvault_xpub`
  (see `.env`).
- **Bitcoin Core Signet node** matching the RPC credentials in `.env`
  (`127.0.0.1:38332`, user `signetbtc` by default — the same node
  `emvault-jade-test` uses). Fund federation receive addresses from a
  Signet faucet or the node's default wallet `sendtoaddress`.
- **A hardware wallet:**
  - **Trezor** (or Trezor Emulator) — loads `@trezor/connect@9` from the
    official CDN; no JS build step. On Linux you may need Trezor's udev
    rules: <https://wiki.trezor.io/Udev_rules>.
  - **Blockstream Jade** (v1 / Plus / DIY ESP32) over **USB** — uses the
    `@emvault/jade` driver bundled into the app's JS build (served from
    `static/dist/`, integrity-checked via `scripts/check-vendor.sh`).
    **Web Serial requires a Chromium-based desktop browser**
    (Chrome/Edge/Brave).

## Configuration

All knobs live in `.env`:

- `APP_HOST`, `APP_PORT` — bind address (default `127.0.0.1:8090`).
- `APP_SESSION_SECRET` — 64-byte hex key signing the session cookie.
  Replace before deploying anything that resembles production.
- `DATABASE_URL` — Postgres connection string.
- `BITCOIN_NETWORK` — `regtest` / `testnet` / `signet` / `mainnet`. Must
  match the network every onboarded Trezor agreed to.
- `APP_FED_DERIVATION_PATH` — the BIP-48 path used during onboarding.
  Default `"m/48'/1'/0'/2'"` (P2WSH multisig, coin type 1 for
  testnet/regtest). **The value must be double-quoted** — bare
  apostrophes are parsed as quote delimiters and would silently strip
  the hardened markers.
- `BITCOIN_RPC_HOST`, `BITCOIN_RPC_PORT`, `BITCOIN_RPC_USER`,
  `BITCOIN_RPC_PASSWORD`, `BITCOIN_WALLET_NAME` — Bitcoin Core RPC
  credentials.
- `TREZOR_COIN` — coin token passed to `@trezor/connect`. `"test"`
  covers both testnet and regtest; `"btc"` is mainnet.
- `TREZOR_MANIFEST_EMAIL`, `TREZOR_MANIFEST_APP_URL` — required Trezor
  Connect manifest fields (cosmetic in dev).
- `RUST_LOG` — `tracing-subscriber` filter.

## Run

```bash
cd test-app-xpub
cargo run
```

On startup the app:

- runs every `migrations/*.sql` in order,
- initialises the `tower-sessions` Postgres store (its own schema
  migration),
- upserts three test users (`test1@test.com`, `test2@test.com`,
  `test3@test.com`, password `test1234`),
- binds `APP_HOST:APP_PORT`.

Open <http://127.0.0.1:8090/> and log in. First-time users are sent to
`/onboard`; returning users land on `/home`.

## Creating a federation

Federations are created **from the UI**. Once each member has onboarded a
signer, any onboarded user visits `GET /federations/new`, picks the
members + threshold, and `POST /federations` mints **version 1** of the
lineage. The descriptor builder lives in `emvault-core` and is invoked by
the `WalletManager`; the BDK wallet is materialised lazily on the first
`/federations/{id}/...` request. To change the roster/threshold later, use
the federation management page (`GET /federations/{id}/federation`), which
mints a new version and plans the migration sweep from the old one.

## Routes

| Method | Path                                                       | Handler                              |
|--------|------------------------------------------------------------|--------------------------------------|
| GET    | `/`                                                        | `home::root` (redirects)             |
| GET    | `/home`                                                    | `home::home`                         |
| GET    | `/login`                                                   | `auth::login_get`                    |
| POST   | `/login`                                                   | `auth::login_post`                   |
| POST   | `/logout`                                                  | `auth::logout_post`                  |
| GET    | `/onboard`                                                 | `onboard::onboard_get`               |
| POST   | `/onboard/signer`                                          | `onboard::onboard_signer_post`       |
| GET    | `/federations/new`                                         | `new_federation::new_federation_get` |
| POST   | `/federations`                                             | `new_federation::new_federation_post`|
| GET    | `/federations/{id}`                                        | `federations::redirect_to_default`   |
| GET    | `/federations/{id}/federation`                             | `migrations::federation_manage`      |
| GET    | `/federations/{id}/migrate`                                | `migrations::redirect_to_federation` |
| GET    | `/federations/{id}/lineage`                                | `migrations::redirect_to_federation` |
| POST   | `/federations/{id}/migrations`                             | `migrations::migrate_post`           |
| POST   | `/federations/{id}/migrations/{mid}/cancel`                | `migrations::cancel_post`            |
| POST   | `/federations/{id}/relay`                                  | `migrations::relay_post`             |
| POST   | `/federations/{id}/resweep`                                | `migrations::resweep_post`           |
| GET    | `/federations/{id}/receive`                                | `federations::receive`               |
| GET    | `/federations/{id}/send`                                   | `federations::send`                  |
| GET    | `/federations/{id}/addresses/{address}`                    | `addresses::show`                    |
| POST   | `/federations/{id}/proposals`                              | `proposals::create`                  |
| GET    | `/federations/{id}/max-spend`                              | `proposals::max_spend`               |
| GET    | `/federations/{id}/proposals/{pid}`                        | `proposals::detail`                  |
| GET    | `/federations/{id}/proposals/{pid}/sign-data`              | `proposals::sign_data`               |
| POST   | `/federations/{id}/proposals/{pid}/signatures`             | `proposals::submit_signature`        |
| POST   | `/federations/{id}/proposals/{pid}/rejections`             | `proposals::submit_rejection`        |
| POST   | `/federations/{id}/proposals/{pid}/cancel`                 | `proposals::cancel`                  |
| POST   | `/federations/{id}/proposals/{pid}/broadcast`              | `proposals::broadcast`               |

## Architecture notes

- **One BDK wallet per federation.** `WalletManager` caches
  `FederationWallet` instances keyed by federation id. Each wraps a
  `bdk_wallet::Wallet` constructed from the federation's two-path
  descriptor and persisted as a serialised `ChangeSet` JSON blob in
  `federations.bdk_changeset`. Chain sync uses
  `bdk_bitcoind_rpc::Emitter` against the regtest node.
- **Reservations.** A federation's "spendable now" balance subtracts
  every input locked by an in-flight proposal (status `proposed`,
  `signing`, or `finalized`). The aggregation is a SQL
  `SUM((coin_selection_json->>'total_input_sat')::bigint)` cast back to
  `bigint` so sqlx can decode it as `i64`.
- **PSBT discipline.** Proposals store the canonical PSBT
  (`transaction_proposals.psbt_b64`) alongside per-signer partials in
  `transaction_signatures.partial_psbt_b64`. Merging is done with
  `Psbt::combine`; finalization probes via `Wallet::finalize_psbt` on a
  clone so failure doesn't poison the canonical PSBT.
- **Rejections are advisory.** A `transaction_rejections` row records
  who pushed back and why, but proposal status does not change. The UI
  surfaces the reject explicitly so the proposer can decide to
  `cancel`.
- **Trezor sighash.** The Trezor payload includes the BDK-chosen
  `version` and `locktime` (BDK enables anti-fee-sniping, which sets
  `nLockTime` to the current chain tip). Without them Trezor signs the
  default `version=1, locktime=0` and bitcoind rejects the broadcast
  with `mempool-script-verify-flag-failed` (NULLFAIL).

## Layout

```
test-app-xpub/
├── Cargo.toml
├── .env
├── README.md
├── migrations/
│   ├── 0001_init.sql                          users, signers, federations, federation_members
│   ├── 0002_bdk_wallet.sql                    bdk_changeset, tip_height, descriptor checksum cache
│   ├── 0003_proposals.sql                     transaction_proposals/_signatures/_rejections
│   ├── 0004_federation_versions.sql           versioned lineage (federations → versions)
│   ├── 0005_migrations.sql                    federation_migrations record (roster change → version)
│   ├── 0006_proposal_kind.sql                 proposal kind: migration/relay sweeps alongside spends
│   ├── 0007_migration_sweep_txid.sql          reorg-reconcile: bind a version's sweep txid to its row
│   ├── 0008_migration_sweep_confirmed_height.sql  reorg-reconcile p2: durable confirmation-loss evidence
│   └── 0009_proposal_kind_resweep.sql         reorg-reconcile p3: funds-preserving re-sweep kind
├── src/
│   ├── main.rs                 router, AppState, startup migrate + seed
│   ├── config.rs               AppConfig::from_env()
│   ├── db.rs                   PgPool helpers (users, signers, federations, versions, migrations, proposals)
│   ├── auth.rs                 Argon2id, AuthUser session extractor
│   ├── error.rs                AppError + IntoResponse (WalletError → 400/502)
│   ├── models.rs               row structs (UserRow, FederationRow, VersionRow, MigrationRow, ProposalRow, …)
│   ├── wallet.rs               WalletManager + FederationWallet (BDK + RPC sync,
│   │                           build_proposal, trezor_sign_request,
│   │                           merge_partial_signature, finalize_and_extract,
│   │                           broadcast_raw, build_migration*, build_migration_resweep,
│   │                           sync_lineage reorg-reconciliation)
│   └── handlers/
│       ├── mod.rs
│       ├── auth.rs             GET/POST /login, POST /logout
│       ├── onboard.rs          GET /onboard, POST /onboard/signer
│       ├── home.rs             GET /, GET /home
│       ├── new_federation.rs   GET /federations/new, POST /federations
│       ├── federations.rs      /federations/{id}/{receive,send} + BalanceView
│       ├── migrations.rs       federation manage + migrate/relay/resweep/cancel + reorg-reconcile
│       ├── addresses.rs        /federations/{id}/addresses/{address} + QR
│       └── proposals.rs        create/detail/sign-data/signatures/rejections/cancel/broadcast/max-spend
├── templates/
│   ├── base.html
│   ├── login.html
│   ├── onboard.html
│   ├── home.html
│   ├── federation_new.html     new-federation form (members + threshold)
│   ├── _federation_layout.html federation header + cosigners + balance + tab strip
│   ├── federation_receive.html "Receive" tab body (address table)
│   ├── federation_send.html    "Send" tab body (form + proposal table)
│   ├── federation_manage.html  federation/migration management (versions, lineage, resweep)
│   ├── address.html            address detail (QR + receipts)
│   └── proposal.html           proposal detail (cosigner status + actions)
└── static/
    ├── styles.css
    └── dist/                   JS build output (served at /static/dist/)
        ├── onboard.js          Trezor/Jade onboarding (XPUB capture)
        ├── proposal-sign.js    Trezor/Jade signTransaction roundtrip
        └── chunks/             shared bundle chunks (incl. the vendored Jade driver)
```

## Development

The crate is wired up for strict clippy:

```bash
cargo clippy --all-features -- -D warnings -W clippy::pedantic -W rust-2018-idioms
```

Run before pushing changes that touch the wallet or proposal modules —
the BDK mutex/changeset patterns are easy to regress and the lints
catch the common slips (drops held across awaits, missing backticks in
public docs, by-value parameters that should be `&`).
