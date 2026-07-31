// Minimal CBOR encoder/decoder for the Blockstream Jade JSON-RPC subset.
//
// Jade speaks CBOR (RFC 7049 / 8949) over its serial transport. The wire
// format is small enough — and our usage focused enough — that pulling a
// general-purpose npm CBOR library would be more risk than reward. This
// file implements only what `jade-rpc.js` needs:
//
// * encode: positive uint (up to 2^53 - 1), text string, byte string
//   (Uint8Array), array, plain object (string keys → CBOR map), boolean,
//   null/undefined.
// * decode: all of the above + tagged values (passed through as
//   `{ tag, value }` so callers can ignore them) + indefinite-length
//   strings/arrays/maps + half/single/double precision floats (decoded
//   to JS Number).
//
// Negative integers are NOT used by any Jade RPC we drive, but we still
// decode them correctly (returning a JS Number when within `[-2^53,
// 2^53)`, otherwise a `BigInt`). Encoding is a one-shot Uint8Array; we
// never need streaming output.
//
// All multi-byte integers on the wire are big-endian per the spec.
//
// Exports: `encode(value): Uint8Array`, `decode(bytes): { value, length }`.

const POW32 = 0x100000000; // 2^32 — used to combine 8-byte uints.

// -- Encode -----------------------------------------------------------------

class Encoder {
    constructor() {
        this.chunks = [];
        this.length = 0;
    }

    push(arr) {
        this.chunks.push(arr);
        this.length += arr.length;
    }

    finish() {
        const out = new Uint8Array(this.length);
        let offset = 0;
        for (const chunk of this.chunks) {
            out.set(chunk, offset);
            offset += chunk.length;
        }
        return out;
    }

    // Write a CBOR head: `major` is 0..7, `value` is the argument
    // (length, magnitude, etc). Produces 1, 2, 3, 5 or 9 bytes per the
    // spec.
    writeHead(major, value) {
        const tag = (major & 7) << 5;
        if (typeof value === "bigint") {
            // BigInt path is only hit for big uints (>= 2^53). Encode as
            // 8-byte uint64.
            const buf = new Uint8Array(9);
            buf[0] = tag | 27;
            const view = new DataView(buf.buffer);
            view.setBigUint64(1, value, false);
            this.push(buf);
            return;
        }
        if (value < 24) {
            this.push(new Uint8Array([tag | value]));
        } else if (value < 0x100) {
            this.push(new Uint8Array([tag | 24, value]));
        } else if (value < 0x10000) {
            const buf = new Uint8Array(3);
            buf[0] = tag | 25;
            buf[1] = (value >> 8) & 0xff;
            buf[2] = value & 0xff;
            this.push(buf);
        } else if (value < POW32) {
            const buf = new Uint8Array(5);
            buf[0] = tag | 26;
            const view = new DataView(buf.buffer);
            view.setUint32(1, value, false);
            this.push(buf);
        } else {
            // 64-bit head encoded from a JS Number. Fine up to 2^53 - 1.
            const buf = new Uint8Array(9);
            buf[0] = tag | 27;
            const view = new DataView(buf.buffer);
            const hi = Math.floor(value / POW32);
            const lo = value >>> 0;
            view.setUint32(1, hi, false);
            view.setUint32(5, lo, false);
            this.push(buf);
        }
    }
}

