/* @ts-self-types="./sous_chef_web.d.ts" */

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
     * Analyze the resident corpus and return the packed findings buffer
     * (§A.1), the same wire shape as the stateless [`analyze_vref`] — a
     * 32-byte header plus one 16-byte record per finding, crossing wasm→JS as
     * one `Uint8Array` (transfer it worker→main with
     * `postMessage(bytes, [bytes.buffer])`). Decode with `decodeFindings(bytes,
     * keys)`; open a finding's full detail with [`finding_args`](Galley::finding_args)
     * under the header's `analysis_id`. Publishes the new `(analysis_id, args
     * table)` only after the pack succeeds; a pack failure leaves the previous
     * publication untouched (§3.3 `EngineCurrentWireStale`).
     * @returns {Uint8Array}
     */
    analyze() {
        const ret = wasm.galley_analyze(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
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
     * The content-derived identity of the current resident inputs (target +
     * reference presence/content + config + engine stamp), as a JS `bigint`.
     * Pure and analysis-free — it folds the corpus's owned per-book hashes
     * (O(book count), no verse walk), so it is callable **before the first
     * `analyze`** and while the handle is dirty. This is the id a persisted
     * buffer must carry to be reused for the current inputs
     * (`decodePersistedFindings`'s `ExpectedAnalysisIdentity.analysisId`). It
     * tracks the current inputs, so it diverges from the last published header
     * id the moment a mutation changes an input.
     * @returns {bigint}
     */
    expectedAnalysisId() {
        const ret = wasm.galley_expectedAnalysisId(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * The target-only content identity (target + config + engine stamp,
     * excluding the reference), as a JS `bigint`. Same pure/analysis-free
     * lifecycle as [`expected_analysis_id`](Galley::expected_analysis_id); its
     * only use is the reference-present -> reference-absent persisted-findings
     * salvage (`ExpectedAnalysisIdentity.targetContextId`).
     * @returns {bigint}
     */
    expectedTargetContextId() {
        const ret = wasm.galley_expectedTargetContextId(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * The lazy args of one finding from the last successful [`analyze`](Galley::analyze),
     * addressed by that analyze's `analysis_id` (the header value) and the
     * record `index`. `null` for a no-interpolation rule. Throws if no analyze
     * has succeeded, `analysis_id` is not the current publication's, or `index`
     * is out of range (§A.3.3). The `analysis_id` marshals as a JS `bigint`.
     * @param {bigint} analysis_id
     * @param {number} index
     * @returns {FindingArgsOut}
     */
    findingArgs(analysis_id, index) {
        const ret = wasm.galley_findingArgs(this.__wbg_ptr, analysis_id, index);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Batch form of [`finding_args`](Galley::finding_args): the lazy args for
     * `indices`, positionally parallel (duplicates and `null`s preserved). The
     * **whole batch** is validated before anything is cloned — one bad index
     * rejects the entire request (§A.3.3).
     * @param {bigint} analysis_id
     * @param {Uint32Array} indices
     * @returns {FindingsArgsOut}
     */
    findingsArgs(analysis_id, indices) {
        const ptr0 = passArray32ToWasm0(indices, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_findingsArgs(this.__wbg_ptr, analysis_id, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Whether a reference (source) corpus is currently resident — the
     * canonical presence bit for persistence validation
     * (`ExpectedAnalysisIdentity.hasReference`). Analysis-free.
     * @returns {boolean}
     */
    hasReference() {
        const ret = wasm.galley_hasReference(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Seed the handle from a single typed args object (`{ target, source?,
     * config? }`; `config` omitted ⇒ `Config::v1_defaults()`, exactly like
     * the stateless exports). The first `analyze` is a full cold pass.
     * @param {GalleyArgs} args
     */
    constructor(args) {
        const ret = wasm.galley_new(args);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        GalleyFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Remove books by slug. Unknown slugs are no-ops; returns the number
     * removed (`0` means unchanged). A positive count stales the wire
     * publication (§3.1).
     * @param {string[]} slugs
     * @returns {number}
     */
    removeBooks(slugs) {
        const ptr0 = passArrayJsValueToWasm0(slugs, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.galley_removeBooks(this.__wbg_ptr, ptr0, len0);
        return ret >>> 0;
    }
    /**
     * Reseed the whole corpus (project switch, git pull). Books absent from the
     * new corpus leave the prior and cache before it is adopted. Returns the
     * `MutationEffect` — `"unchanged"` when the new corpus equals the current.
     * @param {VrefCorpus} target
     * @returns {MutationEffect}
     */
    replaceCorpus(target) {
        const ret = wasm.galley_replaceCorpus(this.__wbg_ptr, target);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Replace the optional reference (source) corpus. The prior is retained;
     * provenance stales the same-slug target books whose source changed on the
     * next analyze. Returns the `MutationEffect`.
     * @param {VrefCorpus | null} [source]
     * @returns {MutationEffect}
     */
    replaceSource(source) {
        const ret = wasm.galley_replaceSource(this.__wbg_ptr, isLikeNone(source) ? 0 : addToExternrefTable0(source));
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Replace one complete book in place, or append it if its slug is new.
     * Atomic (all-or-nothing): a rejected block leaves the handle unchanged.
     * Returns the `MutationEffect` — `"unchanged"` for a byte-identical no-op.
     * Does not analyze.
     * @param {BookUpdateIn} block
     * @returns {MutationEffect}
     */
    updateBook(block) {
        const ret = wasm.galley_updateBook(this.__wbg_ptr, block);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Replace exactly one existing `(slug, chapter)` run. Atomic; a rejected
     * block leaves the handle unchanged. Returns the `MutationEffect`. Does
     * not analyze.
     * @param {ChapterUpdateIn} block
     * @returns {MutationEffect}
     */
    updateChapter(block) {
        const ret = wasm.galley_updateChapter(this.__wbg_ptr, block);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Swap the config. Required (not optional): a config change is explicit,
     * never an accidental reset to defaults. Equal config ⇒ `"unchanged"`;
     * otherwise the prep cache clears and the prior is retained (provenance
     * decides what re-tallies).
     * @param {SousConfig} config
     * @returns {MutationEffect}
     */
    updateConfig(config) {
        const ret = wasm.galley_updateConfig(this.__wbg_ptr, config);
        return ret;
    }
}
if (Symbol.dispose) Galley.prototype[Symbol.dispose] = Galley.prototype.free;

/**
 * Analyze a vref corpus and return the packed findings buffer (§A.1): a
 * 32-byte header plus one fixed 16-byte record per finding, ready to cross
 * wasm→JS as one `Uint8Array` and worker→main as a transferred
 * `ArrayBuffer`. The header carries the same content-derived `analysis_id`
 * a resident [`Galley`] would mint for the same target + optional reference
 * + config (this one-shot path hashes both supplied corpora fresh).
 *
 * This is the compact one-shot surface: list-row summaries come from the
 * per-code digest packed in each record, but full `FindingArgs` are **not**
 * reachable — there is no args accessor without a resident handle. A
 * consumer needing detailed messages uses [`Galley`]. Decode with the
 * official `decodeFindings(bytes, target.keys)`.
 * @param {GalleyArgs} args
 * @returns {Uint8Array}
 */
export function analyze_vref(args) {
    const ret = wasm.analyze_vref(args);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v1;
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
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_3639a60ed15f87e7: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_is_undefined_244a92c34d3b6ec0: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_string_get_965592073e5d848c: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_9c75d47bf9e7731e: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_parse_342d5616e14beccc: function() { return handleError(function (arg0, arg1) {
            const ret = JSON.parse(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_stringify_7fd5cae8859a6f10: function() { return handleError(function (arg0) {
            const ret = JSON.stringify(arg0);
            return ret;
        }, arguments); },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./sous_chef_web_bg.js": import0,
    };
}

const GalleyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_galley_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
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

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
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

function passArray32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getUint32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
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

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
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

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('sous_chef_web_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
