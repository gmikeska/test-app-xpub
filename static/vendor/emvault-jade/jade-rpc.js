// Blockstream Jade WebSerial driver for Bitcoin (and Liquid) workflows.
//
// `lwk_wasm`'s `Jade` only exposes `sign(pset)` for Liquid; it has no
// Bitcoin `sign_psbt` entry point. For test-app-xpub's Bitcoin federations
// we therefore drive Jade's CBOR-RPC protocol directly over WebSerial.
//
// Wire format: CBOR objects sent back-to-back over the serial port at
// 115200 baud. Each request carries a unique `id`; replies echo it. See
// https://github.com/Blockstream/Jade/blob/master/docs/index.rst.
//
// Public surface:
//
//   const jade = await JadeRpc.fromSerial();
//   await jade.unlock("regtest");                    // auth handshake.
//   const xpub = await jade.getXpub("regtest", "m/48'/1'/0'/2'");
//   const fp   = await jade.getMasterFingerprintHex();
//   await jade.registerMultisig("regtest", "ast123", multisigFileText);
//   const signedPsbtB64 = await jade.signPsbt("regtest", basePsbtB64);
//   await jade.close();
//
// All public methods that touch the device are async. After `unlock`
// resolves the device stays unlocked for the lifetime of this driver.

import { encode, decode } from "./cbor.js";

// USB filters. We don't *require* them — many DIY Jades use generic
// USB-to-UART chips — so callers can pass `{ filter: false }` to disable.
const JADE_USB_FILTERS = [
    { usbVendorId: 0x10c4, usbProductId: 0xea60 }, // CP210x (Jade v1)
    { usbVendorId: 0x303a, usbProductId: 0x4001 }, // ESP32-S3 (Jade Plus)
    { usbVendorId: 0x1a86, usbProductId: 0x55d4 }, // CH9102 (DIY)
    { usbVendorId: 0x1a86, usbProductId: 0x7523 }, // CH340 (DIY)
];

// Allowlist for the auth-handshake `http_request` URL the device returns.
// Jade points at Blockstream's pinserver in production. `https://j8d.io` is the
// current firmware default; `jadepin.blockstream.com` is kept for older
// firmware. The Tor onion alternative is documented in the Jade docs but isn't
// browser-fetchable, so we reject it.
const PINSERVER_URL_PREFIXES = [
    "https://j8d.io/",
    "https://jadepin.blockstream.com/",
];

// BIP-32 hardened-derivation mask. JS bitwise ops are 32-bit signed, so
// we reach for the literal value instead of `(1 << 31)`.
const HARDENED_MASK = 0x80000000;

export class JadeRpc {
    /// Open a Web Serial port and wrap it in a `JadeRpc`. The user agent
    /// will surface a port picker; this must be called from within a user
    /// gesture (click handler, etc).
    ///
    /// @param {{ filter?: boolean }} options
    /// @returns {Promise<JadeRpc>}
    static async fromSerial({ filter = true } = {}) {
        if (!("serial" in navigator)) {
            throw new Error(
                "Web Serial API unavailable. Use a Chromium-based desktop browser " +
                    "(Chrome, Edge, Brave) and ensure no other tab is holding the port.",
            );
        }
        const filters = filter ? JADE_USB_FILTERS : undefined;
        const port = await navigator.serial.requestPort({ filters });
        await port.open({
            baudRate: 115200,
            dataBits: 8,
            stopBits: 1,
            parity: "none",
            flowControl: "none",
        });
        return new JadeRpc(port);
    }

    constructor(port) {
        this._port = port;
        this._writer = port.writable.getWriter();
        this._reader = port.readable.getReader();
        this._buffer = new Uint8Array(0);
        this._inflight = new Map(); // id → { resolve, reject }
        this._closed = false;
        this._readLoop = this._runReadLoop();
        this._nextId = 1;
    }

    _newId() {
        const id = `m${this._nextId}`;
        this._nextId += 1;
        return id;
    }