function encodeInto(enc, value) {
    if (value === null || value === undefined) {
        // Major 7, simple value 22 (= null) per RFC 7049 §2.3.
        enc.push(new Uint8Array([0xf6]));
        return;
    }
    if (value === true) {
        enc.push(new Uint8Array([0xf5]));
        return;
    }
    if (value === false) {
        enc.push(new Uint8Array([0xf4]));
        return;
    }
    if (typeof value === "number") {
        if (!Number.isInteger(value)) {
            throw new TypeError(`CBOR: floats not supported (got ${value})`);
        }
        if (value >= 0) {
            enc.writeHead(0, value);
        } else {
            enc.writeHead(1, -value - 1);
        }
        return;
    }
    if (typeof value === "bigint") {
        if (value >= 0n) {
            enc.writeHead(0, value);
        } else {
            enc.writeHead(1, -value - 1n);
        }
        return;
    }
    if (typeof value === "string") {
        const utf8 = new TextEncoder().encode(value);
        enc.writeHead(3, utf8.length);
        enc.push(utf8);
        return;
    }
    if (value instanceof Uint8Array) {
        enc.writeHead(2, value.length);
        enc.push(value);
        return;
    }
    if (ArrayBuffer.isView(value)) {
        // Accept any typed array as bytes. (Buffer subclassing is
        // common.)
        const u8 = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
        enc.writeHead(2, u8.length);
        enc.push(u8);
        return;
    }
    if (Array.isArray(value)) {
        enc.writeHead(4, value.length);
        for (const v of value) {
            encodeInto(enc, v);
        }
        return;
    }
    if (typeof value === "object") {
        const entries = Object.entries(value);
        enc.writeHead(5, entries.length);
        // Jade is order-tolerant for map keys per its docs: "the order
        // of named fields inside the messages is unimportant". We emit
        // in insertion order to match what callers expect.
        for (const [k, v] of entries) {
            encodeInto(enc, k);
            encodeInto(enc, v);
        }
        return;
    }
    throw new TypeError(`CBOR: unsupported value of type ${typeof value}`);
}

export function encode(value) {
    const enc = new Encoder();
    encodeInto(enc, value);
    return enc.finish();
}

// -- Decode -----------------------------------------------------------------

class DecoderState {
    constructor(bytes) {
        this.bytes = bytes;
        this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        this.offset = 0;
    }

    readByte() {
        if (this.offset >= this.bytes.length) {
            throw new RangeError("CBOR: unexpected end of input");
        }
        return this.bytes[this.offset++];
    }

    readBytes(n) {
        if (this.offset + n > this.bytes.length) {
            throw new RangeError("CBOR: unexpected end of input");
        }
        const slice = this.bytes.subarray(this.offset, this.offset + n);
        this.offset += n;
        return slice;
    }

    // Read the argument for an item with `info` minor type. Returns
    // either a Number (when the value fits) or a BigInt (for huge
    // 8-byte values that exceed 2^53 - 1).
    readArg(info) {
        if (info < 24) return info;
        if (info === 24) return this.readByte();
        if (info === 25) {
            const v = this.view.getUint16(this.offset, false);
            this.offset += 2;
            return v;
        }
        if (info === 26) {
            const v = this.view.getUint32(this.offset, false);
            this.offset += 4;
            return v;
        }
        if (info === 27) {
            const hi = this.view.getUint32(this.offset, false);
            const lo = this.view.getUint32(this.offset + 4, false);
            this.offset += 8;
            // Combine as Number when safe (Jade ids fit easily); else BigInt.
            if (hi < 0x200000) {
                return hi * POW32 + lo;
            }
            return (BigInt(hi) << 32n) | BigInt(lo);
        }
        if (info === 31) return -1; // indefinite-length sentinel.
        throw new RangeError(`CBOR: reserved additional info ${info}`);
    }
}

