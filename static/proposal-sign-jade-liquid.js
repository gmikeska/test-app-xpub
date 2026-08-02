// Jade Liquid (Elements) signing flow for proposal pages.
//
// Mirrors `proposal-sign-jade.js` but speaks the PSET / `sign_pset` /
// `register_multisig` (Liquid variant) RPC pair instead of the Bitcoin
// PSBT flow.
//
// Flow:
//   1. Fetch `/sign-data` for the proposal (`psbt_b64` carries the PSET
//      base64 for Liquid federations, `descriptor` is the CT descriptor,
//      `network` is `liquid` / `liquidtestnet` / `elementsregtest`).
//   2. Parse the CT descriptor:
//        ct(slip77(<32-byte hex>), elwsh(sortedmulti(M, key1, key2, ...)))
//      Pull out the SLIP-77 master blinding key (32 bytes) and the inner
//      `elwsh(sortedmulti(...))` we treat as if it were a `wsh` for Jade
//      (Jade derives variant from the network).
//   3. Open WebSerial → JadeRpc → unlock against the pinserver.
//   4. `register_multisig` with `master_blinding_key` set → idempotent on
//      Jade (only re-prompts on content change).
//   5. `sign_pset` → returns a complete signed PSET.
//   6. POST the resulting PSET (base64) to /partial-psbt — the server
//      branches on FederationKind and runs `Pset::combine`.
//   7. Reload.

import {
    JadeRpc,
    pathToU32Array,
    bytesToHex,
    hexToBytes,
    base64ToBytes,
    bytesToBase64,
} from "./vendor/jade-rpc.js?v=j8d1";

const cfg = window.EMVAULT || {};

const btn = document.getElementById("sign-btn");
const statusEl = document.getElementById("sign-status");

if (btn && statusEl) {
    btn.addEventListener("click", () => {
        signProposal().catch((e) => {
            console.error("[jade-liquid] sign failed:", e);
        });
    });
}

function setStatus(msg, kind) {
    if (!statusEl) return;
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
}

/// Map server-side network strings to Jade's Liquid network names.
function jadeLiquidNetworkName(network) {
    switch ((network || "").toLowerCase()) {
        case "liquid":
        case "liquidv1":
            return "liquid";
        case "liquidtestnet":
        case "liquid-testnet":
            return "testnet-liquid";
        case "elementsregtest":
        case "localtest-liquid":
        case "liquid-regtest":
            return "localtest-liquid";
        default:
            return network;
    }
}

/// Parse a `ct(slip77(<hex>), elwsh(sortedmulti(M, key1, ...)))` descriptor
/// into `{ masterBlindingKey: Uint8Array(32), threshold, signers: [...] }`.
///
/// Strips the trailing `#checksum`.
export function parseCtDescriptor(descriptor) {
    if (typeof descriptor !== "string") {
        throw new TypeError("descriptor must be a string");
    }
    let s = descriptor.split("#")[0].trim();
    if (!s.startsWith("ct(") || !s.endsWith(")")) {
        throw new Error(`expected ct(...) wrapper, got ${truncatePreview(s)}`);
    }
    s = s.slice(3, -1).trim();

    const parts = splitTopLevelCommas(s);
    if (parts.length !== 2) {
        throw new Error(`ct(...) must have exactly two args, got ${parts.length}`);
    }

    const blindingArg = parts[0].trim();
    const masterBlindingKey = parseSlip77Key(blindingArg);

    const innerArg = parts[1].trim();
    if (!innerArg.startsWith("elwsh(") || !innerArg.endsWith(")")) {
        throw new Error(
            `expected elwsh(sortedmulti(...)), got ${truncatePreview(innerArg)}`,
        );
    }
    const elwshInner = innerArg.slice("elwsh(".length, -1).trim();

    let smInner;
    if (elwshInner.startsWith("sortedmulti(") && elwshInner.endsWith(")")) {
        smInner = elwshInner.slice("sortedmulti(".length, -1);
    } else {
        throw new Error(
            `expected sortedmulti(...) inside elwsh, got ${truncatePreview(elwshInner)}`,
        );
    }

    const smParts = splitTopLevelCommas(smInner);
    if (smParts.length < 2) {
        throw new Error("sortedmulti must have a threshold + at least one key");
    }
    const threshold = Number.parseInt(smParts[0].trim(), 10);
    if (!Number.isInteger(threshold) || threshold < 1) {
        throw new Error(`bad threshold: ${smParts[0]}`);
    }
    const signers = smParts.slice(1).map((part) => parseDescriptorKey(part.trim()));
    return { masterBlindingKey, threshold, signers };
}

