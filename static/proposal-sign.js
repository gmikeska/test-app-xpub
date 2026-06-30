/*
 * Trezor multisig signing flow for proposal pages.
 *
 *   1. Fetch the server's sign-data JSON for this proposal.
 *   2. Initialise @trezor/connect.
 *   3. Hand `signData.trezor` directly to `TrezorConnect.signTransaction`.
 *   4. Forward the resulting per-input signatures (DER hex, no sighash byte)
 *      to /signatures on the server. The server injects them into a partial
 *      PSBT, archives it, merges it into the canonical PSBT, and (if the
 *      threshold is now met) runs finalize_psbt + extract_tx.
 *   5. Reload the page so the new cosigner status + post-finalize buttons
 *      are visible.
 *
 * The server-side Trezor payload uses `signatures: ["", "", ""]` placeholders
 * inside each input's `multisig` field, so Trezor only ever signs at the slot
 * matching the connected device's public key. The browser doesn't need to
 * compute slot indices — Trezor itself returns one signature per input that
 * we forward verbatim.
 *
 * Jade signing (added): when the server's sign-data is `{ device:"jade", … }`,
 * we register the multisig on the device and call `signPsbt`, which returns a
 * fully signed PSBT we POST as `signed_psbt_b64`. The server merges it directly
 * (no per-input injection). Routing is driven by the signer's onboarded
 * `device_type`, so the user never re-picks a device here.
 */
import {
  JadeRpc,
  hexToBytes,
  pathToU32Array,
  base64ToBytes,
  bytesToBase64,
} from "/static/vendor/emvault-jade/index.js";

