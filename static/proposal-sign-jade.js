// Jade Bitcoin signing flow for proposal pages.
//
// Flow:
//   1. Fetch the server's sign-data JSON for this proposal (`psbt_b64`,
//      `descriptor`, `network`).
//   2. Open WebSerial → JadeRpc, unlock against the pinserver.
//   3. Parse the federation's BIP-380 multipath descriptor into Jade's
//      `descriptor` object (variant + sorted + threshold + signers).
//   4. `register_multisig` on the device. Idempotent: re-registering the
//      same name + content is a no-op on Jade (the user still has to
//      confirm on-device the first time and on any content change). Plan
//      §1.3 calls this "lazy/idempotent registration".
//   5. `sign_psbt` on the device. User confirms each output on the screen.
//   6. POST the resulting partial PSBT (base64) to /partial-psbt.
//   7. Reload.
//
// Unlike the Trezor path (per-input DER signatures), Jade returns a
// complete partial PSBT that we hand straight to the server's
// `Psbt::combine`-based merge endpoint.

import {
    JadeRpc,
    pathToU32Array,
    bytesToHex,
    hexToBytes,
    base64ToBytes,
    bytesToBase64,
} from "./vendor/jade-rpc.js";

const cfg = window.ASTERISM || {};

const btn = document.getElementById("sign-btn");
const statusEl = document.getElementById("sign-status");

// Bail silently if the page isn't rendering the Sign button (proposal in
// a post-signing state, or viewer has no signer).
if (btn && statusEl) {
    btn.addEventListener("click", () => {
        signProposal().catch((e) => {
            console.error("[jade] sign failed:", e);
        });
    });
}

function setStatus(msg, kind) {
    if (!statusEl) return;
    statusEl.textContent = msg || "";
    statusEl.classList.remove("error", "ok");
    if (kind) statusEl.classList.add(kind);
}

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

/// Parse a `wsh(sortedmulti(M, key1, key2, ...))` descriptor into
/// `{ threshold, signers: [{ fingerprintHex, derivation, xpub }] }`.
/// Strips the trailing checksum and the multipath suffix on each key.
export function parseSortedMultiWshDescriptor(descriptor) {
    if (typeof descriptor !== "string") {
        throw new TypeError("descriptor must be a string");
    }
    let s = descriptor.split("#")[0].trim();
    if (!s.startsWith("wsh(") || !s.endsWith(")")) {
        throw new Error(`expected wsh(...) wrapper, got ${truncatePreview(s)}`);
    }
    s = s.slice(4, -1).trim();

    let inner;
    if (s.startsWith("sortedmulti(") && s.endsWith(")")) {
        inner = s.slice("sortedmulti(".length, -1);
    } else {
        throw new Error(`expected sortedmulti(...), got ${truncatePreview(s)}`);
    }

    const parts = splitTopLevelCommas(inner);
    if (parts.length < 2) {
        throw new Error("sortedmulti must have a threshold + at least one key");
    }
    const threshold = Number.parseInt(parts[0].trim(), 10);
    if (!Number.isInteger(threshold) || threshold < 1) {
        throw new Error(`bad threshold: ${parts[0]}`);
    }
    const signers = parts.slice(1).map((part) => parseDescriptorKey(part.trim()));
    return { threshold, signers };
}

/// Parse a single descriptor key `[fp/path]xpub<suffix>` into
/// `{ fingerprintHex, derivation: u32[], xpub }`. The suffix (e.g.
/// `/<0;1>/*`) is stripped — Jade applies its own multisig-side path
/// derivation at signing time.
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

    // Whatever follows `]` is `xpub` plus an optional `/...` suffix
    // (multipath, fixed index, wildcard). Split on the first `/` and
    // keep just the xpub.
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

/// Derive a 1..15 char Jade-friendly multisig name from a federation UUID
/// (`"ast"` + first 8 hex of the id with dashes stripped).
function jadeMultisigName(federationId) {
    const hex = String(federationId).replaceAll("-", "").slice(0, 8);
    return `ast${hex}`;
}

/// Build the Jade `register_multisig` `descriptor` object for our P2WSH
/// `sortedmulti` federations.
function buildJadeDescriptor(parsed) {
    return {
        variant: "wsh(multi(k))",
        sorted: true,
        threshold: parsed.threshold,
        signers: parsed.signers.map((s) => ({
            fingerprint: hexToBytes(s.fingerprintHex),
            derivation: s.derivation,
            xpub: s.xpub,
            // `path` is applied to the xpub before keychain/index
            // derivation. Our multipath descriptors put the keychain +
            // index suffix directly on the xpub (`/<0;1>/*`), so the
            // additional pre-path is empty.
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

    const { psbt_b64, descriptor, network: rawNetwork } = signData;
    if (!psbt_b64 || !descriptor) {
        setStatus("sign-data response missing psbt_b64 / descriptor", "error");
        btn.disabled = false;
        return;
    }
    const network = jadeNetworkName(rawNetwork);

    let parsed;
    try {
        parsed = parseSortedMultiWshDescriptor(descriptor);
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

        setStatus("Registering multisig wallet on Jade — confirm on the device…");
        const name = jadeMultisigName(federationId);
        await jade.registerMultisig(network, name, buildJadeDescriptor(parsed));

        setStatus("Signing PSBT — confirm outputs on the Jade screen…");
        const psbtBytes = base64ToBytes(psbt_b64);
        const signedBytes = await jade.signPsbt(network, psbtBytes);
        const signedB64 = bytesToBase64(signedBytes);

        // Defensive sanity-log: short-print so devs can correlate with
        // server logs without dumping the whole PSBT.
        console.debug(
            "[jade] signed PSBT bytes=%d sha256=%s",
            signedBytes.length,
            await sha256ShortHex(signedBytes),
        );

        setStatus("Submitting signed PSBT…");
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
            console.warn("[jade] close after sign:", e);
        }
    }
}

async function sha256ShortHex(bytes) {
    if (typeof crypto === "undefined" || !crypto.subtle) return "n/a";
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    return bytesToHex(new Uint8Array(digest)).slice(0, 16);
}
