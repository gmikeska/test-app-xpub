# test-app-xpub — Feature Guide

> A complete, developer-oriented tour of every feature in `test-app-xpub`,
> the self-custody reference app for
> [`asterism-xpub`](https://github.com/gmikeska/asterism-xpub) +
> [`asterism-core`](https://github.com/gmikeska/asterism-core).
>
> **Audience:** AI coding agents and human developers who need to understand —
> quickly and exactly — what this app can do, how each capability is wired, and
> which function/route to reach for. Every feature is cross-linked to the source
> symbol that implements it (`src/file.rs::symbol`). For the high-level pitch,
> prerequisites, and config reference, see [`README.md`](README.md); this
> document is the exhaustive companion to it and supersedes the README wherever
> the README is older than a feature (e.g. in-UI federation creation, migration,
> and relay all post-date the README's "not from the UI yet" note).

---

## 1. The use case in one paragraph

`test-app-xpub` is a **self-custody, multi-party** wallet. Each user brings their
own **hardware wallet** (Trezor and other devices), onboards its **XPUB**, and
joins one or more **federations** (m-of-n P2WSH `sortedmulti` groups). Spending is
a **proposal lifecycle**: a member proposes a send, each required cosigner signs
**in their own browser** with their own device, partial signatures are merged
server-side, and once the threshold is met any member can broadcast. The Asterism
Rust library is linked **directly** into the Axum binary — no signing service, no
WASM, no proxy; the hardware wallet only ever talks to the browser, never the
backend. The app also supports **creating federations in the UI**, **migrating** a
federation's roster (with on-chain fund migration), and **relaying** funds that
land on a superseded version forward to the current one. It is the testbed that
exercises interactive (human-in-the-loop) multisig signing.

Mental model: *a shared safe with several keyholders.* Everyone holds their own
key; the safe opens only when enough keyholders turn theirs.

---

## 2. Architecture at a glance

```
   Browser + hardware wallet (Trezor Connect v9, in-page)
            │  XPUB capture (onboard.js) · signTransaction (proposal-sign.js)
            ▼
   ┌────────────────────────────────────────────────────────┐
   │                  Axum router (main.rs)                   │
   │     session layer · ServeDir · TraceLayer                │
   └───┬───────────┬───────────┬───────────┬─────────────────┘
       │           │           │           │
    auth/      onboard/   new_federation/  federations/ · proposals/
    home       (signers)   migrations/relay  (per-federation wallet)
       │           │           │           │
       └───────────┴─────┬─────┴───────────┘
                         ▼
                  WalletManager (wallet.rs)
                   │            │
         FederationWallet   bitcoincore-rpc ──▶ bitcoind regtest
         (BDK + ChangeSet)
                         │
              ┌──────────┴───────────────────────────────┐
              │              PostgreSQL                    │
              │ users · signers · federations ·            │
              │ federation_members · federation_versions · │
              │ migrations · transaction_proposals/        │
              │ _signatures/_rejections · sessions         │
              └────────────────────────────────────────────┘
```

**Boot** (`src/main.rs`): run `migrations/*.sql` in order → init the
`tower-sessions` Postgres store → upsert three test users
(`test1/2/3@test.com`, password `test1234`) → bind `APP_HOST:APP_PORT`
(default `127.0.0.1:8090`).

---

## 3. Feature catalog

### 3.1 Authentication & sessions — `src/auth.rs`, `src/handlers/auth.rs`

Same primitives as the sibling app: Argon2id PHC hashes in `users`, signed
cookie-backed `tower-sessions` sessions in Postgres, an `AuthUser`
login-required extractor (303-redirects to `/login` when anonymous), and idempotent
seeding of three test users. First-time users (no `signers` row) are routed to
`/onboard`; returning users land on `/home`.

### 3.2 Hardware-wallet onboarding — `src/handlers/onboard.rs` + `static/onboard.js`

- `GET /onboard` renders a page that loads `@trezor/connect@9` from the official
  CDN (no JS build step) and calls `TrezorConnect.getPublicKey` at the configured
  BIP-48 path (default `m/48'/1'/0'/2'` — P2WSH multisig).
- The browser assembles a **BIP-380 descriptor key**
  `[<root_fingerprint>/48'/1'/0'/2']<xpub>` and POSTs it to
  `POST /onboard/signer` (JSON).
- The server validates it by constructing an `asterism::xpub::ExternalSigner`
  (which runs all BIP-380/BIP-32 checks) and persists a `signers` row with
  fingerprint, xpub, derivation path, device type, and network.
- **Device types.** The federation builders accept `DeviceType::{Trezor, Jade,
  PassportPrime, Ledger, Coldcard, Generic}` (`new_federation::parse_device_type`).
  The current onboarding handler tags new signers as `Trezor`, but the stored
  `device_type` round-trips through `parse_device_type` so mixed-device federations
  are representable.
- Duplicate-fingerprint onboarding returns `409 Conflict` with a friendly message;
  a rejected key returns `400` with the parser's reason.

### 3.3 In-UI federation creation — `src/handlers/new_federation.rs` + `federation_new.html`

- `GET /federations/new` renders a member picker: every candidate user with their
  P2WSH-signer status badge (`db::list_users_with_p2wsh_signer_status`), the
  creator pre-checked + disabled (with a hidden field so they're always submitted),
  the configured derivation path and network shown for sanity.
- `POST /federations` validates label (≤100 chars) and threshold (`1 ≤ m ≤ n`),
  forces the creator into the member set (`dedupe_and_force_include_creator`),
  resolves each member's P2WSH signer (`resolve_member_signers` — collects **all**
  missing members into one `MissingMemberSigner` error), builds the canonical
  multipath descriptor via `asterism::core::build_federation`, and atomically
  inserts the federation + memberships (`db::insert_federation_with_members`).
- Only `wsh(sortedmulti)` federations are supported in this iteration.

### 3.4 Per-federation BDK wallet — `src/wallet.rs` (`WalletManager`, `FederationWallet`)

One `bdk_wallet::Wallet` per federation, cached behind an async mutex and persisted
as a JSON `ChangeSet` on `federations.bdk_changeset`. Chain data comes from the
local node via `asterism::core::chain_sync::emitter_sync`.

| Feature | Function | Route |
|---|---|---|
| Lazy load / init from row | `WalletManager::load_or_init` | every federation page |
| Chain sync (blocks + mempool, persists delta) | `FederationWallet::sync` | implicit |
| Reveal receive addresses (`REVEAL_COUNT = 20`) | `reveal_addresses` | `GET /federations/{id}/receive` |
| Balance | `FederationWallet::balance` (+ reservations, §3.7) | receive/send/federation cards |
| Address detail (QR + receipt history, spent flags) | `address_history` + `locate_address` | `GET /federations/{id}/addresses/{address}` |
| Tip height | `tip_height` | header |
| First external address (no-persist) | `first_external_address` / `reveal_first_external` | migration/relay routing |

**Persistence model:** `Wallet::take_staged()` returns the delta since the last
take; the manager merges it into the aggregate `ChangeSet` and writes the merged
blob back. DB writes happen **after** the wallet mutex is released so I/O doesn't
block other readers. This is the recommended pattern for BDK backends without a
native `WalletPersister`.

### 3.5 Proposal lifecycle — `src/handlers/proposals.rs`

The core of the self-custody model. A proposal walks an m-of-n P2WSH multisig
through build → sign → finalize → broadcast over multiple HTTP round-trips and
multiple devices.

| Step | Route | What happens |
|---|---|---|
| Create | `POST /federations/{id}/proposals` | `FederationWallet::build_proposal` → unsigned PSBT + cached `proposal_json` / `coin_selection_json`. |
| Detail | `GET /federations/{id}/proposals/{pid}` | cosigner status, actions, current PSBT state. |
| Sign data | `GET /federations/{id}/proposals/{pid}/sign-data` | server returns the Trezor-shaped JSON payload (§3.6). |
| Submit signature | `POST /federations/{id}/proposals/{pid}/signatures` | browser POSTs the device's partials; server injects/merges + tries finalize. |
| Reject | `POST /federations/{id}/proposals/{pid}/rejections` | advisory `transaction_rejections` row; status unchanged. |
| Cancel | `POST /federations/{id}/proposals/{pid}/cancel` | proposer abandons the proposal. |
| Broadcast | `POST /federations/{id}/proposals/{pid}/broadcast` | finalize → extract → `sendrawtransaction`. |

**Statuses:** `proposed` → `signing` → `finalized` → `broadcast`, plus
`cancelled`. **Kinds** (`0006_proposal_kind.sql`): `send` (ordinary spend),
`migration` (roster-change sweep, §3.8), `relay` (forward sweep, §3.9). Rejections
are *advisory only* — they surface the pushback so the proposer can decide to
`cancel`; they do not change status.

### 3.6 Trezor multisig signing protocol — `FederationWallet::trezor_sign_request` + `inject_trezor_signatures` + `static/proposal-sign.js`

This is the subtle, must-not-regress part and the main thing the app proves about
interactive signing. The server builds the exact payload
`TrezorConnect.signTransaction` needs:

- **Per input:** `script_type: "SPENDWITNESS"`, the signing device's BIP-32
  `address_n` (pulled from the PSBT's `bip32_derivation` by master fingerprint),
  and `multisig.pubkeys[]` — each cosigner's `HDNode` + relative `[keychain,
  index]` suffix, **sorted lexicographically by the pubkey each cosigner derives at
  that path**, matching `sortedmulti`'s on-chain script order. All
  `multisig.signatures[]` start blank.
- **Per output:** recipient outputs are `PAYTOADDRESS`; change outputs (detected
  because the signing device's fingerprint appears in the output's
  `bip32_derivation`) are `PAYTOWITNESS` + a `multisig` field — that combination is
  how the firmware whitelists native P2WSH change. (`PAYTOMULTISIG` is the *legacy
  P2SH* path and triggers a "wrong derivation path" warning — don't use it.)
- **refTxs:** every input's previous transaction is fetched via `bitcoincore-rpc`
  (wrapped in `spawn_blocking`) and shipped so the device can verify input amounts.
- **Sighash envelope:** the payload echoes BDK's chosen `version` (2) and
  `lock_time` (BDK's anti-fee-sniping `nLockTime` = current tip). **Omitting these
  makes Trezor sign `version=1, locktime=0`, and bitcoind rejects the broadcast
  with `mempool-script-verify-flag-failed` (NULLFAIL).**
- **Signer slots:** for each input the server computes which slot in the sorted
  pubkey list the signing device occupies, so the browser can pull the right
  signature out of `result.signatures[input][slot]`.

The browser ships per-input DER signatures back; `inject_trezor_signatures` slots
them into a freshly-cloned base PSBT (matching each to its pubkey by fingerprint),
then `merge_partial_signature` (`Psbt::combine`) folds it into the canonical PSBT
and probes `finalize_psbt` on a clone so a failed finalize doesn't poison the base.

### 3.7 Reservations (spendable-now accounting)

`db::sum_inflight_inputs_for_federation` subtracts every input locked by an
in-flight proposal (status `proposed`/`signing`/`finalized`) from the balance, so
the "spendable now" figure (`BalanceView::from_balance(balance, reserved)`) never
double-spends a UTXO that's already committed to a pending proposal. The
aggregation is a SQL `SUM((coin_selection_json->>'total_input_sat')::bigint)`.

### 3.8 Federation migration & lineage — `src/handlers/migrations.rs` + `federation_manage.html`

Roster changes are versioned: a migration mints a **pending successor version**
without moving funds up front, then a `migration`-kind proposal (signed by the
**current** members) sweeps the funds and — on broadcast — enacts the version flip
(*consent-by-signing*).

- `GET /federations/{id}/federation` (`federation_manage`) — the merged
  **Federation tab**: the whole lineage's version history with per-version
  balances/status, a **relay** affordance on funded superseded versions the viewer
  can sign for, and the **migrate form** (shown only to a current signer of the
  active version when no migration is in flight).
- `POST /federations/{id}/migrations` (`migrate_post`) —
  1. Validates membership, "is active", and "no in-flight migration".
  2. Computes the roster delta with `asterism::core::roster::compute_roster_plan`
     and validates the next threshold.
  3. Resolves the next members' signers, builds the successor descriptor
     (`build_federation`).
  4. Builds the **sweep tx to the successor's first address first** (so an unfunded
     federation fails cleanly with no dangling pending version).
  5. Persists the migration + pending version (`db::create_pending_migration`) and
     opens a `migration`-kind proposal (`db::insert_migration_proposal`).
- `POST /federations/{id}/migrations/{mid}/cancel` (`cancel_post`) — abandon the
  pending version + its sweep proposal, freeing the lineage. Members only.
- Back-compat: `/federations/{id}/migrate` and `/federations/{id}/lineage` both
  302 to `/federation` (`redirect_to_federation`).
- Lineage sync fan-out (`WalletManager::sync_lineage`) freshens **every** version's
  wallet so superseded versions still detect late inflows.

### 3.9 Relay sweeps — `migrations::relay_post`

- `POST /federations/{id}/relay` sweeps late inflows that landed on a **superseded**
  version forward to the lineage's **current** version.
- Persisted as a `relay`-kind proposal **on the superseded version**, so *that
  version's* members — including signers removed in later versions — are the ones
  who sign (this is the explicit requirement that removed members can still move old
  funds forward). Broadcasting it moves funds only; it does **not** change versions.
- Relay is offered only on a funded, non-current version the viewer can actually
  sign for.

### 3.10 Configuration surface — `src/config.rs`

`AppConfig::from_env` (sibling `.env` auto-loaded). Fields: `bind`
(`APP_HOST`/`APP_PORT`, default `127.0.0.1:8090`), `session_secret`
(`APP_SESSION_SECRET`, 64-byte hex), `database_url`, `network`
(`BITCOIN_NETWORK`), `federation_derivation_path` (`APP_FED_DERIVATION_PATH`,
default `"m/48'/1'/0'/2'"` — **must be double-quoted** or apostrophes strip the
hardened markers), `bitcoin_rpc_url/user/password`, `bitcoin_wallet_name`, and the
Trezor Connect manifest fields `trezor_coin` (`"test"` covers testnet+regtest,
`"btc"` is mainnet), `trezor_manifest_email`, `trezor_manifest_app_url`. `RUST_LOG`
sets the tracing filter. See the README's Configuration section for prose on each.

### 3.11 Route map (current)

| Method | Path | Handler |
|---|---|---|
| GET | `/`, `/home` | `home::root`, `home::home` |
| GET/POST | `/login` · POST `/logout` | `auth::*` |
| GET | `/onboard` · POST `/onboard/signer` | `onboard::*` |
| GET | `/federations/new` · POST `/federations` | `new_federation::*` |
| GET | `/federations/{id}` | `federations::redirect_to_default` |
| GET | `/federations/{id}/federation` | `migrations::federation_manage` |
| GET | `/federations/{id}/migrate`, `/lineage` | `migrations::redirect_to_federation` |
| POST | `/federations/{id}/migrations` | `migrations::migrate_post` |
| POST | `/federations/{id}/migrations/{mid}/cancel` | `migrations::cancel_post` |
| POST | `/federations/{id}/relay` | `migrations::relay_post` |
| GET | `/federations/{id}/receive`, `/send` | `federations::receive`, `send` |
| GET | `/federations/{id}/addresses/{address}` | `addresses::show` |
| POST | `/federations/{id}/proposals` | `proposals::create` |
| GET | `/federations/{id}/proposals/{pid}` | `proposals::detail` |
| GET | `/federations/{id}/proposals/{pid}/sign-data` | `proposals::sign_data` |
| POST | `/federations/{id}/proposals/{pid}/signatures` | `proposals::submit_signature` |
| POST | `/federations/{id}/proposals/{pid}/rejections` | `proposals::submit_rejection` |
| POST | `/federations/{id}/proposals/{pid}/cancel` | `proposals::cancel` |
| POST | `/federations/{id}/proposals/{pid}/broadcast` | `proposals::broadcast` |

`/static/*` via `ServeDir`; everything wrapped in `TraceLayer` + the session layer.
Pages are Askama renders; the only client-side JS is `onboard.js` and
`proposal-sign.js` (both thin Trezor Connect drivers).

### 3.12 Schema & migrations — `migrations/`

`0001_init` (users, signers, federations, federation_members) ·
`0002_bdk_wallet` (bdk_changeset, tip_height, descriptor checksum) ·
`0003_proposals` (proposals/_signatures/_rejections) ·
`0004_federation_versions` (lineage versioning) · `0005_migrations` (migration
records) · `0006_proposal_kind` (`send`/`migration`/`relay`). The `tower-sessions`
store runs its own schema migration at startup.

---

## 4. Developer entry points (where to start for common tasks)

| I want to… | Start here |
|---|---|
| Add an authenticated route | extractor `auth::AuthUser`; register in `main.rs`; follow `federations::receive`. |
| Change proposal building | `FederationWallet::build_proposal` (+ `proposal_view_models`). |
| Touch the Trezor payload / signing | `FederationWallet::trezor_sign_request` (mind the version/locktime + `sortedmulti` ordering + `PAYTOWITNESS` notes). |
| Change signature merge/finalize/broadcast | `inject_trezor_signatures` → `merge_partial_signature` → proposals `broadcast`. |
| Add a device type | `new_federation::parse_device_type` + onboarding tag. |
| Change roster/migration logic | `migrations::migrate_post` + `asterism::core::roster`. |
| Adjust spendable-now accounting | `db::sum_inflight_inputs_for_federation` + `BalanceView::from_balance`. |
| Add a config knob | `AppConfig` + `from_env` in `src/config.rs`. |

## 5. Relationship to the rest of Asterism

- Library crates: [`asterism-xpub`](https://github.com/gmikeska/asterism-xpub)
  (`ExternalSigner`, `DeviceType`) and
  [`asterism-core`](https://github.com/gmikeska/asterism-core)
  (`build_federation`, `roster`, `chain_sync`, PSBT pipeline), consumed via the
  [`asterism`](https://github.com/gmikeska/asterism) facade.
- Hardware wallet only talks to the **browser** (`@trezor/connect@9`); the backend
  never sees the device.
- Sibling app (custodial, HSM-backed, server-side autonomous signing, two chains):
  [`test-app-pkcs11`](https://github.com/gmikeska/test-app-pkcs11) — see its
  `FEATURES.md` for the contrasting model.
- Migration design references live in `emerald_multisignature/` (e.g.
  `xpub_federation_migration.md`).
