# test-app-xpub

Server-rendered Axum web app that exercises
[`asterism-xpub`](https://github.com/gmikeska/asterism-xpub/) and
[`asterism-core`](https://github.com/gmikeska/asterism-core/) end-to-end against a local Bitcoin
Core regtest node:

1. **User auth.** Email + password login (Argon2id, signed
   cookie-backed sessions stored in Postgres).
2. **Hardware-wallet onboarding (Trezor or Blockstream Jade).** On first
   login (no signer row on file) the user is sent to `/onboard`. The page
   exposes a device picker and runs the matching capture flow:
   - **Trezor.** Uses `@trezor/connect@9` to derive an XPUB at
     `m/48'/1'/0'/2'`.
   - **Jade.** Uses our hand-rolled WebSerial + CBOR-RPC driver
     ([`static/vendor/jade-rpc.js`](static/vendor/jade-rpc.js)) to
     unlock against Blockstream's pinserver and derive the XPUB. We
     drive the protocol directly because `lwk_wasm` only exposes
     Liquid PSET signing, not Bitcoin `sign_psbt`.

   Either flow assembles a BIP-380 descriptor key
   `[<root_fingerprint>/48'/1'/0'/2']<xpub>` and POSTs it back along with
   `device_type: "Trezor" | "Jade"`. The handler validates the key by
   constructing an [`asterism_xpub::ExternalSigner`] and persists the
   result.
3. **Federation membership.** `/home` lists every federation the user
   participates in (label, policy, network, creation date). Clicking a
   federation opens the detail page. Federations come in two flavours,
   selected at creation time:
   - **Bitcoin federations.** `wsh(sortedmulti(M, key1, key2, …))`
     descriptors backed by a per-federation `bdk_wallet::Wallet`.
   - **Liquid (Elements) federations.** `ct(slip77(<mbk>),
     elwsh(sortedmulti(M, …)))` confidential descriptors backed by a
     per-federation `lwk_wollet::Wollet`. Each federation owns its own
     32-byte SLIP-77 master blinding key (generated server-side at
     creation; future iterations may bind the MBK to a hardware
     wallet).
4. **Federation detail.** A header card (descriptor, threshold, members,
   tip height) and balance card sit above two tabs:
   - **Receive.** Address table backed by a per-federation wallet
     (BDK for Bitcoin, LWK for Liquid). Bitcoin addresses link to a
     receipt-history detail page with a QR code; Liquid addresses are
     rendered confidentially (`el1q…` / `tlq1q…`) but per-address
     activity is not tracked in this iteration of the app.
   - **Send.** A proposal form and a table of every proposal for the
     federation with status badges.
5. **Candidate sends + multisig signing.** Each proposal page walks a
   2-of-3 (or any m-of-n) multisig through:
   1. **Bitcoin proposals.** `Wallet::build_tx` produces an unsigned
      PSBT plus a cached `coin_selection_json` (selected UTXOs +
      outputs + fee).

      **Liquid proposals.** `lwk_wollet::TxBuilder` produces an
      unsigned PSET (with rangeproofs/asset commitments) plus a
      `coin_selection_json` summary stored on the proposal row.
   2. The server's `sign-data` endpoint returns `{ psbt_b64, descriptor,
      network, federation_kind, trezor }`. `psbt_b64` carries the PSBT
      base64 for Bitcoin federations and the PSET base64 for Liquid
      ones; `descriptor` is the federation's multipath descriptor;
      `trezor` is populated only for Bitcoin (Trezor cannot sign
      Liquid in this app).
   3. **Trezor cosigners** (Bitcoin only) hand `signData.trezor` to
      `TrezorConnect.signTransaction`, then POST the per-input DER
      signatures to `/signatures`. The server slots each signature into
      a partial PSBT and merges via `Psbt::combine`.
   4. **Jade cosigners (Bitcoin)** parse the descriptor into Jade's
      `register_multisig` shape, register lazily/idempotently on the
      device, then call `sign_psbt` (CBOR-RPC over WebSerial). Jade
      returns a complete partial PSBT, which the browser POSTs to
      `/partial-psbt`; the server merges directly via `Psbt::combine`.
   5. **Jade cosigners (Liquid)** parse the CT descriptor into Jade's
      Liquid `register_multisig` shape (variant + SLIP-77 master
      blinding key), register lazily/idempotently, then call
      `sign_pset`. Jade returns a complete partial PSET, which the
      browser POSTs to `/partial-psbt`; the server branches on
      `FederationKind` and merges via `Pset::combine`.
   6. Once the merged PSBT/PSET finalizes, the proposer (or any
      member) can hit **Broadcast**. Bitcoin transactions go to
      bitcoind via `bitcoincore-rpc`; Liquid transactions go to the
      configured Esplora endpoint.

The Asterism Rust library is linked **directly** into the Axum binary —
there is no separate signing service, no WASM, no proxy. Hardware
wallets only talk to the browser; the backend never sees the device.

## Prerequisites

- **PostgreSQL** with a database `asterism_xpub` reachable via
  `postgres://asterism:asterism@127.0.0.1:5432/asterism_xpub`
  (see `.env`).
- **Bitcoin Core regtest node** matching the RPC credentials in `.env`
  (`127.0.0.1:18443`, user `regtestbtc`, password `regtestbtcpass` by
  default). The docker-compose stack in `../btc_regtest/` provides one.
- **(Liquid only) Elements Core regtest node + an Esplora-shaped
  indexer.** LWK's `Wollet` requires an Electrum/Esplora client for
  chain sync; we do not drive it from `elementsd` RPC directly. Set
  `ELEMENTS_ESPLORA_URL` (e.g. a local
  [`electrs-elements`](https://github.com/Blockstream/electrs) /
  Esplora instance pointed at your `elementsd`). When unset, Liquid
  federations still render but the balance card and address activity
  freeze at zero.
- **A Trezor or Blockstream Jade.** Trezor is supported for Bitcoin
  federations only — Liquid federations require a Jade. You can mix
  Trezor and Jade signers inside the same Bitcoin federation; Liquid
  federations must be all-Jade.
  - *Trezor.* The page loads `@trezor/connect@9` from the official CDN;
    no JS build step is required. On Linux you may need to install
    Trezor's udev rules: <https://wiki.trezor.io/Udev_rules>.
  - *Jade.* Use a Chromium-based desktop browser (Chrome, Edge, Brave)
    with Web Serial support. Plug the Jade in via USB and make sure no
    other tab/desktop app (Green, Sparrow, etc.) is holding the serial
    port. Jade's first unlock contacts Blockstream's pinserver, so
    network access is required at unlock time. On Linux you may need a
    udev rule granting your user access to the Jade USB-UART device
    (CP210x / ESP32-S3 / CH9102 / CH340).

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
- `ELEMENTS_NETWORK` — optional Liquid network identifier. One of
  `liquid`, `liquidtestnet`, `elementsregtest`. When set, the
  federation creation form exposes a "Liquid" option and Liquid
  federations sync via the configured Esplora endpoint.
- `ELEMENTS_ESPLORA_URL` — optional. Esplora HTTP endpoint LWK uses to
  sync Liquid federations. Leave unset to disable Liquid sync. Examples:
  `https://blockstream.info/liquid/api` (mainnet),
  `https://blockstream.info/liquidtestnet/api` (testnet),
  `http://127.0.0.1:3003` (local Esplora pointed at `elementsd`).
- `ELEMENTS_RPC_HOST`, `ELEMENTS_RPC_PORT`, `ELEMENTS_RPC_USER`,
  `ELEMENTS_RPC_PASSWORD`, `ELEMENTS_WALLET_NAME` — currently
  informational; the LWK pipeline does not use them.
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
| POST   | `/federations/{id}/proposals/{pid}/signatures`             | `proposals::submit_signature` (Trezor) |
| POST   | `/federations/{id}/proposals/{pid}/partial-psbt`           | `proposals::submit_partial_psbt` (Jade BTC + Liquid) |
| POST   | `/federations/{id}/proposals/{pid}/rejections`             | `proposals::submit_rejection`        |
| POST   | `/federations/{id}/proposals/{pid}/cancel`                 | `proposals::cancel`                  |
| POST   | `/federations/{id}/proposals/{pid}/broadcast`              | `proposals::broadcast`               |

## Architecture notes

- **One wallet per federation.** Bitcoin federations live in
  `WalletManager` (BDK), Liquid federations live in `LwkWalletManager`
  (LWK). Each manager caches one wallet object per federation id;
  handlers branch on `FederationKind` (parsed from
  `federations.network`) before dispatching.
- **BDK wallets** wrap a `bdk_wallet::Wallet` constructed from the
  federation's two-path descriptor and persisted as a serialised
  `ChangeSet` JSON blob in `federations.bdk_changeset`. Chain sync
  uses `bdk_bitcoind_rpc::Emitter` against the regtest node.
- **LWK wallets** wrap an `lwk_wollet::Wollet` constructed from the
  federation's CT descriptor (`ct(slip77(<mbk>),
  elwsh(sortedmulti(...)))`). Address indices are persisted in
  `federations.next_external_index` /
  `federations.next_internal_index`; LWK has no `ChangeSet`
  equivalent. Chain sync runs against `ELEMENTS_ESPLORA_URL`
  (Electrum/Esplora-shaped). The 32-byte SLIP-77 master blinding key
  lives in `federations.master_blinding_key`.
- **Reservations.** A federation's "spendable now" balance subtracts
  every input locked by an in-flight proposal (status `proposed`,
  `signing`, or `finalized`). The aggregation is a SQL
  `SUM((coin_selection_json->>'total_input_sat')::bigint)` cast back to
  `bigint` so sqlx can decode it as `i64`. The same column tracks
  Liquid L-BTC reservations.
- **PSBT/PSET discipline.** Proposals store the canonical PSBT (or
  PSET) in `transaction_proposals.psbt_b64` alongside per-signer
  partials in `transaction_signatures.partial_psbt_b64`. Bitcoin
  merging uses `Psbt::combine`; Liquid merging uses `Pset::combine`.
  Finalization probes on a clone so failure doesn't poison the
  canonical state.
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
│   ├── 0003_proposals.sql      transaction_proposals/_signatures/_rejections
│   └── 0004_elements.sql       master_blinding_key + next_(external|internal)_index
├── src/
│   ├── main.rs                 router, AppState, startup migrate + seed
│   ├── config.rs               AppConfig::from_env()
│   ├── db.rs                   PgPool helpers + FederationKind
│   ├── auth.rs                 Argon2id, AuthUser session extractor
│   ├── error.rs                AppError + IntoResponse (Wallet/Elements errors)
│   ├── models.rs               row structs (UserRow, FederationRow, ProposalRow, …)
│   ├── wallet.rs               WalletManager + FederationWallet (BDK)
│   ├── elements_wallet.rs      LwkWalletManager + LiquidFederationWallet (LWK)
│   └── handlers/
│       ├── mod.rs
│       ├── auth.rs             GET/POST /login, POST /logout
│       ├── onboard.rs          GET /onboard, POST /onboard/signer
│       ├── home.rs             GET /, GET /home
│       ├── new_federation.rs   GET/POST /federations/new (Bitcoin or Liquid)
│       ├── federations.rs      /federations/{id}/{receive,send} + BalanceView
│       ├── addresses.rs        /federations/{id}/addresses/{address} + QR
│       └── proposals.rs        create/detail/sign-data/signatures/partial-psbt/
│                               rejections/cancel/broadcast
├── templates/
│   ├── base.html
│   ├── login.html
│   ├── onboard.html
│   ├── home.html
│   ├── federation_new.html     federation creation form (Bitcoin / Liquid radio)
│   ├── _federation_layout.html federation header + cosigners + balance + tab strip
│   ├── federation_receive.html "Receive" tab body (address table)
│   ├── federation_send.html    "Send" tab body (form + proposal table)
│   ├── address.html            address detail (QR + receipts; Bitcoin only)
│   └── proposal.html           proposal detail (cosigner status + actions)
└── static/
    ├── styles.css
    ├── onboard.js              Trezor Connect XPUB capture
    ├── onboard-jade.js         Jade WebSerial XPUB capture
    ├── proposal-sign.js        Trezor Connect signTransaction roundtrip
    ├── proposal-sign-jade.js   Jade register_multisig + sign_psbt roundtrip (Bitcoin)
    ├── proposal-sign-jade-liquid.js
    │                           Jade register_multisig + sign_pset roundtrip (Liquid)
    └── vendor/
        ├── cbor.js             Minimal CBOR encoder/decoder for Jade RPC
        └── jade-rpc.js         Jade WebSerial driver (auth, get_xpub,
                                register_multisig, sign_psbt, sign_pset)
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