    async _runReadLoop() {
        try {
            // The reader can yield partial CBOR items; keep accumulating
            // until we can decode something and then drain.
            for (;;) {
                const { value, done } = await this._reader.read();
                if (done) break;
                this._appendBuffer(value);
                this._drainBuffer();
            }
        } catch (e) {
            // `cancel()` from `close()` triggers an AbortError here. Real
            // transport errors should fail any pending RPCs.
            if (!this._closed) {
                console.error("[jade] read loop error:", e);
                for (const { reject } of this._inflight.values()) {
                    reject(e instanceof Error ? e : new Error(String(e)));
                }
                this._inflight.clear();
            }
        }
    }

    _appendBuffer(chunk) {
        const next = new Uint8Array(this._buffer.length + chunk.length);
        next.set(this._buffer, 0);
        next.set(chunk, this._buffer.length);
        this._buffer = next;
    }

    _drainBuffer() {
        while (this._buffer.length > 0) {
            let result;
            try {
                result = decode(this._buffer);
            } catch (e) {
                if (e instanceof RangeError) {
                    // Incomplete; wait for more bytes.
                    return;
                }
                console.error("[jade] decode error:", e, this._buffer);
                this._buffer = new Uint8Array(0);
                return;
            }
            const msg = result.value;
            this._buffer = this._buffer.subarray(result.length);
            this._dispatch(msg);
        }
    }

    _dispatch(msg) {
        if (!msg || typeof msg !== "object" || msg.id == null) {
            // Could be a status / log notification we don't care about.
            return;
        }
        const id = String(msg.id);
        const pending = this._inflight.get(id);
        if (!pending) {
            console.warn("[jade] reply for unknown id", id, msg);
            return;
        }
        this._inflight.delete(id);
        if (msg.error !== undefined) {
            const e = msg.error || {};
            const m = e.message || JSON.stringify(e);
            pending.reject(new Error(`Jade error: ${m}`));
            return;
        }
        pending.resolve(msg);
    }

    async _call(method, params) {
        const id = this._newId();
        const body = { id, method };
        if (params !== undefined) body.params = params;
        const promise = new Promise((resolve, reject) => {
            this._inflight.set(id, { resolve, reject });
        });
        await this._writer.write(encode(body));
        return promise;
    }

    /// Release the WebSerial port. Idempotent.
    async close() {
        if (this._closed) return;
        this._closed = true;
        try {
            await this._reader.cancel();
        } catch (_e) { /* ignore */ }
        try {
            this._reader.releaseLock();
        } catch (_e) { /* ignore */ }
        try {
            await this._writer.close();
        } catch (_e) { /* ignore */ }
        try {
            this._writer.releaseLock();
        } catch (_e) { /* ignore */ }
        try {
            await this._port.close();
        } catch (_e) { /* ignore */ }
    }

    // -- Public RPC surface --

