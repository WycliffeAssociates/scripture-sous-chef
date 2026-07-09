/**
 * Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
 * optional parallel map; `config` overrides the shipped defaults
 * (omitted ⇒ `Config::v1_defaults()`: language-agnostic rules on,
 * convention-dependent rules off). Returns findings with UTF-16 ranges.
 * @param {VrefMap} target
 * @param {VrefMap | null} [source]
 * @param {SousConfig | null} [config]
 * @returns {Findings}
 */
export function analyze_vref(target, source, config) {
    const ret = wasm.analyze_vref(target, isLikeNone(source) ? 0 : addToExternrefTable0(source), isLikeNone(config) ? 0 : addToExternrefTable0(config));
    return ret;
}

/**
 * Stateful analyze (ADR 0017). Same as [`analyze_vref`] but returns the
 * corpus `Stats`; pass it back as `prior` along with only the edited
 * verses in `target` to re-analyze incrementally — the changed books
 * supersede their prior entries and stateful rules re-judge the whole
 * corpus from the cache. Omit `prior` (and pass the whole corpus) on the
 * first call.
 *
 * `changed` (ADR 0043): with a `prior`, book codes (e.g. `["GEN"]`) naming
 * the books edited since that prior — only those are re-counted, while
 * findings still cover everything supplied (the complete-snapshot call at
 * roughly half full-pass cost). A promise, not a filter: name every edited
 * book or its counts go silently stale. Unknown codes are ignored; omit it
 * (or omit `prior`) for the original re-count-everything behavior.
 * @param {VrefMap} target
 * @param {VrefMap | null} [source]
 * @param {SousConfig | null} [config]
 * @param {Stats | null} [prior]
 * @param {string[] | null} [changed]
 * @returns {Analysis}
 */
export function analyze_vref_stateful(target, source, config, prior, changed) {
    var ptr0 = isLikeNone(changed) ? 0 : passArrayJsValueToWasm0(changed, wasm.__wbindgen_malloc);
    var len0 = WASM_VECTOR_LEN;
    const ret = wasm.analyze_vref_stateful(target, isLikeNone(source) ? 0 : addToExternrefTable0(source), isLikeNone(config) ? 0 : addToExternrefTable0(config), isLikeNone(prior) ? 0 : addToExternrefTable0(prior), ptr0, len0);
    return ret;
}

/**
 * The shipped English rule catalog — the reference text a consumer renders
 * (or keys a translation off). Complete by construction: one card per
 * `RuleId`.
 * @returns {RuleCatalog}
 */
export function rule_catalog() {
    const ret = wasm.rule_catalog();
    return ret;
}

/**
 * Drop a book from cached `Stats` (e.g. it was removed from the project),
 * returning the updated stats — the sanctioned deletion path so callers
 * don't mutate the opaque value's internals. `book` is a 3-letter USFM code
 * (e.g. `"GEN"`); an unknown code is a no-op.
 * @param {Stats} stats
 * @param {string} book
 * @returns {Stats}
 */
export function stats_remove_book(stats, book) {
    const ptr0 = passStringToWasm0(book, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.stats_remove_book(stats, ptr0, len0);
    return ret;
}
export function __wbg___wbindgen_is_undefined_244a92c34d3b6ec0(arg0) {
    const ret = arg0 === undefined;
    return ret;
}
export function __wbg___wbindgen_string_get_965592073e5d848c(arg0, arg1) {
    const obj = arg1;
    const ret = typeof(obj) === 'string' ? obj : undefined;
    var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
}
export function __wbg___wbindgen_throw_9c75d47bf9e7731e(arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1));
}
export function __wbg_parse_342d5616e14beccc() { return handleError(function (arg0, arg1) {
    const ret = JSON.parse(getStringFromWasm0(arg0, arg1));
    return ret;
}, arguments); }
export function __wbg_stringify_7fd5cae8859a6f10() { return handleError(function (arg0) {
    const ret = JSON.stringify(arg0);
    return ret;
}, arguments); }
export function __wbindgen_init_externref_table() {
    const table = wasm.__wbindgen_externrefs;
    const offset = table.grow(4);
    table.set(0, undefined);
    table.set(offset + 0, undefined);
    table.set(offset + 1, null);
    table.set(offset + 2, true);
    table.set(offset + 3, false);
}
function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
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

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;


let wasm;
export function __wbg_set_wasm(val) {
    wasm = val;
}
