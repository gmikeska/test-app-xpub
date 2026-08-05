# Hardware Wallet Taproot-Descriptor Compatibility Matrix

**Scope:** Can each hardware signer register + sign our federation's taproot script-path descriptor —
`tr(NUMS, multi_a(m, [origin]xonly_i…))` — and, if not in that exact shape, what shape *does* it accept?
**Method:** primary sources only (BIPs, vendor firmware docs/changelogs). Every claim cited inline.
**Author:** Rosie · **Date:** 2026-08-04 · **Status:** research complete; **no code changed**.

> **One-sentence headline for Greg:** *Our current taproot descriptor — literal 64-hex NUMS + **fixed** x-only
> cosigner keys + literal `multi_a`, `KeyMode::Fixed` — is signable by **zero** consumer hardware wallets. Every
> device that can sign a taproot multisig requires the **BIP-388 wallet-policy shape**: NUMS encoded as an **xpub**
> and cosigners as **ranged xpubs** (`/**`). That is exactly the `Tr + Ranged` combination our code currently
> rejects (`DescriptorError::TaprootRangedUnsupported`). Lifting that gate is the gating work.*

---

## 0. Our shape today (verified against source)

From `emvault-core/src/descriptor.rs` (read 2026-08-04):

- **Internal key:** raw 64-hex x-only literal `NUMS_INTERNAL_KEY = 50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0` (BIP-341 *H*), emitted verbatim into the descriptor string (`descriptor.rs:60,243`).
- **Cosigner keys:** `KeyMode::Fixed` → `SinglePub { key: XOnly(..) }` with `[fingerprint/path]` origin, **no wildcard** (`build_taproot_key`, `descriptor.rs:265`).
- **Leaf:** literal `multi_a(m, …)`, cosigners pre-sorted by x-only bytes to *imitate* `sortedmulti_a` (miniscript 12.3.7 has no native `sortedmulti_a`) (`descriptor.rs:217-246`).
- **Gate:** `ScriptType::Tr` + `KeyMode::Ranged` → hard `TaprootRangedUnsupported` (`descriptor.rs:218-220`).

Emitted string shape:
```
tr(50929b74…803ac0, multi_a(2, [f1/48h/…]xonly1, [f2/48h/…]xonly2, [f3/48h/…]xonly3))
```

**Why no hardware can sign this, in one line:** it is a valid *output-script descriptor* (BIP-386 permits a raw
x-only key as the `tr()` internal key and in `multi_a`), so Bitcoin Core / BDK import it fine — but hardware signers
do not consume raw descriptors; they consume **BIP-388 wallet policies**, whose grammar has **no** raw-literal key
and **no** fixed (wildcard-less) key. [BIP-386: raw x-only key valid in `tr`/`multi_a`.] [BIP-388 §Specification:
every `KEY` is a placeholder `@i` **always** followed by `/**` or `/<M;N>/*`, and each key-info item is an *xpub*.]

---

## 1. The two structural gates (device-independent)

