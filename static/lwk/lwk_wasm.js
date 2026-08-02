let wasm;

let cachedUint8ArrayMemory0 = null;

function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });

cachedTextDecoder.decode();

const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let WASM_VECTOR_LEN = 0;

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedDataViewMemory0 = null;

function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_export_4.set(idx, obj);
    return idx;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(
state => {
    wasm.__wbindgen_export_5.get(state.dtor)(state.a, state.b);
}
);

function makeMutClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            if (--state.cnt === 0) {
                wasm.__wbindgen_export_5.get(state.dtor)(a, state.b);
                CLOSURE_DTORS.unregister(state);
            } else {
                state.a = a;
            }
        }
    };
    real.original = state;
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_export_4.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_export_4.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

let cachedUint32ArrayMemory0 = null;

function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

function getArrayU32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}
/**
 * @returns {Promise<HIDDevice>}
 */
export function searchLedgerDevice() {
    const ret = wasm.searchLedgerDevice();
    return ret;
}

/**
 * Convert the given string to a QR code image uri
 *
 * The image format is monocromatic bitmap, returned as an encoded in base64 uri.
 *
 * Without `pixel_per_module` the default is no border, and 1 pixel per module, to be used
 * for example in html: `style="image-rendering: pixelated; border: 20px solid white;"`
 * @param {string} str
 * @param {number | null} [pixel_per_module]
 * @returns {string}
 */
export function stringToQr(str, pixel_per_module) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(str, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.stringToQr(ptr0, len0, isLikeNone(pixel_per_module) ? 0xFFFFFF : pixel_per_module);
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}
function __wbg_adapter_6(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures_____invoke__hcadcd9ad50d60798(arg0, arg1);
}

function __wbg_adapter_11(arg0, arg1, arg2) {
    wasm.closure1282_externref_shim(arg0, arg1, arg2);
}

function __wbg_adapter_20(arg0, arg1, arg2) {
    wasm.closure807_externref_shim(arg0, arg1, arg2);
}

function __wbg_adapter_29(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures_____invoke__hf60dda57b54e1240(arg0, arg1);
}

function __wbg_adapter_34(arg0, arg1, arg2) {
    wasm.closure2133_externref_shim(arg0, arg1, arg2);
}

function __wbg_adapter_755(arg0, arg1, arg2, arg3) {
    wasm.closure2925_externref_shim(arg0, arg1, arg2, arg3);
}

/**
 * Wallet chain
 * @enum {0 | 1}
 */
export const Chain = Object.freeze({
    /**
     * External address, shown when asked for a payment.
     * Wallet having a single descriptor are considered External
     */
    External: 0, "0": "External",
    /**
     * Internal address, used for the change
     */
    Internal: 1, "1": "Internal",
});

const __wbindgen_enum_BinaryType = ["blob", "arraybuffer"];

const __wbindgen_enum_RequestCache = ["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"];

const __wbindgen_enum_RequestCredentials = ["omit", "same-origin", "include"];

const __wbindgen_enum_RequestMode = ["same-origin", "no-cors", "cors", "navigate"];

const AddressFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_address_free(ptr >>> 0, 1));
/**
 * An Elements (Liquid) address
 */
