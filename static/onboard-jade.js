// Blockstream Jade onboarding ceremony — counterpart to `onboard.js`.
//
// Connects to a Jade over WebSerial via our hand-rolled `JadeRpc` (lwk_wasm
// 0.17 doesn't expose Bitcoin signing, so we drive the CBOR-RPC protocol
// directly), unlocks the device against Blockstream's pinserver, fetches
// the xpub at the federation derivation path plus an `m/0` xpub for master
// fingerprint extraction, and assembles a BIP-380 descriptor key
//   `[<master_fingerprint>/<path>]<xpub>`
// to POST to `/onboard/signer` with `device_type: "Jade"`.

import { JadeRpc, pathToU32Array, base58CheckDecode, bytesToHex } from "./vendor/jade-rpc.js";

const cfg = window.EMVAULT;

const captureBtn = document.getElementById("capture-btn-jade");
const saveBtn = document.getElementById("save-btn-jade");
const labelInput = document.getElementById("label-jade");
const statusEl = document.getElementById("status-jade");
const resultEl = document.getElementById("result-jade");
const rFp = document.getElementById("r-fp-jade");
const rPath = document.getElementById("r-path-jade");
const rXpub = document.getElementById("r-xpub-jade");
const rDk = document.getElementById("r-dk-jade");

let pending = null;

function setStatus(msg, kind) {
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
}

/// Map our app's `BITCOIN_NETWORK` env value to the network identifier
/// Jade firmware expects on `auth_user`, `get_xpub`, etc.
function jadeNetworkName(network) {
    switch ((network || "").toLowerCase()) {
        case "bitcoin":
        case "mainnet":
            return "mainnet";
        case "testnet":
        case "testnet3":
        case "signet":
            return "testnet";
        case "regtest":
        case "localtest":
        case "localtest-bitcoin":
            return "localtest";
        default:
            return network;
    }
}

/// Convert "m/48'/1'/0'/2'" → "48'/1'/0'/2'" (no leading m, hardened with `'`).
function originBody(path) {
    return path.replace(/^m\//, "").replaceAll("h", "'").replaceAll("H", "'");
}

async function capture() {
    captureBtn.disabled = true;
    setStatus("Requesting Jade serial port…");
    let jade;
    try {
        jade = await JadeRpc.fromSerial();
    } catch (e) {
        captureBtn.disabled = false;
        setStatus(`Could not open Jade serial port: ${e.message || e}`, "error");
        return;
    }

    try {
        const network = jadeNetworkName(cfg.network);
        setStatus("Unlocking Jade — confirm the PIN on the device…");
        await jade.unlock(network);

        setStatus("Fetching master fingerprint…");
        const fp = await jade.getMasterFingerprintHex(network);

        setStatus("Fetching xpub at federation path…");
        const fedPathU32 = pathToU32Array(cfg.derivationPath);
        const xpub = await jade.getXpub(network, fedPathU32);

        // Sanity-check: the returned xpub's depth must equal our path length.
        const payload = base58CheckDecode(xpub);
        if (payload.length !== 78) {
            throw new Error(`Jade returned a malformed xpub (payload ${payload.length} bytes)`);
        }
        const depth = payload[4];
        if (depth !== fedPathU32.length) {
            throw new Error(
                `Jade returned an xpub at depth ${depth} but the federation path has ${fedPathU32.length} components`,
            );
        }

        const origin = originBody(cfg.derivationPath);
        const derivationPath = `m/${origin}`;
        const descriptorKey = `[${fp}/${origin}]${xpub}`;
        const fpDisplay = bytesToHex(payload.subarray(5, 9));
        if (fpDisplay.toLowerCase() !== fp.toLowerCase()) {
            // Defensive: if Jade's `m/0` xpub fingerprint doesn't match the
            // parent fingerprint of the federation-path xpub (which is
            // `m/48'/1'/0'`'s fingerprint, not the master's), we just keep
            // the master fingerprint we already computed. This branch is
            // for documentation; the values *should* differ.
        }

        pending = { descriptor_key: descriptorKey };

        rFp.textContent = fp;
        rPath.textContent = derivationPath;
        rXpub.textContent = xpub;
        rDk.textContent = descriptorKey;
        resultEl.hidden = false;
        setStatus(
            "Captured. Review and click \u201CSave and continue\u201D to finish onboarding.",
            "ok",
        );
    } catch (e) {
        console.error(e);
        setStatus(`Capture failed: ${e.message || e}`, "error");
    } finally {
        try {
            await jade.close();
        } catch (e) {
            console.warn("[jade] close after capture:", e);
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
            device_type: "Jade",
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