/// Parse `slip77(<64 hex chars>)` into `Uint8Array(32)`.
function parseSlip77Key(arg) {
    const trimmed = arg.trim();
    if (!trimmed.startsWith("slip77(") || !trimmed.endsWith(")")) {
        throw new Error(`expected slip77(<hex>), got ${truncatePreview(trimmed)}`);
    }
    const inner = trimmed.slice("slip77(".length, -1).trim();
    if (!/^[0-9a-fA-F]{64}$/.test(inner)) {
        throw new Error(
            `expected slip77 inner to be 64 hex chars, got ${inner.length} chars`,
        );
    }
    return hexToBytes(inner.toLowerCase());
}

/// Parse a single descriptor key `[fp/path]xpub<suffix>` into
/// `{ fingerprintHex, derivation: u32[], xpub }`.
function parseDescriptorKey(key) {
    if (!key.startsWith("[")) {
        throw new Error(`descriptor key missing origin: ${truncatePreview(key)}`);
    }
    const close = key.indexOf("]");
    if (close < 0) {
        throw new Error(`descriptor key missing origin terminator: ${truncatePreview(key)}`);
    }
    const origin = key.slice(1, close);
    const slash = origin.indexOf("/");
    if (slash < 1) {
        throw new Error(`descriptor origin missing path: ${origin}`);
    }
    const fingerprintHex = origin.slice(0, slash).toLowerCase();
    if (!/^[0-9a-f]{8}$/.test(fingerprintHex)) {
        throw new Error(`bad fingerprint: ${fingerprintHex}`);
    }
    const pathStr = origin.slice(slash + 1).replaceAll("h", "'").replaceAll("H", "'");
    const derivation = pathToU32Array(`m/${pathStr}`);

    const after = key.slice(close + 1);
    const firstSlash = after.indexOf("/");
    const xpub = firstSlash < 0 ? after : after.slice(0, firstSlash);
    if (xpub.length === 0) {
        throw new Error("descriptor key has no xpub after origin");
    }
    return { fingerprintHex, derivation, xpub };
}

function splitTopLevelCommas(s) {
    const out = [];
    let depth = 0;
    let buf = "";
    for (let i = 0; i < s.length; i += 1) {
        const ch = s[i];
        if (ch === "(") depth += 1;
        else if (ch === ")") depth -= 1;
        if (ch === "," && depth === 0) {
            out.push(buf);
            buf = "";
        } else {
            buf += ch;
        }
    }
    if (buf.length > 0) out.push(buf);
    return out;
}

function truncatePreview(s) {
    return s.length > 60 ? `${s.slice(0, 57)}...` : s;
}

/// Derive a 1..15 char Jade-friendly multisig name from a federation UUID.
/// Uses `astL` prefix (vs `ast` for Bitcoin) so a Jade simultaneously
/// holding a Bitcoin and a Liquid registration for the same federation id
/// can keep them under distinct slot names.
function jadeLiquidMultisigName(federationId) {
    const hex = String(federationId).replaceAll("-", "").slice(0, 8);
    return `astL${hex}`;
}