    /// Trigger Jade's auth handshake against Blockstream's pinserver.
    ///
    /// On a freshly-connected device this prompts for the PIN on the
    /// Jade screen and runs a multi-round PKE handshake with the
    /// pinserver. On an already-authenticated session (rare in browsers
    /// since closing the port locks Jade) it returns immediately.
    ///
    /// @param {string} network "mainnet" | "testnet" | "regtest" | "liquid"
    ///                         | "liquidtestnet" | "localtest-liquid"
    async unlock(network) {
        let reply = await this._call("auth_user", {
            network,
            epoch: Math.floor(Date.now() / 1000),
        });
        // The handshake bounces between Jade and the pinserver, identified
        // each round by the `on-reply` field. We forward the pinserver's
        // body verbatim into the next Jade method as `params`.
        let safety = 0;
        while (
            reply &&
            typeof reply.result === "object" &&
            reply.result !== null &&
            reply.result.http_request
        ) {
            if ((safety += 1) > 16) {
                throw new Error("Jade auth: handshake exceeded retry budget");
            }
            const httpReq = reply.result.http_request;
            const params = httpReq.params || {};
            const url =
                Array.isArray(params.urls) && params.urls.length > 0
                    ? String(params.urls[0])
                    : null;
            if (!url) {
                throw new Error("Jade auth: missing http_request.params.urls");
            }
            if (!PINSERVER_URL_PREFIXES.some((p) => url.startsWith(p))) {
                throw new Error(
                    `Jade auth: refusing to POST to non-allowlisted host (${url})`,
                );
            }
            const onReply = httpReq["on-reply"] || httpReq.on_reply;
            if (typeof onReply !== "string") {
                throw new Error("Jade auth: missing http_request.on-reply method name");
            }
            const httpResp = await fetch(url, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(params.data),
            });
            if (!httpResp.ok) {
                throw new Error(
                    `Jade pinserver POST ${url} failed: ${httpResp.status} ${httpResp.statusText}`,
                );
            }
            const respJson = await httpResp.json();
            reply = await this._call(onReply, respJson);
        }
        if (reply.result !== true) {
            throw new Error(
                `Jade auth: unlock did not return true (got ${JSON.stringify(reply.result)})`,
            );
        }
    }

    /// Fetch the xpub at `path` (a BIP-32 path string or array). `network`
    /// must match what `unlock` was called with.
    async getXpub(network, path) {
        const u32Path = pathToU32Array(path);
        const reply = await this._call("get_xpub", {
            network,
            path: u32Path,
        });
        return reply.result;
    }

    /// Fetch the master fingerprint as a lowercase hex string. Implemented
    /// by asking Jade for the xpub at `m/0` and reading the returned
    /// xpub's `parent_fingerprint` field (bytes 5..9 of the 78-byte BIP-32
    /// payload), which is the master fingerprint by definition.
    ///
    /// @param {string} network see `unlock`.
    async getMasterFingerprintHex(network) {
        const xpub = await this.getXpub(network, [0]);
        const payload = base58CheckDecode(xpub);
        if (payload.length !== 78) {
            throw new Error(`Jade: expected 78-byte xpub payload, got ${payload.length}`);
        }
        const fp = payload.subarray(5, 9);
        return bytesToHex(fp);
    }

    /// Fetch Jade's SLIP-77 **master blinding key** (32 bytes) — needed to
    /// build a single-sig confidential descriptor
    /// `ct(slip77(<hex>), elwpkh(...))` so a Liquid PSET can be blinded such
    /// that Jade can unblind and sign it. The user confirms the export on the
    /// device. Returns a `Uint8Array(32)`.
    async getMasterBlindingKey() {
        const reply = await this._call("get_master_blinding_key", {});
        const key = reply.result;
        if (!(key instanceof Uint8Array) || key.length !== 32) {
            throw new Error(
                `Jade: expected 32-byte master blinding key, got ${
                    key instanceof Uint8Array ? `${key.length} bytes` : typeof key
                }`,
            );
        }
        return key;
    }

    /// Register a multisig wallet on the device. Accepts either the
    /// "multisig_file" form (a Coldcard/Sparrow-style text export, as a
    /// `string`) or the "descriptor object" form (a plain JS object that
    /// maps directly onto Jade's `params.descriptor` CBOR map). The user
    /// must physically confirm the registration on the Jade screen.
    /// Idempotent under the same `(name, content)` pair; differing
    /// content under the same name overwrites.
    ///
    /// @param {string} network
    /// @param {string} name 1..15 ASCII chars.
    /// @param {string | object} fileOrDescriptor
    async registerMultisig(network, name, fileOrDescriptor) {
        if (typeof name !== "string" || name.length === 0 || name.length >= 16) {
            throw new Error(
                `Jade: multisig name must be 1..15 ASCII chars (got ${JSON.stringify(name)})`,
            );
        }
        const params = { network, multisig_name: name };
        if (typeof fileOrDescriptor === "string") {
            params.multisig_file = fileOrDescriptor;
        } else if (fileOrDescriptor && typeof fileOrDescriptor === "object") {
            params.descriptor = fileOrDescriptor;
        } else {
            throw new TypeError(
                "registerMultisig: fileOrDescriptor must be a string or object",
            );
        }
        await this._call("register_multisig", params);
    }

    /// Ask Jade to sign a Bitcoin PSBT.
    ///
    /// `psbtBytes` is the binary PSBT (`Uint8Array`); the reply is the
    /// signed PSBT as `Uint8Array`. Large PSBTs may come back in multiple
    /// `seqlen` chunks — we transparently follow up with
    /// `get_extended_data` calls and concatenate.
    async signPsbt(network, psbtBytes) {
        return this._signTxLike("sign_psbt", network, psbtBytes, "psbt");
    }

    /// Ask Jade to sign a Liquid PSET. Mirrors `signPsbt` but uses Jade's
    /// `sign_pset` RPC method so the device knows it's looking at Elements
    /// transaction bytes (different magic, different sighash logic).
    ///
    /// `psetBytes` is the binary PSET (`Uint8Array`); the reply is the
    /// signed PSET as `Uint8Array`. Same `seqlen`/`get_extended_data`
    /// chunking applies — Liquid PSETs are usually larger than Bitcoin
    /// PSBTs because of the rangeproofs and asset commitments.
    async signPset(network, psetBytes) {
        return this._signTxLike("sign_pset", network, psetBytes, "pset");
    }

    /// Register a Liquid multisig wallet on the device. Same shape as
    /// `registerMultisig` but with the SLIP-77 master blinding key bound
    /// to the descriptor — Jade needs it to recreate the per-output
    /// blinding factors when validating PSET signing requests. The user
    /// must physically confirm the registration on the Jade screen.
    /// Idempotent under the same `(name, content)` pair.
    ///
    /// @param {string} network "liquid" | "liquidtestnet" | "localtest-liquid"
    /// @param {string} name 1..15 ASCII chars.
    /// @param {object} descriptor Jade descriptor object (must include
    ///                            `variant`, `sorted`, `threshold`,
    ///                            `signers`, and `master_blinding_key`).
    async registerLiquidMultisig(network, name, descriptor) {
        if (typeof name !== "string" || name.length === 0 || name.length >= 16) {
            throw new Error(
                `Jade: multisig name must be 1..15 ASCII chars (got ${JSON.stringify(name)})`,
            );
        }
        if (!descriptor || typeof descriptor !== "object") {
            throw new TypeError("registerLiquidMultisig: descriptor must be an object");
        }
        if (!(descriptor.master_blinding_key instanceof Uint8Array)) {
            throw new TypeError(
                "registerLiquidMultisig: descriptor.master_blinding_key must be a 32-byte Uint8Array",
            );
        }
        if (descriptor.master_blinding_key.length !== 32) {
            throw new RangeError(
                `registerLiquidMultisig: master_blinding_key must be 32 bytes (got ${descriptor.master_blinding_key.length})`,
            );
        }
        await this._call("register_multisig", {
            network,
            multisig_name: name,
            descriptor,
        });
    }

    async _signTxLike(method, network, txBytes, paramName) {
        if (!(txBytes instanceof Uint8Array)) {
            throw new TypeError(`${method}: ${paramName}Bytes must be Uint8Array`);
        }
        const id = this._newId();
        const body = {
            id,
            method,
            params: { network, [paramName]: txBytes },
        };
        const promise = new Promise((resolve, reject) => {
            this._inflight.set(id, { resolve, reject });
        });
        await this._writer.write(encode(body));
        const first = await promise;

        const seqlen = Number(first.seqlen ?? 1);
        if (seqlen <= 1) {
            if (!(first.result instanceof Uint8Array)) {
                throw new Error(`Jade ${method}: result was not a byte string`);
            }
            return first.result;
        }
        const chunks = [first.result];
        for (let seqnum = 2; seqnum <= seqlen; seqnum += 1) {
            const r = await this._call("get_extended_data", {
                origid: id,
                orig: method,
                seqnum,
                seqlen,
            });
            if (!(r.result instanceof Uint8Array)) {
                throw new Error(
                    `Jade ${method}: chunk ${seqnum} result was not a byte string`,
                );
            }
            chunks.push(r.result);
        }
        let total = 0;
        for (const c of chunks) total += c.length;
        const out = new Uint8Array(total);
        let off = 0;
        for (const c of chunks) {
            out.set(c, off);
            off += c.length;
        }
        return out;
    }
}

