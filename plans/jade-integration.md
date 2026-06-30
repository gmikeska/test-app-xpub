# Plan — Jade integration + Signet cutover (test-app-xpub)

> Implementation plan for adding **Blockstream Jade** onboarding + signing
> alongside the existing **Trezor** flow, and moving test-app-xpub from the
> regtest node to **Signet**.
>
> Design: `emvault_design/jade-integration.md`. Driver: `@emvault/jade`
> (`emvault-jade`). Spike: complete (Jade multisig-registration mapping verified
> by protocol analysis; hardware confirmation happens in Phase 7).
>
> **Guiding principle:** the published library crates do **not** change — all work
> is in this app + a vendored copy of the JS driver. Bitcoin only, Signet only.

## Status (code complete, pending hardware)

- ✅ **Phases 0–6 implemented and verified** in the dev environment: `cargo build`
  + `clippy --all-targets` clean, `jade::` unit tests green, both JS modules parse,
  vendor drift-check passing, `.env` on Signet.
- ⏳ **Phase 7 (hardware verification)** is the only remaining gate — it needs a
  physical Jade + Trezor against the live Signet node and a browser, so it's
  yours to run. Steps are below.

---

## Phase 0 — Branch & vendor the driver

- [ ] Create working branch `feature/jade-onboarding`.
- [ ] Copy `emvault-jade/src/{index.js,jade-rpc.js,cbor.js}` →
      `test-app-xpub/static/vendor/emvault-jade/` (mirrors `emvault-jade-test`).
- [ ] Add a **drift check**: a `scripts/check-vendor.sh` (or a `cargo test` /
      build-script assertion) that diffs the vendored copy against the source repo
      and fails if they differ. (Removed once `@emvault/jade` is npm-published.)
- [ ] Record the vendored driver's source commit hash in
      `static/vendor/emvault-jade/VENDOR.md`.

**Done when:** the driver loads in the browser from `/vendor/emvault-jade/index.js`
and the drift check passes.

---

## Phase 1 — Signet cutover (drop regtest)

- [ ] `.env`: point at the Signet node from `emvault-jade-test/.env` —
      `BITCOIN_RPC_URL=http://127.0.0.1:38332`, `BITCOIN_RPC_USER=signetbtc`,
      `BITCOIN_RPC_PASSWORD=signetbtcpass`, `BITCOIN_NETWORK=signet`.
      (Reminder: `.env` is gitignored — also update `.env.example`/README if present.)
- [ ] Confirm `AppConfig`/`config.rs` parses `signet` → `Network::Signet` (it
      already accepts `signet`); remove/ignore regtest-specific assumptions.
- [ ] Verify the federation derivation path stays `m/48'/1'/0'/2'` (coin type 1 is
      correct for Signet) — no change expected, just assert.
- [ ] `src/config.rs`: add a `jade_network()` helper mapping
      `Network::Signet → "testnet"` (and the rest of the table from the design doc),
      surfaced to templates + the Jade `sign-data` payload.
- [ ] Smoke test: app boots, `chain_sync` syncs against the Signet node, a
      federation receive page renders a `tb1…` address.

**Done when:** the app runs end-to-end against Signet (existing Trezor flow still
builds; no regtest references remain in config/templates).

---

## Phase 2 — Onboarding: device picker + server `device_type`

### Server (`src/handlers/onboard.rs`)
- [ ] Extend `OnboardSignerBody` with `device_type: String` (default `"Trezor"`
      for back-compat).
- [ ] Replace the hardcoded `DeviceType::Trezor` with
      `new_federation::parse_device_type(&body.device_type)`.
- [ ] Validate with the **same** `ExternalSigner::from_descriptor_key` (unchanged);
      persist the chosen `device_type`. Keep the `409` duplicate-fingerprint path.

### UI (`templates/onboard.html`)
- [ ] Add a **Trezor / Jade** radio picker; reveal the matching action button.
- [ ] Inject Jade config into `window.EMVAULT` (already used by onboard.js):
      add `jadeNetwork` (e.g. `"testnet"`) + the derivation path.
- [ ] Add `<script type="module">` import of `/vendor/emvault-jade/index.js` for
      the Jade branch (Trezor branch keeps the CDN `@trezor/connect`).

### UI (`static/onboard.js`)
- [ ] Trezor branch: unchanged (POST now includes `device_type:"Trezor"`).
- [ ] Jade branch: `JadeRpc.fromSerial → unlock(jadeNetwork) →
      getMasterFingerprintHex + getXpub(jadeNetwork, path)` → assemble
      `[fp/48'/1'/0'/2']xpub` → `POST /onboard/signer { descriptor_key,
      device_type:"Jade" }` → `close()`. Surface device-prompt/PIN states + errors.

**Done when:** a user can pick Jade, onboard over USB, and a `signers` row lands
with `device_type=Jade` and a valid descriptor key (verified in Phase 7 on
hardware; logic verifiable now with the Jade-test device or a stubbed key).

---

## Phase 3 — Server: Jade descriptor-object builder