export class Address {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Address.prototype);
        obj.__wbg_ptr = ptr;
        AddressFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AddressFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_address_free(ptr, 0);
    }
    /**
     * Creates an `Address`
     *
     * If you know the network, you can use `parse()` to validate that the network is consistent.
     * @param {string} s
     */
    constructor(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.address_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        AddressFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Parses an `Address` ensuring is for the right network
     * @param {string} s
     * @param {Network} network
     * @returns {Address}
     */
    static parse(s, network) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(network, Network);
        const ret = wasm.address_parse(ptr0, len0, network.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Address.__wrap(ret[0]);
    }
    /**
     * Return the script pubkey of the address.
     * @returns {Script}
     */
    scriptPubkey() {
        const ret = wasm.address_scriptPubkey(this.__wbg_ptr);
        return Script.__wrap(ret);
    }
    /**
     * Return true if the address is blinded, in other words, if it has a blinding key.
     * @returns {boolean}
     */
    isBlinded() {
        const ret = wasm.address_isBlinded(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return true if the address is for mainnet.
     * @returns {boolean}
     */
    isMainnet() {
        const ret = wasm.address_isMainnet(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return the unconfidential address, in other words, the address without the blinding key.
     * @returns {Address}
     */
    toUnconfidential() {
        const ret = wasm.address_toUnconfidential(this.__wbg_ptr);
        return Address.__wrap(ret);
    }
    /**
     * Return the string representation of the address.
     * This representation can be used to recreate the address via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.address_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Returns a string encoding an image in a uri
     *
     * The string can be open in the browser or be used as `src` field in `img` in HTML
     *
     * For max efficiency we suggest to pass `None` to `pixel_per_module`, get a very small image
     * and use styling to scale up the image in the browser. eg
     * `style="image-rendering: pixelated; border: 20px solid white;"`
     * @param {number | null} [pixel_per_module]
     * @returns {string}
     */
    QRCodeUri(pixel_per_module) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.address_QRCodeUri(this.__wbg_ptr, isLikeNone(pixel_per_module) ? 0xFFFFFF : pixel_per_module);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Returns a string of the QR code printable in a terminal environment
     * @returns {string}
     */
    QRCodeText() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.address_QRCodeText(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) Address.prototype[Symbol.dispose] = Address.prototype.free;

const AddressResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_addressresult_free(ptr >>> 0, 1));
/**
 * Value returned from asking an address to the wallet.
 * Containing the confidential address and its
 * derivation index (the last element in the derivation path)
 */
export class AddressResult {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AddressResult.prototype);
        obj.__wbg_ptr = ptr;
        AddressResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AddressResultFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_addressresult_free(ptr, 0);
    }
    /**
     * Return the address.
     * @returns {Address}
     */
    address() {
        const ret = wasm.addressresult_address(this.__wbg_ptr);
        return Address.__wrap(ret);
    }
    /**
     * Return the derivation index of the address.
     * @returns {number}
     */
    index() {
        const ret = wasm.addressresult_index(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) AddressResult.prototype[Symbol.dispose] = AddressResult.prototype.free;

const Amp0Finalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp0_free(ptr >>> 0, 1));
/**
 * Context for actions related to an AMP0 (sub)account
 *
 * <div class="warning">
 * <b>WARNING:</b>
 *
 * AMP0 is based on a legacy system, and some things do not fit precisely the way LWK allows to do
 * things.
 *
 * Callers must be careful with the following:
 * * <b>Addresses: </b>
 *   to get addresses use [`Amp0::address()`]. This ensures
 *   that all addresses used are correctly monitored by the AMP0 server.
 * * <b>Syncing: </b>
 *   to sync the AMP0 [`crate::Wollet`], use [`Amp0::last_index()`] and [`crate::clients::blocking::BlockchainBackend::full_scan_to_index()`]. This ensures that all utxos are synced, even if there are gaps between higher than the GAP LIMIT.
 *
 * <i>
 * Failing to do the above might lead to inconsistent states, where funds are not shown or they
 * cannot be spent!
 * </i>
 * </div>
 */
export class Amp0 {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp0.prototype);
        obj.__wbg_ptr = ptr;
        Amp0Finalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp0Finalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp0_free(ptr, 0);
    }
    /**
     * Create a new AMP0 context for the specified network
     * @param {Network} network
     * @param {string} username
     * @param {string} password
     * @param {string} amp_id
     * @returns {Promise<Amp0>}
     */
    static newWithNetwork(network, username, password, amp_id) {
        _assertClass(network, Network);
        const ptr0 = passStringToWasm0(username, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(amp_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.amp0_newWithNetwork(network.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        return ret;
    }
    /**
     * Create a new AMP0 context for testnet
     * @param {string} username
     * @param {string} password
     * @param {string} amp_id
     * @returns {Promise<Amp0>}
     */
    static newTestnet(username, password, amp_id) {
        const ptr0 = passStringToWasm0(username, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(amp_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.amp0_newTestnet(ptr0, len0, ptr1, len1, ptr2, len2);
        return ret;
    }
    /**
     * Create a new AMP0 context for mainnet
     * @param {string} username
     * @param {string} password
     * @param {string} amp_id
     * @returns {Promise<Amp0>}
     */
    static newMainnet(username, password, amp_id) {
        const ptr0 = passStringToWasm0(username, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(amp_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.amp0_newMainnet(ptr0, len0, ptr1, len1, ptr2, len2);
        return ret;
    }
    /**
     * Index of the last returned address
     *
     * Use this and [`crate::EsploraClient::full_scan_to_index()`] to sync the `Wollet`
     * @returns {number}
     */
    lastIndex() {
        const ret = wasm.amp0_lastIndex(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * AMP ID
     * @returns {string}
     */
    ampId() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.amp0_ampId(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Get an address
     *
     * If `index` is None, a new address is returned.
     * @param {number | null} [index]
     * @returns {Promise<AddressResult>}
     */
    address(index) {
        const ret = wasm.amp0_address(this.__wbg_ptr, isLikeNone(index) ? 0x100000001 : (index) >>> 0);
        return ret;
    }
    /**
     * The LWK watch-only wallet corresponding to the AMP0 (sub)account.
     * @returns {Wollet}
     */
    wollet() {
        const ret = wasm.amp0_wollet(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Wollet.__wrap(ret[0]);
    }
    /**
     * Ask AMP0 server to cosign
     * @param {Amp0Pset} amp0pset
     * @returns {Promise<Transaction>}
     */
    sign(amp0pset) {
        _assertClass(amp0pset, Amp0Pset);
        const ret = wasm.amp0_sign(this.__wbg_ptr, amp0pset.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) Amp0.prototype[Symbol.dispose] = Amp0.prototype.free;

const Amp0ConnectedFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp0connected_free(ptr >>> 0, 1));
/**
 * Session connecting to AMP0
 */
export class Amp0Connected {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp0Connected.prototype);
        obj.__wbg_ptr = ptr;
        Amp0ConnectedFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp0ConnectedFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp0connected_free(ptr, 0);
    }
    /**
     * Connect and register to AMP0.
     *
     * Deprecated: use `connect()` instead.
     *
     * `wasm-bindgen` deprecated async constructors because they generate
     * invalid TypeScript declarations. See https://github.com/wasm-bindgen/wasm-bindgen/pull/4402.
     * @param {Network} network
     * @param {Amp0SignerData} signer_data
     */
    constructor(network, signer_data) {
        _assertClass(network, Network);
        _assertClass(signer_data, Amp0SignerData);
        const ret = wasm.amp0connected_new(network.__wbg_ptr, signer_data.__wbg_ptr);
        return ret;
    }
    /**
     * Connect and register to AMP0.
     * @param {Network} network
     * @param {Amp0SignerData} signer_data
     * @returns {Promise<Amp0Connected>}
     */
    static connect(network, signer_data) {
        _assertClass(network, Network);
        _assertClass(signer_data, Amp0SignerData);
        const ret = wasm.amp0connected_connect(network.__wbg_ptr, signer_data.__wbg_ptr);
        return ret;
    }
    /**
     * Obtain a login challenge
     *
     * This must be signed with [`amp0_sign_challenge()`].
     * @returns {Promise<string>}
     */
    getChallenge() {
        const ret = wasm.amp0connected_getChallenge(this.__wbg_ptr);
        return ret;
    }
    /**
     * Log in
     *
     * `sig` must be obtained from [`amp0_sign_challenge()`] called with the value returned
     * by [`Amp0Connected::get_challenge()`]
     * @param {string} sig
     * @returns {Promise<Amp0LoggedIn>}
     */
    login(sig) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(sig, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.amp0connected_login(ptr, ptr0, len0);
        return ret;
    }
}
if (Symbol.dispose) Amp0Connected.prototype[Symbol.dispose] = Amp0Connected.prototype.free;

const Amp0LoggedInFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp0loggedin_free(ptr >>> 0, 1));
/**
 * Session logged in AMP0
 */
export class Amp0LoggedIn {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp0LoggedIn.prototype);
        obj.__wbg_ptr = ptr;
        Amp0LoggedInFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp0LoggedInFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp0loggedin_free(ptr, 0);
    }
    /**
     * List of AMP IDs.
     * @returns {string[]}
     */
    getAmpIds() {
        const ret = wasm.amp0loggedin_getAmpIds(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Get the next account for AMP0 account creation
     *
     * This must be given to [`amp0_account_xpub()`] to obtain the xpub to pass to
     * [`Amp0LoggedIn::create_amp0_account()`]
     * @returns {number}
     */
    nextAccount() {
        const ret = wasm.amp0loggedin_nextAccount(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Create a new AMP0 account
     *
     * `account_xpub` must be obtained from [`amp0_account_xpub()`] called with the value obtained from
     * [`Amp0LoggedIn::next_account()`]
     * @param {number} pointer
     * @param {string} account_xpub
     * @returns {Promise<string>}
     */
    createAmp0Account(pointer, account_xpub) {
        const ptr0 = passStringToWasm0(account_xpub, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.amp0loggedin_createAmp0Account(this.__wbg_ptr, pointer, ptr0, len0);
        return ret;
    }
    /**
     * Create a new Watch-Only entry for this wallet
     * @param {string} username
     * @param {string} password
     * @returns {Promise<void>}
     */
    createWatchOnly(username, password) {
        const ptr0 = passStringToWasm0(username, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(password, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.amp0loggedin_createWatchOnly(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
}
if (Symbol.dispose) Amp0LoggedIn.prototype[Symbol.dispose] = Amp0LoggedIn.prototype.free;

const Amp0PsetFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp0pset_free(ptr >>> 0, 1));
/**
 * A PSET to use with AMP0
 *
 * When asking AMP0 to cosign, the caller must pass some extra data that does not belong to the
 * PSET. This struct holds and manage the necessary data.
 *
 * If you're not dealing with AMP0, do not use this struct.
 */
export class Amp0Pset {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp0Pset.prototype);
        obj.__wbg_ptr = ptr;
        Amp0PsetFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp0PsetFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp0pset_free(ptr, 0);
    }
    /**
     * Creates a `Amp0Pset`
     * @param {Pset} pset
     * @param {string[]} blinding_nonces
     */
    constructor(pset, blinding_nonces) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ptr1 = passArrayJsValueToWasm0(blinding_nonces, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.amp0pset_new(ptr0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        Amp0PsetFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Get the PSET
     * @returns {Pset}
     */
    pset() {
        const ret = wasm.amp0pset_pset(this.__wbg_ptr);
        return Pset.__wrap(ret);
    }
    /**
     * Get the blinding nonces
     * @returns {string[]}
     */
    blindingNonces() {
        const ret = wasm.amp0pset_blindingNonces(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) Amp0Pset.prototype[Symbol.dispose] = Amp0Pset.prototype.free;

const Amp0SignerDataFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp0signerdata_free(ptr >>> 0, 1));

export class Amp0SignerData {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp0SignerData.prototype);
        obj.__wbg_ptr = ptr;
        Amp0SignerDataFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp0SignerDataFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp0signerdata_free(ptr, 0);
    }
}
if (Symbol.dispose) Amp0SignerData.prototype[Symbol.dispose] = Amp0SignerData.prototype.free;

const Amp2Finalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp2_free(ptr >>> 0, 1));
/**
 * Context for actions interacting with Asset Management Platform version 2
 */
export class Amp2 {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp2.prototype);
        obj.__wbg_ptr = ptr;
        Amp2Finalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp2Finalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp2_free(ptr, 0);
    }
    /**
     * Create a new AMP2 client
     *
     *  * `server_key` - The keyorigin xpub of the AMP2 server key
     *  * `url` - The URL of the AMP2 server
     * @param {string} server_key
     * @param {string} url
     * @returns {Amp2}
     */
    static new(server_key, url) {
        const ptr0 = passStringToWasm0(server_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.amp2_new(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Amp2.__wrap(ret[0]);
    }
    /**
     * Create a new AMP2 client with the default url and server key for the testnet network.
     * @returns {Amp2}
     */
    static newTestnet() {
        const ret = wasm.amp2_newTestnet();
        return Amp2.__wrap(ret);
    }
    /**
     * Get an AMP2 wallet descriptor from the keyorigin xpub string obtained from a signer
     * @param {string} keyorigin_xpub
     * @param {string} descriptor_blinding_key
     * @returns {Amp2Descriptor}
     */
    descriptorFromStr(keyorigin_xpub, descriptor_blinding_key) {
        const ptr0 = passStringToWasm0(keyorigin_xpub, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(descriptor_blinding_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.amp2_descriptorFromStr(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Amp2Descriptor.__wrap(ret[0]);
    }
    /**
     * Register an AMP2 wallet with the AMP2 server
     * @param {Amp2Descriptor} desc
     * @returns {Promise<string>}
     */
    register(desc) {
        _assertClass(desc, Amp2Descriptor);
        const ret = wasm.amp2_register(this.__wbg_ptr, desc.__wbg_ptr);
        return ret;
    }
    /**
     * Ask the AMP2 server to cosign a PSET
     * @param {Pset} pset
     * @returns {Promise<Pset>}
     */
    cosign(pset) {
        _assertClass(pset, Pset);
        const ret = wasm.amp2_cosign(this.__wbg_ptr, pset.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) Amp2.prototype[Symbol.dispose] = Amp2.prototype.free;

const Amp2DescriptorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_amp2descriptor_free(ptr >>> 0, 1));
/**
 * An Asset Management Platform version 2 descriptor
 */
export class Amp2Descriptor {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Amp2Descriptor.prototype);
        obj.__wbg_ptr = ptr;
        Amp2DescriptorFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        Amp2DescriptorFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_amp2descriptor_free(ptr, 0);
    }
    /**
     * Return the descriptor as a `WolletDescriptor`
     * @returns {WolletDescriptor}
     */
    descriptor() {
        const ret = wasm.amp2descriptor_descriptor(this.__wbg_ptr);
        return WolletDescriptor.__wrap(ret);
    }
    /**
     * Return the string representation of the descriptor.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.amp2descriptor_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Create an `Amp2Descriptor` using any `WolletDescriptor`
     *
     * Warning: AMP2 server only supports a limited subset of descriptors.
     * To make sure this AMP2 descriptor can be used safely,
     * register this with AMP2 as soon as possible.
     * @param {WolletDescriptor} desc
     * @returns {Amp2Descriptor}
     */
    static newWithCustomDescriptor(desc) {
        _assertClass(desc, WolletDescriptor);
        const ret = wasm.amp2descriptor_newWithCustomDescriptor(desc.__wbg_ptr);
        return Amp2Descriptor.__wrap(ret);
    }
}
if (Symbol.dispose) Amp2Descriptor.prototype[Symbol.dispose] = Amp2Descriptor.prototype.free;

const AssetAmountFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_assetamount_free(ptr >>> 0, 1));
/**
 * An asset identifier and an amount in satoshi units
 */
export class AssetAmount {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AssetAmount.prototype);
        obj.__wbg_ptr = ptr;
        AssetAmountFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AssetAmountFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_assetamount_free(ptr, 0);
    }
    /**
     * @returns {bigint}
     */
    amount() {
        const ret = wasm.assetamount_amount(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {AssetId}
     */
    asset() {
        const ret = wasm.assetamount_asset(this.__wbg_ptr);
        return AssetId.__wrap(ret);
    }
}
if (Symbol.dispose) AssetAmount.prototype[Symbol.dispose] = AssetAmount.prototype.free;

const AssetBlindingFactorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_assetblindingfactor_free(ptr >>> 0, 1));
/**
 * A blinding factor for asset commitments.
 */
export class AssetBlindingFactor {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AssetBlindingFactor.prototype);
        obj.__wbg_ptr = ptr;
        AssetBlindingFactorFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AssetBlindingFactorFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_assetblindingfactor_free(ptr, 0);
    }
    /**
     * Creates an `AssetBlindingFactor` from a string.
     * @param {string} s
     * @returns {AssetBlindingFactor}
     */
    static fromString(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.assetblindingfactor_fromString(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetBlindingFactor.__wrap(ret[0]);
    }
    /**
     * Creates an `AssetBlindingFactor` from a byte slice.
     * @param {Uint8Array} bytes
     * @returns {AssetBlindingFactor}
     */
    static fromBytes(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.assetblindingfactor_fromBytes(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetBlindingFactor.__wrap(ret[0]);
    }
    /**
     * Returns a zero asset blinding factor.
     * @returns {AssetBlindingFactor}
     */
    static zero() {
        const ret = wasm.assetblindingfactor_zero();
        return AssetBlindingFactor.__wrap(ret);
    }
    /**
     * Returns the bytes (32 bytes) in little-endian byte order.
     *
     * This is the internal representation used by secp256k1. The byte order is
     * reversed compared to the hex string representation (which uses big-endian,
     * following Bitcoin display conventions).
     * @returns {Uint8Array}
     */
    toBytes() {
        const ret = wasm.assetblindingfactor_toBytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Returns string representation of the ABF
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.assetblindingfactor_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) AssetBlindingFactor.prototype[Symbol.dispose] = AssetBlindingFactor.prototype.free;

const AssetIdFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_assetid_free(ptr >>> 0, 1));
/**
 * A valid asset identifier.
 *
 * 32 bytes encoded as hex string.
 */
export class AssetId {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AssetId.prototype);
        obj.__wbg_ptr = ptr;
        AssetIdFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AssetIdFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_assetid_free(ptr, 0);
    }
    /**
     * Creates an `AssetId`
     *
     * Deprecated: use `from_string()` instead
     * @param {string} asset_id
     */
    constructor(asset_id) {
        const ptr0 = passStringToWasm0(asset_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.assetid_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        AssetIdFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Creates an `AssetId` from hex string
     * @param {string} s
     * @returns {AssetId}
     */
    static fromString(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.assetid_fromString(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetId.__wrap(ret[0]);
    }
    /**
     * Creates an `AssetId` from a bytes.
     * @param {Uint8Array} bytes
     * @returns {AssetId}
     */
    static fromBytes(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.assetid_fromBytes(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetId.__wrap(ret[0]);
    }
    /**
     * Returns the `AssetId` bytes in little-endian byte order.
     * @returns {Uint8Array}
     */
    toBytes() {
        const ret = wasm.assetid_toBytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Return the string representation of the asset identifier (64 hex characters).
     * This representation can be used to recreate the asset identifier via `fromString()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.assetid_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) AssetId.prototype[Symbol.dispose] = AssetId.prototype.free;

const AssetIdsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_assetids_free(ptr >>> 0, 1));
/**
 * An ordered collection of asset identifiers.
 */
export class AssetIds {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AssetIds.prototype);
        obj.__wbg_ptr = ptr;
        AssetIdsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AssetIdsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_assetids_free(ptr, 0);
    }
    /**
     * Return an empty list of asset identifiers.
     * @returns {AssetIds}
     */
    static empty() {
        const ret = wasm.assetids_empty();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetIds.__wrap(ret[0]);
    }
    /**
     * Return the string representation of this list of asset identifiers.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.assetids_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) AssetIds.prototype[Symbol.dispose] = AssetIds.prototype.free;

const AssetMetaFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_assetmeta_free(ptr >>> 0, 1));
/**
 * Data related to an asset in the registry:
 * - contract: the contract of the asset
 * - tx: the issuance transaction of the asset
 */
export class AssetMeta {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(AssetMeta.prototype);
        obj.__wbg_ptr = ptr;
        AssetMetaFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        AssetMetaFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_assetmeta_free(ptr, 0);
    }
    /**
     * Return the contract of the asset.
     * @returns {Contract}
     */
    contract() {
        const ret = wasm.assetmeta_contract(this.__wbg_ptr);
        return Contract.__wrap(ret);
    }
    /**
     * Return the issuance transaction of the asset.
     * @returns {Transaction}
     */
    tx() {
        const ret = wasm.assetmeta_tx(this.__wbg_ptr);
        return Transaction.__wrap(ret);
    }
}
if (Symbol.dispose) AssetMeta.prototype[Symbol.dispose] = AssetMeta.prototype.free;

const BalanceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_balance_free(ptr >>> 0, 1));
/**
 * A signed balance of assets, to represent a balance with negative values such
 * as the results of a transaction from the perspective of a wallet.
 */
export class Balance {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Balance.prototype);
        obj.__wbg_ptr = ptr;
        BalanceFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BalanceFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_balance_free(ptr, 0);
    }
    /**
     * Convert the balance to a JsValue for serialization
     *
     * Note: the amounts are strings since `JSON.stringify` cannot handle `BigInt`s.
     * Use `entries()` to get the raw data.
     * @returns {any}
     */
    toJSON() {
        const ret = wasm.balance_toJSON(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Returns the balance as a JavaScript `Map` of asset id to amount.
     * @returns {any}
     */
    entries() {
        const ret = wasm.balance_entries(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Return the string representation of the balance.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.balance_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Balance.prototype[Symbol.dispose] = Balance.prototype.free;

const BipFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_bip_free(ptr >>> 0, 1));
/**
 * The bip variant for a descriptor like specified in the bips (49, 84, 86, 87)
 */
export class Bip {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Bip.prototype);
        obj.__wbg_ptr = ptr;
        BipFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BipFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_bip_free(ptr, 0);
    }
    /**
     * Creates a bip49 variant
     * @returns {Bip}
     */
    static bip49() {
        const ret = wasm.bip_bip49();
        return Bip.__wrap(ret);
    }
    /**
     * Creates a bip84 variant
     * @returns {Bip}
     */
    static bip84() {
        const ret = wasm.bip_bip84();
        return Bip.__wrap(ret);
    }
    /**
     * Creates a bip87 variant
     * @returns {Bip}
     */
    static bip87() {
        const ret = wasm.bip_bip87();
        return Bip.__wrap(ret);
    }
    /**
     * Creates a bip86 variant
     * @returns {Bip}
     */
    static bip86() {
        const ret = wasm.bip_bip86();
        return Bip.__wrap(ret);
    }
    /**
     * Return the string representation of the bip variant, such as "bip49", "bip84" or "bip87"
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.bip_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Bip.prototype[Symbol.dispose] = Bip.prototype.free;

const BoltzSessionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_boltzsession_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_boltz::BoltzSession`]
 */
export class BoltzSession {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(BoltzSession.prototype);
        obj.__wbg_ptr = ptr;
        BoltzSessionFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BoltzSessionFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_boltzsession_free(ptr, 0);
    }
    /**
     * Get the rescue file
     * @returns {string}
     */
    rescueFile() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.boltzsession_rescueFile(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Prepare a lightning invoice payment
     * @param {LightningPayment} lightning_payment
     * @param {Address} refund_address
     * @returns {Promise<PreparePayResponse>}
     */
    preparePay(lightning_payment, refund_address) {
        _assertClass(lightning_payment, LightningPayment);
        _assertClass(refund_address, Address);
        const ret = wasm.boltzsession_preparePay(this.__wbg_ptr, lightning_payment.__wbg_ptr, refund_address.__wbg_ptr);
        return ret;
    }
    /**
     * Fetch a BOLT12 invoice without creating or starting a swap
     * @param {LightningPayment} lightning_payment
     * @returns {Promise<Invoice>}
     */
    fetchBolt12Invoice(lightning_payment) {
        _assertClass(lightning_payment, LightningPayment);
        const ret = wasm.boltzsession_fetchBolt12Invoice(this.__wbg_ptr, lightning_payment.__wbg_ptr);
        return ret;
    }
    /**
     * Create a lightning invoice for receiving payment
     * @param {bigint} amount
     * @param {string | null | undefined} description
     * @param {Address} claim_address
     * @returns {Promise<InvoiceResponse>}
     */
    invoice(amount, description, claim_address) {
        var ptr0 = isLikeNone(description) ? 0 : passStringToWasm0(description, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        _assertClass(claim_address, Address);
        const ret = wasm.boltzsession_invoice(this.__wbg_ptr, amount, ptr0, len0, claim_address.__wbg_ptr);
        return ret;
    }
    /**
     * Restore a swap from its serialized data
     * @param {string} data
     * @returns {Promise<PreparePayResponse>}
     */
    restorePreparePay(data) {
        const ptr0 = passStringToWasm0(data, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.boltzsession_restorePreparePay(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Restore a swap from its serialized data
     * @param {string} data
     * @returns {Promise<InvoiceResponse>}
     */
    restoreInvoice(data) {
        const ptr0 = passStringToWasm0(data, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.boltzsession_restoreInvoice(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
}
if (Symbol.dispose) BoltzSession.prototype[Symbol.dispose] = BoltzSession.prototype.free;

const BoltzSessionBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_boltzsessionbuilder_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_boltz::BoltzSessionBuilder`]
 */
export class BoltzSessionBuilder {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(BoltzSessionBuilder.prototype);
        obj.__wbg_ptr = ptr;
        BoltzSessionBuilderFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BoltzSessionBuilderFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_boltzsessionbuilder_free(ptr, 0);
    }
    /**
     * Create a new BoltzSessionBuilder with the given network
     *
     * This creates a builder with default Esplora client for the network.
     * @param {Network} network
     * @param {EsploraClient} esplora_client
     */
    constructor(network, esplora_client) {
        _assertClass(network, Network);
        _assertClass(esplora_client, EsploraClient);
        const ret = wasm.boltzsessionbuilder_new(network.__wbg_ptr, esplora_client.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        BoltzSessionBuilderFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Set the timeout for creating swaps
     *
     * If not set, the default timeout of 10 seconds is used.
     * @param {bigint} timeout_seconds
     * @returns {BoltzSessionBuilder}
     */
    createSwapTimeout(timeout_seconds) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_createSwapTimeout(ptr, timeout_seconds);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the timeout for the advance call
     *
     * If not set, the default timeout of 3 minutes is used.
     * @param {bigint} timeout_seconds
     * @returns {BoltzSessionBuilder}
     */
    timeoutAdvance(timeout_seconds) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_timeoutAdvance(ptr, timeout_seconds);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the mnemonic for deriving swap keys
     *
     * If not set, a new random mnemonic will be generated.
     * @param {Mnemonic} mnemonic
     * @returns {BoltzSessionBuilder}
     */
    mnemonic(mnemonic) {
        const ptr = this.__destroy_into_raw();
        _assertClass(mnemonic, Mnemonic);
        const ret = wasm.boltzsessionbuilder_mnemonic(ptr, mnemonic.__wbg_ptr);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the polling flag
     *
     * If true, the advance call will not await on the websocket connection returning immediately
     * even if there is no update, thus requiring the caller to poll for updates.
     *
     * If true, the timeout_advance will be ignored even if set.
     * @param {boolean} polling
     * @returns {BoltzSessionBuilder}
     */
    polling(polling) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_polling(ptr, polling);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the next index to use for deriving keypairs
     *
     * Avoid a call to the boltz API to recover this information.
     *
     * When the mnemonic is not set, this is ignored.
     * @param {number} next_index_to_use
     * @returns {BoltzSessionBuilder}
     */
    nextIndexToUse(next_index_to_use) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_nextIndexToUse(ptr, next_index_to_use);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the referral id for the BoltzSession
     * @param {string} referral_id
     * @returns {BoltzSessionBuilder}
     */
    referralId(referral_id) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(referral_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.boltzsessionbuilder_referralId(ptr, ptr0, len0);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the Boltz API base URL
     *
     * The caller is responsible for ensuring the provider behind this URL matches the session
     * network passed to the builder.
     *
     * If this is used together with a persistent store on the Rust side, the caller must use a
     * different store per provider. Persisted swap data is not namespaced by provider, so
     * reusing the same store across different `apiUrl` values can mix swaps from different
     * providers.
     * @param {string} api_url
     * @returns {BoltzSessionBuilder}
     */
    apiUrl(api_url) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(api_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.boltzsessionbuilder_apiUrl(ptr, ptr0, len0);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Set the url of the bitcoin electrum client
     * @param {string} bitcoin_electrum_client
     * @returns {BoltzSessionBuilder}
     */
    bitcoinElectrumClient(bitcoin_electrum_client) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(bitcoin_electrum_client, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.boltzsessionbuilder_bitcoinElectrumClient(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return BoltzSessionBuilder.__wrap(ret[0]);
    }
    /**
     * Set the random preimages flag
     *
     * The default is false, the preimages will be deterministic and the rescue file will be
     * compatible with the Boltz web app.
     * If true, the preimages will be random potentially allowing concurrent sessions with the same
     * mnemonic, but completing the swap will be possible only with the preimage data. For example
     * the boltz web app will be able only to refund the swap, not to bring it to completion.
     * If true, when serializing the swap data, the preimage will be saved in the data.
     * @param {boolean} random_preimages
     * @returns {BoltzSessionBuilder}
     */
    randomPreimages(random_preimages) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_randomPreimages(ptr, random_preimages);
        return BoltzSessionBuilder.__wrap(ret);
    }
    /**
     * Build the BoltzSession
     * @returns {Promise<BoltzSession>}
     */
    build() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.boltzsessionbuilder_build(ptr);
        return ret;
    }
}
if (Symbol.dispose) BoltzSessionBuilder.prototype[Symbol.dispose] = BoltzSessionBuilder.prototype.free;

const ContractFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_contract_free(ptr >>> 0, 1));
/**
 * A contract defining metadata of an asset such the name and the ticker
 */
export class Contract {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Contract.prototype);
        obj.__wbg_ptr = ptr;
        ContractFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ContractFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_contract_free(ptr, 0);
    }
    /**
     * Creates a `Contract`
     * @param {string} domain
     * @param {string} issuer_pubkey
     * @param {string} name
     * @param {number} precision
     * @param {string} ticker
     * @param {number} version
     */
    constructor(domain, issuer_pubkey, name, precision, ticker, version) {
        const ptr0 = passStringToWasm0(domain, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(issuer_pubkey, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(ticker, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.contract_new(ptr0, len0, ptr1, len1, ptr2, len2, precision, ptr3, len3, version);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        ContractFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return the string representation of the contract.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.contract_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the domain of the issuer of the contract.
     * @returns {string}
     */
    domain() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.contract_domain(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Make a copy of the contract.
     *
     * This is needed to pass it to a function that requires a `Contract` (without borrowing)
     * but you need the same contract after that call.
     * @returns {Contract}
     */
    clone() {
        const ret = wasm.contract_clone(this.__wbg_ptr);
        return Contract.__wrap(ret);
    }
}
if (Symbol.dispose) Contract.prototype[Symbol.dispose] = Contract.prototype.free;

const CurrencyCodeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_currencycode_free(ptr >>> 0, 1));

export class CurrencyCode {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(CurrencyCode.prototype);
        obj.__wbg_ptr = ptr;
        CurrencyCodeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        CurrencyCodeFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_currencycode_free(ptr, 0);
    }
    /**
     * @param {string} code
     */
    constructor(code) {
        const ptr0 = passStringToWasm0(code, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.currencycode_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        CurrencyCodeFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {string}
     */
    name() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.currencycode_name(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    alpha3() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.currencycode_alpha3(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    exp() {
        const ret = wasm.currencycode_exp(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) CurrencyCode.prototype[Symbol.dispose] = CurrencyCode.prototype.free;

const EsploraClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_esploraclient_free(ptr >>> 0, 1));
/**
 * A blockchain backend implementation based on the
 * [esplora HTTP API](https://github.com/blockstream/esplora/blob/master/API.md).
 * But can also use the [waterfalls](https://github.com/RCasatta/waterfalls)
 * endpoint to speed up the scan if supported by the server.
 */
export class EsploraClient {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(EsploraClient.prototype);
        obj.__wbg_ptr = ptr;
        EsploraClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        EsploraClientFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_esploraclient_free(ptr, 0);
    }
    /**
     * Creates an Esplora client with the given options
     * @param {Network} network
     * @param {string} url
     * @param {boolean} waterfalls
     * @param {number} concurrency
     * @param {boolean} utxo_only
     */
    constructor(network, url, waterfalls, concurrency, utxo_only) {
        _assertClass(network, Network);
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.esploraclient_new(network.__wbg_ptr, ptr0, len0, waterfalls, concurrency, utxo_only);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        EsploraClientFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Scan the blockchain for the scripts generated by a watch-only wallet
     *
     * This method scans both external and internal address chains, stopping after finding
     * 20 consecutive unused addresses (the gap limit) as recommended by
     * [BIP44](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki#address-gap-limit).
     *
     * Returns `Some(Update)` if any changes were found during scanning, or `None` if no changes
     * were detected.
     *
     * To scan beyond the gap limit use `full_scan_to_index()` instead.
     * @param {Wollet} wollet
     * @returns {Promise<Update | undefined>}
     */
    fullScan(wollet) {
        _assertClass(wollet, Wollet);
        const ret = wasm.esploraclient_fullScan(this.__wbg_ptr, wollet.__wbg_ptr);
        return ret;
    }
    /**
     * Scan the blockchain for the scripts generated by a watch-only wallet up to a specified derivation index
     *
     * While `full_scan()` stops after finding 20 consecutive unused addresses (the gap limit),
     * this method will scan at least up to the given derivation index. This is useful to prevent
     * missing funds in cases where outputs exist beyond the gap limit.
     *
     * Will scan both external and internal address chains up to the given index for maximum safety,
     * even though internal addresses may not need such deep scanning.
     *
     * If transactions are found beyond the gap limit during this scan, subsequent calls to
     * `full_scan()` will automatically scan up to the highest used index, preventing any
     * previously-found transactions from being missed.
     *
     * See `full_scan_to_index()` for a blocking version of this method.
     * @param {Wollet} wollet
     * @param {number} index
     * @returns {Promise<Update | undefined>}
     */
    fullScanToIndex(wollet, index) {
        _assertClass(wollet, Wollet);
        const ret = wasm.esploraclient_fullScanToIndex(this.__wbg_ptr, wollet.__wbg_ptr, index);
        return ret;
    }
    /**
     * Broadcast a transaction to the network so that a miner can include it in a block.
     * @param {Transaction} tx
     * @returns {Promise<Txid>}
     */
    broadcastTx(tx) {
        _assertClass(tx, Transaction);
        const ret = wasm.esploraclient_broadcastTx(this.__wbg_ptr, tx.__wbg_ptr);
        return ret;
    }
    /**
     * Broadcast a PSET by extracting the transaction from the PSET and broadcasting it.
     * @param {Pset} pset
     * @returns {Promise<Txid>}
     */
    broadcast(pset) {
        _assertClass(pset, Pset);
        const ret = wasm.esploraclient_broadcast(this.__wbg_ptr, pset.__wbg_ptr);
        return ret;
    }
    /**
     * Set the waterfalls server recipient key. This is used to encrypt the descriptor when calling the waterfalls endpoint.
     * @param {string} recipient
     * @returns {Promise<void>}
     */
    setWaterfallsServerRecipient(recipient) {
        const ptr0 = passStringToWasm0(recipient, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.esploraclient_setWaterfallsServerRecipient(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Return the descriptor string to use with Waterfalls descriptor endpoints.
     *
     * This is a temporary API exposed to let callers use Waterfalls subscription
     * endpoints directly. It may be removed once subscription support is
     * implemented in LWK.
     *
     * The returned descriptor has key origin information stripped and is encrypted
     * for the Waterfalls server recipient unless descriptor encryption has been
     * explicitly disabled on this client.
     * @param {WolletDescriptor} descriptor
     * @returns {Promise<string>}
     */
    waterfallsDescriptor(descriptor) {
        _assertClass(descriptor, WolletDescriptor);
        const ret = wasm.esploraclient_waterfallsDescriptor(this.__wbg_ptr, descriptor.__wbg_ptr);
        return ret;
    }
    /**
     * Query the last used derivation index for a wallet's descriptor from the waterfalls server.
     *
     * This method queries the waterfalls `/v1/last_used_index` endpoint to get the last used
     * derivation index for both external and internal chains of the wallet's descriptor.
     *
     * Returns `LastUsedIndexResponse` containing the last used indexes and the tip block hash.
     *
     * # Errors
     *
     * Returns an error if this client was not configured with waterfalls support,
     * if the descriptor does not contain a wildcard,
     * or if the descriptor uses ELIP151 blinding.
     * @param {WolletDescriptor} descriptor
     * @returns {Promise<LastUsedIndexResponse>}
     */
    lastUsedIndex(descriptor) {
        _assertClass(descriptor, WolletDescriptor);
        const ret = wasm.esploraclient_lastUsedIndex(this.__wbg_ptr, descriptor.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) EsploraClient.prototype[Symbol.dispose] = EsploraClient.prototype.free;

const ExchangeRatesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_exchangerates_free(ptr >>> 0, 1));
/**
 * Multiple exchange rates against BTC provided from various sources
 */
export class ExchangeRates {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(ExchangeRates.prototype);
        obj.__wbg_ptr = ptr;
        ExchangeRatesFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ExchangeRatesFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_exchangerates_free(ptr, 0);
    }
    /**
     * Get the median exchange rate
     * @returns {number}
     */
    median() {
        const ret = wasm.exchangerates_median(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get the individual exchange rates as a JSON array
     *
     * Each rate contains: rate, currency, source, and timestamp
     * @returns {any}
     */
    results() {
        const ret = wasm.exchangerates_results(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Get the number of sources that provided rates
     * @returns {number}
     */
    resultsCount() {
        const ret = wasm.exchangerates_resultsCount(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Serialize the entire response to JSON string
     * @returns {string}
     */
    serialize() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.exchangerates_serialize(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) ExchangeRates.prototype[Symbol.dispose] = ExchangeRates.prototype.free;

const ExternalUtxoFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_externalutxo_free(ptr >>> 0, 1));
/**
 * An external UTXO, owned by another wallet.
 */
export class ExternalUtxo {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ExternalUtxoFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_externalutxo_free(ptr, 0);
    }
    /**
     * Construct an ExternalUtxo
     * @param {number} vout
     * @param {Transaction} tx
     * @param {TxOutSecrets} unblinded
     * @param {number} max_weight_to_satisfy
     * @param {boolean} is_segwit
     */
    constructor(vout, tx, unblinded, max_weight_to_satisfy, is_segwit) {
        _assertClass(tx, Transaction);
        _assertClass(unblinded, TxOutSecrets);
        const ret = wasm.externalutxo_new(vout, tx.__wbg_ptr, unblinded.__wbg_ptr, max_weight_to_satisfy, is_segwit);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        ExternalUtxoFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
}
if (Symbol.dispose) ExternalUtxo.prototype[Symbol.dispose] = ExternalUtxo.prototype.free;

const FeesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fees_free(ptr >>> 0, 1));
/**
 * The total fee paid by the transaction for each asset type.
 */
export class Fees {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Fees.prototype);
        obj.__wbg_ptr = ptr;
        FeesFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FeesFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fees_free(ptr, 0);
    }
    /**
     * Returns the fees as a JavaScript `Map` of asset id to amount.
     * @returns {any}
     */
    entries() {
        const ret = wasm.fees_entries(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Note: the amounts are strings since `JSON.stringify` cannot handle `BigInt`s.
     * Use `entries()` to get the raw data.
     * @returns {any}
     */
    toJSON() {
        const ret = wasm.fees_toJSON(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Return the string representation of the fee.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.fees_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Fees.prototype[Symbol.dispose] = Fees.prototype.free;

const InvoiceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_invoice_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_boltz::Invoice`]
 */
export class Invoice {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Invoice.prototype);
        obj.__wbg_ptr = ptr;
        InvoiceFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        InvoiceFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_invoice_free(ptr, 0);
    }
    /**
     * Return a string representation of the invoice
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.invoice_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the invoice amount in whole satoshis
     * @returns {bigint}
     */
    amountSats() {
        const ret = wasm.invoice_amountSats(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return BigInt.asUintN(64, ret[0]);
    }
    /**
     * Return true if this is a BOLT11 invoice
     * @returns {boolean}
     */
    isBolt11() {
        const ret = wasm.invoice_isBolt11(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return true if this is a BOLT12 invoice
     * @returns {boolean}
     */
    isBolt12() {
        const ret = wasm.invoice_isBolt12(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return the BOLT11 invoice string if this is a BOLT11 invoice
     * @returns {string | undefined}
     */
    bolt11Invoice() {
        const ret = wasm.invoice_bolt11Invoice(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
    /**
     * Return the BOLT12 invoice string if this is a BOLT12 invoice
     * @returns {string | undefined}
     */
    bolt12Invoice() {
        const ret = wasm.invoice_bolt12Invoice(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
}
if (Symbol.dispose) Invoice.prototype[Symbol.dispose] = Invoice.prototype.free;

const InvoiceResponseFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_invoiceresponse_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_boltz::InvoiceResponse`]
 */
export class InvoiceResponse {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(InvoiceResponse.prototype);
        obj.__wbg_ptr = ptr;
        InvoiceResponseFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        InvoiceResponseFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_invoiceresponse_free(ptr, 0);
    }
    /**
     * Serialize the response to JSON string for JS interop
     * @returns {string}
     */
    serialize() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.invoiceresponse_serialize(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Return the bolt11 invoice string
     * @returns {string}
     */
    bolt11Invoice() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.invoiceresponse_bolt11Invoice(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    swapId() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.invoiceresponse_swapId(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The fee of the swap provider
     *
     * It is equal to the amount of the invoice minus the amount of the onchain transaction.
     * Does not include the fee of the onchain transaction.
     * @returns {bigint | undefined}
     */
    fee() {
        const ret = wasm.invoiceresponse_fee(this.__wbg_ptr);
        return ret[0] === 0 ? undefined : BigInt.asUintN(64, ret[1]);
    }
    /**
     * Complete the payment by advancing through the swap states until completion or failure
     * Consumes self as the inner method does
     * @returns {Promise<boolean>}
     */
    completePay() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.invoiceresponse_completePay(ptr);
        return ret;
    }
}
if (Symbol.dispose) InvoiceResponse.prototype[Symbol.dispose] = InvoiceResponse.prototype.free;

const IssuanceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_issuance_free(ptr >>> 0, 1));
/**
 * The details of an issuance or reissuance.
 */
export class Issuance {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Issuance.prototype);
        obj.__wbg_ptr = ptr;
        IssuanceFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IssuanceFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_issuance_free(ptr, 0);
    }
    /**
     * Return the asset id or None if it's a null issuance
     * @returns {AssetId | undefined}
     */
    asset() {
        const ret = wasm.issuance_asset(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * Return the token id or None if it's a null issuance
     * @returns {AssetId | undefined}
     */
    token() {
        const ret = wasm.issuance_token(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * Return the previous output index or None if it's a null issuance
     * @returns {number | undefined}
     */
    prevVout() {
        const ret = wasm.issuance_prevVout(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Return the previous transaction id or None if it's a null issuance
     * @returns {Txid | undefined}
     */
    prevTxid() {
        const ret = wasm.issuance_prevTxid(this.__wbg_ptr);
        return ret === 0 ? undefined : Txid.__wrap(ret);
    }
    /**
     * Return true if this is effectively an issuance
     * @returns {boolean}
     */
    isIssuance() {
        const ret = wasm.issuance_isIssuance(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return true if this is effectively a reissuance
     * @returns {boolean}
     */
    isReissuance() {
        const ret = wasm.issuance_isReissuance(this.__wbg_ptr);
        return ret !== 0;
    }
}
if (Symbol.dispose) Issuance.prototype[Symbol.dispose] = Issuance.prototype.free;

const JadeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jade_free(ptr >>> 0, 1));
/**
 * A Jade hardware wallet usable via Web Serial
 */
export class Jade {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Jade.prototype);
        obj.__wbg_ptr = ptr;
        JadeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JadeFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jade_free(ptr, 0);
    }
    /**
     * Creates a Jade from Web Serial for the given network
     *
     * When filter is true, it will filter available serial with Blockstream released chips, use
     * false if you don't see your DYI jade
     *
     * Deprecated: use `fromSerial()` instead.
     *
     * `wasm-bindgen` deprecated async constructors because they generate
     * invalid TypeScript declarations. See https://github.com/wasm-bindgen/wasm-bindgen/pull/4402.
     * @param {Network} network
     * @param {boolean} filter
     */
    constructor(network, filter) {
        _assertClass(network, Network);
        const ret = wasm.jade_new(network.__wbg_ptr, filter);
        return ret;
    }
    /**
     * Creates a Jade from Web Serial for the given network.
     *
     * When filter is true, it will filter available serial with Blockstream released chips, use
     * false if you don't see your DYI jade
     * @param {Network} network
     * @param {boolean} filter
     * @returns {Promise<Jade>}
     */
    static fromSerial(network, filter) {
        _assertClass(network, Network);
        const ret = wasm.jade_fromSerial(network.__wbg_ptr, filter);
        return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    getVersion() {
        const ret = wasm.jade_getVersion(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<Xpub>}
     */
    getMasterXpub() {
        const ret = wasm.jade_getMasterXpub(this.__wbg_ptr);
        return ret;
    }
    /**
     * Return a single sig address with the given `variant` and `path` derivation
     * @param {Singlesig} variant
     * @param {Uint32Array} path
     * @returns {Promise<string>}
     */
    getReceiveAddressSingle(variant, path) {
        _assertClass(variant, Singlesig);
        var ptr0 = variant.__destroy_into_raw();
        const ptr1 = passArray32ToWasm0(path, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.jade_getReceiveAddressSingle(this.__wbg_ptr, ptr0, ptr1, len1);
        return ret;
    }
    /**
     * Return a multisig address of a registered `multisig_name` wallet
     *
     * This method accept `path` and `path_n` in place of a single `Vec<Vec<u32>>` because the
     * latter is not supported by wasm_bindgen (and neither `(u32, Vec<u32>)`). `path` and `path_n`
     * are converted internally to a `Vec<Vec<u32>>` with the caveat all the paths are the same,
     * which is almost always the case.
     * @param {string} multisig_name
     * @param {Uint32Array} path
     * @returns {Promise<string>}
     */
    getReceiveAddressMulti(multisig_name, path) {
        const ptr0 = passStringToWasm0(multisig_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray32ToWasm0(path, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.jade_getReceiveAddressMulti(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * Sign and consume the given PSET, returning the signed one
     * @param {Pset} pset
     * @returns {Promise<Pset>}
     */
    sign(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.jade_sign(this.__wbg_ptr, ptr0);
        return ret;
    }
    /**
     * @returns {Promise<WolletDescriptor>}
     */
    wpkh() {
        const ret = wasm.jade_wpkh(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<WolletDescriptor>}
     */
    shWpkh() {
        const ret = wasm.jade_shWpkh(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} name
     * @returns {Promise<WolletDescriptor>}
     */
    multi(name) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jade_multi(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    getRegisteredMultisigs() {
        const ret = wasm.jade_getRegisteredMultisigs(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {Bip} bip
     * @returns {Promise<string>}
     */
    keyoriginXpub(bip) {
        _assertClass(bip, Bip);
        const ret = wasm.jade_keyoriginXpub(this.__wbg_ptr, bip.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} name
     * @param {WolletDescriptor} desc
     * @returns {Promise<boolean>}
     */
    registerDescriptor(name, desc) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(desc, WolletDescriptor);
        const ret = wasm.jade_registerDescriptor(this.__wbg_ptr, ptr0, len0, desc.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) Jade.prototype[Symbol.dispose] = Jade.prototype.free;

const JadeWebSocketFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jadewebsocket_free(ptr >>> 0, 1));
/**
 * WebSocket-based `Jade` useful for testing in the browser with the Jade emulator.
 */
export class JadeWebSocket {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(JadeWebSocket.prototype);
        obj.__wbg_ptr = ptr;
        JadeWebSocketFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JadeWebSocketFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jadewebsocket_free(ptr, 0);
    }
    /**
     * Creates a Jade from WebSocket for the given network
     *
     * The url should point to your WebSocket bridge that connects to the Docker Jade emulator
     *
     * Deprecated: use `fromWebSocket()` instead.
     *
     * `wasm-bindgen` deprecated async constructors because they generate
     * invalid TypeScript declarations. See https://github.com/wasm-bindgen/wasm-bindgen/pull/4402.
     * @param {Network} network
     * @param {string} url
     */
    constructor(network, url) {
        _assertClass(network, Network);
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jadewebsocket_new(network.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Creates a Jade from WebSocket for the given network
     *
     * The url should point to your WebSocket bridge that connects to the Docker Jade emulator
     * @param {Network} network
     * @param {string} url
     * @returns {Promise<JadeWebSocket>}
     */
    static fromWebSocket(network, url) {
        _assertClass(network, Network);
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jadewebsocket_fromWebSocket(network.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    getVersion() {
        const ret = wasm.jadewebsocket_getVersion(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<Xpub>}
     */
    getMasterXpub() {
        const ret = wasm.jadewebsocket_getMasterXpub(this.__wbg_ptr);
        return ret;
    }
    /**
     * Return a single sig address with the given `variant` and `path` derivation
     * @param {Singlesig} variant
     * @param {Uint32Array} path
     * @returns {Promise<string>}
     */
    getReceiveAddressSingle(variant, path) {
        _assertClass(variant, Singlesig);
        var ptr0 = variant.__destroy_into_raw();
        const ptr1 = passArray32ToWasm0(path, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.jadewebsocket_getReceiveAddressSingle(this.__wbg_ptr, ptr0, ptr1, len1);
        return ret;
    }
    /**
     * Return a multisig address of a registered `multisig_name` wallet
     *
     * This method accept `path` and `path_n` in place of a single `Vec<Vec<u32>>` because the
     * latter is not supported by wasm_bindgen (and neither `(u32, Vec<u32>)`). `path` and `path_n`
     * are converted internally to a `Vec<Vec<u32>>` with the caveat all the paths are the same,
     * which is almost always the case.
     * @param {string} multisig_name
     * @param {Uint32Array} path
     * @returns {Promise<string>}
     */
    getReceiveAddressMulti(multisig_name, path) {
        const ptr0 = passStringToWasm0(multisig_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray32ToWasm0(path, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.jadewebsocket_getReceiveAddressMulti(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret;
    }
    /**
     * Sign and consume the given PSET, returning the signed one
     * @param {Pset} pset
     * @returns {Promise<Pset>}
     */
    sign(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.jadewebsocket_sign(this.__wbg_ptr, ptr0);
        return ret;
    }
    /**
     * @returns {Promise<WolletDescriptor>}
     */
    wpkh() {
        const ret = wasm.jadewebsocket_wpkh(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<WolletDescriptor>}
     */
    shWpkh() {
        const ret = wasm.jadewebsocket_shWpkh(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} name
     * @returns {Promise<WolletDescriptor>}
     */
    multi(name) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jadewebsocket_multi(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * @returns {Promise<any>}
     */
    getRegisteredMultisigs() {
        const ret = wasm.jadewebsocket_getRegisteredMultisigs(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {Bip} bip
     * @returns {Promise<string>}
     */
    keyoriginXpub(bip) {
        _assertClass(bip, Bip);
        const ret = wasm.jadewebsocket_keyoriginXpub(this.__wbg_ptr, bip.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} name
     * @param {WolletDescriptor} desc
     * @returns {Promise<boolean>}
     */
    registerDescriptor(name, desc) {
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(desc, WolletDescriptor);
        const ret = wasm.jadewebsocket_registerDescriptor(this.__wbg_ptr, ptr0, len0, desc.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) JadeWebSocket.prototype[Symbol.dispose] = JadeWebSocket.prototype.free;

const JsStoreLinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jsstorelink_free(ptr >>> 0, 1));
/**
 * A bridge that connects a [`JsStorage`] to [`lwk_common::Store`].
 */
export class JsStoreLink {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JsStoreLinkFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jsstorelink_free(ptr, 0);
    }
    /**
     * Create a new `JsStoreLink` from a JavaScript storage object.
     *
     * The JS object must have `get(key)`, `put(key, value)`, `remove(key)`,
     * and `isPersisted()` methods.
     * @param {any} storage
     */
    constructor(storage) {
        const ret = wasm.jsstorelink_new(storage);
        this.__wbg_ptr = ret >>> 0;
        JsStoreLinkFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
}
if (Symbol.dispose) JsStoreLink.prototype[Symbol.dispose] = JsStoreLink.prototype.free;

const JsTestStoreFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_jsteststore_free(ptr >>> 0, 1));
/**
 * Test helper to verify Rust can read/write through a JS store.
 */
export class JsTestStore {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        JsTestStoreFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_jsteststore_free(ptr, 0);
    }
    /**
     * Create a new test helper wrapping the given JS storage.
     * @param {any} storage
     */
    constructor(storage) {
        const ret = wasm.jsteststore_new(storage);
        this.__wbg_ptr = ret >>> 0;
        JsTestStoreFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Write a key-value pair to the store.
     * @param {string} key
     * @param {Uint8Array} value
     */
    write(key, value) {
        const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(value, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.jsteststore_write(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Read a value from the store.
     * @param {string} key
     * @returns {Uint8Array | undefined}
     */
    read(key) {
        const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jsteststore_read(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        let v2;
        if (ret[0] !== 0) {
            v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v2;
    }
    /**
     * Remove a key from the store.
     * @param {string} key
     */
    remove(key) {
        const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.jsteststore_remove(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) JsTestStore.prototype[Symbol.dispose] = JsTestStore.prototype.free;

const LastUsedIndexResponseFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_lastusedindexresponse_free(ptr >>> 0, 1));
/**
 * Response from the last_used_index endpoint
 *
 * Returns the highest derivation index that has been used (has transaction history)
 * for both external and internal chains. This is useful for quickly determining
 * the next unused address without downloading full transaction history.
 */
export class LastUsedIndexResponse {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(LastUsedIndexResponse.prototype);
        obj.__wbg_ptr = ptr;
        LastUsedIndexResponseFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        LastUsedIndexResponseFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_lastusedindexresponse_free(ptr, 0);
    }
    /**
     * Last used index on the external (receive) chain, or undefined if no addresses have been used.
     * @returns {number | undefined}
     */
    get external() {
        const ret = wasm.lastusedindexresponse_external(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Last used index on the internal (change) chain, or undefined if no addresses have been used.
     * @returns {number | undefined}
     */
    get internal() {
        const ret = wasm.lastusedindexresponse_internal(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Current blockchain tip hash for reference.
     * @returns {string | undefined}
     */
    get tip() {
        const ret = wasm.lastusedindexresponse_tip(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
}
if (Symbol.dispose) LastUsedIndexResponse.prototype[Symbol.dispose] = LastUsedIndexResponse.prototype.free;

const LedgerWebFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_ledgerweb_free(ptr >>> 0, 1));

export class LedgerWeb {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        LedgerWebFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_ledgerweb_free(ptr, 0);
    }
    /**
     * hid_device must be already opened
     * @param {HIDDevice} hid_device
     * @param {Network} network
     */
    constructor(hid_device, network) {
        _assertClass(network, Network);
        const ret = wasm.ledgerweb_new(hid_device, network.__wbg_ptr);
        this.__wbg_ptr = ret >>> 0;
        LedgerWebFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {Promise<string>}
     */
    getVersion() {
        const ret = wasm.ledgerweb_getVersion(this.__wbg_ptr);
        return ret;
    }
    /**
     * @param {string} path
     * @returns {Promise<string>}
     */
    deriveXpub(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.ledgerweb_deriveXpub(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * @returns {Promise<string>}
     */
    slip77MasterBlindingKey() {
        const ret = wasm.ledgerweb_slip77MasterBlindingKey(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {Promise<string>}
     */
    fingerprint() {
        const ret = wasm.ledgerweb_fingerprint(this.__wbg_ptr);
        return ret;
    }
    /**
     * TODO Should use Signer::wpkh_slip77_descriptor
     * @returns {Promise<WolletDescriptor>}
     */
    wpkhSlip77Descriptor() {
        const ret = wasm.ledgerweb_wpkhSlip77Descriptor(this.__wbg_ptr);
        return ret;
    }
    /**
     * Sign and consume the given PSET, returning the signed one
     * @param {Pset} pset
     * @returns {Promise<Pset>}
     */
    sign(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.ledgerweb_sign(this.__wbg_ptr, ptr0);
        return ret;
    }
    /**
     * Return a single sig address with the given `variant` and `index`
     * @param {number} index
     * @returns {Promise<string>}
     */
    getReceiveAddressSingle(index) {
        const ret = wasm.ledgerweb_getReceiveAddressSingle(this.__wbg_ptr, index);
        return ret;
    }
}
if (Symbol.dispose) LedgerWeb.prototype[Symbol.dispose] = LedgerWeb.prototype.free;

const LightningPaymentFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_lightningpayment_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_boltz::LightningPayment`]
 */
export class LightningPayment {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        LightningPaymentFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_lightningpayment_free(ptr, 0);
    }
    /**
     * Create a LightningPayment from a bolt11 invoice string or a bolt12 offer
     * @param {string} invoice
     */
    constructor(invoice) {
        const ptr0 = passStringToWasm0(invoice, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.lightningpayment_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        LightningPaymentFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return a string representation of the LightningPayment
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.lightningpayment_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return a QR code image uri for the LightningPayment
     * @param {number | null} [pixel_per_module]
     * @returns {string}
     */
    toUriQr(pixel_per_module) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.lightningpayment_toUriQr(this.__wbg_ptr, isLikeNone(pixel_per_module) ? 0xFFFFFF : pixel_per_module);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) LightningPayment.prototype[Symbol.dispose] = LightningPayment.prototype.free;

const MagicRoutingHintFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_magicroutinghint_free(ptr >>> 0, 1));
/**
 * A struct representing a magic routing hint, with details on how to pay directly without using Boltz
 */
export class MagicRoutingHint {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MagicRoutingHint.prototype);
        obj.__wbg_ptr = ptr;
        MagicRoutingHintFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MagicRoutingHintFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_magicroutinghint_free(ptr, 0);
    }
    /**
     * The address to pay directly to
     * @returns {string}
     */
    address() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.magicroutinghint_address(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The amount to pay directly to
     * @returns {bigint}
     */
    amount() {
        const ret = wasm.magicroutinghint_amount(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * The URI to pay directly to
     * @returns {string}
     */
    uri() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.magicroutinghint_uri(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) MagicRoutingHint.prototype[Symbol.dispose] = MagicRoutingHint.prototype.free;

const MnemonicFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mnemonic_free(ptr >>> 0, 1));
/**
 * A mnemonic secret code used as a master secret for a bip39 wallet.
 *
 * Supported number of words are 12, 15, 18, 21, and 24.
 */
export class Mnemonic {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Mnemonic.prototype);
        obj.__wbg_ptr = ptr;
        MnemonicFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MnemonicFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mnemonic_free(ptr, 0);
    }
    /**
     * Creates a Mnemonic
     * @param {string} s
     */
    constructor(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mnemonic_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        MnemonicFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return the string representation of the Mnemonic.
     * This representation can be used to recreate the Mnemonic via `new()`
     *
     * Note this is secret information, do not log it.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.mnemonic_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Creates a Mnemonic from entropy, at least 16 bytes are needed.
     * @param {Uint8Array} b
     * @returns {Mnemonic}
     */
    static fromEntropy(b) {
        const ptr0 = passArray8ToWasm0(b, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mnemonic_fromEntropy(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Mnemonic.__wrap(ret[0]);
    }
    /**
     * Creates a random Mnemonic of given words (12,15,18,21,24)
     * @param {number} word_count
     * @returns {Mnemonic}
     */
    static fromRandom(word_count) {
        const ret = wasm.mnemonic_fromRandom(word_count);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Mnemonic.__wrap(ret[0]);
    }
}
if (Symbol.dispose) Mnemonic.prototype[Symbol.dispose] = Mnemonic.prototype.free;

const NetworkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_network_free(ptr >>> 0, 1));
/**
 * The network of the elements blockchain such as mainnet, testnet or regtest.
 */
export class Network {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Network.prototype);
        obj.__wbg_ptr = ptr;
        NetworkFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        NetworkFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_network_free(ptr, 0);
    }
    /**
     * Creates a mainnet `Network``
     * @returns {Network}
     */
    static mainnet() {
        const ret = wasm.network_mainnet();
        return Network.__wrap(ret);
    }
    /**
     * Creates a testnet `Network``
     * @returns {Network}
     */
    static testnet() {
        const ret = wasm.network_testnet();
        return Network.__wrap(ret);
    }
    /**
     * Creates a regtest `Network``
     * @param {AssetId} policy_asset
     * @returns {Network}
     */
    static regtest(policy_asset) {
        _assertClass(policy_asset, AssetId);
        const ret = wasm.network_regtest(policy_asset.__wbg_ptr);
        return Network.__wrap(ret);
    }
    /**
     * Creates the default regtest `Network` with the policy asset `5ac9f65c0efcc4775e0baec4ec03abdde22473cd3cf33c0419ca290e0751b225`
     * @returns {Network}
     */
    static regtestDefault() {
        const ret = wasm.network_regtestDefault();
        return Network.__wrap(ret);
    }
    /**
     * Return the default esplora client for this network
     * @returns {EsploraClient}
     */
    defaultEsploraClient() {
        const ret = wasm.network_defaultEsploraClient(this.__wbg_ptr);
        return EsploraClient.__wrap(ret);
    }
    /**
     * Return true if the network is a mainnet network
     * @returns {boolean}
     */
    isMainnet() {
        const ret = wasm.network_isMainnet(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return true if the network is a testnet network
     * @returns {boolean}
     */
    isTestnet() {
        const ret = wasm.network_isTestnet(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return true if the network is a regtest network
     * @returns {boolean}
     */
    isRegtest() {
        const ret = wasm.network_isRegtest(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Return a string representation of the network, like "liquid", "liquid-testnet" or "liquid-regtest"
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.network_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the policy asset for this network
     * @returns {AssetId}
     */
    policyAsset() {
        const ret = wasm.network_policyAsset(this.__wbg_ptr);
        return AssetId.__wrap(ret);
    }
    /**
     * Return the genesis block hash for this network as hex string.
     * @returns {string}
     */
    genesisBlockHash() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.network_genesisBlockHash(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the transaction builder for this network
     * @returns {TxBuilder}
     */
    txBuilder() {
        const ret = wasm.network_txBuilder(this.__wbg_ptr);
        return TxBuilder.__wrap(ret);
    }
    /**
     * Return the default explorer URL for this network
     * @returns {string}
     */
    defaultExplorerUrl() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.network_defaultExplorerUrl(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Network.prototype[Symbol.dispose] = Network.prototype.free;

const OptionWalletTxOutFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_optionwallettxout_free(ptr >>> 0, 1));
/**
 * An optional wallet transaction output. Could be None when it's not possible to unblind.
 * It seems required by wasm_bindgen because we can't return `Vec<Option<WalletTxOut>>`
 */
export class OptionWalletTxOut {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(OptionWalletTxOut.prototype);
        obj.__wbg_ptr = ptr;
        OptionWalletTxOutFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        OptionWalletTxOutFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_optionwallettxout_free(ptr, 0);
    }
    /**
     * Return a copy of the WalletTxOut if it exists, otherwise None
     * @returns {WalletTxOut | undefined}
     */
    get() {
        const ret = wasm.optionwallettxout_get(this.__wbg_ptr);
        return ret === 0 ? undefined : WalletTxOut.__wrap(ret);
    }
}
if (Symbol.dispose) OptionWalletTxOut.prototype[Symbol.dispose] = OptionWalletTxOut.prototype.free;

const OutPointFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_outpoint_free(ptr >>> 0, 1));
/**
 * A reference to a transaction output
 */
export class OutPoint {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(OutPoint.prototype);
        obj.__wbg_ptr = ptr;
        OutPointFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    static __unwrap(jsValue) {
        if (!(jsValue instanceof OutPoint)) {
            return 0;
        }
        return jsValue.__destroy_into_raw();
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        OutPointFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_outpoint_free(ptr, 0);
    }
    /**
     * Creates an `OutPoint` from a string representation.
     * @param {string} s
     */
    constructor(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.outpoint_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        OutPointFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Creates an `OutPoint` from a transaction ID and output index.
     * @param {Txid} txid
     * @param {number} vout
     * @returns {OutPoint}
     */
    static fromParts(txid, vout) {
        _assertClass(txid, Txid);
        const ret = wasm.outpoint_fromParts(txid.__wbg_ptr, vout);
        return OutPoint.__wrap(ret);
    }
    /**
     * Return the transaction identifier.
     * @returns {Txid}
     */
    txid() {
        const ret = wasm.outpoint_txid(this.__wbg_ptr);
        return Txid.__wrap(ret);
    }
    /**
     * Return the output index.
     * @returns {number}
     */
    vout() {
        const ret = wasm.outpoint_vout(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) OutPoint.prototype[Symbol.dispose] = OutPoint.prototype.free;

const PosConfigFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_posconfig_free(ptr >>> 0, 1));
/**
 * POS (Point of Sale) configuration for encoding/decoding
 */
export class PosConfig {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PosConfig.prototype);
        obj.__wbg_ptr = ptr;
        PosConfigFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PosConfigFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_posconfig_free(ptr, 0);
    }
    /**
     * Create a new POS configuration
     * @param {WolletDescriptor} descriptor
     * @param {CurrencyCode} currency
     */
    constructor(descriptor, currency) {
        _assertClass(descriptor, WolletDescriptor);
        _assertClass(currency, CurrencyCode);
        const ret = wasm.posconfig_new(descriptor.__wbg_ptr, currency.__wbg_ptr);
        this.__wbg_ptr = ret >>> 0;
        PosConfigFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Create a POS configuration with all options
     * @param {WolletDescriptor} descriptor
     * @param {CurrencyCode} currency
     * @param {boolean | null} [show_gear]
     * @param {boolean | null} [show_description]
     * @returns {PosConfig}
     */
    static withOptions(descriptor, currency, show_gear, show_description) {
        _assertClass(descriptor, WolletDescriptor);
        _assertClass(currency, CurrencyCode);
        const ret = wasm.posconfig_withOptions(descriptor.__wbg_ptr, currency.__wbg_ptr, isLikeNone(show_gear) ? 0xFFFFFF : show_gear ? 1 : 0, isLikeNone(show_description) ? 0xFFFFFF : show_description ? 1 : 0);
        return PosConfig.__wrap(ret);
    }
    /**
     * Decode a POS configuration from a URL-safe base64 encoded string
     * @param {string} encoded
     * @returns {PosConfig}
     */
    static decode(encoded) {
        const ptr0 = passStringToWasm0(encoded, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.posconfig_decode(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return PosConfig.__wrap(ret[0]);
    }
    /**
     * Encode the POS configuration to a URL-safe base64 string
     * @returns {string}
     */
    encode() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.posconfig_encode(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Get the wallet descriptor
     * @returns {WolletDescriptor}
     */
    get descriptor() {
        const ret = wasm.posconfig_descriptor(this.__wbg_ptr);
        return WolletDescriptor.__wrap(ret);
    }
    /**
     * Get the currency code
     * @returns {CurrencyCode}
     */
    get currency() {
        const ret = wasm.posconfig_currency(this.__wbg_ptr);
        return CurrencyCode.__wrap(ret);
    }
    /**
     * Get whether to show the gear/settings button
     * @returns {boolean | undefined}
     */
    get showGear() {
        const ret = wasm.posconfig_show_gear(this.__wbg_ptr);
        return ret === 0xFFFFFF ? undefined : ret !== 0;
    }
    /**
     * Get whether to show the description/note field
     * @returns {boolean | undefined}
     */
    get showDescription() {
        const ret = wasm.posconfig_show_description(this.__wbg_ptr);
        return ret === 0xFFFFFF ? undefined : ret !== 0;
    }
    /**
     * Return a string representation of the POS configuration
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.posconfig_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) PosConfig.prototype[Symbol.dispose] = PosConfig.prototype.free;

const PrecisionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_precision_free(ptr >>> 0, 1));
/**
 * Helper to convert satoshi values of an asset to the value with the given precision and viceversa.
 *
 * For example 100 satoshi with precision 2 is "1.00"
 */
export class Precision {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PrecisionFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_precision_free(ptr, 0);
    }
    /**
     * Create a new Precision, useful to encode e decode values for assets with precision.
     * erroring if the given precision is greater than the allowed maximum (8)
     * @param {number} precision
     */
    constructor(precision) {
        const ret = wasm.precision_new(precision);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        PrecisionFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Convert the given satoshi value to the formatted value according to our precision
     *
     * For example 100 satoshi with precision 2 is "1.00"
     * @param {bigint} sats
     * @returns {string}
     */
    satsToString(sats) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.precision_satsToString(this.__wbg_ptr, sats);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Convert the given string with precision to satoshi units.
     *
     * For example the string "1.00" of an asset with precision 2 is 100 satoshi.
     * @param {string} sats
     * @returns {bigint}
     */
    stringToSats(sats) {
        const ptr0 = passStringToWasm0(sats, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.precision_stringToSats(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
}
if (Symbol.dispose) Precision.prototype[Symbol.dispose] = Precision.prototype.free;

const PreparePayResponseFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_preparepayresponse_free(ptr >>> 0, 1));

export class PreparePayResponse {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PreparePayResponse.prototype);
        obj.__wbg_ptr = ptr;
        PreparePayResponseFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PreparePayResponseFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_preparepayresponse_free(ptr, 0);
    }
    /**
     * Serialize the response to JSON string for JS interop
     * @returns {string}
     */
    serialize() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.preparepayresponse_serialize(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    swapId() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.preparepayresponse_swapId(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    uri() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.preparepayresponse_uri(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Address}
     */
    uriAddress() {
        const ret = wasm.preparepayresponse_uriAddress(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Address.__wrap(ret[0]);
    }
    /**
     * @returns {bigint}
     */
    uriAmount() {
        const ret = wasm.preparepayresponse_uriAmount(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * The fee of the swap provider
     *
     * It is equal to the amount requested onchain minus the amount of the bolt11 invoice
     * Does not include the fee of the onchain transaction.
     * @returns {bigint | undefined}
     */
    fee() {
        const ret = wasm.preparepayresponse_fee(this.__wbg_ptr);
        return ret[0] === 0 ? undefined : BigInt.asUintN(64, ret[1]);
    }
    /**
     * @returns {Promise<boolean>}
     */
    completePay() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.preparepayresponse_completePay(ptr);
        return ret;
    }
}
if (Symbol.dispose) PreparePayResponse.prototype[Symbol.dispose] = PreparePayResponse.prototype.free;

const PricesFetcherFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_pricesfetcher_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_wollet::PricesFetcher`]
 */
export class PricesFetcher {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PricesFetcherFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_pricesfetcher_free(ptr, 0);
    }
    /**
     * Create a new PricesFetcher with default settings
     */
    constructor() {
        const ret = wasm.pricesfetcher_new();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        PricesFetcherFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Fetch exchange rates for the given currency (e.g., "USD", "EUR", "CHF")
     *
     * Returns an ExchangeRates object containing rates from multiple sources and the median
     * @param {CurrencyCode} currency
     * @returns {Promise<ExchangeRates>}
     */
    rates(currency) {
        _assertClass(currency, CurrencyCode);
        const ret = wasm.pricesfetcher_rates(this.__wbg_ptr, currency.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) PricesFetcher.prototype[Symbol.dispose] = PricesFetcher.prototype.free;

const PricesFetcherBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_pricesfetcherbuilder_free(ptr >>> 0, 1));
/**
 * Wrapper over [`lwk_wollet::PricesFetcherBuilder`]
 */
export class PricesFetcherBuilder {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PricesFetcherBuilderFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_pricesfetcherbuilder_free(ptr, 0);
    }
}
if (Symbol.dispose) PricesFetcherBuilder.prototype[Symbol.dispose] = PricesFetcherBuilder.prototype.free;

const PsetFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_pset_free(ptr >>> 0, 1));
/**
 * Partially Signed Elements Transaction
 */
export class Pset {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Pset.prototype);
        obj.__wbg_ptr = ptr;
        PsetFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_pset_free(ptr, 0);
    }
    /**
     * Creates a `Pset` from its base64 string representation.
     * @param {string} base64
     */
    constructor(base64) {
        const ptr0 = passStringToWasm0(base64, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.pset_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        PsetFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return a base64 string representation of the Pset.
     * The string can be used to re-create the Pset via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.pset_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Extract the Transaction from a Pset by filling in
     * the available signature information in place.
     * @returns {Transaction}
     */
    extractTx() {
        const ret = wasm.pset_extractTx(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Transaction.__wrap(ret[0]);
    }
    /**
     * Get the unique id of the PSET as defined by [BIP-370](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#unique-identification)
     *
     * The unique id is the txid of the PSET with sequence numbers of inputs set to 0
     * @returns {Txid}
     */
    uniqueId() {
        const ret = wasm.pset_uniqueId(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Txid.__wrap(ret[0]);
    }
    /**
     * Attempt to merge with another `Pset`.
     * @param {Pset} other
     */
    combine(other) {
        _assertClass(other, Pset);
        var ptr0 = other.__destroy_into_raw();
        const ret = wasm.pset_combine(this.__wbg_ptr, ptr0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Return a copy of the inputs of this PSET
     * @returns {PsetInput[]}
     */
    inputs() {
        const ret = wasm.pset_inputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Return a copy of the outputs of this PSET
     * @returns {PsetOutput[]}
     */
    outputs() {
        const ret = wasm.pset_outputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Add wallet details to this PSET in place
     * @param {Wollet} wollet
     */
    addDetails(wollet) {
        _assertClass(wollet, Wollet);
        const ret = wasm.pset_addDetails(this.__wbg_ptr, wollet.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) Pset.prototype[Symbol.dispose] = Pset.prototype.free;

const PsetBalanceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_psetbalance_free(ptr >>> 0, 1));
/**
 * The details regarding balance and amounts in a PSET:
 *
 * - The fee of the transaction in the PSET
 * - The net balance of the assets in the PSET from the point of view of the wallet
 * - The outputs going out of the wallet
 */
export class PsetBalance {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PsetBalance.prototype);
        obj.__wbg_ptr = ptr;
        PsetBalanceFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetBalanceFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_psetbalance_free(ptr, 0);
    }
    /**
     * Fee paid by this transaction.
     *
     * Warning: if there are multiple assets paying fees this function can return an incorrect value.
     *
     * Deprecated: use `feesIn(assetId)` or `fees()` instead.
     * @returns {bigint}
     */
    fee() {
        const ret = wasm.psetbalance_fee(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Fees paid by this transaction.
     * @returns {Fees}
     */
    fees() {
        const ret = wasm.psetbalance_fees(this.__wbg_ptr);
        return Fees.__wrap(ret);
    }
    /**
     * The amount of fee with given asset id
     * @param {AssetId} asset
     * @returns {bigint}
     */
    feesIn(asset) {
        _assertClass(asset, AssetId);
        var ptr0 = asset.__destroy_into_raw();
        const ret = wasm.psetbalance_feesIn(this.__wbg_ptr, ptr0);
        return BigInt.asUintN(64, ret);
    }
    /**
     * The net balance for every asset with respect of the wallet asking the pset details
     * @returns {Balance}
     */
    balances() {
        const ret = wasm.psetbalance_balances(this.__wbg_ptr);
        return Balance.__wrap(ret);
    }
    /**
     * @returns {Recipient[]}
     */
    recipients() {
        const ret = wasm.psetbalance_recipients(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) PsetBalance.prototype[Symbol.dispose] = PsetBalance.prototype.free;

const PsetDetailsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_psetdetails_free(ptr >>> 0, 1));
/**
 * The details of a Partially Signed Elements Transaction:
 *
 * - the net balance from the point of view of the wallet
 * - the available and missing signatures for each input
 * - for issuances and reissuances transactions contains the issuance or reissuance details
 */
export class PsetDetails {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PsetDetails.prototype);
        obj.__wbg_ptr = ptr;
        PsetDetailsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetDetailsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_psetdetails_free(ptr, 0);
    }
    /**
     * Return the balance of the PSET from the point of view of the wallet
     * that generated this via `psetDetails()`
     * @returns {PsetBalance}
     */
    balance() {
        const ret = wasm.psetdetails_balance(this.__wbg_ptr);
        return PsetBalance.__wrap(ret);
    }
    /**
     * For each input existing or missing signatures
     * @returns {PsetSignatures[]}
     */
    signatures() {
        const ret = wasm.psetdetails_signatures(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Set of fingerprints for which the PSET is missing a signature
     * @returns {string[]}
     */
    fingerprintsMissing() {
        const ret = wasm.psetdetails_fingerprintsMissing(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * List of fingerprints for which the PSET has a signature
     * @returns {string[]}
     */
    fingerprintsHas() {
        const ret = wasm.psetdetails_fingerprintsHas(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Return an element for every input that could possibly be a issuance or a reissuance
     * @returns {Issuance[]}
     */
    inputsIssuances() {
        const ret = wasm.psetdetails_inputsIssuances(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) PsetDetails.prototype[Symbol.dispose] = PsetDetails.prototype.free;

const PsetInputFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_psetinput_free(ptr >>> 0, 1));
/**
 * PSET input
 */
export class PsetInput {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PsetInput.prototype);
        obj.__wbg_ptr = ptr;
        PsetInputFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetInputFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_psetinput_free(ptr, 0);
    }
    /**
     * Prevout TXID of the input
     * @returns {Txid}
     */
    previousTxid() {
        const ret = wasm.psetinput_previousTxid(this.__wbg_ptr);
        return Txid.__wrap(ret);
    }
    /**
     * Prevout vout of the input
     * @returns {number}
     */
    previousVout() {
        const ret = wasm.psetinput_previousVout(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Prevout scriptpubkey of the input
     * @returns {Script | undefined}
     */
    previousScriptPubkey() {
        const ret = wasm.psetinput_previousScriptPubkey(this.__wbg_ptr);
        return ret === 0 ? undefined : Script.__wrap(ret);
    }
    /**
     * Redeem script of the input
     * @returns {Script | undefined}
     */
    redeemScript() {
        const ret = wasm.psetinput_redeemScript(this.__wbg_ptr);
        return ret === 0 ? undefined : Script.__wrap(ret);
    }
    /**
     * If the input has an issuance, the asset id
     * @returns {AssetId | undefined}
     */
    issuanceAsset() {
        const ret = wasm.psetinput_issuanceAsset(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * If the input has an issuance, the token id
     * @returns {AssetId | undefined}
     */
    issuanceToken() {
        const ret = wasm.psetinput_issuanceToken(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * If the input has a (re)issuance, the issuance object
     * @returns {Issuance | undefined}
     */
    issuance() {
        const ret = wasm.psetinput_issuance(this.__wbg_ptr);
        return ret === 0 ? undefined : Issuance.__wrap(ret);
    }
    /**
     * Input sighash
     * @returns {number}
     */
    sighash() {
        const ret = wasm.psetinput_sighash(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * If the input has an issuance, returns [asset_id, token_id].
     * Returns undefined if the input has no issuance.
     * @returns {AssetId[] | undefined}
     */
    issuanceIds() {
        const ret = wasm.psetinput_issuanceIds(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        }
        return v1;
    }
}
if (Symbol.dispose) PsetInput.prototype[Symbol.dispose] = PsetInput.prototype.free;

const PsetOutputFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_psetoutput_free(ptr >>> 0, 1));
/**
 * PSET output
 */
export class PsetOutput {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PsetOutput.prototype);
        obj.__wbg_ptr = ptr;
        PsetOutputFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetOutputFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_psetoutput_free(ptr, 0);
    }
    /**
     * Get the script pubkey
     * @returns {Script}
     */
    scriptPubkey() {
        const ret = wasm.psetoutput_scriptPubkey(this.__wbg_ptr);
        return Script.__wrap(ret);
    }
    /**
     * Get the explicit amount, if set
     * @returns {bigint | undefined}
     */
    amount() {
        const ret = wasm.psetoutput_amount(this.__wbg_ptr);
        return ret[0] === 0 ? undefined : BigInt.asUintN(64, ret[1]);
    }
    /**
     * Get the explicit asset ID, if set
     * @returns {AssetId | undefined}
     */
    asset() {
        const ret = wasm.psetoutput_asset(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * Get the blinder index, if set
     * @returns {number | undefined}
     */
    blinderIndex() {
        const ret = wasm.psetoutput_blinderIndex(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
}
if (Symbol.dispose) PsetOutput.prototype[Symbol.dispose] = PsetOutput.prototype.free;

const PsetSignaturesFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_psetsignatures_free(ptr >>> 0, 1));
/**
 * The details of the signatures in a PSET, divided in available and missing signatures.
 */
export class PsetSignatures {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(PsetSignatures.prototype);
        obj.__wbg_ptr = ptr;
        PsetSignaturesFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PsetSignaturesFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_psetsignatures_free(ptr, 0);
    }
    /**
     * Returns `Vec<(PublicKey, KeySource)>`
     * @returns {any}
     */
    hasSignature() {
        const ret = wasm.psetsignatures_hasSignature(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {any}
     */
    missingSignature() {
        const ret = wasm.psetsignatures_missingSignature(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) PsetSignatures.prototype[Symbol.dispose] = PsetSignatures.prototype.free;

const RecipientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_recipient_free(ptr >>> 0, 1));
/**
 * Recipient of a PSET, in other words outputs that doesn't belong to the wallet
 */
export class Recipient {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Recipient.prototype);
        obj.__wbg_ptr = ptr;
        RecipientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RecipientFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_recipient_free(ptr, 0);
    }
    /**
     * @returns {AssetId | undefined}
     */
    asset() {
        const ret = wasm.recipient_asset(this.__wbg_ptr);
        return ret === 0 ? undefined : AssetId.__wrap(ret);
    }
    /**
     * @returns {bigint | undefined}
     */
    value() {
        const ret = wasm.recipient_value(this.__wbg_ptr);
        return ret[0] === 0 ? undefined : BigInt.asUintN(64, ret[1]);
    }
    /**
     * @returns {Address | undefined}
     */
    address() {
        const ret = wasm.recipient_address(this.__wbg_ptr);
        return ret === 0 ? undefined : Address.__wrap(ret);
    }
    /**
     * @returns {number}
     */
    vout() {
        const ret = wasm.recipient_vout(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Recipient.prototype[Symbol.dispose] = Recipient.prototype.free;

const RegistryFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_registry_free(ptr >>> 0, 1));
/**
 * A Registry, a repository to store and retrieve asset metadata, like the name or the ticker of an asset.
 */
export class Registry {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Registry.prototype);
        obj.__wbg_ptr = ptr;
        RegistryFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RegistryFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_registry_free(ptr, 0);
    }
    /**
     * Create a new registry cache specifying the URL of the registry,
     * fetch the assets metadata identified by the given asset ids and cache them for later local retrieval.
     * Use `default_for_network()` to get the default registry for the given network.
     * @param {string} url
     * @param {AssetIds} asset_ids
     * @returns {Promise<Registry>}
     */
    static new(url, asset_ids) {
        const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(asset_ids, AssetIds);
        const ret = wasm.registry_new(ptr0, len0, asset_ids.__wbg_ptr);
        return ret;
    }
    /**
     * Return the default registry for the given network,
     * fetch the assets metadata identified by the given asset ids and cache them for later local retrieval.
     * Use `new()` to specify a custom URL
     * @param {Network} network
     * @param {AssetIds} asset_ids
     * @returns {Promise<Registry>}
     */
    static defaultForNetwork(network, asset_ids) {
        _assertClass(network, Network);
        _assertClass(asset_ids, AssetIds);
        const ret = wasm.registry_defaultForNetwork(network.__wbg_ptr, asset_ids.__wbg_ptr);
        return ret;
    }
    /**
     * Create a new registry cache, using only the hardcoded assets.
     *
     * Hardcoded assets are the policy assets (LBTC, tLBTC, rLBTC) and the USDT asset on mainnet.
     * @param {Network} network
     * @returns {Registry}
     */
    static defaultHardcodedForNetwork(network) {
        _assertClass(network, Network);
        const ret = wasm.registry_defaultHardcodedForNetwork(network.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Registry.__wrap(ret[0]);
    }
    /**
     * Fetch the contract and the issuance transaction of the given asset id from the registry
     * @param {AssetId} asset_id
     * @param {EsploraClient} client
     * @returns {Promise<AssetMeta>}
     */
    fetchWithTx(asset_id, client) {
        _assertClass(asset_id, AssetId);
        _assertClass(client, EsploraClient);
        const ret = wasm.registry_fetchWithTx(this.__wbg_ptr, asset_id.__wbg_ptr, client.__wbg_ptr);
        return ret;
    }
    /**
     * Post a contract to the registry for registration.
     * @param {RegistryPost} data
     * @returns {Promise<void>}
     */
    post(data) {
        _assertClass(data, RegistryPost);
        const ret = wasm.registry_post(this.__wbg_ptr, data.__wbg_ptr);
        return ret;
    }
    /**
     * Return the asset metadata related to the given asset id if it exists in this registry.
     * @param {AssetId} asset_id
     * @returns {RegistryData | undefined}
     */
    get(asset_id) {
        _assertClass(asset_id, AssetId);
        const ret = wasm.registry_get(this.__wbg_ptr, asset_id.__wbg_ptr);
        return ret === 0 ? undefined : RegistryData.__wrap(ret);
    }
    /**
     * Return the asset metadata related to the given token id,
     * in other words `token_id` is the reissuance token of the returned asset
     * @param {AssetId} token_id
     * @returns {RegistryData | undefined}
     */
    getAssetOfToken(token_id) {
        _assertClass(token_id, AssetId);
        const ret = wasm.registry_getAssetOfToken(this.__wbg_ptr, token_id.__wbg_ptr);
        return ret === 0 ? undefined : RegistryData.__wrap(ret);
    }
    /**
     * Add the contracts information of the assets used in the Pset
     * if available in this registry.
     * Without the contract information, the partially signed transaction
     * is valid but will not show asset information when signed with an hardware wallet.
     * @param {Pset} pset
     * @returns {Pset}
     */
    addContracts(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.registry_addContracts(this.__wbg_ptr, ptr0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Pset.__wrap(ret[0]);
    }
}
if (Symbol.dispose) Registry.prototype[Symbol.dispose] = Registry.prototype.free;

const RegistryDataFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_registrydata_free(ptr >>> 0, 1));

export class RegistryData {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(RegistryData.prototype);
        obj.__wbg_ptr = ptr;
        RegistryDataFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RegistryDataFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_registrydata_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    precision() {
        const ret = wasm.registrydata_precision(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {string}
     */
    ticker() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.registrydata_ticker(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    name() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.registrydata_name(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    domain() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.registrydata_domain(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) RegistryData.prototype[Symbol.dispose] = RegistryData.prototype.free;

const RegistryPostFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_registrypost_free(ptr >>> 0, 1));
/**
 * The data to post to the registry to publish a contract for an asset id
 */
export class RegistryPost {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        RegistryPostFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_registrypost_free(ptr, 0);
    }
    /**
     * Create a new registry post object to be used to publish a contract for an asset id in the registry.
     * @param {Contract} contract
     * @param {AssetId} asset_id
     */
    constructor(contract, asset_id) {
        _assertClass(contract, Contract);
        var ptr0 = contract.__destroy_into_raw();
        _assertClass(asset_id, AssetId);
        var ptr1 = asset_id.__destroy_into_raw();
        const ret = wasm.registrypost_new(ptr0, ptr1);
        this.__wbg_ptr = ret >>> 0;
        RegistryPostFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return a string representation of the registry post (mostly for debugging).
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.registrypost_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) RegistryPost.prototype[Symbol.dispose] = RegistryPost.prototype.free;

const ScriptFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_script_free(ptr >>> 0, 1));
/**
 * An Elements (Liquid) script
 */
export class Script {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Script.prototype);
        obj.__wbg_ptr = ptr;
        ScriptFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ScriptFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_script_free(ptr, 0);
    }
    /**
     * Creates a `Script` from its hex string representation.
     * @param {string} s
     */
    constructor(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.script_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        ScriptFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Creates an empty `Script`.
     * @returns {Script}
     */
    static empty() {
        const ret = wasm.script_empty();
        return Script.__wrap(ret);
    }
    /**
     * Return the consensus encoded bytes of the script.
     * @returns {Uint8Array}
     */
    bytes() {
        const ret = wasm.script_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Returns SHA256 of the script's consensus bytes.
     *
     * Returns an equivalent value to the `jet::input_script_hash(index)`/`jet::output_script_hash(index)`.
     * @returns {string}
     */
    jet_sha256_hex() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.script_jet_sha256_hex(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the string of the script showing op codes and their arguments.
     *
     * For example: "OP_DUP OP_HASH160 OP_PUSHBYTES_20 088ac47276d105b91cf9aa27a00112421dd5f23c OP_EQUALVERIFY OP_CHECKSIG"
     * @returns {string}
     */
    asm() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.script_asm(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Creates an OP_RETURN script with the given data.
     * @param {Uint8Array} data
     * @returns {Script}
     */
    static newOpReturn(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.script_newOpReturn(ptr0, len0);
        return Script.__wrap(ret);
    }
    /**
     * Returns true if the script is provably unspendable.
     *
     * A script is provably unspendable if it starts with OP_RETURN or is larger
     * than the maximum script size.
     * @returns {boolean}
     */
    isProvablyUnspendable() {
        const ret = wasm.script_isProvablyUnspendable(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Returns true if this script_pubkey is provably SegWit.
     *
     * This checks if the script_pubkey is provably SegWit based on the
     * script_pubkey itself and an optional redeem_script.
     * @param {Script | null} [redeem_script]
     * @returns {boolean}
     */
    isProvablySegwit(redeem_script) {
        let ptr0 = 0;
        if (!isLikeNone(redeem_script)) {
            _assertClass(redeem_script, Script);
            ptr0 = redeem_script.__destroy_into_raw();
        }
        const ret = wasm.script_isProvablySegwit(this.__wbg_ptr, ptr0);
        return ret !== 0;
    }
    /**
     * Return the string representation of the script (hex encoding of its consensus encoded bytes).
     * This representation can be used to recreate the script via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.script_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Script.prototype[Symbol.dispose] = Script.prototype.free;

const SignerFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_signer_free(ptr >>> 0, 1));
/**
 * A Software signer.
 */
export class Signer {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SignerFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_signer_free(ptr, 0);
    }
    /**
     * AMP0 signer data for login
     * @returns {Amp0SignerData}
     */
    amp0SignerData() {
        const ret = wasm.signer_amp0SignerData(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Amp0SignerData.__wrap(ret[0]);
    }
    /**
     * AMP0 sign login challenge
     * @param {string} challenge
     * @returns {string}
     */
    amp0SignChallenge(challenge) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(challenge, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.signer_amp0SignChallenge(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * AMP0 account xpub
     * @param {number} account
     * @returns {string}
     */
    amp0AccountXpub(account) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.signer_amp0AccountXpub(this.__wbg_ptr, account);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Creates a `Signer`
     * @param {Mnemonic} mnemonic
     * @param {Network} network
     */
    constructor(mnemonic, network) {
        _assertClass(mnemonic, Mnemonic);
        _assertClass(network, Network);
        const ret = wasm.signer_new(mnemonic.__wbg_ptr, network.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        SignerFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Sign and consume the given PSET, returning the signed one
     * @param {Pset} pset
     * @returns {Pset}
     */
    sign(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.signer_sign(this.__wbg_ptr, ptr0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Pset.__wrap(ret[0]);
    }
    /**
     * Sign a message with the master key, return the signature as a base64 string
     * @param {string} message
     * @returns {string}
     */
    signMessage(message) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(message, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.signer_signMessage(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Return the witness public key hash, slip77 descriptor of this signer
     * @returns {WolletDescriptor}
     */
    wpkhSlip77Descriptor() {
        const ret = wasm.signer_wpkhSlip77Descriptor(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WolletDescriptor.__wrap(ret[0]);
    }
    /**
     * Return the extended public key of the signer
     * @returns {Xpub}
     */
    getMasterXpub() {
        const ret = wasm.signer_getMasterXpub(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Xpub.__wrap(ret[0]);
    }
    /**
     * Return keyorigin and xpub, like "[73c5da0a/84h/1h/0h]tpub..."
     * @param {Bip} bip
     * @returns {string}
     */
    keyoriginXpub(bip) {
        let deferred2_0;
        let deferred2_1;
        try {
            _assertClass(bip, Bip);
            const ret = wasm.signer_keyoriginXpub(this.__wbg_ptr, bip.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Return the signer fingerprint
     * @returns {string}
     */
    fingerprint() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.signer_fingerprint(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Return the mnemonic of the signer
     * @returns {Mnemonic}
     */
    mnemonic() {
        const ret = wasm.signer_mnemonic(this.__wbg_ptr);
        return Mnemonic.__wrap(ret);
    }
    /**
     * Return the derived BIP85 mnemonic
     * @param {number} index
     * @param {number} word_count
     * @returns {Mnemonic}
     */
    derive_bip85_mnemonic(index, word_count) {
        const ret = wasm.signer_derive_bip85_mnemonic(this.__wbg_ptr, index, word_count);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Mnemonic.__wrap(ret[0]);
    }
}
if (Symbol.dispose) Signer.prototype[Symbol.dispose] = Signer.prototype.free;

const SinglesigFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_singlesig_free(ptr >>> 0, 1));

export class Singlesig {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Singlesig.prototype);
        obj.__wbg_ptr = ptr;
        SinglesigFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SinglesigFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_singlesig_free(ptr, 0);
    }
    /**
     * @param {string} variant
     * @returns {Singlesig}
     */
    static from(variant) {
        const ptr0 = passStringToWasm0(variant, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.singlesig_from(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Singlesig.__wrap(ret[0]);
    }
}
if (Symbol.dispose) Singlesig.prototype[Symbol.dispose] = Singlesig.prototype.free;

const TipFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_tip_free(ptr >>> 0, 1));
/**
 * Blockchain tip, the highest valid block in the blockchain
 */
export class Tip {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Tip.prototype);
        obj.__wbg_ptr = ptr;
        TipFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TipFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_tip_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    height() {
        const ret = wasm.tip_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {string}
     */
    hash() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.tip_hash(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number | undefined}
     */
    timestamp() {
        const ret = wasm.tip_timestamp(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
}
if (Symbol.dispose) Tip.prototype[Symbol.dispose] = Tip.prototype.free;

const TransactionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_transaction_free(ptr >>> 0, 1));
/**
 * A Liquid transaction
 *
 * See `WalletTx` for the transaction as seen from the perspective of the wallet
 * where you can actually see unblinded amounts and tx net-balance.
 */
export class Transaction {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Transaction.prototype);
        obj.__wbg_ptr = ptr;
        TransactionFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TransactionFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_transaction_free(ptr, 0);
    }
    /**
     * Creates a `Transaction`
     *
     * Deprecated: use `fromString()` instead.
     * @param {string} tx_hex
     */
    constructor(tx_hex) {
        const ptr0 = passStringToWasm0(tx_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.transaction_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        TransactionFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Creates a `Transaction` from hex-encoded consensus bytes.
     * @param {string} s
     * @returns {Transaction}
     */
    static fromString(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.transaction_fromString(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Transaction.__wrap(ret[0]);
    }
    /**
     * Creates a `Transaction` from consensus-encoded bytes.
     * @param {Uint8Array} bytes
     * @returns {Transaction}
     */
    static fromBytes(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.transaction_fromBytes(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Transaction.__wrap(ret[0]);
    }
    /**
     * Return the transaction identifier.
     * @returns {Txid}
     */
    txid() {
        const ret = wasm.transaction_txid(this.__wbg_ptr);
        return Txid.__wrap(ret);
    }
    /**
     * Return the consensus encoded bytes of the transaction.
     * @returns {Uint8Array}
     */
    toBytes() {
        const ret = wasm.transaction_toBytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Return the consensus encoded bytes of the transaction.
     *
     * Deprecated: use `toBytes()` instead.
     * @returns {Uint8Array}
     */
    bytes() {
        const ret = wasm.transaction_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Return the fee of the transaction in the given asset.
     * At the moment the only asset that can be used as fee is the policy asset (LBTC for mainnet).
     * @param {AssetId} policy_asset
     * @returns {bigint}
     */
    fee(policy_asset) {
        _assertClass(policy_asset, AssetId);
        const ret = wasm.transaction_fee(this.__wbg_ptr, policy_asset.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Return the hex representation of the transaction. More precisely, they are the consensus encoded bytes of the transaction converted in hex.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.transaction_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Transaction.prototype[Symbol.dispose] = Transaction.prototype.free;

const TxBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txbuilder_free(ptr >>> 0, 1));
/**
 * A transaction builder
 */
export class TxBuilder {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxBuilder.prototype);
        obj.__wbg_ptr = ptr;
        TxBuilderFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxBuilderFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txbuilder_free(ptr, 0);
    }
    /**
     * Creates a transaction builder
     * @param {Network} network
     */
    constructor(network) {
        _assertClass(network, Network);
        const ret = wasm.network_txBuilder(network.__wbg_ptr);
        this.__wbg_ptr = ret >>> 0;
        TxBuilderFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Build the transaction
     * @param {Wollet} wollet
     * @returns {Pset}
     */
    finish(wollet) {
        const ptr = this.__destroy_into_raw();
        _assertClass(wollet, Wollet);
        const ret = wasm.txbuilder_finish(ptr, wollet.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Pset.__wrap(ret[0]);
    }
    /**
     * Build the transaction for AMP0
     * @param {Wollet} wollet
     * @returns {Amp0Pset}
     */
    finishForAmp0(wollet) {
        const ptr = this.__destroy_into_raw();
        _assertClass(wollet, Wollet);
        const ret = wasm.txbuilder_finishForAmp0(ptr, wollet.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Amp0Pset.__wrap(ret[0]);
    }
    /**
     * Set the fee rate
     * @param {number | null} [fee_rate]
     * @returns {TxBuilder}
     */
    feeRate(fee_rate) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.txbuilder_feeRate(ptr, isLikeNone(fee_rate) ? 0x100000001 : Math.fround(fee_rate));
        return TxBuilder.__wrap(ret);
    }
    /**
     * Select all available L-BTC inputs
     * @returns {TxBuilder}
     */
    drainLbtcWallet() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.txbuilder_drainLbtcWallet(ptr);
        return TxBuilder.__wrap(ret);
    }
    /**
     * Sets the address to drain excess L-BTC to
     * @param {Address} address
     * @returns {TxBuilder}
     */
    drainLbtcTo(address) {
        const ptr = this.__destroy_into_raw();
        _assertClass(address, Address);
        var ptr0 = address.__destroy_into_raw();
        const ret = wasm.txbuilder_drainLbtcTo(ptr, ptr0);
        return TxBuilder.__wrap(ret);
    }
    /**
     * Add a recipient receiving L-BTC
     *
     * Errors if address's network is incompatible
     * @param {Address} address
     * @param {bigint} satoshi
     * @returns {TxBuilder}
     */
    addLbtcRecipient(address, satoshi) {
        const ptr = this.__destroy_into_raw();
        _assertClass(address, Address);
        const ret = wasm.txbuilder_addLbtcRecipient(ptr, address.__wbg_ptr, satoshi);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Add a recipient receiving the given asset
     *
     * Errors if address's network is incompatible
     * @param {Address} address
     * @param {bigint} satoshi
     * @param {AssetId} asset
     * @returns {TxBuilder}
     */
    addRecipient(address, satoshi, asset) {
        const ptr = this.__destroy_into_raw();
        _assertClass(address, Address);
        _assertClass(asset, AssetId);
        const ret = wasm.txbuilder_addRecipient(ptr, address.__wbg_ptr, satoshi, asset.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Burn satoshi units of the given asset
     * @param {bigint} satoshi
     * @param {AssetId} asset
     * @returns {TxBuilder}
     */
    addBurn(satoshi, asset) {
        const ptr = this.__destroy_into_raw();
        _assertClass(asset, AssetId);
        const ret = wasm.txbuilder_addBurn(ptr, satoshi, asset.__wbg_ptr);
        return TxBuilder.__wrap(ret);
    }
    /**
     * Add explicit recipient
     * @param {Address} address
     * @param {bigint} satoshi
     * @param {AssetId} asset
     * @returns {TxBuilder}
     */
    addExplicitRecipient(address, satoshi, asset) {
        const ptr = this.__destroy_into_raw();
        _assertClass(address, Address);
        _assertClass(asset, AssetId);
        const ret = wasm.txbuilder_addExplicitRecipient(ptr, address.__wbg_ptr, satoshi, asset.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Issue an asset
     *
     * There will be `asset_sats` units of this asset that will be received by
     * `asset_receiver` if it's set, otherwise to an address of the wallet generating the issuance.
     *
     * There will be `token_sats` reissuance tokens that allow token holder to reissue the created
     * asset. Reissuance token will be received by `token_receiver` if it's some, or to an
     * address of the wallet generating the issuance if none.
     *
     * If a `contract` is provided, it's metadata will be committed in the generated asset id.
     *
     * Can't be used if `reissue_asset` has been called
     * @param {bigint} asset_sats
     * @param {Address | null | undefined} asset_receiver
     * @param {bigint} token_sats
     * @param {Address | null} [token_receiver]
     * @param {Contract | null} [contract]
     * @returns {TxBuilder}
     */
    issueAsset(asset_sats, asset_receiver, token_sats, token_receiver, contract) {
        const ptr = this.__destroy_into_raw();
        let ptr0 = 0;
        if (!isLikeNone(asset_receiver)) {
            _assertClass(asset_receiver, Address);
            ptr0 = asset_receiver.__destroy_into_raw();
        }
        let ptr1 = 0;
        if (!isLikeNone(token_receiver)) {
            _assertClass(token_receiver, Address);
            ptr1 = token_receiver.__destroy_into_raw();
        }
        let ptr2 = 0;
        if (!isLikeNone(contract)) {
            _assertClass(contract, Contract);
            ptr2 = contract.__destroy_into_raw();
        }
        const ret = wasm.txbuilder_issueAsset(ptr, asset_sats, ptr0, token_sats, ptr1, ptr2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Reissue an asset
     *
     * reissue the asset defined by `asset_to_reissue`, provided the reissuance token is owned
     * by the wallet generating te reissuance.
     *
     * Generated transaction will create `satoshi_to_reissue` new asset units, and they will be
     * sent to the provided `asset_receiver` address if some, or to an address from the wallet
     * generating the reissuance transaction if none.
     *
     * If the issuance transaction does not involve this wallet,
     * pass the issuance transaction in `issuance_tx`.
     * @param {AssetId} asset_to_reissue
     * @param {bigint} satoshi_to_reissue
     * @param {Address | null} [asset_receiver]
     * @param {Transaction | null} [issuance_tx]
     * @returns {TxBuilder}
     */
    reissueAsset(asset_to_reissue, satoshi_to_reissue, asset_receiver, issuance_tx) {
        const ptr = this.__destroy_into_raw();
        _assertClass(asset_to_reissue, AssetId);
        let ptr0 = 0;
        if (!isLikeNone(asset_receiver)) {
            _assertClass(asset_receiver, Address);
            ptr0 = asset_receiver.__destroy_into_raw();
        }
        let ptr1 = 0;
        if (!isLikeNone(issuance_tx)) {
            _assertClass(issuance_tx, Transaction);
            ptr1 = issuance_tx.__destroy_into_raw();
        }
        const ret = wasm.txbuilder_reissueAsset(ptr, asset_to_reissue.__wbg_ptr, satoshi_to_reissue, ptr0, ptr1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Switch to manual coin selection by giving a list of internal UTXOs to use.
     *
     * All passed UTXOs are added to the transaction.
     * No other wallet UTXO is added to the transaction, caller is supposed to add enough UTXOs to
     * cover for all recipients and fees.
     *
     * This method never fails, any error will be raised in [`TxBuilder::finish`].
     *
     * Possible errors:
     * * OutPoint doesn't belong to the wallet
     * * Insufficient funds (remember to include L-BTC utxos for fees)
     * @param {OutPoint[]} outpoints
     * @returns {TxBuilder}
     */
    setWalletUtxos(outpoints) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passArrayJsValueToWasm0(outpoints, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.txbuilder_setWalletUtxos(ptr, ptr0, len0);
        return TxBuilder.__wrap(ret);
    }
    /**
     * Return a string representation of the transaction builder (mostly for debugging)
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.txbuilder_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Set data to create a PSET from which you
     * can create a LiquiDEX proposal
     * @param {OutPoint} utxo
     * @param {Address} address
     * @param {bigint} satoshi
     * @param {AssetId} asset_id
     * @returns {TxBuilder}
     */
    liquidexMake(utxo, address, satoshi, asset_id) {
        const ptr = this.__destroy_into_raw();
        _assertClass(utxo, OutPoint);
        _assertClass(address, Address);
        _assertClass(asset_id, AssetId);
        const ret = wasm.txbuilder_liquidexMake(ptr, utxo.__wbg_ptr, address.__wbg_ptr, satoshi, asset_id.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Set data to take LiquiDEX proposals
     * @param {ValidatedLiquidexProposal[]} proposals
     * @returns {TxBuilder}
     */
    liquidexTake(proposals) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passArrayJsValueToWasm0(proposals, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.txbuilder_liquidexTake(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return TxBuilder.__wrap(ret[0]);
    }
    /**
     * Add input rangeproofs
     * @param {boolean} add_rangeproofs
     * @returns {TxBuilder}
     */
    addInputRangeproofs(add_rangeproofs) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.txbuilder_addInputRangeproofs(ptr, add_rangeproofs);
        return TxBuilder.__wrap(ret);
    }
}
if (Symbol.dispose) TxBuilder.prototype[Symbol.dispose] = TxBuilder.prototype.free;

const TxDetailsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txdetails_free(ptr >>> 0, 1));
/**
 * Transaction details
 */
export class TxDetails {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxDetails.prototype);
        obj.__wbg_ptr = ptr;
        TxDetailsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxDetailsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txdetails_free(ptr, 0);
    }
    /**
     * Transaction
     * @returns {Transaction | undefined}
     */
    tx() {
        const ret = wasm.txdetails_tx(this.__wbg_ptr);
        return ret === 0 ? undefined : Transaction.__wrap(ret);
    }
    /**
     * Txid
     * @returns {Txid}
     */
    txid() {
        const ret = wasm.txdetails_txid(this.__wbg_ptr);
        return Txid.__wrap(ret);
    }
    /**
     * Blockchain height
     * @returns {number | undefined}
     */
    height() {
        const ret = wasm.txdetails_height(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Timestamp
     *
     * A reasonable timestamp, that however can be inaccurate.
     * If you need a precise timestamp, do not use this value.
     * @returns {number | undefined}
     */
    timestamp() {
        const ret = wasm.txdetails_timestamp(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Transaction type
     *
     * A tentative description of the transaction type, which
     * however might be inaccurate. Use this if you want a simple
     * description of what this transaction is doing, but do
     * not rely on the value returned.
     * @returns {string}
     */
    txType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.txdetails_txType(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Balance
     *
     * Net balance from the `Wollet` perspective
     * @returns {Balance}
     */
    balance() {
        const ret = wasm.txdetails_balance(this.__wbg_ptr);
        return Balance.__wrap(ret);
    }
    /**
     * Fees paid by this transaction.
     * @returns {Fees}
     */
    fees() {
        const ret = wasm.txdetails_fees(this.__wbg_ptr);
        return Fees.__wrap(ret);
    }
    /**
     * Asset fees
     * @param {AssetId} asset
     * @returns {bigint}
     */
    feesAsset(asset) {
        _assertClass(asset, AssetId);
        const ret = wasm.txdetails_feesAsset(this.__wbg_ptr, asset.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Unblinded URL
     * @param {string} explorer_url
     * @returns {string}
     */
    unblindedUrl(explorer_url) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(explorer_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.txdetails_unblindedUrl(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Inputs
     * @returns {TxOutDetails[]}
     */
    inputs() {
        const ret = wasm.txdetails_inputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Outputs
     * @returns {TxOutDetails[]}
     */
    outputs() {
        const ret = wasm.txdetails_outputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) TxDetails.prototype[Symbol.dispose] = TxDetails.prototype.free;

const TxOptFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txopt_free(ptr >>> 0, 1));
/**
 * Options for transaction details
 */
export class TxOpt {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxOpt.prototype);
        obj.__wbg_ptr = ptr;
        TxOptFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxOptFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txopt_free(ptr, 0);
    }
    /**
     * @returns {TxOpt}
     */
    static default() {
        const ret = wasm.txopt_default();
        return TxOpt.__wrap(ret);
    }
}
if (Symbol.dispose) TxOpt.prototype[Symbol.dispose] = TxOpt.prototype.free;

const TxOutDetailsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txoutdetails_free(ptr >>> 0, 1));
/**
 * Transaction output details
 */
export class TxOutDetails {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxOutDetails.prototype);
        obj.__wbg_ptr = ptr;
        TxOutDetailsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxOutDetailsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txoutdetails_free(ptr, 0);
    }
    /**
     * Outpoint
     * @returns {OutPoint}
     */
    outpoint() {
        const ret = wasm.txoutdetails_outpoint(this.__wbg_ptr);
        return OutPoint.__wrap(ret);
    }
    /**
     * Scriptpubkey
     * @returns {Script | undefined}
     */
    script_pubkey() {
        const ret = wasm.txoutdetails_script_pubkey(this.__wbg_ptr);
        return ret === 0 ? undefined : Script.__wrap(ret);
    }
    /**
     * Height
     * @returns {number | undefined}
     */
    height() {
        const ret = wasm.txoutdetails_height(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Address
     * @returns {Address | undefined}
     */
    address() {
        const ret = wasm.txoutdetails_address(this.__wbg_ptr);
        return ret === 0 ? undefined : Address.__wrap(ret);
    }
    /**
     * Unblinded values (asset, amount, blinders)
     * @returns {TxOutSecrets | undefined}
     */
    unblinded() {
        const ret = wasm.txoutdetails_unblinded(this.__wbg_ptr);
        return ret === 0 ? undefined : TxOutSecrets.__wrap(ret);
    }
    /**
     * Whether the transaction output is explicit
     * @returns {boolean}
     */
    is_explicit() {
        const ret = wasm.txoutdetails_is_explicit(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Whether the output is spent by a previously downloaded transaction
     *
     * Note: this value might be inaccurate. We compute this from downloaded
     * transactions, however we only download transactions relevant for the
     * wallet (i.e. if they include inputs or outputs that belong to the
     * wallet), thus for non-wallet outputs we might set this value
     * incorrectly. For wallet outputs, it can be outdated.
     * @returns {boolean}
     */
    is_spent() {
        const ret = wasm.txoutdetails_is_spent(this.__wbg_ptr);
        return ret !== 0;
    }
}
if (Symbol.dispose) TxOutDetails.prototype[Symbol.dispose] = TxOutDetails.prototype.free;

const TxOutSecretsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txoutsecrets_free(ptr >>> 0, 1));
/**
 * Contains unblinded information such as the asset and the value of a transaction output
 */
export class TxOutSecrets {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxOutSecrets.prototype);
        obj.__wbg_ptr = ptr;
        TxOutSecretsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxOutSecretsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txoutsecrets_free(ptr, 0);
    }
    /**
     * Creates a new `TxOutSecrets` for an explicit (unblinded) output.
     *
     * The blinding factors are set to zero.
     * @param {AssetId} asset_id
     * @param {bigint} value
     * @returns {TxOutSecrets}
     */
    static fromExplicit(asset_id, value) {
        _assertClass(asset_id, AssetId);
        const ret = wasm.txoutsecrets_fromExplicit(asset_id.__wbg_ptr, value);
        return TxOutSecrets.__wrap(ret);
    }
    /**
     * Return the asset of the output.
     * @returns {AssetId}
     */
    asset() {
        const ret = wasm.txoutsecrets_asset(this.__wbg_ptr);
        return AssetId.__wrap(ret);
    }
    /**
     * Return the asset blinding factor as a typed object.
     * @returns {AssetBlindingFactor}
     */
    assetBlindingFactor() {
        const ret = wasm.txoutsecrets_assetBlindingFactor(this.__wbg_ptr);
        return AssetBlindingFactor.__wrap(ret);
    }
    /**
     * Return the value of the output.
     * @returns {bigint}
     */
    value() {
        const ret = wasm.txoutsecrets_value(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Return the value blinding factor as a typed object.
     * @returns {ValueBlindingFactor}
     */
    valueBlindingFactor() {
        const ret = wasm.txoutsecrets_valueBlindingFactor(this.__wbg_ptr);
        return ValueBlindingFactor.__wrap(ret);
    }
    /**
     * Return true if the output is explicit (no blinding factors).
     * @returns {boolean}
     */
    isExplicit() {
        const ret = wasm.txoutsecrets_isExplicit(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Get the asset commitment
     *
     * If the output is explicit, returns the empty string
     * @returns {string}
     */
    assetCommitment() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.txoutsecrets_assetCommitment(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Get the value commitment
     *
     * If the output is explicit, returns the empty string
     * @returns {string}
     */
    valueCommitment() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.txoutsecrets_valueCommitment(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) TxOutSecrets.prototype[Symbol.dispose] = TxOutSecrets.prototype.free;

const TxidFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txid_free(ptr >>> 0, 1));
/**
 * A valid transaction identifier.
 *
 * 32 bytes encoded as hex string.
 */
export class Txid {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Txid.prototype);
        obj.__wbg_ptr = ptr;
        TxidFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxidFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txid_free(ptr, 0);
    }
    /**
     * Creates a `Txid` from its hex string representation (64 characters).
     * @param {string} tx_id
     */
    constructor(tx_id) {
        const ptr0 = passStringToWasm0(tx_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.txid_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        TxidFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return the string representation of the transaction identifier as shown in the explorer.
     * This representation can be used to recreate the transaction identifier via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.txid_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Txid.prototype[Symbol.dispose] = Txid.prototype.free;

const TxsOptFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_txsopt_free(ptr >>> 0, 1));
/**
 * Options for transaction details
 */
export class TxsOpt {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(TxsOpt.prototype);
        obj.__wbg_ptr = ptr;
        TxsOptFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TxsOptFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_txsopt_free(ptr, 0);
    }
    /**
     * @returns {TxsOpt}
     */
    static default() {
        const ret = wasm.txsopt_default();
        return TxsOpt.__wrap(ret);
    }
    /**
     * @param {number} offset
     * @param {number} limit
     * @returns {TxsOpt}
     */
    static withPagination(offset, limit) {
        const ret = wasm.txsopt_withPagination(offset, limit);
        return TxsOpt.__wrap(ret);
    }
}
if (Symbol.dispose) TxsOpt.prototype[Symbol.dispose] = TxsOpt.prototype.free;

const UnvalidatedLiquidexProposalFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_unvalidatedliquidexproposal_free(ptr >>> 0, 1));
/**
 * LiquiDEX swap proposal
 *
 * A LiquiDEX swap proposal is a transaction with one input and one output created by the "maker".
 * The transaction "swaps" the input for the output, meaning that the "maker" sends the input and
 * receives the output.
 * However the transaction is incomplete (unbalanced and without a fee output), thus it cannot be
 * broadcast.
 * The "taker" can "complete" the transaction (using [`crate::TxBuilder::liquidex_take()`]) by
 * adding more inputs and more outputs to balance the amounts, meaning that the "taker" sends the
 * output and receives the input.
 */
export class UnvalidatedLiquidexProposal {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(UnvalidatedLiquidexProposal.prototype);
        obj.__wbg_ptr = ptr;
        UnvalidatedLiquidexProposalFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        UnvalidatedLiquidexProposalFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_unvalidatedliquidexproposal_free(ptr, 0);
    }
    /**
     * @param {string} s
     * @returns {UnvalidatedLiquidexProposal}
     */
    static new(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.unvalidatedliquidexproposal_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return UnvalidatedLiquidexProposal.__wrap(ret[0]);
    }
    /**
     * @param {Pset} pset
     * @returns {UnvalidatedLiquidexProposal}
     */
    static fromPset(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.unvalidatedliquidexproposal_fromPset(ptr0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return UnvalidatedLiquidexProposal.__wrap(ret[0]);
    }
    /**
     * @returns {ValidatedLiquidexProposal}
     */
    insecureValidate() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.unvalidatedliquidexproposal_insecureValidate(ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ValidatedLiquidexProposal.__wrap(ret[0]);
    }
    /**
     * @param {Transaction} tx
     * @returns {ValidatedLiquidexProposal}
     */
    validate(tx) {
        const ptr = this.__destroy_into_raw();
        _assertClass(tx, Transaction);
        var ptr0 = tx.__destroy_into_raw();
        const ret = wasm.unvalidatedliquidexproposal_validate(ptr, ptr0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ValidatedLiquidexProposal.__wrap(ret[0]);
    }
    /**
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.unvalidatedliquidexproposal_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) UnvalidatedLiquidexProposal.prototype[Symbol.dispose] = UnvalidatedLiquidexProposal.prototype.free;

const UpdateFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_update_free(ptr >>> 0, 1));
/**
 * An Update contains the delta of information to be applied to the wallet to reach the latest status.
 * It's created passing a reference to the wallet to the blockchain client
 */
export class Update {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Update.prototype);
        obj.__wbg_ptr = ptr;
        UpdateFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        UpdateFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_update_free(ptr, 0);
    }
    /**
     * Creates an `Update`
     * @param {Uint8Array} bytes
     */
    constructor(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.update_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        UpdateFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Serialize an update to a byte array
     * @returns {Uint8Array}
     */
    serialize() {
        const ret = wasm.update_serialize(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Serialize an update to a base64 encoded string,
     * encrypted with a key derived from the descriptor.
     * Decrypt using `deserialize_decrypted_base64()`
     * @param {WolletDescriptor} desc
     * @returns {string}
     */
    serializeEncryptedBase64(desc) {
        let deferred2_0;
        let deferred2_1;
        try {
            _assertClass(desc, WolletDescriptor);
            const ret = wasm.update_serializeEncryptedBase64(this.__wbg_ptr, desc.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Deserialize an update from a base64 encoded string,
     * decrypted with a key derived from the descriptor.
     * Create the base64 using `serialize_encrypted_base64()`
     * @param {string} base64
     * @param {WolletDescriptor} desc
     * @returns {Update}
     */
    static deserializeDecryptedBase64(base64, desc) {
        const ptr0 = passStringToWasm0(base64, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(desc, WolletDescriptor);
        const ret = wasm.update_deserializeDecryptedBase64(ptr0, len0, desc.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Update.__wrap(ret[0]);
    }
    /**
     * Whether this update only changes the tip
     * @returns {boolean}
     */
    onlyTip() {
        const ret = wasm.update_onlyTip(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Prune the update, removing unneeded data from transactions.
     * @param {Wollet} wollet
     */
    prune(wollet) {
        _assertClass(wollet, Wollet);
        wasm.update_prune(this.__wbg_ptr, wollet.__wbg_ptr);
    }
}
if (Symbol.dispose) Update.prototype[Symbol.dispose] = Update.prototype.free;

const ValidatedLiquidexProposalFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_validatedliquidexproposal_free(ptr >>> 0, 1));
/**
 * Created by validating `UnvalidatedLiquidexProposal` via `validate()` or `insecure_validate()`
 */
export class ValidatedLiquidexProposal {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(ValidatedLiquidexProposal.prototype);
        obj.__wbg_ptr = ptr;
        ValidatedLiquidexProposalFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    static __unwrap(jsValue) {
        if (!(jsValue instanceof ValidatedLiquidexProposal)) {
            return 0;
        }
        return jsValue.__destroy_into_raw();
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ValidatedLiquidexProposalFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_validatedliquidexproposal_free(ptr, 0);
    }
    /**
     * @returns {AssetAmount}
     */
    input() {
        const ret = wasm.validatedliquidexproposal_input(this.__wbg_ptr);
        return AssetAmount.__wrap(ret);
    }
    /**
     * @returns {AssetAmount}
     */
    output() {
        const ret = wasm.validatedliquidexproposal_output(this.__wbg_ptr);
        return AssetAmount.__wrap(ret);
    }
    /**
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.validatedliquidexproposal_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) ValidatedLiquidexProposal.prototype[Symbol.dispose] = ValidatedLiquidexProposal.prototype.free;

const ValueBlindingFactorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_valueblindingfactor_free(ptr >>> 0, 1));
/**
 * A blinding factor for value commitments.
 */
export class ValueBlindingFactor {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(ValueBlindingFactor.prototype);
        obj.__wbg_ptr = ptr;
        ValueBlindingFactorFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ValueBlindingFactorFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_valueblindingfactor_free(ptr, 0);
    }
    /**
     * Creates a `ValueBlindingFactor` from a string.
     * @param {string} s
     * @returns {ValueBlindingFactor}
     */
    static fromString(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.valueblindingfactor_fromString(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ValueBlindingFactor.__wrap(ret[0]);
    }
    /**
     * Creates a `ValueBlindingFactor` from a byte slice.
     * @param {Uint8Array} bytes
     * @returns {ValueBlindingFactor}
     */
    static fromBytes(bytes) {
        const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.valueblindingfactor_fromBytes(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ValueBlindingFactor.__wrap(ret[0]);
    }
    /**
     * Returns a zero value blinding factor.
     * @returns {ValueBlindingFactor}
     */
    static zero() {
        const ret = wasm.valueblindingfactor_zero();
        return ValueBlindingFactor.__wrap(ret);
    }
    /**
     * Returns the bytes (32 bytes) in little-endian byte order.
     *
     * This is the internal representation used by secp256k1. The byte order is
     * reversed compared to the hex string representation (which uses big-endian,
     * following Bitcoin display conventions).
     * @returns {Uint8Array}
     */
    toBytes() {
        const ret = wasm.valueblindingfactor_toBytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Returns string representation of the VBF
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.valueblindingfactor_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) ValueBlindingFactor.prototype[Symbol.dispose] = ValueBlindingFactor.prototype.free;

const WalletTxFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wallettx_free(ptr >>> 0, 1));
/**
 * Value returned by asking transactions to the wallet. Contains details about a transaction
 * from the perspective of the wallet, for example the net-balance of the transaction for the
 * wallet.
 */
export class WalletTx {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WalletTx.prototype);
        obj.__wbg_ptr = ptr;
        WalletTxFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WalletTxFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wallettx_free(ptr, 0);
    }
    /**
     * Return a copy of the transaction.
     * @returns {Transaction}
     */
    tx() {
        const ret = wasm.wallettx_tx(this.__wbg_ptr);
        return Transaction.__wrap(ret);
    }
    /**
     * Return the height of the block containing the transaction if it's confirmed.
     * @returns {number | undefined}
     */
    height() {
        const ret = wasm.wallettx_height(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Return the net balance of the transaction for the wallet.
     * @returns {Balance}
     */
    balance() {
        const ret = wasm.wallettx_balance(this.__wbg_ptr);
        return Balance.__wrap(ret);
    }
    /**
     * Return the transaction identifier.
     * @returns {Txid}
     */
    txid() {
        const ret = wasm.wallettx_txid(this.__wbg_ptr);
        return Txid.__wrap(ret);
    }
    /**
     * Return the fee of the transaction.
     * @returns {bigint}
     */
    fee() {
        const ret = wasm.wallettx_fee(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Return the type of the transaction. Can be "issuance", "reissuance", "burn", "redeposit", "incoming", "outgoing" or "unknown".
     * @returns {string}
     */
    txType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wallettx_txType(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the timestamp of the block containing the transaction if it's confirmed.
     * @returns {number | undefined}
     */
    timestamp() {
        const ret = wasm.wallettx_timestamp(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Return a list with the same number of elements as the inputs of the transaction.
     * The element in the list is a `WalletTxOut` (the output spent to create the input)
     * if it belongs to the wallet, while it is None for inputs owned by others
     * @returns {OptionWalletTxOut[]}
     */
    inputs() {
        const ret = wasm.wallettx_inputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Return a list with the same number of elements as the outputs of the transaction.
     * The element in the list is a `WalletTxOut` if it belongs to the wallet,
     * while it is None for inputs owned by others
     * @returns {OptionWalletTxOut[]}
     */
    outputs() {
        const ret = wasm.wallettx_outputs(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Return the URL to the transaction on the given explorer including the information
     * needed to unblind the transaction in the explorer UI.
     * @param {string} explorer_url
     * @returns {string}
     */
    unblindedUrl(explorer_url) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ptr0 = passStringToWasm0(explorer_url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.wallettx_unblindedUrl(this.__wbg_ptr, ptr0, len0);
            deferred2_0 = ret[0];
            deferred2_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
}
if (Symbol.dispose) WalletTx.prototype[Symbol.dispose] = WalletTx.prototype.free;

const WalletTxOutFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wallettxout_free(ptr >>> 0, 1));
/**
 * Details of a wallet transaction output used in `WalletTx`
 */
export class WalletTxOut {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WalletTxOut.prototype);
        obj.__wbg_ptr = ptr;
        WalletTxOutFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WalletTxOutFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wallettxout_free(ptr, 0);
    }
    /**
     * Return the outpoint (txid and vout) of this `WalletTxOut`.
     * @returns {OutPoint}
     */
    outpoint() {
        const ret = wasm.wallettxout_outpoint(this.__wbg_ptr);
        return OutPoint.__wrap(ret);
    }
    /**
     * Return the script pubkey of the address of this `WalletTxOut`.
     * @returns {Script}
     */
    scriptPubkey() {
        const ret = wasm.wallettxout_scriptPubkey(this.__wbg_ptr);
        return Script.__wrap(ret);
    }
    /**
     * Return the height of the block containing this output if it's confirmed.
     * @returns {number | undefined}
     */
    height() {
        const ret = wasm.wallettxout_height(this.__wbg_ptr);
        return ret === 0x100000001 ? undefined : ret;
    }
    /**
     * Return the unblinded values of this `WalletTxOut`.
     * @returns {TxOutSecrets}
     */
    unblinded() {
        const ret = wasm.wallettxout_unblinded(this.__wbg_ptr);
        return TxOutSecrets.__wrap(ret);
    }
    /**
     * Return the wildcard index used to derive the address of this `WalletTxOut`.
     * @returns {number}
     */
    wildcardIndex() {
        const ret = wasm.wallettxout_wildcardIndex(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Return the chain of this `WalletTxOut`. Can be "Chain::External" or "Chain::Internal" (change).
     * @returns {Chain}
     */
    extInt() {
        const ret = wasm.wallettxout_extInt(this.__wbg_ptr);
        return ret;
    }
    /**
     * Return the address of this `WalletTxOut`.
     * @returns {Address}
     */
    address() {
        const ret = wasm.wallettxout_address(this.__wbg_ptr);
        return Address.__wrap(ret);
    }
}
if (Symbol.dispose) WalletTxOut.prototype[Symbol.dispose] = WalletTxOut.prototype.free;

const WolletFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wollet_free(ptr >>> 0, 1));
/**
 * A watch-only wallet defined by a CT descriptor.
 */
export class Wollet {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Wollet.prototype);
        obj.__wbg_ptr = ptr;
        WolletFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WolletFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wollet_free(ptr, 0);
    }
    /**
     * Create a `Wollet`
     * @param {Network} network
     * @param {WolletDescriptor} descriptor
     */
    constructor(network, descriptor) {
        _assertClass(network, Network);
        _assertClass(descriptor, WolletDescriptor);
        const ret = wasm.wollet_new(network.__wbg_ptr, descriptor.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        WolletFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Get a wallet address with the correspondong derivation index
     *
     * If Some return the address at the given index,
     * otherwise the last unused address.
     * @param {number | null} [index]
     * @returns {AddressResult}
     */
    address(index) {
        const ret = wasm.wollet_address(this.__wbg_ptr, isLikeNone(index) ? 0x100000001 : (index) >>> 0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AddressResult.__wrap(ret[0]);
    }
    /**
     * Return the [ELIP152](https://github.com/ElementsProject/ELIPs/blob/main/elip-0152.mediawiki) deterministic wallet identifier.
     * @returns {string}
     */
    dwid() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.wollet_dwid(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Get the full derivation path for an address
     *
     * Note: will be removed once we add the full path to lwk_wollet::AddressResult
     * @param {number} index
     * @returns {Uint32Array}
     */
    addressFullPath(index) {
        const ret = wasm.wollet_addressFullPath(this.__wbg_ptr, index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Apply an update containing blockchain data
     *
     * To update the wallet you need to first obtain the blockchain data relevant for the wallet.
     * This can be done using a `full_scan()`, which
     * returns an `Update` that contains new transaction and other data relevant for the
     * wallet.
     * The update must then be applied to the `Wollet` so that wollet methods such as
     * `balance()` or `transactions()` include the new data.
     *
     * However getting blockchain data involves network calls, so between the full scan start and
     * when the update is applied it might elapse a significant amount of time.
     * In that interval, applying any update, or any transaction using `apply_transaction()`,
     * will cause this function to return a `Error::UpdateOnDifferentStatus`.
     * Callers should either avoid applying updates and transactions, or they can catch the error
     * and wait for a new full scan to be completed and applied.
     * @param {Update} update
     */
    applyUpdate(update) {
        _assertClass(update, Update);
        const ret = wasm.wollet_applyUpdate(this.__wbg_ptr, update.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Apply a transaction to the wallet state
     *
     * Wallet transactions are normally obtained using a `full_scan()`
     * and applying the result with `apply_update()`. However a
     * full scan involves network calls and it can take a significant amount of time.
     *
     * If the caller does not want to wait for a full scan containing the transaction, it can
     * apply the transaction to the wallet state using this function.
     *
     * Note: if this transaction is *not* returned by a next full scan, after `apply_update()` it will disappear from the
     * transactions list, will not be included in balance computations, and by the remaining
     * wollet methods.
     *
     * Calling this method, might cause `apply_update()` to fail with a
     * `Error::UpdateOnDifferentStatus`, make sure to either avoid it or handle the error properly.
     * @param {Transaction} tx
     * @returns {Balance}
     */
    applyTransaction(tx) {
        _assertClass(tx, Transaction);
        const ret = wasm.wollet_applyTransaction(this.__wbg_ptr, tx.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Balance.__wrap(ret[0]);
    }
    /**
     * Get the wallet balance for each assets
     * @returns {Balance}
     */
    balance() {
        const ret = wasm.wollet_balance(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Balance.__wrap(ret[0]);
    }
    /**
     * Get the asset identifiers owned by the wallet
     * @returns {AssetIds}
     */
    assetsOwned() {
        const ret = wasm.wollet_assetsOwned(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return AssetIds.__wrap(ret[0]);
    }
    /**
     * Get the wallet transactions, sorted by height descending, then txid descending with unconfirmed first
     * @returns {WalletTx[]}
     */
    transactions() {
        const ret = wasm.wollet_transactions(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Get the wallet transactions with pagination sorted by height descending, then txid descending with unconfirmed first
     * @param {number} offset
     * @param {number} limit
     * @returns {WalletTx[]}
     */
    transactionsPaginated(offset, limit) {
        const ret = wasm.wollet_transactionsPaginated(this.__wbg_ptr, offset, limit);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Get the unspent transaction outputs of the wallet
     * @returns {WalletTxOut[]}
     */
    utxos() {
        const ret = wasm.wollet_utxos(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Get all the transaction outputs of the wallet, both spent and unspent
     * @returns {WalletTxOut[]}
     */
    txos() {
        const ret = wasm.wollet_txos(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Finalize and consume the given PSET, returning the finalized one
     * @param {Pset} pset
     * @returns {Pset}
     */
    finalize(pset) {
        _assertClass(pset, Pset);
        var ptr0 = pset.__destroy_into_raw();
        const ret = wasm.wollet_finalize(this.__wbg_ptr, ptr0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Pset.__wrap(ret[0]);
    }
    /**
     * Get the PSET details with respect to the wallet
     * @param {Pset} pset
     * @returns {PsetDetails}
     */
    psetDetails(pset) {
        _assertClass(pset, Pset);
        const ret = wasm.wollet_psetDetails(this.__wbg_ptr, pset.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return PsetDetails.__wrap(ret[0]);
    }
    /**
     * Get a copy of the wallet descriptor of this wallet.
     * @returns {WolletDescriptor}
     */
    descriptor() {
        const ret = wasm.wollet_descriptor(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WolletDescriptor.__wrap(ret[0]);
    }
    /**
     * A deterministic value derived from the descriptor, the config and the content of this wollet,
     * including what's in the wallet store (transactions etc)
     *
     * In this case, we don't need cryptographic assurance guaranteed by the std default hasher (siphash)
     * And we can use a much faster hasher, which is used also in the rust compiler.
     * ([source](https://nnethercote.github.io/2021/12/08/a-brutally-effective-hash-function-in-rust.html))
     * @returns {bigint}
     */
    status() {
        const ret = wasm.wollet_status(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * Get the blockchain tip at the time of the last update of this wollet.
     * @returns {Tip}
     */
    tip() {
        const ret = wasm.wollet_tip(this.__wbg_ptr);
        return Tip.__wrap(ret);
    }
    /**
     * Returns true if this wollet has never received an updated applyed to it
     * @returns {boolean}
     */
    neverScanned() {
        const ret = wasm.wollet_neverScanned(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Whether the wallet is AMP0
     * @returns {boolean}
     */
    isAmp0() {
        const ret = wasm.wollet_isAmp0(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Get the transaction list
     *
     * **Experimental**: This API may change without notice.
     * @param {TxsOpt} opt
     * @returns {TxDetails[]}
     */
    txs(opt) {
        _assertClass(opt, TxsOpt);
        const ret = wasm.wollet_txs(this.__wbg_ptr, opt.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Number of transactions
     * @returns {number}
     */
    numTxs() {
        const ret = wasm.wollet_numTxs(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * Get the details of a transaction
     *
     * **Experimental**: This API may change without notice.
     * @param {Txid} txid
     * @param {TxOpt} opt
     * @returns {TxDetails | undefined}
     */
    txDetails(txid, opt) {
        _assertClass(txid, Txid);
        _assertClass(opt, TxOpt);
        const ret = wasm.wollet_txDetails(this.__wbg_ptr, txid.__wbg_ptr, opt.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] === 0 ? undefined : TxDetails.__wrap(ret[0]);
    }
}
if (Symbol.dispose) Wollet.prototype[Symbol.dispose] = Wollet.prototype.free;

const WolletBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wolletbuilder_free(ptr >>> 0, 1));
/**
 * A builder for constructing [`Wollet`] instances.
 */
export class WolletBuilder {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WolletBuilder.prototype);
        obj.__wbg_ptr = ptr;
        WolletBuilderFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WolletBuilderFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wolletbuilder_free(ptr, 0);
    }
    /**
     * Create a builder for a watch-only wallet.
     * @param {Network} network
     * @param {WolletDescriptor} descriptor
     */
    constructor(network, descriptor) {
        _assertClass(network, Network);
        _assertClass(descriptor, WolletDescriptor);
        const ret = wasm.wolletbuilder_new(network.__wbg_ptr, descriptor.__wbg_ptr);
        this.__wbg_ptr = ret >>> 0;
        WolletBuilderFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Set the threshold used to merge persisted updates during build.
     *
     * **Experimental**: This API may change without notice.
     *
     * `None` disables merging (default behavior).
     * @param {number | null} [merge_threshold]
     * @returns {WolletBuilder}
     */
    withMergeThreshold(merge_threshold) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_withMergeThreshold(ptr, isLikeNone(merge_threshold) ? 0x100000001 : (merge_threshold) >>> 0);
        return WolletBuilder.__wrap(ret);
    }
    /**
     * Set the wallet as "utxo only".
     *
     * **Experimental**: This API may change without notice.
     * @param {boolean} utxo_only
     * @returns {WolletBuilder}
     */
    utxoOnly(utxo_only) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_utxoOnly(ptr, utxo_only);
        return WolletBuilder.__wrap(ret);
    }
    /**
     * Persist wallet updates in the given JavaScript storage object.
     *
     * **Experimental**: This API may change without notice.
     *
     * Wallet data is persisted in clear.
     *
     * The JS object must have `get(key)`, `put(key, value)`, and `remove(key)` methods.
     * @param {any} storage
     * @returns {WolletBuilder}
     */
    withExperimentalStore(storage) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_withExperimentalStore(ptr, storage);
        return WolletBuilder.__wrap(ret);
    }
    /**
     * Persist wallet transactions in the given JavaScript
     * storage object.
     *
     * **Experimental**: This API may change without notice.
     *
     * The JS object must have `get(key)`, `put(key, value)`, and `remove(key)` methods.
     * @param {any} storage
     * @returns {WolletBuilder}
     */
    withTxsStore(storage) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_withTxsStore(ptr, storage);
        return WolletBuilder.__wrap(ret);
    }
    /**
     * Set encryption for the transactions store.
     *
     * **Experimental**: This API may change without notice.
     *
     * Default: encrypted if the store is persisted.
     * @param {boolean} encrypt
     * @returns {WolletBuilder}
     */
    setEncryptionTxsStore(encrypt) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_setEncryptionTxsStore(ptr, encrypt);
        return WolletBuilder.__wrap(ret);
    }
    /**
     * Build the wallet from this builder.
     * @returns {Wollet}
     */
    build() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.wolletbuilder_build(ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Wollet.__wrap(ret[0]);
    }
}
if (Symbol.dispose) WolletBuilder.prototype[Symbol.dispose] = WolletBuilder.prototype.free;

const WolletDescriptorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wolletdescriptor_free(ptr >>> 0, 1));
/**
 * A wrapper that contains only the subset of CT descriptors handled by wollet
 */
export class WolletDescriptor {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WolletDescriptor.prototype);
        obj.__wbg_ptr = ptr;
        WolletDescriptorFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WolletDescriptorFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wolletdescriptor_free(ptr, 0);
    }
    /**
     * Creates a `WolletDescriptor`
     * @param {string} descriptor
     */
    constructor(descriptor) {
        const ptr0 = passStringToWasm0(descriptor, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wolletdescriptor_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        WolletDescriptorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return the string representation of the descriptor, including the checksum.
     * This representation can be used to recreate the descriptor via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wolletdescriptor_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Create a new multisig descriptor, where each participant is a keyorigin_xpub and it requires at least threshold signatures to spend.
     * Errors if the threshold is 0 or greater than the number of participants.
     * Uses slip77 for the blinding key.
     * @param {number} threshold
     * @param {string[]} participants
     * @returns {WolletDescriptor}
     */
    static newMultiWshSlip77(threshold, participants) {
        const ptr0 = passArrayJsValueToWasm0(participants, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wolletdescriptor_newMultiWshSlip77(threshold, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WolletDescriptor.__wrap(ret[0]);
    }
    /**
     * Whether the descriptor is for mainnet
     * @returns {boolean}
     */
    isMainnet() {
        const ret = wasm.wolletdescriptor_isMainnet(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Whether the descriptor is AMP0
     * @returns {boolean}
     */
    isAmp0() {
        const ret = wasm.wolletdescriptor_isAmp0(this.__wbg_ptr);
        return ret !== 0;
    }
}
if (Symbol.dispose) WolletDescriptor.prototype[Symbol.dispose] = WolletDescriptor.prototype.free;

const XpubFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_xpub_free(ptr >>> 0, 1));
/**
 * An extended public key
 */
export class Xpub {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Xpub.prototype);
        obj.__wbg_ptr = ptr;
        XpubFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        XpubFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_xpub_free(ptr, 0);
    }
    /**
     * Creates a Xpub
     * @param {string} s
     */
    constructor(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.xpub_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        XpubFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Return the string representation of the Xpub.
     * This representation can be used to recreate the Xpub via `new()`
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.xpub_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the identifier of the Xpub.
     * This is a 40 hex characters string (20 bytes).
     * @returns {string}
     */
    identifier() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.xpub_identifier(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Return the first four bytes of the identifier as hex string
     * This is a 8 hex characters string (4 bytes).
     * @returns {string}
     */
    fingerprint() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.xpub_fingerprint(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Returns true if the passed string is a valid xpub with a valid keyorigin if present.
     * For example: "[73c5da0a/84h/1h/0h]tpub..."
     * @param {string} s
     * @returns {boolean}
     */
    static isValidWithKeyOrigin(s) {
        const ptr0 = passStringToWasm0(s, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.xpub_isValidWithKeyOrigin(ptr0, len0);
        return ret !== 0;
    }
}
if (Symbol.dispose) Xpub.prototype[Symbol.dispose] = Xpub.prototype.free;

const EXPECTED_RESPONSE_TYPES = new Set(['basic', 'cors', 'default']);

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);

            } catch (e) {
                const validResponse = module.ok && EXPECTED_RESPONSE_TYPES.has(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);

    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };

        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbg_Error_e17e777aac105295 = function(arg0, arg1) {
        const ret = Error(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_String_8f0eb39a4a4c2f66 = function(arg0, arg1) {
        const ret = String(arg1);
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_abort_67e1b49bf6614565 = function(arg0) {
        arg0.abort();
    };
    imports.wbg.__wbg_abort_d830bf2e9aa6ec5b = function(arg0, arg1) {
        arg0.abort(arg1);
    };
    imports.wbg.__wbg_addressresult_new = function(arg0) {
        const ret = AddressResult.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_amp0_new = function(arg0) {
        const ret = Amp0.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_amp0connected_new = function(arg0) {
        const ret = Amp0Connected.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_amp0loggedin_new = function(arg0) {
        const ret = Amp0LoggedIn.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_append_72a3c0addd2bce38 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        arg0.append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
    }, arguments) };
    imports.wbg.__wbg_arrayBuffer_9c99b8e2809e8cbb = function() { return handleError(function (arg0) {
        const ret = arg0.arrayBuffer();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_assetid_new = function(arg0) {
        const ret = AssetId.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_assetmeta_new = function(arg0) {
        const ret = AssetMeta.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_boltzsession_new = function(arg0) {
        const ret = BoltzSession.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_buffer_8d40b1d762fb3c66 = function(arg0) {
        const ret = arg0.buffer;
        return ret;
    };
    imports.wbg.__wbg_byteLength_8ccf68e3ac59ff5e = function(arg0) {
        const ret = arg0.byteLength;
        return ret;
    };
    imports.wbg.__wbg_byteOffset_673c717c222fef8e = function(arg0) {
        const ret = arg0.byteOffset;
        return ret;
    };
    imports.wbg.__wbg_call_13410aac570ffff7 = function() { return handleError(function (arg0, arg1) {
        const ret = arg0.call(arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_call_641db1bb5db5a579 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.call(arg1, arg2, arg3);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_call_a5400b25a865cfd8 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.call(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_clearTimeout_5a54f8841c30079a = function(arg0) {
        const ret = clearTimeout(arg0);
        return ret;
    };
    imports.wbg.__wbg_clearTimeout_7a42b49784aea641 = function(arg0) {
        const ret = clearTimeout(arg0);
        return ret;
    };
    imports.wbg.__wbg_close_6437264570d2d37f = function() { return handleError(function (arg0) {
        arg0.close();
    }, arguments) };
    imports.wbg.__wbg_close_dadc273a120c03ec = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        arg0.close(arg1, getStringFromWasm0(arg2, arg3));
    }, arguments) };
    imports.wbg.__wbg_code_177e3bed72688e58 = function(arg0) {
        const ret = arg0.code;
        return ret;
    };
    imports.wbg.__wbg_crypto_574e78ad8b13b65f = function(arg0) {
        const ret = arg0.crypto;
        return ret;
    };
    imports.wbg.__wbg_data_8cd553c9786c9361 = function(arg0) {
        const ret = arg0.data;
        return ret;
    };
    imports.wbg.__wbg_data_9ab529722bcc4e6c = function(arg0) {
        const ret = arg0.data;
        return ret;
    };
    imports.wbg.__wbg_done_75ed0ee6dd243d9d = function(arg0) {
        const ret = arg0.done;
        return ret;
    };
    imports.wbg.__wbg_exchangerates_new = function(arg0) {
        const ret = ExchangeRates.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_fetch_74a3e84ebd2c9a0e = function(arg0) {
        const ret = fetch(arg0);
        return ret;
    };
    imports.wbg.__wbg_fetch_87aed7f306ec6d63 = function(arg0, arg1) {
        const ret = arg0.fetch(arg1);
        return ret;
    };
    imports.wbg.__wbg_getDevices_abebcc7be30632a7 = function(arg0) {
        const ret = arg0.getDevices();
        return ret;
    };
    imports.wbg.__wbg_getPorts_3a0fce782d01e063 = function(arg0) {
        const ret = arg0.getPorts();
        return ret;
    };
    imports.wbg.__wbg_getRandomValues_b8f5dbd5f3995a9e = function() { return handleError(function (arg0, arg1) {
        arg0.getRandomValues(arg1);
    }, arguments) };
    imports.wbg.__wbg_getUint8_083c6ed87b45a86a = function(arg0, arg1) {
        const ret = arg0.getUint8(arg1 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_getWriter_03d7689e275ac6a4 = function() { return handleError(function (arg0) {
        const ret = arg0.getWriter();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_get_0da715ceaecea5c8 = function(arg0, arg1) {
        const ret = arg0[arg1 >>> 0];
        return ret;
    };
    imports.wbg.__wbg_get_2bb361e6f1bdde50 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg1.get(getStringFromWasm0(arg2, arg3));
        var ptr1 = isLikeNone(ret) ? 0 : passArray8ToWasm0(ret, wasm.__wbindgen_malloc);
        var len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    }, arguments) };
    imports.wbg.__wbg_get_458e874b43b18b25 = function() { return handleError(function (arg0, arg1) {
        const ret = Reflect.get(arg0, arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_has_b89e451f638123e3 = function() { return handleError(function (arg0, arg1) {
        const ret = Reflect.has(arg0, arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_headers_29fec3c72865cd75 = function(arg0) {
        const ret = arg0.headers;
        return ret;
    };
    imports.wbg.__wbg_hid_fc50a7126d53c63c = function(arg0) {
        const ret = arg0.hid;
        return ret;
    };
    imports.wbg.__wbg_instanceof_ArrayBuffer_67f3012529f6a2dd = function(arg0) {
        let result;
        try {
            result = arg0 instanceof ArrayBuffer;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Blob_3db67efd3f1b960f = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Blob;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_HidDevice_ab5b9b1c208f5539 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof HIDDevice;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Response_50fde2cd696850bf = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Response;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_SerialPort_5f6213e35dce8b2a = function(arg0) {
        let result;
        try {
            result = arg0 instanceof SerialPort;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_instanceof_Window_12d20d558ef92592 = function(arg0) {
        let result;
        try {
            result = arg0 instanceof Window;
        } catch (_) {
            result = false;
        }
        const ret = result;
        return ret;
    };
    imports.wbg.__wbg_invoice_new = function(arg0) {
        const ret = Invoice.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_invoiceresponse_new = function(arg0) {
        const ret = InvoiceResponse.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_isArray_030cce220591fb41 = function(arg0) {
        const ret = Array.isArray(arg0);
        return ret;
    };
    imports.wbg.__wbg_isPersisted_354c6f4a4e0f19cb = function() { return handleError(function (arg0) {
        const ret = arg0.isPersisted();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_issuance_new = function(arg0) {
        const ret = Issuance.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_iterator_f370b34483c71a1c = function() {
        const ret = Symbol.iterator;
        return ret;
    };
    imports.wbg.__wbg_jade_new = function(arg0) {
        const ret = Jade.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_jadewebsocket_new = function(arg0) {
        const ret = JadeWebSocket.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_lastusedindexresponse_new = function(arg0) {
        const ret = LastUsedIndexResponse.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_length_186546c51cd61acd = function(arg0) {
        const ret = arg0.length;
        return ret;
    };
    imports.wbg.__wbg_length_6bb7e81f9d7713e4 = function(arg0) {
        const ret = arg0.length;
        return ret;
    };
    imports.wbg.__wbg_log_0bf9a59e21c21d51 = function(arg0, arg1) {
        console.log(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_magicroutinghint_new = function(arg0) {
        const ret = MagicRoutingHint.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_msCrypto_a61aeb35a24c1329 = function(arg0) {
        const ret = arg0.msCrypto;
        return ret;
    };
    imports.wbg.__wbg_navigator_65d5ad763926b868 = function(arg0) {
        const ret = arg0.navigator;
        return ret;
    };
    imports.wbg.__wbg_new_19c25a3f2fa63a02 = function() {
        const ret = new Object();
        return ret;
    };
    imports.wbg.__wbg_new_1f3a344cf3123716 = function() {
        const ret = new Array();
        return ret;
    };
    imports.wbg.__wbg_new_2e3c58a15f39f5f9 = function(arg0, arg1) {
        try {
            var state0 = {a: arg0, b: arg1};
            var cb0 = (arg0, arg1) => {
                const a = state0.a;
                state0.a = 0;
                try {
                    return __wbg_adapter_755(a, state0.b, arg0, arg1);
                } finally {
                    state0.a = a;
                }
            };
            const ret = new Promise(cb0);
            return ret;
        } finally {
            state0.a = state0.b = 0;
        }
    };
    imports.wbg.__wbg_new_2ff1f68f3676ea53 = function() {
        const ret = new Map();
        return ret;
    };
    imports.wbg.__wbg_new_638ebfaedbf32a5e = function(arg0) {
        const ret = new Uint8Array(arg0);
        return ret;
    };
    imports.wbg.__wbg_new_66b9434b4e59b63e = function() { return handleError(function () {
        const ret = new AbortController();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_9033836f0fffb971 = function() { return handleError(function (arg0) {
        const ret = new ReadableStreamDefaultReader(arg0);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_da9dc54c5db29dfa = function(arg0, arg1) {
        const ret = new Error(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_new_e213f63d18b0de01 = function() { return handleError(function (arg0, arg1) {
        const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
        return ret;
    }, arguments) };
    imports.wbg.__wbg_new_f6e53210afea8e45 = function() { return handleError(function () {
        const ret = new Headers();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_newfromslice_074c56947bd43469 = function(arg0, arg1) {
        const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_newnoargs_254190557c45b4ec = function(arg0, arg1) {
        const ret = new Function(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg_newwithlength_a167dcc7aaa3ba77 = function(arg0) {
        const ret = new Uint8Array(arg0 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_newwithstr_4fbb4e3ba652aee4 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = new WebSocket(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
        return ret;
    }, arguments) };
    imports.wbg.__wbg_newwithstrandinit_b5d168a29a3fd85f = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = new Request(getStringFromWasm0(arg0, arg1), arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_newwithstrsequence_f7e2d4848dd49d98 = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = new WebSocket(getStringFromWasm0(arg0, arg1), arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_next_5b3530e612fde77d = function(arg0) {
        const ret = arg0.next;
        return ret;
    };
    imports.wbg.__wbg_next_692e82279131b03c = function() { return handleError(function (arg0) {
        const ret = arg0.next();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_node_905d3e251edff8a2 = function(arg0) {
        const ret = arg0.node;
        return ret;
    };
    imports.wbg.__wbg_now_1e80617bcee43265 = function() {
        const ret = Date.now();
        return ret;
    };
    imports.wbg.__wbg_now_886b39d7ec380719 = function(arg0) {
        const ret = arg0.now();
        return ret;
    };
    imports.wbg.__wbg_open_28c0958dd0e03186 = function(arg0) {
        const ret = arg0.open();
        return ret;
    };
    imports.wbg.__wbg_open_65ec985b581cbed8 = function(arg0, arg1) {
        const ret = arg0.open(arg1);
        return ret;
    };
    imports.wbg.__wbg_opened_fc99ed1e3c1035f0 = function(arg0) {
        const ret = arg0.opened;
        return ret;
    };
    imports.wbg.__wbg_optionwallettxout_new = function(arg0) {
        const ret = OptionWalletTxOut.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_outpoint_unwrap = function(arg0) {
        const ret = OutPoint.__unwrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_performance_a221af8decc752fb = function(arg0) {
        const ret = arg0.performance;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_preparepayresponse_new = function(arg0) {
        const ret = PreparePayResponse.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_process_dc0fbacc7c1c06f7 = function(arg0) {
        const ret = arg0.process;
        return ret;
    };
    imports.wbg.__wbg_prototypesetcall_3d4a26c1ed734349 = function(arg0, arg1, arg2) {
        Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
    };
    imports.wbg.__wbg_pset_new = function(arg0) {
        const ret = Pset.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_psetinput_new = function(arg0) {
        const ret = PsetInput.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_psetoutput_new = function(arg0) {
        const ret = PsetOutput.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_psetsignatures_new = function(arg0) {
        const ret = PsetSignatures.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_push_330b2eb93e4e1212 = function(arg0, arg1) {
        const ret = arg0.push(arg1);
        return ret;
    };
    imports.wbg.__wbg_put_9fbd837e976dd910 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
        arg0.put(getStringFromWasm0(arg1, arg2), getArrayU8FromWasm0(arg3, arg4));
    }, arguments) };
    imports.wbg.__wbg_queueMicrotask_25d0739ac89e8c88 = function(arg0) {
        queueMicrotask(arg0);
    };
    imports.wbg.__wbg_queueMicrotask_4488407636f5bf24 = function(arg0) {
        const ret = arg0.queueMicrotask;
        return ret;
    };
    imports.wbg.__wbg_randomFillSync_ac0988aba3254290 = function() { return handleError(function (arg0, arg1) {
        arg0.randomFillSync(arg1);
    }, arguments) };
    imports.wbg.__wbg_read_bc925c758aa4d897 = function(arg0) {
        const ret = arg0.read();
        return ret;
    };
    imports.wbg.__wbg_readable_d8d52c48b16a493f = function(arg0) {
        const ret = arg0.readable;
        return ret;
    };
    imports.wbg.__wbg_readyState_b0d20ca4531d3797 = function(arg0) {
        const ret = arg0.readyState;
        return ret;
    };
    imports.wbg.__wbg_reason_97efd955be6394bd = function(arg0, arg1) {
        const ret = arg1.reason;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_recipient_new = function(arg0) {
        const ret = Recipient.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_registry_new = function(arg0) {
        const ret = Registry.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_remove_a94bdea90b3fa4b7 = function() { return handleError(function (arg0, arg1, arg2) {
        arg0.remove(getStringFromWasm0(arg1, arg2));
    }, arguments) };
    imports.wbg.__wbg_requestDevice_225891d9a80a7b19 = function(arg0, arg1) {
        const ret = arg0.requestDevice(arg1);
        return ret;
    };
    imports.wbg.__wbg_requestPort_07a56c2addedc9d1 = function(arg0) {
        const ret = arg0.requestPort();
        return ret;
    };
    imports.wbg.__wbg_requestPort_379ba5dd17575815 = function(arg0, arg1) {
        const ret = arg0.requestPort(arg1);
        return ret;
    };
    imports.wbg.__wbg_require_60cc747a6bc5215a = function() { return handleError(function () {
        const ret = module.require;
        return ret;
    }, arguments) };
    imports.wbg.__wbg_resolve_4055c623acdd6a1b = function(arg0) {
        const ret = Promise.resolve(arg0);
        return ret;
    };
    imports.wbg.__wbg_sendReport_c8d0c7c3c3abf30a = function() { return handleError(function (arg0, arg1, arg2, arg3) {
        const ret = arg0.sendReport(arg1, getArrayU8FromWasm0(arg2, arg3));
        return ret;
    }, arguments) };
    imports.wbg.__wbg_send_0f09f4487d932d86 = function() { return handleError(function (arg0, arg1) {
        arg0.send(arg1);
    }, arguments) };
    imports.wbg.__wbg_send_aa9cb445685f0fd0 = function() { return handleError(function (arg0, arg1, arg2) {
        arg0.send(getArrayU8FromWasm0(arg1, arg2));
    }, arguments) };
    imports.wbg.__wbg_send_bdda9fac7465e036 = function() { return handleError(function (arg0, arg1, arg2) {
        arg0.send(getStringFromWasm0(arg1, arg2));
    }, arguments) };
    imports.wbg.__wbg_serial_a0dde47bbd77e1b3 = function(arg0) {
        const ret = arg0.serial;
        return ret;
    };
    imports.wbg.__wbg_setTimeout_2966518f28aef92e = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = arg0.setTimeout(arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_setTimeout_7bb3429662ab1e70 = function(arg0, arg1) {
        const ret = setTimeout(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbg_setTimeout_db2dbaeefb6f39c7 = function() { return handleError(function (arg0, arg1) {
        const ret = setTimeout(arg0, arg1);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_set_1353b2a5e96bc48c = function(arg0, arg1, arg2) {
        arg0.set(getArrayU8FromWasm0(arg1, arg2));
    };
    imports.wbg.__wbg_set_3f1d0b984ed272ed = function(arg0, arg1, arg2) {
        arg0[arg1] = arg2;
    };
    imports.wbg.__wbg_set_453345bcda80b89a = function() { return handleError(function (arg0, arg1, arg2) {
        const ret = Reflect.set(arg0, arg1, arg2);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_set_90f6c0f7bd8c0415 = function(arg0, arg1, arg2) {
        arg0[arg1 >>> 0] = arg2;
    };
    imports.wbg.__wbg_set_b7f1cf4fae26fe2a = function(arg0, arg1, arg2) {
        const ret = arg0.set(arg1, arg2);
        return ret;
    };
    imports.wbg.__wbg_setbaudrate_88a136cc0a6d4b97 = function(arg0, arg1) {
        arg0.baudRate = arg1 >>> 0;
    };
    imports.wbg.__wbg_setbinaryType_37f3cd35d7775a47 = function(arg0, arg1) {
        arg0.binaryType = __wbindgen_enum_BinaryType[arg1];
    };
    imports.wbg.__wbg_setbody_c8460bdf44147df8 = function(arg0, arg1) {
        arg0.body = arg1;
    };
    imports.wbg.__wbg_setcache_90ca4ad8a8ad40d3 = function(arg0, arg1) {
        arg0.cache = __wbindgen_enum_RequestCache[arg1];
    };
    imports.wbg.__wbg_setcredentials_9cd60d632c9d5dfc = function(arg0, arg1) {
        arg0.credentials = __wbindgen_enum_RequestCredentials[arg1];
    };
    imports.wbg.__wbg_setfilters_58b1e783022cd737 = function(arg0, arg1) {
        arg0.filters = arg1;
    };
    imports.wbg.__wbg_setfilters_a0bf46d04ea0dd78 = function(arg0, arg1) {
        arg0.filters = arg1;
    };
    imports.wbg.__wbg_setheaders_0052283e2f3503d1 = function(arg0, arg1) {
        arg0.headers = arg1;
    };
    imports.wbg.__wbg_setmethod_9b504d5b855b329c = function(arg0, arg1, arg2) {
        arg0.method = getStringFromWasm0(arg1, arg2);
    };
    imports.wbg.__wbg_setmode_a23e1a2ad8b512f8 = function(arg0, arg1) {
        arg0.mode = __wbindgen_enum_RequestMode[arg1];
    };
    imports.wbg.__wbg_setname_832b43d4602cb930 = function(arg0, arg1, arg2) {
        arg0.name = getStringFromWasm0(arg1, arg2);
    };
    imports.wbg.__wbg_setonclose_159c0332c2d91b09 = function(arg0, arg1) {
        arg0.onclose = arg1;
    };
    imports.wbg.__wbg_setonerror_5d9bff045f909e89 = function(arg0, arg1) {
        arg0.onerror = arg1;
    };
    imports.wbg.__wbg_setoninputreport_5d827d5c7285b78f = function(arg0, arg1) {
        arg0.oninputreport = arg1;
    };
    imports.wbg.__wbg_setonmessage_5e486f326638a9da = function(arg0, arg1) {
        arg0.onmessage = arg1;
    };
    imports.wbg.__wbg_setonopen_3e43af381c2901f8 = function(arg0, arg1) {
        arg0.onopen = arg1;
    };
    imports.wbg.__wbg_setsignal_8c45ad1247a74809 = function(arg0, arg1) {
        arg0.signal = arg1;
    };
    imports.wbg.__wbg_signal_da4d466ce86118b5 = function(arg0) {
        const ret = arg0.signal;
        return ret;
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_8921f820c2ce3f12 = function() {
        const ret = typeof global === 'undefined' ? null : global;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_GLOBAL_THIS_f0a4409105898184 = function() {
        const ret = typeof globalThis === 'undefined' ? null : globalThis;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_SELF_995b214ae681ff99 = function() {
        const ret = typeof self === 'undefined' ? null : self;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_static_accessor_WINDOW_cde3890479c675ea = function() {
        const ret = typeof window === 'undefined' ? null : window;
        return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
    };
    imports.wbg.__wbg_status_3fea3036088621d6 = function(arg0) {
        const ret = arg0.status;
        return ret;
    };
    imports.wbg.__wbg_stringify_b98c93d0a190446a = function() { return handleError(function (arg0) {
        const ret = JSON.stringify(arg0);
        return ret;
    }, arguments) };
    imports.wbg.__wbg_subarray_70fd07feefe14294 = function(arg0, arg1, arg2) {
        const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
        return ret;
    };
    imports.wbg.__wbg_text_0f69a215637b9b34 = function() { return handleError(function (arg0) {
        const ret = arg0.text();
        return ret;
    }, arguments) };
    imports.wbg.__wbg_then_b33a773d723afa3e = function(arg0, arg1, arg2) {
        const ret = arg0.then(arg1, arg2);
        return ret;
    };
    imports.wbg.__wbg_then_e22500defe16819f = function(arg0, arg1) {
        const ret = arg0.then(arg1);
        return ret;
    };
    imports.wbg.__wbg_transaction_new = function(arg0) {
        const ret = Transaction.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_txdetails_new = function(arg0) {
        const ret = TxDetails.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_txid_new = function(arg0) {
        const ret = Txid.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_txoutdetails_new = function(arg0) {
        const ret = TxOutDetails.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_update_new = function(arg0) {
        const ret = Update.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_url_e5720dfacf77b05e = function(arg0, arg1) {
        const ret = arg1.url;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_validatedliquidexproposal_unwrap = function(arg0) {
        const ret = ValidatedLiquidexProposal.__unwrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_value_dd9372230531eade = function(arg0) {
        const ret = arg0.value;
        return ret;
    };
    imports.wbg.__wbg_versions_c01dfd4722a88165 = function(arg0) {
        const ret = arg0.versions;
        return ret;
    };
    imports.wbg.__wbg_wallettx_new = function(arg0) {
        const ret = WalletTx.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_wallettxout_new = function(arg0) {
        const ret = WalletTxOut.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_wbindgencbdrop_eb10308566512b88 = function(arg0) {
        const obj = arg0.original;
        if (obj.cnt-- == 1) {
            obj.a = 0;
            return true;
        }
        const ret = false;
        return ret;
    };
    imports.wbg.__wbg_wbindgendebugstring_99ef257a3ddda34d = function(arg0, arg1) {
        const ret = debugString(arg1);
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_wbindgenisfunction_8cee7dce3725ae74 = function(arg0) {
        const ret = typeof(arg0) === 'function';
        return ret;
    };
    imports.wbg.__wbg_wbindgenisobject_307a53c6bd97fbf8 = function(arg0) {
        const val = arg0;
        const ret = typeof(val) === 'object' && val !== null;
        return ret;
    };
    imports.wbg.__wbg_wbindgenisstring_d4fa939789f003b0 = function(arg0) {
        const ret = typeof(arg0) === 'string';
        return ret;
    };
    imports.wbg.__wbg_wbindgenisundefined_c4b71d073b92f3c5 = function(arg0) {
        const ret = arg0 === undefined;
        return ret;
    };
    imports.wbg.__wbg_wbindgenstringget_0f16a6ddddef376f = function(arg0, arg1) {
        const obj = arg1;
        const ret = typeof(obj) === 'string' ? obj : undefined;
        var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbg_wbindgenthrow_451ec1a8469d7eb6 = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_wolletdescriptor_new = function(arg0) {
        const ret = WolletDescriptor.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbg_writable_3d863701c7ab161b = function(arg0) {
        const ret = arg0.writable;
        return ret;
    };
    imports.wbg.__wbg_write_2e39e04a4c8c9e9d = function(arg0, arg1) {
        const ret = arg0.write(arg1);
        return ret;
    };
    imports.wbg.__wbg_xpub_new = function(arg0) {
        const ret = Xpub.__wrap(arg0);
        return ret;
    };
    imports.wbg.__wbindgen_cast_0e64dbf2b9cf7b6a = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 806, function: Function { arguments: [NamedExternref("HIDInputReportEvent")], shim_idx: 807, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 806, __wbg_adapter_20);
        return ret;
    };
    imports.wbg.__wbindgen_cast_15a787bee2baf894 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 2122, function: Function { arguments: [Externref], shim_idx: 2133, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 2122, __wbg_adapter_34);
        return ret;
    };
    imports.wbg.__wbindgen_cast_1c26d653ac31462b = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1281, function: Function { arguments: [NamedExternref("Event")], shim_idx: 1282, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 1281, __wbg_adapter_11);
        return ret;
    };
    imports.wbg.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(String) -> Externref`.
        const ret = getStringFromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_4625c577ab2ec9ee = function(arg0) {
        // Cast intrinsic for `U64 -> Externref`.
        const ret = BigInt.asUintN(64, arg0);
        return ret;
    };
    imports.wbg.__wbindgen_cast_9ae0607507abb057 = function(arg0) {
        // Cast intrinsic for `I64 -> Externref`.
        const ret = arg0;
        return ret;
    };
    imports.wbg.__wbindgen_cast_b3c9dd9879dfca69 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1281, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 1282, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 1281, __wbg_adapter_11);
        return ret;
    };
    imports.wbg.__wbindgen_cast_c3bb21eb93199689 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 2087, function: Function { arguments: [], shim_idx: 2088, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 2087, __wbg_adapter_6);
        return ret;
    };
    imports.wbg.__wbindgen_cast_c7a18157f636a887 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1281, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 1282, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 1281, __wbg_adapter_11);
        return ret;
    };
    imports.wbg.__wbindgen_cast_cb9088102bce6b30 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
        const ret = getArrayU8FromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_d19fdd27e8cc74d2 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1547, function: Function { arguments: [], shim_idx: 1548, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 1547, __wbg_adapter_29);
        return ret;
    };
    imports.wbg.__wbindgen_cast_d675b14c513ce161 = function(arg0, arg1) {
        // Cast intrinsic for `Closure(Closure { dtor_idx: 1281, function: Function { arguments: [NamedExternref("ErrorEvent")], shim_idx: 1282, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
        const ret = makeMutClosure(arg0, arg1, 1281, __wbg_adapter_11);
        return ret;
    };
    imports.wbg.__wbindgen_cast_d6cd19b81560fd6e = function(arg0) {
        // Cast intrinsic for `F64 -> Externref`.
        const ret = arg0;
        return ret;
    };
    imports.wbg.__wbindgen_init_externref_table = function() {
        const table = wasm.__wbindgen_export_4;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
        ;
    };

    return imports;
}

function __wbg_init_memory(imports, memory) {

}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedDataViewMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;


    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (typeof module !== 'undefined') {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();

    __wbg_init_memory(imports);

    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }

    const instance = new WebAssembly.Instance(module, imports);

    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (typeof module_or_path !== 'undefined') {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (typeof module_or_path === 'undefined') {
        module_or_path = new URL('lwk_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    __wbg_init_memory(imports);

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync };
export default __wbg_init;
