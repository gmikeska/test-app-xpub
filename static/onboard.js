/*
 * Trezor onboarding ceremony.
 *
 * Initialises @trezor/connect, captures an XPUB at the federation
 * derivation path, assembles a BIP-380 descriptor key
 *   "[<master_fingerprint>/<path>]<xpub>"
 * and POSTs it to /onboard/signer. On success the server returns the URL
 * to redirect the user to.
 *
 * Master-fingerprint sourcing (in order of preference):
 *
 *   1. `payload.rootFingerprint` — camelCase field on the JS payload that
 *      Trezor Connect maps from the protobuf `PublicKey.root_fingerprint`.
 *      Confirmed by reading `packages/connect/src/device/DeviceCommands.ts`
 *      and `packages/connect-common/src/types/api/getPublicKey.ts` in the
 *      trezor-suite repo. The connect.trezor.io docs schema omits this
 *      field but the runtime payload includes it for any firmware that
 *      ships the protobuf field (Trezor Safe 3 / Safe 5 / Model T family,
 *      and any Model One on firmware >= 1.10).
 *
 *   2. `payload.descriptor` — BIP-380 descriptor already formatted by
 *      Trezor firmware as e.g.
 *      `wpkh([d34db33f/48h/1h/0h/2h]tpub.../<0;1>/*)#abcd1234`. Parsed for
 *      master fingerprint. Used as a defensive fallback if (1) is absent.
 *
 * Jade onboarding (added): drives a Blockstream Jade over Web Serial via the
 * vendored `@emvault/jade` driver — unlock → getXpub → assemble the same
 * BIP-380 descriptor key — and POSTs it with `device_type:"Jade"`.
 */
import { JadeRpc } from "/static/vendor/emvault-jade/index.js";

