# test-app-xpub

Server-rendered Axum web app that exercises
[`asterism-xpub`](https://github.com/gmikeska/asterism-xpub/) and
[`asterism-core`](https://github.com/gmikeska/asterism-core/) end-to-end against a local Bitcoin
Core regtest node:

1. **User auth.** Email + password login (Argon2id, signed
   cookie-backed sessions stored in Postgres).
2. **Trezor onboarding.** On first login (no signer row on file) the
   user is sent to `/onboard`. The page uses `@trezor/connect@9` in the
   browser to derive an XPUB at `m/48'/1'/0'/2'`, assembles a BIP-380
   descriptor key `[<root_fingerprint>/48'/1'/0'/2']<xpub>`, and POSTs
   it back to the server. The handler validates the key by constructing
   an [`asterism_xpub::ExternalSigner`] and persists the result.
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

The Asterism Rust library is linked **directly** into the Axum binary —
there is no separate signing service, no WASM, no proxy. Trezor only
talks to the browser; the backend never sees the device.

## Prerequisites

- **PostgreSQL** with a database `asterism_xpub` reachable via
  `postgres://asterism:asterism@127.0.0.1:5432/asterism_xpub`
  (see `.env`).
- **Bitcoin Core regtest node** matching the RPC credentials in `.env`
  (`127.0.0.1:18443`, user `regtestbtc`, password `regtestbtcpass` by
  default). The docker-compose stack in `../btc_regtest/` provides one.
- **Trezor device or Trezor Emulator.** The page loads
  `@trezor/connect@9` from the official CDN; no JS build step is
  required. On Linux you may need to install Trezor's udev rules for
  the device to be picked up by the browser:
  <https://wiki.trezor.io/Udev_rules>.

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

## Seeding a federation

Federations are not created from the UI yet — onboarding stops once
every member has an `ExternalSigner` row. Once each test user has
onboarded a unique Trezor account, you can seed a federation directly
in psql (the descriptor builder lives in `asterism-core` and is invoked
by the `WalletManager` at first wallet load):

```bash
PGPASSWORD=asterism psql -h localhost -U asterism -d asterism_xpub
```

Insert a row in `federations` referencing the three signer rows and
their parent users; the wallet is materialised lazily on the first
`/federations/{id}/...` request.

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
| GET    | `/federations/{id}`                                        | redirect → `/receive`                |
| GET    | `/federations/{id}/receive`                                | `federations::receive`               |
| GET    | `/federations/{id}/send`                                   | `federations::send`                  |
| GET    | `/federations/{id}/addresses/{address}`                    | `addresses::show`                    |
| POST   | `/federations/{id}/proposals`                              | `proposals::create`                  |
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
│   ├── 0001_init.sql           users, signers, federations, federation_members
│   ├── 0002_bdk_wallet.sql     bdk_changeset, tip_height, descriptor checksum cache
│   └── 0003_proposals.sql      transaction_proposals/_signatures/_rejections
├── src/
│   ├── main.rs                 router, AppState, startup migrate + seed
│   ├── config.rs               AppConfig::from_env()
│   ├── db.rs                   PgPool helpers (users, signers, federations, proposals)
│   ├── auth.rs                 Argon2id, AuthUser session extractor
│   ├── error.rs                AppError + IntoResponse (WalletError → 400/502)
│   ├── models.rs               row structs (UserRow, FederationRow, ProposalRow, …)
│   ├── wallet.rs               WalletManager + FederationWallet (BDK + RPC sync,
│   │                           build_proposal, trezor_sign_request,
│   │                           merge_partial_signature, finalize_and_extract,
│   │                           broadcast_raw)
│   └── handlers/
│       ├── mod.rs
│       ├── auth.rs             GET/POST /login, POST /logout
│       ├── onboard.rs          GET /onboard, POST /onboard/signer
│       ├── home.rs             GET /, GET /home
│       ├── federations.rs      /federations/{id}/{receive,send} + BalanceView
│       ├── addresses.rs        /federations/{id}/addresses/{address} + QR
│       └── proposals.rs        create/detail/sign-data/signatures/rejections/cancel/broadcast
├── templates/
│   ├── base.html
│   ├── login.html
│   ├── onboard.html
│   ├── home.html
│   ├── _federation_layout.html federation header + cosigners + balance + tab strip
│   ├── federation_receive.html "Receive" tab body (address table)
│   ├── federation_send.html    "Send" tab body (form + proposal table)
│   ├── address.html            address detail (QR + receipts)
│   └── proposal.html           proposal detail (cosigner status + actions)
└── static/
    ├── styles.css
    ├── onboard.js              Trezor Connect XPUB capture
    └── proposal-sign.js        Trezor Connect signTransaction roundtrip
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
