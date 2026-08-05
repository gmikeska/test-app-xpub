# Passport Prime ↔ EmVault Integration — Design Document

**Scope:** Foundation Devices **Passport Prime** as an **external cosigner** (PSBT/PSET-from-computer, "mode 2" only) into **test-app-xpub**, Bitcoin/PSBT first, Elements/PSET later.
**Purpose:** first step of the "Passport Prime → GroupVault" roadmap; immediate goal is to **confirm and refine the new Taproot implementation on real hardware before publishing emvault 0.8.0.**
**Status:** design / investigation. No code changed. All code claims verified against `~/Projects/asterism` at time of writing (2026-08-04).
**Explicitly out of scope:** writing a Passport-native (KeyOS) app ("mode 1").

---

## 0. TL;DR — read this first

1. **The stated goal is currently BLOCKED at the device, not at our code.** Passport / Passport Prime **stock firmware does not sign Taproot *multisig*** (`tr(NUMS, multi_a(...))` / tapscript / miniscript). It signs **single-sig Taproot** (send/receive `bc1p…`) and **P2WSH multisig** — not our federation's taproot script path. So Passport Prime **cannot be the hardware that confirms our taproot implementation in mode 2 on stock firmware.** (§3, §4.)
2. **Two EmVault code gaps would block *any* external taproot-multisig signer** — independent of which device we use — and must be fixed before *any* hardware taproot confirmation over the external round-trip:
   - `SigningCoordinator::receive_signature` only inspects `partial_sigs` (ECDSA) and **rejects a valid taproot `tap_script_sigs` signature** (`emvault-core/src/psbt.rs:388-415`). (§4.1)
   - The capability matrix **over-claims** `taproot: true` for `DeviceType::PassportPrime` (`emvault-xpub/src/signer.rs:271-276`) — inaccurate for multisig. (§4.2)
   - Plus: test-app-xpub only ever builds `wsh(sortedmulti)` federations; the `tr(...)` path exists in the library but is not threaded through the app. (§4.3)
3. **The good news:** the **external round-trip contract already exists and needs zero new server signing logic.** `DeviceType::PassportPrime` is already wired; a Passport is functionally the Jade "returns a full partial PSBT" case. **P2WSH** Passport support is an onboard gate + two front-end files. (§5, §6.)
4. **Recommendation:** de-risk in the order the constraints allow — (Phase 0) land Passport **P2WSH** integration now to prove the whole transport/registration/round-trip end-to-end on real hardware; (Phase 1) fix the external-taproot code gaps and confirm the **taproot** path against a device that *does* sign `tr(multi_a)` today (Coldcard Edge / Ledger / BitBox02) or against our own HSM/dev-signer; (Phase 2) enable Passport taproot-multisig only once Foundation ships miniscript/tapscript firmware. (§8.)