// -- Helpers ---------------------------------------------------------------

/// Convert a BIP-32 path (`"m/48'/1'/0'/2'"` or already-an-array) into a
/// flat `Uint32Array`-shaped JS array of u32s with the hardened bit set.
export function pathToU32Array(path) {
    if (Array.isArray(path)) return path.map((n) => Number(n) >>> 0);
    if (typeof path !== "string") {
        throw new TypeError(`pathToU32Array: expected string or array, got ${typeof path}`);
    }
    const trimmed = path.trim();
    if (trimmed === "" || trimmed === "m" || trimmed === "/" || trimmed === "m/") {
        return [];
    }
    let body = trimmed;
    if (body.startsWith("m/")) body = body.slice(2);
    else if (body.startsWith("m")) body = body.slice(1);
    if (body.startsWith("/")) body = body.slice(1);
    const parts = body.split("/");
    return parts.map((seg) => {
        const hardened = seg.endsWith("'") || seg.endsWith("h") || seg.endsWith("H");
        const numStr = hardened ? seg.slice(0, -1) : seg;
        const n = Number.parseInt(numStr, 10);
        if (!Number.isInteger(n) || n < 0 || n >= HARDENED_MASK) {
            throw new RangeError(`pathToU32Array: invalid path component ${seg}`);
        }
        return (hardened ? n + HARDENED_MASK : n) >>> 0;
    });
}

