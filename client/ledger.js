/*
 * Ledger driver — combined onboarding + signing for a single bundle.
 *
 * One file, both operations (as with the Jade driver): the Ledger Bitcoin app
 * exposes onboarding (`getExtendedPubkey`) and signing (`registerWallet` +
 * `signPsbt`) through the *same* `AppClient` over the *same* WebUSB transport,
 * so there's no reason to split them. The file is page-dispatched — it wires the
 * onboard flow when the onboard pane is present, and (increment 3) the signing
 * flow when the proposal sign button is present.
 *
 * Bundled by Vite (imports npm packages) to `static/dist/ledger.js`, loaded by
 * both `onboard.html` and `proposal.html`.
 *
 * SegWit first: onboarding captures the XPUB at the federation path
 * (`m/48'/1'/0'/2'`, coin-type 1' for signet/testnet/regtest) and assembles the
 * BIP-380 descriptor key `[<fpr>/<path>]<xpub>` exactly like the Trezor/Jade
 * flows, then POSTs it with `device_type:"Ledger"`. Requires the **Bitcoin Test**
 * app open on the device for testnet-family paths.
 */

import { AppClient, WalletPolicy } from "ledger-bitcoin";
import TransportWebUSB from "@ledgerhq/hw-transport-webusb";
import { Psbt } from "bitcoinjs-lib";

const cfg = window.EMVAULT || {};

