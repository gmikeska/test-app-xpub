# test-app-xpub

Server-rendered Axum web app that exercises [`asterism-xpub`](../asterism-xpub):

1. User logs in with email + password.
2. On **first** login (no Trezor on file) the user is sent to `/onboard`.
   That page uses `@trezor/connect@9` in the browser to call
   `getPublicKey({ path: m/48'/1'/0'/2', coin: 'test' })`, assembles a
   BIP-380 descriptor key
   `[<root_fingerprint>/48'/1'/0'/2']<xpub>`, and POSTs it back to the
   server. The handler validates the key by constructing an
   [`asterism_xpub::ExternalSigner`] and persists the result.
3. On **subsequent** logins (a signer row already exists for the user) the
   user is sent to `/home`, which lists every federation they're a member
   of (an empty state, until federation construction is wired up).

The Asterism Rust library is linked **directly** into the Axum binary —
there is no separate signing service, no WASM, no proxy. Trezor only
talks to the browser; the backend never sees the device.

## Prerequisites

- PostgreSQL with a database `asterism_xpub` reachable via
  `postgres://asterism:asterism@127.0.0.1:5432/asterism_xpub`
  (see `.env`).
- A Trezor device or the Trezor Emulator. The page loads
  `@trezor/connect` from JSDelivr; no JS build step is required.

## Run

```bash
cd test-app-xpub
cargo run
```

On startup the app:
- runs `migrations/0001_init.sql`,
- upserts three test users (`test1@test.com`, `test2@test.com`,
  `test3@test.com`, password `test1234`),
- binds `APP_HOST:APP_PORT` (default `127.0.0.1:8090`).

Log in as any of the three users; you'll be sent to `/onboard` the first
time and `/home` thereafter.

## Layout

```
test-app-xpub/
├── Cargo.toml
├── .env
├── README.md
├── migrations/0001_init.sql
├── src/
│   ├── main.rs           router, app state, startup migrate + seed
│   ├── config.rs         AppConfig::from_env()
│   ├── db.rs             PgPool helpers (users, signers, federations)
│   ├── auth.rs           Argon2id, AuthUser session extractor
│   ├── error.rs          AppError + IntoResponse
│   ├── models.rs         row structs
│   └── handlers/
│       ├── mod.rs
│       ├── auth.rs       GET/POST /login, POST /logout
│       ├── onboard.rs    GET /onboard, POST /onboard/signer
│       └── home.rs       GET /, GET /home
├── templates/
│   ├── base.html
│   ├── login.html
│   ├── onboard.html
│   └── home.html
└── static/
    ├── styles.css
    └── onboard.js
```
