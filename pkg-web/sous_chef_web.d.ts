/* tslint:disable */
/* eslint-disable */
/**
 * A finding as the editor sees it: UTF-16 ranges, string code/severity.
 */
export interface Finding {
    sid: string;
    code: string;
    severity: string;
    /**
     * UTF-16 code-unit offsets into the verse text.
     */
    start: number;
    end: number;
    score: number | null;
}

/**
 * The return type. TS: `Finding[]`.
 */
export type Findings = Finding[];

/**
 * `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
 */
export type VrefMap = Record<string, string>;


/**
 * Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
 * optional parallel map. Returns findings with UTF-16 ranges.
 */
export function analyze_vref(target: VrefMap, source?: VrefMap | null): Findings;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_vref: (a: any, b: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
