/**
 * The resident analysis handle for the editor. Wraps [`ssc_galley::Galley`],
 * which owns the corpus, optional source, config, prep cache, and prior across
 * calls. The caller updates the corpus/source/config and asks for findings or
 * an inventory; it never threads a prior, stats, cache, or changed set.
 *
 * **Lifetime:** the handle owns wasm-linear-memory-resident state. JS **must**
 * call `free()` when swapping workspace or unmounting (the worker's `dispose`
 * message is the home for that). `FinalizationRegistry` is a backstop some
 * runtimes provide, never the contract — an un-`free`d handle leaks until the
 * worker itself is torn down.
 */
export class Galley {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        GalleyFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_galley_free(ptr, 0);
    }
    /**
     * Analyze the resident corpus; findings carry UTF-16 ranges, the same wire
     * shape as the stateless [`analyze_vref`].
     * @returns {Findings}
     */
    analyze() {
        const ret = wasm.galley_analyze(this.__wbg_ptr);
        return ret;
    }
    /**
     * Census (absolute inventory) over the resident corpus, serialized to the
     * ADR 0058 JSON string, exactly like the stateless [`census`].
     * @param {number | null} [example_cap]
     * @returns {string}
     */
    census(example_cap) {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.galley_census(this.__wbg_ptr, isLikeNone(example_cap) ? Number.MAX_SAFE_INTEGER : (example_cap) >>> 0);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Seed the handle. `source` is an optional parallel corpus; `config`
     * omitted ⇒ `Config::v1_defaults()`, exactly like the stateless exports.
     * The first `analyze` is a full cold pass.
     * @param {VrefCorpus} target
     * @param {VrefCorpus | null} [source]
     * @param {SousConfig | null} [config]
     */
    constructor(target, source, config) {
        const ret = wasm.galley_new(target, isLikeNone(source) ? 0 : addToExternrefTable0(source), isLikeNone(config) ? 0 : addToExternrefTable0(config));
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        GalleyFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Remove books by slug. Unknown slugs are no-ops; returns the number removed.
     * @param {string[]} slugs
     * @returns {number}
     */
    remove_books(slugs) {
        const ptr0 = passArrayJsValueToWasm0(slugs, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_remove_books(this.__wbg_ptr, ptr0, len0);
        return ret >>> 0;
    }
    /**
     * Reseed the whole corpus (project switch, git pull). Books absent from the
     * new corpus leave the prior and cache before it is adopted.
     * @param {VrefCorpus} target
     */
    replace_corpus(target) {
        const ret = wasm.galley_replace_corpus(this.__wbg_ptr, target);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Batch replace/insert whole books. Atomic (all-or-nothing): a rejected
     * batch leaves the handle unchanged. Does not analyze.
     * @param {BookUpdateIn[]} batch
     */
    update_books(batch) {
        const ptr0 = passArrayJsValueToWasm0(batch, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_update_books(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Swap the config. Required (not optional): a config change is explicit,
     * never an accidental reset to defaults. Equal config ⇒ no-op; otherwise
     * the prep cache clears and the prior is retained (provenance decides what
     * re-tallies).
     * @param {SousConfig} config
     */
    update_config(config) {
        const ret = wasm.galley_update_config(this.__wbg_ptr, config);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Swap the source corpus. The prior is retained; provenance stales the
     * same-slug target books whose source changed on the next analyze.
     * @param {VrefCorpus | null} [source]
     */
    update_source(source) {
        const ret = wasm.galley_update_source(this.__wbg_ptr, isLikeNone(source) ? 0 : addToExternrefTable0(source));
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) Galley.prototype[Symbol.dispose] = Galley.prototype.free;

/**
 * Analyze a vref corpus. `source` is an optional parallel corpus; `config`
 * overrides the shipped defaults (omitted ⇒ `Config::v1_defaults()`:
 * language-agnostic rules on, convention-dependent rules off). Returns
 * findings with UTF-16 ranges.
 * @param {VrefCorpus} target
 * @param {VrefCorpus | null} [source]
 * @param {SousConfig | null} [config]
 * @returns {Findings}
 */
export function analyze_vref(target, source, config) {
    const ret = wasm.analyze_vref(target, isLikeNone(source) ? 0 : addToExternrefTable0(source), isLikeNone(config) ? 0 : addToExternrefTable0(config));
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Stateful analyze (ADR 0017). Same as [`analyze_vref`] but returns the
 * corpus `Stats`; pass it back as `prior` along with the corpus (or just the
 * edited books) to re-analyze incrementally. Counting is proof-driven: each
 * supplied book re-tallies only if its content, same-slug source, or enabled
 * rule set differs from the prior's recorded provenance — the caller declares
 * nothing. Omit `prior` (and pass the whole corpus) on the first call.
 * @param {VrefCorpus} target
 * @param {VrefCorpus | null} [source]
 * @param {SousConfig | null} [config]
 * @param {Stats | null} [prior]
 * @returns {Analysis}
 */
export function analyze_vref_stateful(target, source, config, prior) {
    const ret = wasm.analyze_vref_stateful(target, isLikeNone(source) ? 0 : addToExternrefTable0(source), isLikeNone(config) ? 0 : addToExternrefTable0(config), isLikeNone(prior) ? 0 : addToExternrefTable0(prior));
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
}

/**
 * Census a vref corpus (ADR 0058): the knob-free absolute-count report
 * (`ssc_core::Inventory`, eight lanes) as opposed to `analyze`'s judged
 * findings. `target` is the same shape as [`analyze_vref`]'s; `example_cap`
 * bounds the example sites retained per row (omitted ⇒ core's default of 8;
 * a payload-size cap, not a statistical knob).
 *
 * Returns the `Inventory` serialized to a JSON **string**, deliberately not
 * a Tsify-typed object: the wire schema is ADR 0058's `Inventory` and
 * carries a top-level `schema` version field (currently `1`) that a viewer
 * checks before parsing. A JS/TS consumer owns its own types for this
 * shape — census is a cold, occasionally-invoked report, not the hot
 * `analyze` path that the rest of this boundary optimizes for.
 * @param {VrefCorpus} target
 * @param {number | null} [example_cap]
 * @returns {string}
 */
export function census(target, example_cap) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ret = wasm.census(target, isLikeNone(example_cap) ? Number.MAX_SAFE_INTEGER : (example_cap) >>> 0);
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
export function __wbg_Error_3639a60ed15f87e7(arg0, arg1) {
    const ret = Error(getStringFromWasm0(arg0, arg1));
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
const GalleyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_galley_free(ptr, 1));

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

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
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