/// Build the Jade `register_multisig` `descriptor` object for our
/// `elwsh(sortedmulti(...))` Liquid federations.
function buildJadeLiquidDescriptor(parsed) {
    return {
        variant: "wsh(multi(k))",
        sorted: true,
        threshold: parsed.threshold,
        master_blinding_key: parsed.masterBlindingKey,
        signers: parsed.signers.map((s) => ({
            fingerprint: hexToBytes(s.fingerprintHex),
            derivation: s.derivation,
            xpub: s.xpub,
            // BIP-380 multipath suffix (`/<0;1>/*`) is applied on-device
            // after keychain selection; nothing extra needed here.
            path: [],
        })),
    };
}

async function signProposal() {
    const federationId = btn.dataset.federationId || cfg.federationId;
    const proposalId = btn.dataset.proposalId || cfg.proposalId;
    if (!federationId || !proposalId) {
        setStatus("Missing federation/proposal id on Sign button", "error");
        return;
    }

    btn.disabled = true;
    setStatus("Loading sign data…");

    let signData;
    try {
        const resp = await fetch(
            `/federations/${federationId}/proposals/${proposalId}/sign-data`,
            { credentials: "same-origin" },
        );
        if (!resp.ok) {
            const body = await resp.text();
            throw new Error(`sign-data HTTP ${resp.status}: ${body.slice(0, 200)}`);
        }
        signData = await resp.json();
    } catch (e) {
        setStatus(`Sign-data fetch failed: ${e.message || e}`, "error");
        btn.disabled = false;
        return;
    }

    const { psbt_b64: psetB64, descriptor, network: rawNetwork } = signData;
    if (!psetB64 || !descriptor) {
        setStatus("sign-data response missing pset / descriptor", "error");
        btn.disabled = false;
        return;
    }
    const network = jadeLiquidNetworkName(rawNetwork);

    let parsed;
    try {
        parsed = parseCtDescriptor(descriptor);
    } catch (e) {
        setStatus(`Descriptor parse failed: ${e.message || e}`, "error");
        btn.disabled = false;
        return;
    }

    setStatus("Requesting Jade serial port…");
    let jade;
    try {
        jade = await JadeRpc.fromSerial();
    } catch (e) {
        setStatus(`Could not open Jade: ${e.message || e}`, "error");
        btn.disabled = false;
        return;
    }

    try {
        setStatus("Unlocking Jade — confirm the PIN on the device…");
        await jade.unlock(network);

        setStatus("Registering Liquid multisig wallet on Jade — confirm on the device…");
        const name = jadeLiquidMultisigName(federationId);
        await jade.registerLiquidMultisig(network, name, buildJadeLiquidDescriptor(parsed));

        setStatus("Signing PSET — confirm outputs on the Jade screen…");
        const psetBytes = base64ToBytes(psetB64);
        const signedBytes = await jade.signPset(network, psetBytes);
        const signedB64 = bytesToBase64(signedBytes);

        console.debug(
            "[jade-liquid] signed PSET bytes=%d sha256=%s",
            signedBytes.length,
            await sha256ShortHex(signedBytes),
        );

        setStatus("Submitting signed PSET…");
        const submitResp = await fetch(
            `/federations/${federationId}/proposals/${proposalId}/partial-psbt`,
            {
                method: "POST",
                headers: { "content-type": "application/json" },
                credentials: "same-origin",
                body: JSON.stringify({ partial_psbt_b64: signedB64 }),
            },
        );
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
    } catch (e) {
        console.error(e);
        setStatus(`Signing failed: ${e.message || e}`, "error");
        btn.disabled = false;
    } finally {
        try {
            await jade.close();
        } catch (e) {
            console.warn("[jade-liquid] close after sign:", e);
        }
    }
}

async function sha256ShortHex(bytes) {
    if (typeof crypto === "undefined" || !crypto.subtle) return "n/a";
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    return bytesToHex(new Uint8Array(digest)).slice(0, 16);
}