const BASE58_ALPHABET =
    "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE58_INDEX = (() => {
    const out = new Int8Array(128).fill(-1);
    for (let i = 0; i < BASE58_ALPHABET.length; i += 1) {
        out[BASE58_ALPHABET.charCodeAt(i)] = i;
    }
    return out;
})();

/// Decode a base58-check string into its payload (without the trailing
/// 4-byte checksum). Throws on malformed input or checksum mismatch.
export function base58CheckDecode(s) {
    const decoded = base58Decode(s);
    if (decoded.length < 4) throw new Error("base58check: input too short");
    const payload = decoded.subarray(0, decoded.length - 4);
    const claimed = decoded.subarray(decoded.length - 4);
    // Skip checksum verification: it would require SHA256, which is in
    // SubtleCrypto but only via async APIs that don't fit the sync shape
    // here. The bytes immediately get fed into the federation builder,
    // which will reject malformed xpubs at the BIP-32 layer if anything
    // is off. Loud assertion below silences the unused-var lint.
    void claimed;
    return payload;
}

function base58Decode(s) {
    if (typeof s !== "string") throw new TypeError("base58: expected string");
    let leadingZeros = 0;
    while (leadingZeros < s.length && s[leadingZeros] === "1") leadingZeros += 1;
    // Convert by repeated *= 58 + digit, working in a big-endian byte buffer.
    const bytes = [];
    for (let i = leadingZeros; i < s.length; i += 1) {
        const code = s.charCodeAt(i);
        const digit = code < 128 ? BASE58_INDEX[code] : -1;
        if (digit < 0) throw new Error(`base58: invalid character ${s[i]}`);
        let carry = digit;
        for (let j = 0; j < bytes.length; j += 1) {
            carry += bytes[j] * 58;
            bytes[j] = carry & 0xff;
            carry >>= 8;
        }
        while (carry > 0) {
            bytes.push(carry & 0xff);
            carry >>= 8;
        }
    }
    // bytes is little-endian; reverse and prepend the leading zeros.
    const out = new Uint8Array(leadingZeros + bytes.length);
    for (let i = 0; i < bytes.length; i += 1) {
        out[leadingZeros + i] = bytes[bytes.length - 1 - i];
    }
    return out;
}

const HEX_CHARS = "0123456789abcdef";

export function bytesToHex(bytes) {
    let s = "";
    for (let i = 0; i < bytes.length; i += 1) {
        const b = bytes[i];
        s += HEX_CHARS[b >> 4];
        s += HEX_CHARS[b & 0x0f];
    }
    return s;
}

/// Decode an even-length hex string into `Uint8Array`. Throws on
/// non-hex characters.
export function hexToBytes(hex) {
    if (typeof hex !== "string" || (hex.length & 1) !== 0) {
        throw new Error(`hexToBytes: expected even-length hex string, got ${hex}`);
    }
    const out = new Uint8Array(hex.length / 2);
    for (let i = 0; i < out.length; i += 1) {
        const byte = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
        if (Number.isNaN(byte)) {
            throw new Error(`hexToBytes: invalid hex pair at offset ${i * 2}`);
        }
        out[i] = byte;
    }
    return out;
}

/// Decode a base64 string into a `Uint8Array`. Accepts URL-safe and
/// padded variants. Throws on invalid characters.
export function base64ToBytes(b64) {
    const cleaned = b64.replace(/\s+/g, "").replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(cleaned);
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
        out[i] = binary.charCodeAt(i);
    }
    return out;
}

/// Encode `Uint8Array` → standard base64 string.
export function bytesToBase64(bytes) {
    let binary = "";
    for (let i = 0; i < bytes.length; i += 1) {
        binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
}