(function () {
  "use strict";

  const cfg = window.EMVAULT || {};
  const TrezorConnect = window.TrezorConnect;

  const btn = document.getElementById("sign-btn");
  const statusEl = document.getElementById("sign-status");

  // Bail silently when the Sign button isn't rendered (proposal in a
  // post-signing state, or viewer is missing a signer).
  if (!btn || !statusEl) return;

  let initialized = false;

  function setStatus(msg, kind) {
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
  }

  async function ensureInit() {
    if (initialized) return;
    if (!TrezorConnect) {
      throw new Error("@trezor/connect failed to load. Check your network connection.");
    }
    await TrezorConnect.init({
      lazyLoad: true,
      manifest: {
        email: cfg.manifestEmail || "dev@emvault.test",
        appUrl: cfg.manifestAppUrl || window.location.origin,
      },
    });
    initialized = true;
  }

  /**
   * Extract the per-input signature array (hex DER, no sighash byte) from
   * Trezor's signTransaction payload.
   *
   * Trezor Connect returns `signatures: string[]` directly on
   * `result.payload`, one entry per input, ordered by input index, each
   * being the signing device's contribution at its own pubkey slot.
   */
  function extractSignaturesHex(payload) {
    if (!payload) throw new Error("Trezor returned no payload");
    if (!Array.isArray(payload.signatures)) {
      throw new Error("Trezor payload missing `signatures` array");
    }
    return payload.signatures.map((s) => (typeof s === "string" ? s : ""));
  }

  /** POST the cosigner contribution, report status, and reload. Shared. */
  async function finishSubmit(submitUrl, body) {
    setStatus("Submitting signature…");
    const submitResp = await fetch(submitUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      credentials: "same-origin",
      body: JSON.stringify(body),
    });
    const submitJson = await submitResp.json().catch(() => null);
    if (!submitResp.ok) {
      const msg = (submitJson && submitJson.message) || `HTTP ${submitResp.status}`;
      throw new Error(msg);
    }
    const ok = submitJson || { status: "?", fully_signed: false };
    setStatus(
      ok.fully_signed
        ? "Signed — proposal finalized. Reloading…"
        : `Signed (status: ${ok.status}). Reloading…`,
      "ok",
    );
    window.setTimeout(() => window.location.reload(), 600);
  }

  async function signWithTrezor(signData, submitUrl) {
    const trezor = signData.trezor;
    if (!trezor || !Array.isArray(trezor.inputs) || !Array.isArray(trezor.outputs)) {
      throw new Error("sign-data response missing inputs/outputs");
    }
    setStatus("Waiting on Trezor approval…");
    await ensureInit();

    // `version` and `locktime` MUST be forwarded — Trezor Connect defaults to
    // `version: 1, locktime: 0` if either is omitted, while BDK builds tx with
    // `version: 2` and `locktime` set to the current tip (anti-fee-sniping). A
    // mismatch makes Trezor sign the wrong BIP-143 sighash and bitcoind rejects
    // the broadcast with NULLFAIL.
    const result = await TrezorConnect.signTransaction({
      coin: trezor.coin,
      inputs: trezor.inputs,
      outputs: trezor.outputs,
      refTxs: trezor.refTxs,
      version: trezor.version,
      locktime: trezor.locktime,
    });
    if (!result || !result.success) {
      const err = (result && result.payload && result.payload.error) || "Unknown Trezor error";
      throw new Error(err);
    }
    const signaturesHex = extractSignaturesHex(result.payload);
    await finishSubmit(submitUrl, { signatures_hex: signaturesHex });
  }

  async function signWithJade(signData, submitUrl) {
    const reg = signData.jade && signData.jade.register;
    const jadeNetwork = signData.jade && signData.jade.jade_network;
    if (!reg || !jadeNetwork || !signData.psbt_b64) {
      throw new Error("sign-data response missing Jade register / network / psbt");
    }

    // Convert the server's JSON-friendly register into Jade's native descriptor
    // object: fingerprint hex → bytes, derivation path string → hardened u32[].
    const descriptor = {
      variant: reg.variant,
      sorted: reg.sorted,
      threshold: reg.threshold,
      signers: reg.signers.map((s) => ({
        fingerprint: hexToBytes(s.fingerprint),
        derivation: pathToU32Array(s.derivation_path),
        xpub: s.xpub,
        path: [],
      })),
    };

    let jade;
    setStatus("Requesting Jade serial port…");
    try {
      jade = await JadeRpc.fromSerial();
      setStatus("Unlock the Jade (confirm PIN on the device)…");
      await jade.unlock(jadeNetwork);
      setStatus("Confirm the multisig registration on the Jade…");
      await jade.registerMultisig(jadeNetwork, reg.name, descriptor);
      setStatus("Confirm the transaction on the Jade…");
      const signedBytes = await jade.signPsbt(jadeNetwork, base64ToBytes(signData.psbt_b64));
      await finishSubmit(submitUrl, { signed_psbt_b64: bytesToBase64(signedBytes) });
    } finally {
      try {
        if (jade) await jade.close();
      } catch (_e) { /* ignore */ }
    }
  }

  async function signProposal() {
    btn.disabled = true;
    setStatus("Loading sign data…");
    try {
      const federationId = btn.dataset.federationId || cfg.federationId;
      const proposalId = btn.dataset.proposalId || cfg.proposalId;
      if (!federationId || !proposalId) {
        throw new Error("Missing federation/proposal id on Sign button");
      }
      const signDataUrl = `/federations/${federationId}/proposals/${proposalId}/sign-data`;
      const submitUrl = `/federations/${federationId}/proposals/${proposalId}/signatures`;

      const resp = await fetch(signDataUrl, { credentials: "same-origin" });
      if (!resp.ok) {
        const body = await resp.text();
        throw new Error(`sign-data HTTP ${resp.status}: ${body.slice(0, 200)}`);
      }
      const signData = await resp.json();

      // Auto-routed by the server from the signer's onboarded device_type.
      if ((signData.device || "trezor") === "jade") {
        await signWithJade(signData, submitUrl);
      } else {
        await signWithTrezor(signData, submitUrl);
      }
    } catch (e) {
      console.error(e);
      setStatus(`Signing failed: ${e.message || e}`, "error");
      btn.disabled = false;
    }
  }

  btn.addEventListener("click", signProposal);
})();