(function () {
  "use strict";

  const cfg = window.EMVAULT;
  const TrezorConnect = window.TrezorConnect;

  const captureBtn = document.getElementById("capture-btn");
  const saveBtn = document.getElementById("save-btn");
  const labelInput = document.getElementById("label");
  const statusEl = document.getElementById("status");
  const resultEl = document.getElementById("result");
  const rFp = document.getElementById("r-fp");
  const rPath = document.getElementById("r-path");
  const rXpub = document.getElementById("r-xpub");
  const rDk = document.getElementById("r-dk");

  let pending = null;
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
        email: cfg.manifestEmail,
        appUrl: cfg.manifestAppUrl,
      },
    });
    initialized = true;
  }

  function fingerprintHex(num) {
    const hex = (num >>> 0).toString(16);
    return hex.padStart(8, "0");
  }

  /**
   * Strip leading "m/" if present and normalise hardened markers so we can
   * produce a descriptor-key origin in the canonical
   * "[fp/48'/1'/0'/2']xpub..." form.
   */
  function originBody(path) {
    return path.replace(/^m\//, "").replaceAll("h", "'").replaceAll("H", "'");
  }

  /**
   * Peel descriptor-fragment wrappers (`pkh(...)`, `wpkh(...)`, `sh(...)`,
   * `wsh(...)`, `tr(...)`) until we hit the bare `[origin]xpub.../<...>/*`
   * descriptor-key form. Returns the inner key string, or null if we can't
   * make sense of it.
   */
  function unwrapDescriptor(d) {
    if (typeof d !== "string" || d.length === 0) return null;
    let s = d.split("#")[0].trim();
    const wrappers = ["pkh(", "wpkh(", "sh(", "wsh(", "tr("];
    let safety = 4;
    while (safety-- > 0) {
      let matched = false;
      for (const w of wrappers) {
        if (s.startsWith(w) && s.endsWith(")")) {
          s = s.slice(w.length, -1).trim();
          matched = true;
          break;
        }
      }
      if (!matched) break;
    }
    return s;
  }

  /**
   * Extract `{ fingerprintHex, originPath, xpub }` from Trezor's BIP-380
   * descriptor. Returns null if parsing fails.
   */
  function parseTrezorDescriptor(d) {
    const inner = unwrapDescriptor(d);
    if (!inner) return null;
    const m = inner.match(/^\[([^\]]+)\]([1-9A-HJ-NP-Za-km-z]+)(\/.*)?$/);
    if (!m) return null;
    const origin = m[1];
    const xpub = m[2];
    const slash = origin.indexOf("/");
    if (slash < 1) return null;
    const fpHex = origin.slice(0, slash).toLowerCase();
    if (!/^[0-9a-f]{8}$/.test(fpHex)) return null;
    const path = origin.slice(slash + 1).replaceAll("h", "'").replaceAll("H", "'");
    return { fingerprintHex: fpHex, originPath: path, xpub };
  }

  async function captureTrezor() {
    setStatus("Connecting to your Trezor…");
    captureBtn.disabled = true;
    try {
      await ensureInit();

      // Use `SPENDADDRESS` (not `SPENDWITNESS` or `SPENDMULTISIG`) for two
      // reasons:
      //
      //   * The firmware serialises with the standard `xpub_magic` (tpub
      //     on testnet, xpub on mainnet), which `bitcoin::bip32::Xpub`
      //     parses directly — no SLIP-132 Vpub/Zpub conversion needed.
      //   * Trezor Connect's MITM-protection step (a child-derivation
      //     roundtrip in `DeviceCommands.getHDNode`) compares the returned
      //     xpub's version bytes against `getBech32Network(coinInfo)` for
      //     SPENDWITNESS or `getSegwitNetwork(coinInfo)` for
      //     SPENDP2SHWITNESS. With `ignoreXpubMagic: true` those mismatch
      //     and Connect rejects the response with "Invalid network
      //     version". SPENDADDRESS expects the coin's default network
      //     (tpub on testnet), which is what we get.
      //
      // The script_type does not change the key material — it only picks
      // the xpub version bytes. We're not signing, just exporting the
      // federation xpub.
      //
      // SPENDADDRESS also makes the firmware populate `payload.descriptor`
      // (as `pkh([fp/path]tpub.../<0;1>/*)#cksum`), which we use as a
      // defensive fallback if `rootFingerprint` is missing.
      const result = await TrezorConnect.getPublicKey({
        path: cfg.derivationPath,
        coin: cfg.trezorCoin,
        scriptType: "SPENDADDRESS",
        showOnTrezor: false,
      });

      if (!result || !result.success) {
        const err = (result && result.payload && result.payload.error) || "Unknown Trezor error";
        throw new Error(err);
      }

      const p = result.payload;
      const xpub = p.xpub;
      if (typeof xpub !== "string" || xpub.length === 0) {
        throw new Error("Trezor did not return an xpub for the federation path.");
      }

      let fp;
      let derivationPath;
      let descriptorKey;
      let source;

      if (typeof p.rootFingerprint === "number") {
        fp = fingerprintHex(p.rootFingerprint);
        const origin = originBody(cfg.derivationPath);
        derivationPath = `m/${origin}`;
        descriptorKey = `[${fp}/${origin}]${xpub}`;
        source = "rootFingerprint";
      } else {
        const fromDescriptor = parseTrezorDescriptor(p.descriptor);
        if (fromDescriptor && fromDescriptor.xpub === xpub) {
          fp = fromDescriptor.fingerprintHex;
          derivationPath = `m/${fromDescriptor.originPath}`;
          descriptorKey = `[${fp}/${fromDescriptor.originPath}]${xpub}`;
          source = "descriptor";
        } else {
          throw new Error(
            "Trezor did not return either rootFingerprint or a parseable BIP-380 " +
              "descriptor. Please update your Trezor firmware via Trezor Suite and " +
              "retry.",
          );
        }
      }

      pending = {
        descriptor_key: descriptorKey,
        fingerprint: fp,
        derivation_path: derivationPath,
        xpub,
        device_type: "Trezor",
      };

      rFp.textContent = fp;
      rPath.textContent = derivationPath;
      rXpub.textContent = xpub;
      rDk.textContent = descriptorKey;
      resultEl.hidden = false;
      setStatus(
        `Captured (master fingerprint via ${source}). Review and click \u201CSave and continue\u201D to finish onboarding.`,
        "ok",
      );
    } catch (e) {
      console.error(e);
      setStatus(`Capture failed: ${e.message || e}`, "error");
    } finally {
      captureBtn.disabled = false;
    }
  }

  /**
   * Jade onboarding over Web Serial (USB). Mirrors the Trezor capture but uses
   * the vendored `@emvault/jade` driver; produces the same `pending` shape with
   * `device_type:"Jade"`.
   */
  async function captureJade() {
    setStatus("Requesting Jade serial port\u2026");
    captureBtn.disabled = true;
    let jade;
    try {
      jade = await JadeRpc.fromSerial();
      setStatus("Unlock the Jade (confirm PIN on the device)\u2026");
      await jade.unlock(cfg.jadeNetwork);
      const fp = await jade.getMasterFingerprintHex(cfg.jadeNetwork);
      const xpub = await jade.getXpub(cfg.jadeNetwork, cfg.derivationPath);
      if (typeof xpub !== "string" || xpub.length === 0) {
        throw new Error("Jade did not return an xpub for the federation path.");
      }
      const origin = originBody(cfg.derivationPath);
      const derivationPath = `m/${origin}`;
      const descriptorKey = `[${fp}/${origin}]${xpub}`;

      pending = {
        descriptor_key: descriptorKey,
        fingerprint: fp,
        derivation_path: derivationPath,
        xpub,
        device_type: "Jade",
      };

      rFp.textContent = fp;
      rPath.textContent = derivationPath;
      rXpub.textContent = xpub;
      rDk.textContent = descriptorKey;
      resultEl.hidden = false;
      setStatus(
        "Captured from Jade. Review and click \u201CSave and continue\u201D to finish onboarding.",
        "ok",
      );
    } catch (e) {
      console.error(e);
      setStatus(`Capture failed: ${e.message || e}`, "error");
    } finally {
      try {
        if (jade) await jade.close();
      } catch (_e) { /* ignore */ }
      captureBtn.disabled = false;
    }
  }

  function selectedDevice() {
    const r = document.querySelector('input[name="device"]:checked');
    return r ? r.value : "Trezor";
  }

  /** Dispatch the capture to the device the user selected. */
  async function capture() {
    pending = null;
    resultEl.hidden = true;
    if (selectedDevice() === "Jade") {
      await captureJade();
    } else {
      await captureTrezor();
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
        label: (labelInput.value || "").trim() || null,
        device_type: pending.device_type || "Trezor",
      };
      const resp = await fetch("/onboard/signer", {
        method: "POST",
        headers: { "content-type": "application/json" },
        credentials: "same-origin",
        body: JSON.stringify(body),
      });
      const json = await resp.json().catch(() => null);
      if (!resp.ok) {
        const msg = (json && json.message) || `HTTP ${resp.status}`;
        throw new Error(msg);
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
})();