/** Convert "m/48'/1'/0'/2'" → "48'/1'/0'/2'" (no leading m; hardened as `'`). */
function originBody(path) {
  return (path || "")
    .replace(/^m\//, "")
    .replaceAll("h", "'")
    .replaceAll("H", "'");
}

/** Open a WebUSB transport + Ledger app client. Caller must `transport.close()`. */
async function openApp() {
  const transport = await TransportWebUSB.create();
  return { transport, app: new AppClient(transport) };
}

// ---------------------------------------------------------------------------
// Onboarding (XPUB capture)
// ---------------------------------------------------------------------------

function wireOnboard() {
  const captureBtn = document.getElementById("capture-btn-ledger");
  const saveBtn = document.getElementById("save-btn-ledger");
  const labelInput = document.getElementById("label-ledger");
  const statusEl = document.getElementById("status-ledger");
  const resultEl = document.getElementById("result-ledger");
  const rFp = document.getElementById("r-fp-ledger");
  const rPath = document.getElementById("r-path-ledger");
  const rXpub = document.getElementById("r-xpub-ledger");
  const rDk = document.getElementById("r-dk-ledger");

  // Not on the onboard page (or the Ledger pane isn't present) — nothing to do.
  if (!captureBtn) return;

  let pending = null;

  function setStatus(msg, kind) {
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
  }

  async function capture() {
    captureBtn.disabled = true;
    setStatus("Requesting Ledger via WebUSB — approve the connection prompt…");
    let transport;
    let app;
    try {
      ({ transport, app } = await openApp());
    } catch (e) {
      captureBtn.disabled = false;
      setStatus(`Could not open the Ledger: ${e.message || e}`, "error");
      return;
    }

    try {
      setStatus("Reading master fingerprint (open the Bitcoin Test app)…");
      const fpr = await app.getMasterFingerprint();

      const origin = originBody(cfg.derivationPath);
      setStatus("Reading XPUB at the federation path…");
      const xpub = await app.getExtendedPubkey(`m/${origin}`);

      const derivationPath = `m/${origin}`;
      const descriptorKey = `[${fpr}/${origin}]${xpub}`;
      pending = { descriptor_key: descriptorKey };

      rFp.textContent = fpr;
      rPath.textContent = derivationPath;
      rXpub.textContent = xpub;
      rDk.textContent = descriptorKey;
      resultEl.hidden = false;
      setStatus(
        "Captured. Review and click “Save and continue” to finish onboarding.",
        "ok",
      );
    } catch (e) {
      console.error(e);
      setStatus(`Capture failed: ${e.message || e}`, "error");
    } finally {
      try {
        await transport.close();
      } catch (e) {
        console.warn("[ledger] close after capture:", e);
      }
      captureBtn.disabled = false;
    }
  }

  async function save() {
    if (!pending) {
      setStatus("Nothing to save. Capture the XPUB first.", "error");
      return;
    }
    saveBtn.disabled = true;
    setStatus("Saving…");
    try {
      const body = {
        descriptor_key: pending.descriptor_key,
        device_type: "Ledger",
        label: (labelInput.value || "").trim() || null,
      };
      const resp = await fetch("/onboard/signer", {
        method: "POST",
        headers: { "content-type": "application/json" },
        credentials: "same-origin",
        body: JSON.stringify(body),
      });
      const json = await resp.json().catch(() => null);
      if (!resp.ok) {
        throw new Error((json && json.message) || `HTTP ${resp.status}`);
      }
      setStatus("Saved. Redirecting…", "ok");
      window.location.href = (json && json.redirect) || "/home";
    } catch (e) {
      console.error(e);
      setStatus(`Save failed: ${e.message || e}`, "error");
      saveBtn.disabled = false;
    }
  }

  captureBtn.addEventListener("click", capture);
  saveBtn.addEventListener("click", save);
}

// ---------------------------------------------------------------------------
// Signing (register wallet policy + signPsbt → partial PSBT)
// ---------------------------------------------------------------------------
//
// The Ledger Bitcoin app can only sign a *registered* wallet policy, so for a
// `wsh(sortedmulti(...))` federation we register the BIP-388 policy the server
// provides (via `build_ledger_policy`), then sign. We register-then-sign in one
// flow, which means the policy is approved on-device each time (HMAC caching to
// skip re-approval is a follow-up). `signPsbt` returns per-input partial
// signatures; we merge them into the base PSBT with bitcoinjs and POST the
// result to the generic, device-agnostic `/partial-psbt` endpoint — the same
// path Jade uses — so mixed Trezor+Ledger federations merge cleanly server-side.

function wireSign() {
  const btn = document.getElementById("sign-btn");
  const statusEl = document.getElementById("sign-status");
  // Only the proposal page, and only when the viewer signs with a Ledger.
  if (!btn || (cfg.viewerDeviceType || "").toLowerCase() !== "ledger") return;

  function setStatus(msg, kind) {
    if (!statusEl) return;
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
  }

  btn.addEventListener("click", async () => {
    const federationId = btn.dataset.federationId || cfg.federationId;
    const proposalId = btn.dataset.proposalId || cfg.proposalId;
    if (!federationId || !proposalId) {
      setStatus("Missing federation/proposal id on Sign button", "error");
      return;
    }

    btn.disabled = true;
    setStatus("Loading sign data…");
    let sd;
    try {
      const resp = await fetch(
        `/federations/${federationId}/proposals/${proposalId}/sign-data`,
        { credentials: "same-origin" },
      );
      if (!resp.ok) {
        throw new Error(`sign-data HTTP ${resp.status}: ${(await resp.text()).slice(0, 200)}`);
      }
      sd = await resp.json();
    } catch (e) {
      setStatus(`Sign-data fetch failed: ${e.message || e}`, "error");
      btn.disabled = false;
      return;
    }
    if (!sd.psbt_b64 || !sd.ledger) {
      setStatus("sign-data response missing psbt_b64 / ledger policy", "error");
      btn.disabled = false;
      return;
    }

    setStatus("Requesting Ledger via WebUSB — approve the connection prompt…");
    let transport;
    let app;
    try {
      ({ transport, app } = await openApp());
    } catch (e) {
      setStatus(`Could not open the Ledger: ${e.message || e}`, "error");
      btn.disabled = false;
      return;
    }

    try {
      const policy = new WalletPolicy(
        sd.ledger.name,
        sd.ledger.descriptor_template,
        sd.ledger.keys,
      );

      setStatus("Registering the wallet policy — review and approve it on the Ledger…");
      const [, hmac] = await app.registerWallet(policy);

      setStatus("Signing — confirm the amount and address on the Ledger…");
      const entries = await app.signPsbt(sd.psbt_b64, policy, hmac);
      if (!entries.length) {
        throw new Error("Ledger returned no signatures — is this device a cosigner on this federation?");
      }

      // Merge the per-input partial signatures into the base PSBT. Taproot
      // script-path sigs (multi_a) carry a `tapleafHash` and go into
      // `tapScriptSig` (Schnorr, x-only pubkey); SegWit sigs (ECDSA) go into
      // `partialSig`. `signPsbt` tells us which via the presence of tapleafHash.
      const psbt = Psbt.fromBase64(sd.psbt_b64);
      for (const [index, ps] of entries) {
        if (ps.tapleafHash) {
          // Ledger returns an x-only (32-byte) pubkey for taproot leaves; guard
          // against a compressed (33-byte) form just in case.
          const xonly = ps.pubkey.length === 33 ? ps.pubkey.subarray(1) : ps.pubkey;
          psbt.updateInput(index, {
            tapScriptSig: [
              { leafHash: ps.tapleafHash, pubkey: xonly, signature: ps.signature },
            ],
          });
        } else {
          psbt.updateInput(index, {
            partialSig: [{ pubkey: ps.pubkey, signature: ps.signature }],
          });
        }
      }
      const partialB64 = psbt.toBase64();

      setStatus("Submitting signed PSBT…");
      const submitResp = await fetch(
        `/federations/${federationId}/proposals/${proposalId}/partial-psbt`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          credentials: "same-origin",
          body: JSON.stringify({ partial_psbt_b64: partialB64 }),
        },
      );
      const submitJson = await submitResp.json().catch(() => null);
      if (!submitResp.ok) {
        throw new Error((submitJson && submitJson.message) || `HTTP ${submitResp.status}`);
      }
      setStatus(
        submitJson && submitJson.fully_signed
          ? "Signed — proposal finalized. Reloading…"
          : `Signed (status: ${(submitJson && submitJson.status) || "?"}). Reloading…`,
        "ok",
      );
      window.setTimeout(() => window.location.reload(), 600);
    } catch (e) {
      console.error(e);
      setStatus(`Signing failed: ${e.message || e}`, "error");
      btn.disabled = false;
    } finally {
      try {
        await transport.close();
      } catch (e) {
        console.warn("[ledger] close after sign:", e);
      }
    }
  });
}

wireOnboard();
wireSign();