function decodeItem(s) {
    const initial = s.readByte();
    const major = initial >> 5;
    const info = initial & 0x1f;

    switch (major) {
        case 0: {
            // Unsigned integer.
            return s.readArg(info);
        }
        case 1: {
            // Negative integer, value = -1 - arg.
            const arg = s.readArg(info);
            if (typeof arg === "bigint") {
                return -1n - arg;
            }
            return -1 - arg;
        }
        case 2: {
            // Byte string.
            if (info === 31) return decodeIndefiniteString(s, 2);
            const len = s.readArg(info);
            return new Uint8Array(s.readBytes(Number(len)));
        }
        case 3: {
            // UTF-8 text string.
            if (info === 31) return new TextDecoder().decode(decodeIndefiniteString(s, 3));
            const len = s.readArg(info);
            return new TextDecoder().decode(s.readBytes(Number(len)));
        }
        case 4: {
            // Array.
            if (info === 31) return decodeIndefiniteArray(s);
            const len = s.readArg(info);
            const arr = new Array(Number(len));
            for (let i = 0; i < arr.length; i += 1) {
                arr[i] = decodeItem(s);
            }
            return arr;
        }
        case 5: {
            // Map.
            if (info === 31) return decodeIndefiniteMap(s);
            const len = s.readArg(info);
            const obj = {};
            for (let i = 0; i < Number(len); i += 1) {
                const key = decodeItem(s);
                const val = decodeItem(s);
                // Jade only uses string keys; coerce to be safe.
                obj[String(key)] = val;
            }
            return obj;
        }
        case 6: {
            // Tagged value. Pass through as { tag, value } so callers
            // that don't care can still walk the structure.
            const tag = s.readArg(info);
            const value = decodeItem(s);
            return { tag, value };
        }
        case 7: {
            // Floats / simple values.
            if (info === 20) return false;
            if (info === 21) return true;
            if (info === 22) return null;
            if (info === 23) return undefined;
            if (info === 24) {
                // 1-byte simple value.
                return { simple: s.readByte() };
            }
            if (info === 25) {
                // Half-precision float.
                const u = s.view.getUint16(s.offset, false);
                s.offset += 2;
                return halfPrecisionToFloat(u);
            }
            if (info === 26) {
                const v = s.view.getFloat32(s.offset, false);
                s.offset += 4;
                return v;
            }
            if (info === 27) {
                const v = s.view.getFloat64(s.offset, false);
                s.offset += 8;
                return v;
            }
            if (info === 31) {
                // Break stop code; should only appear inside indefinite-length items.
                throw new RangeError("CBOR: stray break stop code");
            }
            throw new RangeError(`CBOR: reserved simple value ${info}`);
        }
        default:
            throw new RangeError(`CBOR: unknown major ${major}`);
    }
}

function decodeIndefiniteString(s, major) {
    const chunks = [];
    let total = 0;
    for (;;) {
        const initial = s.readByte();
        if (initial === 0xff) break;
        const m = initial >> 5;
        const info = initial & 0x1f;
        if (m !== major || info === 31) {
            throw new RangeError("CBOR: invalid chunk in indefinite-length string");
        }
        const len = Number(s.readArg(info));
        const chunk = s.readBytes(len);
        chunks.push(chunk);
        total += chunk.length;
    }
    const out = new Uint8Array(total);
    let off = 0;
    for (const c of chunks) {
        out.set(c, off);
        off += c.length;
    }
    return out;
}

function decodeIndefiniteArray(s) {
    const arr = [];
    for (;;) {
        if (s.bytes[s.offset] === 0xff) {
            s.offset += 1;
            break;
        }
        arr.push(decodeItem(s));
    }
    return arr;
}

function decodeIndefiniteMap(s) {
    const obj = {};
    for (;;) {
        if (s.bytes[s.offset] === 0xff) {
            s.offset += 1;
            break;
        }
        const k = decodeItem(s);
        const v = decodeItem(s);
        obj[String(k)] = v;
    }
    return obj;
}

function halfPrecisionToFloat(u) {
    // RFC 7049 Appendix D's reference impl, transcribed.
    const exp = (u & 0x7c00) >> 10;
    const mant = u & 0x3ff;
    const sign = u & 0x8000 ? -1 : 1;
    if (exp === 0) return sign * Math.pow(2, -14) * (mant / 1024);
    if (exp === 31) return mant === 0 ? sign * Infinity : NaN;
    return sign * Math.pow(2, exp - 15) * (1 + mant / 1024);
}

/// Decode the first complete CBOR item from `bytes`. Returns
/// `{ value, length }` where `length` is the number of bytes consumed.
/// Throws `RangeError` if `bytes` does not contain a complete item — the
/// caller should buffer more data and retry.
export function decode(bytes) {
    const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const s = new DecoderState(u8);
    const value = decodeItem(s);
    return { value, length: s.offset };
}