> **Flag for Greg (as prominent as I'd make it):** If "confirm the new Taproot impl on real hardware before 0.8.0" specifically means *Passport Prime signing our `tr(multi_a)` federation*, that is **not achievable today** — it depends on Foundation firmware we do not control. We need a decision: (a) confirm taproot on a different, taproot-multisig-capable device now; (b) confirm taproot against HSM/software and use Passport only for the P2WSH hardware shakedown; or (c) slip the "hardware taproot" gate on 0.8.0 until Passport firmware lands. See §8 / §9-Q1.

---

## 1. Architecture overview

test-app-xpub is a **watch-only Axum web app**. It holds only public descriptors, builds unsigned PSBTs server-side, hands them to a device **in the trustee's browser**, and takes signatures back over HTTP. There is no server-side private key and **no server-side signing** — every signer is inherently "external."

```
                 test-app-xpub (server, watch-only)
  build unsigned PSBT ──► GET /…/sign-data ──► { psbt_b64, descriptor, network }
                                                       │
                                          (browser transport layer)
                                                       │  QR / USB / (SD dropped on Prime)
                                                       ▼
                                              ┌─────────────────┐
                                              │  Passport Prime │  signs, returns signed PSBT
                                              └─────────────────┘
                                                       │
  merge + finalize ◄── POST /…/partial-psbt ◄──────────┘   { partial_psbt_b64 }
```

Mode-2 means: **the computer owns the transaction; Passport is a stateless signer** that ingests a PSBT, signs, and returns it. This maps *exactly* onto EmVault's existing `SignerType::External` model (§5) — the coordinator hands out a `SigningRequest.psbt` and later ingests the signed PSBT. Passport requires **no new Rust signer type**.

---

## 2. Transports — how Passport Prime exchanges a PSBT with a computer

Verified against Foundation product/docs/blog (citations §10). **Passport Prime differs from the original Passport**: it **drops the microSD slot** and adds USB-C data, NFC, and QuantumLink BLE.

| Transport | Prime? | PSBT encoding | Viable for test-app-xpub (mode 2)? |
|---|---|---|---|
| **Animated QR** | ✅ camera + 3.5" display | **UR2.0 / BC-UR** (`crypto-psbt` for PSBTs, `crypto-output`/`crypto-account` for descriptors); **BBQr** also supported (better for large payloads). Fountain-coded multi-frame. | **✅ RECOMMENDED.** Canonical air-gapped path; interoperable (Sparrow/Nunchuk/Envoy use the same). Human-in-the-loop: browser renders animated QR → Passport camera; Passport displays signed-PSBT QR → webcam scans back. |
| **USB-C (data)** | ✅ (Prime adds data port) | Foundation states "data transfer including PSBTs," format/third-party protocol **UNCONFIRMED**. Original Passport had no USB signing. | ⚠️ Possible, **unverified**. No public third-party USB PSBT protocol confirmed; do not design against it until proven. |
| **QuantumLink (secure BLE)** | ✅ | Proprietary, dedicated BT chip, post-quantum enc; real-time PSBT via Envoy. | ❌ for now — **proprietary; no third-party SDK confirmed.** Assume unavailable to test-app-xpub. |
| **NFC** | ✅ | approvals / data exchange | ❌ not a general PSBT transport for a browser web app. |
| **microSD** | ❌ **removed on Prime** | (was the easy file path on Gen1/2) | ❌ **not available on Prime.** Note: `capabilities_for()` still advertises `SdCard` for Passport — see §4.2. |

**Decision:** design the mode-2 integration around **animated QR (UR2.0 `crypto-psbt`, BBQr fallback)** as the primary/only assumed transport. Treat USB-C and QuantumLink as **future enhancements pending confirmation from Foundation** (open questions §9-Q2). Because the server contract is transport-agnostic (§6), adding USB/BLE later is a browser-side change only.

**Rust building blocks:** UR (`ur` crate / Foundation's own Rust UR — KeyOS is Rust), `crypto-psbt`/`crypto-output` UR types; a BBQr Rust implementation exists; PSBT is already rust-bitcoin `Psbt` in emvault-core. **However, the animated-QR encode/decode lives in the *browser* (JS)** in this architecture, mirroring `emvault-jade`'s JS SDK — so the primary new code is a JS transport module, not Rust (§6).

---

## 3. Taproot-support finding (device firmware) — MAKE-OR-BREAK

**Question:** does Passport Prime firmware sign **P2TR multisig / `tr(...multi_a...)`** at all?
**Answer: No — not in stock firmware (as of mid-2026).** Verified against primary Foundation sources.

| Capability | Passport / Prime stock firmware | Source |
|---|---|---|
| Single-sig Taproot (send/receive `bc1p…`, key-path) | ✅ since fw **v2.3.0** (Feb 2024), "full support for sending and receiving using Taproot" | foundation.xyz/blog/passport-version-2-3-0-is-now-live |
| Standard multisig (P2WSH / native segwit) via PSBT | ✅ (Sparrow/Nunchuk/Envoy interop) | econoalchemist multisig guide; foundation docs |
| **Taproot MULTISIG (`tr()`, `multi_a`, tapscript) / miniscript** | ❌ **not in core firmware; roadmapped post-launch** | docs.foundation.xyz/faq; community.foundation.xyz/t/when-miniscript-on-passport/390 (community-thread body did not extract cleanly — **re-verify at integration time**) |
| KeyOS third-party app (Liana Signer) miniscript | ⚠️ P2WSH-focused; **taproot shelved**. And a KeyOS app = **mode 1 (out of scope)** | foundation.xyz/app-showcase/liana-signer |

**Our federation descriptor is exactly the unsupported shape.** From `emvault-core/src/descriptor.rs`: `tr(<NUMS>, multi_a(<m>, <x-only keys…>))` with the BIP-341 NUMS-unspendable internal key (H = `50929b74…03ac0`) — **the `multi_a` script leaf is the only spend path** (there is no key-path spend). This is precisely the "Taproot multisig / tapscript" class Passport firmware does not yet sign.

**Consequence:** In mode 2 with stock firmware, **Passport Prime cannot co-sign our taproot federation.** This is the same class of gap we hit with LWK for Elements. It **blocks the stated 0.8.0 hardware-taproot-confirmation goal *if that goal is specifically Passport*.**

**Industry context (for the alternatives in §8):** taproot-multisig hardware signing is nascent but exists. As of mid-2026, **Coldcard (Mk4/Q, Edge fw), Ledger (Bitcoin app v2.2+), and BitBox02** sign `tr(multi_a)`/tapscript-miniscript; **Blockstream Jade** (like Passport) supports SegWit miniscript but **not** full taproot `multi_a`. (Verify per-device before relying — citations §10.) So a *different* device can confirm our taproot impl on hardware now, even though Passport cannot.

---

## 4. EmVault code gaps that block external taproot-multisig (any device)

Even once a taproot-capable device is in hand, the current EmVault external round-trip cannot yet carry a taproot script-path signature. Three concrete, code-verified gaps:

### 4.1 `receive_signature` ignores `tap_script_sigs` (CORRECTNESS BUG for external taproot)
`emvault-core/src/psbt.rs:388-415` — `SigningCoordinator::receive_signature` decides "did this signer contribute a new signature?" by iterating **`input.partial_sigs.keys()`** only (the ECDSA map). A taproot script-path signature lands in **`tap_script_sigs`** keyed by `(x-only, TapLeafHash)` — never in `partial_sigs`. So when a Passport (or any external device) returns a `multi_a` signature:
- `Psbt::combine` **does** merge the `tap_script_sigs` entry (good), but
- `found_new` stays `false` → `receive_signature` returns **`PsbtError::InvalidSignature`** and never records the signer or advances the threshold.

Note the asymmetry: `signers_with_sigs` (`psbt.rs:483-489`), used by the **local HSM** path, *does* count `tap_script_sigs`/`tap_key_sig`. So local-HSM taproot signing is tracked correctly; the **external round-trip is not.** **Fix:** extend `receive_signature`'s attribution scan to also match new `tap_script_sigs`/`tap_key_sig` entries by `tap_key_origins` fingerprint. (Custody-critical: this decides threshold satisfaction — must be correct and tested.)

> **Note on test-app-xpub specifically:** the web app does **not** call `receive_signature`. It merges via `FederationWallet::merge_partial_signature` → `core_psbt::combine_psbt` + a BDK `finalize_psbt` probe (`test-app-xpub/src/wallet.rs:2118-2154`). `Psbt::combine` unions `tap_script_sigs` fine, and BDK's finalizer counts taproot sigs — so **the app's merge/finalize may work for taproot even while the library coordinator's `receive_signature` is broken.** This must be validated end-to-end (§9-Q3); do not assume. The library gap still must be fixed because `receive_signature` is the "intended" abstraction and is used by the library round-trip tests / other consumers.

### 4.2 Capability matrix over-claims taproot for Passport
`emvault-xpub/src/signer.rs:271-276`:
```rust
DeviceType::PassportPrime => SignerCapabilities {
    blind_signing: false,
    taproot: true,                                   // ← inaccurate for MULTISIG (see §3)
    musig2: false,
    transports: vec![TransportType::Usb, TransportType::Qr, TransportType::SdCard],
},
```
Two inaccuracies: (a) `taproot: true` is only true for single-sig, not our `multi_a` multisig; (b) `transports` lists `SdCard`, which **Prime removed** (§2). **Fix:** either narrow the semantics of the `taproot` capability flag (e.g. split single-sig vs multisig/tapscript) or set it to reflect real multisig support, and drop `SdCard` for Prime. Low risk, but it currently makes the system *believe* Passport can taproot-multisig when it cannot — a latent foot-gun.

### 4.3 test-app-xpub never builds `tr()` federations
`test-app-xpub/src/handlers/new_federation.rs:272-274` always calls `emvault::core::build_federation(...)`, which emits **`wsh(sortedmulti)`** only. The library *can* emit `tr(NUMS, multi_a)` via `DescriptorBuilder::script_type(ScriptType::Tr)` (`emvault-core/src/descriptor.rs:131,193-246`), but the app never selects it. So even against a taproot-capable device, **the app cannot currently create the taproot federation to test.** **Fix:** thread a `ScriptType` choice through `create_federation` / the `POST /federations` handler.

---

## 5. Mapping to the EmVault `External` signer abstraction (what already exists)

**No new Rust signer type is required.** Verified touchpoints:

- **`SignerType::External`** — `emvault-core/src/signer.rs:72-80`. The *only* trait-level marker of external-ness.
- **`Signer` trait** — `emvault-core/src/signer.rs:184-219`. **Deliberately has no `sign` method**; signing happens off-process. An external signer implements 9 identity/capability methods only.
- **`ExternalSigner`** — `emvault-xpub/src/signer.rs:29-215`. The single, generic external signer used for *all* consumer devices; `signer_type()` always returns `External`. Construct a Passport cosigner via `ExternalSigner::from_descriptor_key(key, network, DeviceType::PassportPrime, label)` — parses the `[fp/48h/…]xpub` export format devices emit. **No per-device struct exists or is needed.**
- **`DeviceType::PassportPrime`** — `emvault-core/src/signer.rs:117` (present); string-parsed in `test-app-xpub/src/handlers/common.rs:25` and `new_federation.rs:384`.
- **`TransportType`** — `emvault-core/src/signer.rs:87-102` (`Usb/Ble/Qr/SdCard/Nfc/Pkcs11`). **Advertisement-only metadata — nothing in Rust dispatches on it.** Transport is a browser concern.
- **`SigningCoordinator`** — `emvault-core/src/psbt.rs:254`. `request_signatures` (`psbt.rs:311-351`) routes each `SignerType::External` signer to `SigningAction::External(ExternalSigning{ request: SigningRequest{ signer_id, fingerprint, psbt }, … })` (`psbt.rs:209-244`); non-blocking. `receive_signature` (`psbt.rs:367-419`) ingests the signed PSBT, `Psbt::combine`s it, attributes by fingerprint. (Bitcoin. Elements mirror: `ElementsSigningCoordinator`, `emvault-elements/src/pset.rs:254-315`.)

**PSBT taproot field contract** (from `emvault-pkcs11/src/signer.rs:418-536`, `sign_taproot_input`) — what an external device must be **given** vs **return** to participate in `multi_a`:

| PSBT field | Written by | Needed by external signer |
|---|---|---|
| `witness_utxo` (on **every** input) | BDK | **Given** — taproot sighash commits to all prevouts; missing any = hard fail |
| `tap_internal_key` (NUMS) / `tap_scripts` / `tap_merkle_root` | BDK (from descriptor) | **Given** — detection + BDK finalize |
| `tap_key_origins` (`PSBT_IN_TAP_BIP32_DERIVATION`): `x-only → (Vec<TapLeafHash>, (fp, path))` | BDK | **Given** — device finds its key + the `TapLeafHash` to build the script-path sighash; fp used for attribution |
| `sighash_type` | optional | defaults to `Default` |
| **`tap_script_sig`**: `(x-only, TapLeafHash) → Schnorr sig` | the signer | **Returned** — one per leaf; disjoint keys per cosigner, unioned by `Psbt::combine` |
| `tap_key_sig` | never (NUMS ⇒ no key-path) | must **not** be touched |

This is a clean, standards-based contract (BIP-341/342 script-path). A Passport that *did* implement tapscript multisig would slot in here with no server signing changes — the blockers are §3 (device) and §4 (our external-attribution + build-flow gaps), not the field contract.

---

## 6. Concrete wiring into test-app-xpub

The external round-trip contract **already exists and is device-agnostic.** A Passport is functionally identical to the Jade "returns a full partial PSBT" case.

**The seam (no new server signing logic):**
- **PSBT out:** `handlers::proposals::sign_data` — `test-app-xpub/src/handlers/proposals.rs:524`. Returns `{ psbt_b64, descriptor, network, federation_kind }`; the Trezor-only payload branch (`proposals.rs:563-577`) correctly returns `None` for non-Trezor. Passport consumes `psbt_b64` + `descriptor` + `network` like Jade.
- **Signed PSBT back:** `handlers::proposals::submit_partial_psbt` — `proposals.rs:754`, route `POST /federations/{id}/proposals/{pid}/partial-psbt` (`src/main.rs:204`). Already accepts any complete signed PSBT and merges/finalizes via `FederationWallet::merge_partial_signature` (`src/wallet.rs:2118`).

**The actual work (P2WSH path — works with Passport today):**
1. **Onboard gate (REQUIRED):** `parse_onboard_device_type` (`test-app-xpub/src/handlers/common.rs:43`) currently **hard-rejects** anything but Trezor/Jade. Add `PassportPrime` so a Passport `signers` row can be written. (`parse_device_type` at `common.rs:25`/`new_federation.rs:384` already accepts it; `DeviceType::PassportPrime` exists.)
2. **Onboard capture UI:** new Passport pane in `templates/onboard.html` (today only Trezor/Jade panes at `onboard.html:42-97`) that ingests Passport's exported descriptor key (`[fp/path]xpub`) via **animated QR (UR `crypto-account`/`crypto-output`)**, POSTed to the unchanged `POST /onboard/signer`.
3. **Signing UI:** Passport branch in `templates/proposal.html:149-151` (the "not yet supported" fallthrough) + a new `static/proposal-sign-passport.js` transport module modeled on `static/proposal-sign-jade.js` — GET `sign-data`, encode `psbt_b64` as animated UR `crypto-psbt` QR, scan Passport's returned signed-PSBT QR, POST `{ partial_psbt_b64 }` to `partial-psbt`.
4. **Descriptor registration parity:** like Jade's `registerMultisig`, Passport must import/recognize the federation multisig descriptor before signing. `sign_data` already returns `descriptor` — client-side only.
5. **(Optional) transport metadata:** no `TransportType` column exists; add `signers.transport` (migration) only if we want to record/validate QR-vs-USB per signer. Not required for a functioning round-trip.

**Additional work for the TAPROOT path (needed for the 0.8.0 goal):**
6. Fix §4.1 (`receive_signature` taproot attribution) and validate the app's `merge_partial_signature`/`finalize_psbt` probe on a `tap_script_sigs` PSBT (§9-Q3).
7. Fix §4.3 (thread `ScriptType::Tr` through `create_federation` so the app can *build* a taproot federation).
8. Fix §4.2 (capability accuracy).
9. A taproot-capable signer to actually test against (§3 / §8) — **not Passport (stock fw).**

---

## 7. Flows (sequence-level)

### 7.1 Registration (xpub + multisig descriptor)
```
Passport: Manage Account → Connect Wallet → (coordinator) → export account xpub as UR crypto-account QR
Browser (onboard.html Passport pane): scan animated QR → derive [fp/48h/1h/0h/2h]xpub descriptor key
  → POST /onboard/signer  → signers row { device_type: PassportPrime, descriptor_key, xpub, fingerprint, path }
Later, at federation build (POST /federations):
  ExternalSigner::from_descriptor_key(..., PassportPrime) for each member
  → build_federation → wsh(sortedmulti)  [or tr(NUMS,multi_a) once §4.3 lands]
  → descriptor persisted
Coordinator app / browser: display federation descriptor as UR crypto-output QR → Passport imports & stores multisig config
```

### 7.2 Signing (mode 2, P2WSH today)
```
User: POST /federations/{id}/proposals            → build unsigned PSBT (watch-only), status=proposed
Browser: GET /…/sign-data                          → { psbt_b64, descriptor, network }
  encode psbt_b64 as animated UR crypto-psbt QR    → Passport camera
Passport: verify (multisig config recognized), sign → display signed-PSBT UR QR
Browser: scan signed PSBT                           → POST /…/partial-psbt { partial_psbt_b64 }
Server: merge_partial_signature (combine_psbt + finalize probe)
  → when threshold met: finalize_and_extract → status=finalized
User: POST /…/broadcast                             → broadcast_raw
```
The taproot flow is identical at the transport/server level; it differs only in the PSBT fields carried (§5) and requires §4/§3 resolved.

---

## 8. Recommendation & phased plan

Ordered to de-risk what we *can* control now and isolate the device-firmware dependency.

**Phase 0 — Passport ↔ EmVault P2WSH integration (works today; proves everything except taproot).**
- Onboard gate + Passport onboard pane (UR `crypto-account`). (§6.1-2)
- `proposal-sign-passport.js` animated-QR transport (UR `crypto-psbt` / BBQr). (§6.3)
- Descriptor registration parity. (§6.4)
- **Exit:** a real Passport Prime co-signs a live P2WSH federation spend through test-app-xpub end-to-end. Confirms transport, registration, round-trip, and the whole browser↔server contract on real hardware. *Does not confirm taproot.*

**Phase 1 — External taproot-multisig readiness (our code) + interim hardware taproot confirmation.**
- Fix `receive_signature` taproot attribution + regression test (§4.1); validate app merge/finalize on `tap_script_sigs` (§9-Q3).
- Thread `ScriptType::Tr` through `create_federation` (§4.3).
- Correct the capability matrix (§4.2).
- **Confirm the taproot implementation on hardware using a device that signs `tr(multi_a)` today** — Coldcard (Edge), Ledger, or BitBox02 — wired via the *same* `ExternalSigner`/`partial-psbt` seam (they're all just `DeviceType` + a browser transport module), **or** against our own `emvault-dev-signer` / HSM taproot path already proven locally (`emvault-dev-signer/examples/dev_taproot_spend_probe.rs`).
- **Exit:** the new taproot impl is confirmed on real hardware (non-Passport) over the external round-trip → **satisfies the 0.8.0 "confirm on real hardware" goal without waiting on Foundation.**

**Phase 2 — Passport Prime taproot-multisig (GATED on Foundation firmware).**
- Track Foundation's miniscript/tapscript firmware + KeyOS. When `tr(multi_a)` signing ships in stock firmware, Passport slots into the Phase-1 taproot seam with no server changes.
- **Exit:** Passport Prime co-signs a taproot federation → completes the original "Passport confirms taproot" intent.

**Elements/PSET:** defer to a follow-up; mirrors the Bitcoin path (`ElementsSigningCoordinator`, `emvault-elements/src/pset.rs`), and inherits the same taproot-multisig device gap plus the LWK Elements gaps already tracked.

---

## 9. Open questions (need answers before/at implementation)

**Q1 (BLOCKER — for Greg).** Does "confirm the new Taproot impl on real hardware before 0.8.0" require **Passport specifically**, or is any taproot-multisig-capable hardware acceptable? If Passport-specific, the 0.8.0 hardware-taproot gate depends on Foundation firmware we don't control (§3) — do we (a) confirm on Coldcard/Ledger/BitBox02 now, (b) confirm against HSM/software + use Passport for the P2WSH shakedown, or (c) slip the gate? *This decision drives Phases 1-2.*

**Q2 (Foundation).** Confirm current Passport Prime firmware status for: (i) taproot **multisig**/miniscript/tapscript signing (timeline?); (ii) third-party **USB-C** PSBT protocol/format; (iii) **QuantumLink** third-party SDK availability. Our transport design assumes animated QR only until these are confirmed.

**Q3 (us).** Validate whether test-app-xpub's `merge_partial_signature` + BDK `finalize_psbt` probe (`wallet.rs:2118-2154`) already handles a taproot `tap_script_sigs` PSBT correctly end-to-end (it may, since it bypasses the broken `receive_signature`). If yes, the app path needs less change than the library; if no, both need the §4.1 fix.

**Q4.** Which UR types + which library (JS) for the browser transport — confirm `crypto-psbt` (PSBT) and `crypto-account`/`crypto-output` (descriptors), and whether to support BBQr from day one (Passport supports both; large taproot PSBTs favor BBQr).

**Q5.** Descriptor-registration UX: does Passport require pre-registering the multisig config before it will sign (Jade does), and does its importer accept our exact `wsh(sortedmulti)` / future `tr(NUMS,multi_a)` descriptor string as emitted by `emvault-core::descriptor`? Needs a real-device check (x-only sort order, origins).

**Q6.** Elements/PSET: does Passport Prime sign Liquid PSETs at all in mode 2? (Likely no / limited — track alongside the taproot question.)

---

## 10. Sources

**EmVault code (verified, `~/Projects/asterism`):** `emvault-core/src/signer.rs` (SignerType/Signer/DeviceType/TransportType), `emvault-core/src/psbt.rs` (SigningCoordinator/receive_signature/combine/finalize), `emvault-core/src/descriptor.rs` (NUMS, `tr(NUMS,multi_a)`, ScriptType::Tr), `emvault-pkcs11/src/signer.rs:418-536` (sign_taproot_input), `emvault-xpub/src/signer.rs` (ExternalSigner + capability matrix), `test-app-xpub/src/handlers/{proposals,new_federation,onboard,common}.rs`, `test-app-xpub/src/wallet.rs`, `test-app-xpub/src/main.rs`, `test-app-xpub/migrations/`.

**Foundation / external (untrusted web; re-verify at integration time):**
- foundation.xyz/blog/passport-version-2-3-0-is-now-live — Taproot single-sig support (fw v2.3.0)
- foundation.xyz/products/passport-prime + foundation.xyz/2026/03/from-qr-air-gapping-to-quantumlink-what-comes-next/ — Prime transports (USB-C data, NFC, QuantumLink BLE; microSD dropped)
- docs.foundation.xyz/passport/connect + /passport/passport-menu/settings/bitcoin — multisig descriptor QR import/export
- econoalchemist Foundation-Passport MultiSig guide — P2WSH multisig flow
- docs.foundation.xyz/faq + community.foundation.xyz/t/when-miniscript-on-passport/390 — miniscript/taproot-multisig roadmap (NOT shipped)
- foundation.xyz/app-showcase/liana-signer — KeyOS Liana (P2WSH; taproot shelved) [mode-1, out of scope]
- bbqr.org, developer.blockchaincommons.com (UR/animated QR) — transport encodings
- nunchuk.io/blog/miniscript-programmable-bitcoin, coldcard.com/docs/upgrade, ledger.com/blog-musig2-ledger-bitcoin-app, blog.bitbox.swiss — which hardware wallets sign `tr(multi_a)` today (Coldcard Edge / Ledger / BitBox02 yes; Jade no)