- [ ] New helper (e.g. `src/jade.rs`): `fn jade_register_descriptor(federation
      members + threshold + network) -> JadeMultisig` producing the object:
      `{ variant:"wsh(multi(k))", sorted:true, threshold, signers:[{fingerprint,
      derivation:[48',1',0',2'], xpub, path:[]}] }` from each member's `signers`
      row (`fingerprint`/`xpub`/`derivation_path`).
- [ ] `fn jade_reg_name(federation_id) -> String` — deterministic, **1–15 ASCII**
      (e.g. `format!("ev{}", &hex[..12])`).
- [ ] Unit tests: object shape, sorted=true, threshold, signer count, name length
      bound, fingerprint/xpub round-trip from a fixture federation.

**Done when:** `cargo test` covers the builder against a known 2-of-3 fixture and
the name constraint.

---

## Phase 4 — Server: device-aware `sign_data` + `submit_signature`

### `sign_data` (`src/handlers/proposals.rs`)
- [ ] Look up the requesting member's signer `device_type`.
- [ ] Trezor → existing `{ device:"trezor", trezor: <TrezorSignRequest> }`.
- [ ] Jade → `{ device:"jade", jade: { psbt_b64: proposal.psbt_b64, register:{
      name, descriptor }, jade_network } }` (no `trezor_sign_request`,
      no `refTxs`).
- [ ] Make `SignDataResponse` a tagged enum (`device` discriminator) so the JS
      can branch.

### `submit_signature` (`src/handlers/proposals.rs`)
- [ ] Branch on the member's `device_type`.
- [ ] Trezor → existing `inject_trezor_signatures` → `merge_partial_signature`.
- [ ] Jade → body carries a **full signed PSBT (base64)**; call
      `merge_partial_signature(proposal.psbt_b64, signed_psbt_b64)` directly.
- [ ] Both converge on the existing finalize → broadcast path (unchanged).

**Done when:** `sign-data` returns the right shape per device and `submit_signature`
merges a Jade-signed PSBT (unit-testable with a fixture signed PSBT).

---

## Phase 5 — Sign: Jade signing JS

### `static/proposal-sign.js`
- [ ] Fetch `sign-data`; branch on `device`.
- [ ] Trezor branch: unchanged.
- [ ] Jade branch: `JadeRpc.fromSerial → unlock(jade_network) →
      registerMultisig(jade_network, register.name, register.descriptor) →
      signPsbt(jade_network, base64ToBytes(psbt_b64))` → `bytesToBase64(signed)`
      → `POST …/signatures { signed_psbt_b64 }` → `close()`.
- [ ] UX: show register-confirm + sign-confirm device prompts; clear errors for
      "wrong device / not unlocked / user rejected".
- [ ] Import the vendored driver as an ES module on the proposal page
      (`templates/proposal.html`).

**Done when:** the proposal page drives a Jade through register → sign → submit
and the partial lands server-side (hardware-verified in Phase 7).

---

## Phase 6 — Docs + cleanup

- [ ] Update `test-app-xpub/README.md` + `FEATURES.md`: device picker, Jade flow,
      Signet, the `@emvault/jade` vendor + drift note.
- [ ] Update the `emvault_design` cross-refs if anything shifted from the design.
- [ ] `cargo clippy --all-targets -- -D warnings` clean; existing tests green.

**Done when:** docs match behavior and lints/tests pass.

---

## Phase 7 — Hardware verification (gates "done")

Real Blockstream Jade + Trezor against the Signet node. These confirm the
spike's protocol analysis on actual devices.

- [ ] **Jade onboard** — USB onboard yields a valid `[fp/48'/1'/0'/2']xpub`,
      stored `device_type=Jade`.
- [ ] **Jade multisig registration** — `registerMultisig` accepts our descriptor
      object (the primary spike→hardware confirmation); device shows the cosigners.
- [ ] **Jade sign** — `signPsbt` recognizes the inputs and returns a signed PSBT
      that merges + finalizes; broadcast confirms on Signet.
- [ ] **Trezor-on-Signet** — Trezor (`coin:"test"`) onboards + signs a Signet PSBT
      the node accepts (anti-fee-sniping `version`/`locktime` echo still correct).
- [ ] **Mixed federation** — a 2-of-3 with one Trezor member + one Jade member:
      both sign their own way, partials merge, threshold finalizes, broadcast OK.
- [ ] Capture funding steps used (Signet faucet / node default-wallet
      `sendtoaddress`) in the README test section.

**Done when:** all six checks pass on hardware → feature complete.

---

## Risk register (carried from the design)
- Jade rejects our descriptor object → fall back to the `multisig_file` (Coldcard/
  Sparrow text) form `registerMultisig` also accepts; build that from the
  federation instead. (Mitigation already supported by the driver.)
- Trezor balks at Signet → confirm `TREZOR_COIN`/coin handling; worst case, scope
  Trezor verification as a fast-follow (Greg wants both, so treat as must-fix).
- Vendored-driver drift → the Phase 0 check guards it until npm publish.

## Out of scope
Liquid/PSET via Jade; regtest/testnet/mainnet; npm publish of `@emvault/jade`
(planned separately); Jade QR/air-gap.