Both come straight from **BIP-388** (Salvatore Ingala, *Complete*, v1.1.0) and apply to **every** wallet-policy device
(Ledger, BitBox02, and Coldcard's descriptor grammar mirrors it):

**Gate A — NUMS must be an xpub, not a 64-hex literal.**
BIP-388 `KEY` = key-placeholder `@i` + derivation; the key-information vector holds **extended public keys** only.
There is no production for a bare 32-byte hex point. So *H* has to be re-expressed as a BIP-32 xpub: take *H* as the
public key with a chaincode (random for privacy, or a published constant), serialize as `xpub…`, and reference it as
`@0/**`. BIP-32 public child derivation adds `IL·G` to the point, so `H + IL·G` is **still** of unknown discrete log
at every index → the ranged NUMS xpub stays provably unspendable. (This is Coldcard's *recommended* option 1 —
"Origin-less extended key serialization with H … as BIP-32 key and random chaincode.")

**Gate B — cosigners must be ranged xpubs, not fixed x-only keys.**
Same clause: `@i` is **always** followed by `/**` or `/<M;N>/*`. A wallet policy cannot express "one fixed public key,
one address." Every signer must contribute an **xpub with a wildcard**. This is the `KeyMode::Ranged` our taproot path
rejects. (Our `wsh` federations already ship `Fixed`; taproot-on-hardware forces `Ranged`.)

**Consequence:** to put *any* of Ledger/Coldcard/BitBox02 on the federation, the descriptor must become
```
tr( <NUMS_as_xpub>/**, multi_a( m, [origin]xpub_1/**, …, [origin]xpub_n/** ) )   ← or sortedmulti_a
```
i.e. **`Tr + Ranged`**, the currently-rejected combination.

---

## 2. Per-device matrix

| Device | Taproot **multisig** (`tr`+`multi_a`/tapscript)? | (1) NUMS internal-key accepted form | (2) Fixed x-only vs required xpub/`/**` | (3) `multi_a` vs `sortedmulti_a` + limits |
|---|---|---|---|---|
| **Ledger** (Nano S+/X, Flex, Stax; Bitcoin app ≥ 2.1) | **YES** | **xpub placeholder only** (`@i/**`); raw 64-hex literal **not expressible** (BIP-388). NUMS = external xpub, no origin. | **Ranged xpub required** (`@i/**` or `@i/<M;N>/*`); fixed x-only **not expressible**. | **Both** `multi_a` and `sortedmulti_a` accepted as tapleaves (also taproot-miniscript, `musig()`). No small hard key cap documented; large *n* supported. |
| **Coldcard** (Mk4 & Q, **EDGE** firmware ≥ 6.3.5X/QX) | **YES** | Prefers **unspendable xpub** (option 1: *H* as BIP-32 key + random chaincode). **Static 64-hex literal `tr(50929b74…, …)` DEPRECATED in 6.3.5X/6.3.5QX** (still an option historically; not recommended, leaks unspendability). Also supports `r=@` / `r=<hex>` `H+rG` forms. | **Ranged xpub** (`xpub/<0;1>/*`, wildcard implied if omitted). Docs are xpub-only; **no fixed x-only single** in examples. | Examples are **`sortedmulti_a`**. Raw unsorted `multi_a` **not shown** → treat as *unverified / likely-rejected* (open Q1). Limits: **max 8 tapleaves**, tree depth ≤ 4, **≤ 32 keys** total, single-leaf multisig ≤ 32-of-32, no dup keys. |
| **BitBox02** (Multi / BTC-only / Nova; firmware **≥ 9.21.0**) | **YES** (v9.21.0 "Taproot wallet policies and Miniscript on Taproot") | **xpub placeholder** (BIP-388). Raw literal not expressible. | **Ranged xpub required** (BIP-388 `@i/**`). | **`sortedmulti_a`** confirmed (BIP-388 lists BitBox02 example `tr(@0/**,{sortedmulti_a(1,@0/<2;3>/*,@1/**),…})`). Raw `multi_a` acceptance **unverified** (open Q1). Segwit-v0 miniscript key cap 20; taproot-policy caps not separately published. |
| **Blockstream Jade** | **NO** (for our shape, today) | n/a | n/a | Single-sig taproot (`tr(k)`, BIP-86) since fw 1.0.34; **P2WSH multisig** registration yes; **`multi_a` taproot-multisig registration/sign NOT confirmed native**. Miniscript (2024) is the substrate but no confirmed "import `multi_a` → register → sign" flow. Treat as **NO** for `tr(NUMS, multi_a)`. |
| **Trezor Safe 3 / 5** | **NO** | n/a | n/a | **Key-path (BIP-86) taproot only; NO tapscript / script-path; NO taproot multisig.** General multisig is P2WSH-only. Hard blocker at firmware. |
| **Passport Prime** (Foundation, stock fw) | **NO** | n/a | n/a | Stock firmware signs **single-sig taproot** + **P2WSH multisig**; **does not sign taproot *multisig* / tapscript / miniscript**. (Confirmed in `passport-prime-integration.md` §3.) Needs Foundation miniscript/tapscript firmware — not in our control. |

**Sources:** BIP-386 (raw x-only key legal in `tr`/`multi_a`); BIP-388 v1.1.0 (wallet-policy grammar, xpub-only key vector, `@i/**` derivation, BitBox02 `sortedmulti_a` example); Ledger `app-bitcoin-new/doc/wallet.md` (supported scripts: `tr(KP)`/`tr(KP,TREE)`, leaves `multi_a`/`sortedmulti_a`/miniscript/`musig`; key-origin compulsory only for device-owned xpubs); Coldcard `firmware/new_edge/docs/taproot.md` + `miniscript.md` (allowed descriptors, 4 unspendable-internal-key methods, static-literal deprecation in 6.3.5X, 8-leaf/32-key limits); BitBox02 firmware `CHANGELOG.md` (v9.21.0 taproot wallet policies + MiniTapscript, v9.15.0 `wsh` miniscript); Blockstream Jade help/blog (miniscript 2024, multisig backup, BIP-86); Trezor/Spark support tracker (key-path-only); internal `passport-prime-integration.md` §3.

---

## 3. Per-device register-template strings

Notation: `<NUMS_xpub>` = an xpub whose public key is BIP-341 *H* (`0250929b74…803ac0`) with a chaincode
(random per-federation for privacy, or a documented constant); `xpubA/xpubB/xpubC` = the three cosigner xpubs at the
federation account path; `FP*` = 8-hex master fingerprints. 2-of-3 shown; generalize to m-of-n.

**Ledger (BIP-388 wallet policy) — descriptor template + key vector registered under a name:**
```
Template:  tr(@0/**, sortedmulti_a(2, @1/**, @2/**, @3/**))
Keys:      @0 = <NUMS_xpub>                      (external, NO origin → unspendable internal key)
           @1 = [FP1/48h/1h/0h/2h]xpubA
           @2 = [FP2/48h/1h/0h/2h]xpubB
           @3 = [FP3/48h/1h/0h/2h]xpubC
```
(Ledger also accepts `multi_a` in place of `sortedmulti_a`.)

**Coldcard EDGE (recommended option 1, unspendable xpub) — import descriptor file (`Settings → Miniscript → Import`):**
```
tr(<NUMS_xpub>/<0;1>/*, sortedmulti_a(2,
     [FP1/48h/1h/0h/2h]xpubA/<0;1>/*,
     [FP2/48h/1h/0h/2h]xpubB/<0;1>/*,
     [FP3/48h/1h/0h/2h]xpubC/<0;1>/*))
```
(Deprecated-but-historically-accepted static-literal variant, **not recommended**:
`tr(50929b74…803ac0, sortedmulti_a(2, …))`.)

**BitBox02 (BIP-388 via BitBoxApp / Sparrow) — same wallet-policy template as Ledger:**
```
tr(@0/**, sortedmulti_a(2, @1/**, @2/**, @3/**))     keys: @0=<NUMS_xpub>, @1..3 = [origin]xpub{A,B,C}
```

**Jade / Trezor Safe / Passport Prime:** no valid taproot-multisig register string today (see matrix).

---

## 4. Purchase list — devices that CAN sign our (properly-diversified) taproot federation

Cheapest → priciest, official-store USD, Aug 2026. All three below sign `tr(NUMS_xpub, {sorted}multi_a)` today with
the diversifications in §5 applied.

| # | Device | Price (USD) | Firmware gate | Notes |
|---|---|---|---|---|
| 1 | **Ledger Nano S Plus** | **$59** | Bitcoin app ≥ 2.1 | Cheapest path to a signing device; USB-C, no battery/screen frills. **Best value for our must-work "Ledger" target.** |
| 2 | **Coldcard Mk4** ("Mk5" listed $169.94) | **~$170** | **EDGE** firmware (Mk4) | Requires EDGE branch for tapscript; standard branch won't do `multi_a`. Air-gap/SD + USB. |
| 3 | **BitBox02 Nova, Bitcoin-only** | **~$181** (disc. from $201) | fw ≥ 9.21.0 | Clean BIP-388 policy UX; USB-C + BLE. Older BitBox02 (non-Nova) also fine if ≥ 9.21.0. |
| 4 | **Coldcard Q** | **~$249** (sale; $289 list) | **EDGE** firmware | Same signing capability as Mk4-EDGE; adds QWERTY + QR + battery. Pay for UX, not capability. |
| 5 | **Ledger Flex** | **$249** | Bitcoin app ≥ 2.1 | Same capability as Nano S+; E-Ink touchscreen + NFC. |
| 6 | **Ledger Stax** | **$399** | Bitcoin app ≥ 2.1 | Premium; capability identical to Nano S+. |

**Minimum viable 2-of-3 signing set (cheapest, all-signing):** Ledger Nano S+ ($59) + Coldcard Mk4-EDGE (~$170) +
BitBox02 Nova BTC-only (~$181) = **~$410** for three *distinct-vendor* signers (good for vendor-diversity of a
federation). Must-work targets **Ledger + Coldcard** are both covered by #1 and #2.

**Explicitly NOT on the list (cannot sign our taproot multisig today):** Blockstream Jade, Trezor Safe 3/5,
Passport Prime (stock fw). Buying these for *this* federation shape wastes money until firmware changes.

---

## 5. emvault-core / GroupVault diversifications required

Separated into **Rust changes** (code) vs **descriptor formatting** (string shape). This is the concrete gating work.

### 5A. Rust changes (emvault-core)

1. **Lift the `Tr + Ranged` gate.** `build_taproot` currently returns `TaprootRangedUnsupported` for `KeyMode::Ranged`
   (`descriptor.rs:218-220`). Hardware *requires* ranged taproot, so this branch must be implemented, not rejected.
2. **Ranged taproot key builder.** Add the taproot analogue of `build_descriptor_key`'s `Ranged` arm: emit
   `DescriptorPublicKey::XPub` with `Wildcard::Unhardened` (and the `/<0;1>/*` multipath via the existing
   `to_multipath_string` seam) instead of `SinglePub::XOnly`. miniscript 12.3.7 *can* parse a `tr(...,multi_a(...))`
   with ranged xpub keys — the blocker was our own guard, **not** miniscript, **provided we use fixed-order `multi_a`**
   (next point).
3. **Key ordering decision — fixed order, NOT per-index re-sort (recommended).**
   - Our `Fixed` path sorts *derived x-only bytes* once and bakes it in — fine when there is one key per signer.
   - With **ranged** xpubs the derived key changes per index, so true `sortedmulti_a` semantics would require
     **re-sorting cosigners at every derivation index** — which miniscript 12.3.7 cannot express (the exact reason
     the module doc gives for rejecting ranged taproot).
   - **Escape hatch:** BIP-388 + Ledger + BitBox02 all accept **plain `multi_a`** (unsorted). So emit `multi_a` with a
     **fixed canonical xpub order** (e.g. lexicographic over the serialized xpub) held constant across all indices.
     No per-index recompute, stays inside pinned miniscript, and the hardware registers the identical string. This is
     the cheapest correct route and it is what unblocks Ledger/BitBox02.
   - **Caveat that forces an open question:** Coldcard's docs only show **`sortedmulti_a`**. If Coldcard-EDGE
     *rejects* plain `multi_a`, then covering Coldcard needs true `sortedmulti_a` — which needs either (a) a miniscript
     bump to a version exposing `sortedmulti_a`, or (b) hand-rolled per-index sorted expansion. That is a materially
     bigger lift than the fixed-order route. **Resolve Q1 before picking.**
4. **Do NOT keep silently pre-sorting then calling it `multi_a`.** Today's `Fixed` path sorts and emits `multi_a` — for
   `Fixed` that is byte-identical to `sortedmulti_a` because there is exactly one derivation. For `Ranged` it is *not*
   equivalent; be explicit that ranged output is **fixed-order `multi_a`**, and document that watch-only descriptor and
   the device register-template must use the **same** canonical order or addresses/change detection diverge.
5. **PSBT taproot fields (coordinator side).** Independently of descriptor shape, external taproot signing needs the
   coordinator to (a) populate `PSBT_IN/OUT_TAP_*` fields (internal key, BIP32-derivation w/ leaf hashes, leaf script,
   merkle root) that Coldcard/Ledger require, and (b) ingest `tap_script_sigs` on return — `receive_signature` today
   only reads `partial_sigs` (ECDSA) and drops taproot sigs (`emvault-core/src/psbt.rs`, per `passport-prime-integration.md` §4.1). This is a hard prerequisite for *any* taproot hardware signer, not just Passport.

### 5B. Descriptor formatting (string shape, no signing-logic change)

1. **NUMS-as-xpub encoding.** Add a formatter that serializes BIP-341 *H* as an xpub (`<NUMS_xpub>`) for the
   wallet-policy path, in place of the raw 64-hex literal. Decide the **chaincode policy**: random-per-federation
   (privacy: hides that key-path is dead) vs a documented constant (reproducibility/backup simplicity). Recommend
   random-per-federation, stored with the federation record. The raw literal stays valid for Core/BDK watch-only, but
   is **not** what we hand hardware.
2. **Per-device NUMS encoding is NOT uniform — abstract it.** Wallet-policy devices (Ledger/BitBox02) want
   `@0 = <NUMS_xpub>` in the key vector; Coldcard-EDGE wants the same xpub inline (`tr(<NUMS_xpub>/<0;1>/*, …)`) and
   additionally offers `r=@`/`r=<hex>` `H+rG` forms. A small per-device "internal-key rendering" strategy keeps the
   core descriptor clean while emitting each device's expected register string.
3. **Wallet-policy template emitter.** For Ledger/BitBox02 we must emit the **template + key-vector** pair
   (`tr(@0/**, {sorted}multi_a(m, @1/**, …))` + ordered xpub list with origins), not just a flat descriptor string.
   That is a formatting/serialization addition on top of the existing `Descriptor` output — GroupVault registration
   UX will need it.
4. **Origin metadata discipline.** Ledger signs only for xpubs carrying **their own** key-origin `[FP/path]`; the NUMS
   xpub must be emitted **without** origin (so it's treated as external/unspendable). Our formatter must include origins
   for real cosigners and omit it for NUMS.

**Split summary:** the *unblock* is mostly **Rust** (lift the gate + ranged taproot key builder + PSBT taproot
plumbing); the *device fan-out* is mostly **formatting** (NUMS-as-xpub, per-device internal-key rendering, wallet-policy
template/key-vector emitter). Ordering choice (§5A.3) is the one place where a device quirk (Coldcard `multi_a` vs
`sortedmulti_a`) could push a formatting decision back into a **Rust/dependency** decision.

---

## 6. Open questions

- **Q1 (blocks §5A.3 route choice):** Does **Coldcard-EDGE** accept plain unsorted **`multi_a`**, or strictly
  `sortedmulti_a`? Docs show only `sortedmulti_a`. If strict, covering Coldcard needs real `sortedmulti_a` (miniscript
  bump or hand-rolled per-index sort) — a bigger lift than fixed-order `multi_a`. *Needs a device/regtest test to settle.*
- **Q2:** Does **Ledger** register a `tr()` policy whose **internal key is a purely external unspendable xpub** (no
  device-owned key at the internal position, device key only in a leaf)? Standard pattern (Liana-style) says yes, but
  Ledger's "key origin compulsory for device-controlled xpubs" wording should be confirmed on-device before we design
  around it.
- **Q3:** BitBox02 raw-`multi_a` acceptance (same as Q1 for BitBox) — BIP-388 example is `sortedmulti_a`; confirm
  whether unsorted `multi_a` also registers.
- **Q4:** Exact **key/leaf caps** for Ledger and BitBox02 taproot policies (Coldcard is documented: 8 leaves / 32 keys).
  Matters only if federations get large.
- **Q5 (chaincode policy):** random-per-federation vs constant NUMS xpub — decide and record where it's stored, since
  it becomes part of the unrecoverable-from-seed wallet backup (BIP-388 §Implementation guidelines warning).

---

## 7. Constraints honored

Primary sources only (cited). **No code changed.** Staged diffs and gv-regtest untouched — this is a planning doc under
`test-app-xpub/plans/`. Partial findings gathered before the reboot (our exact shape, BIP-386 legality, Passport row)
folded in rather than re-derived.
