/* tslint:disable */
/* eslint-disable */
/**
 * A finding as the editor sees it: UTF-16 ranges; `code`/`severity` are
 * the closed `RuleId`/`Severity` string unions (a new rule shows up as a
 * new union member, so exhaustive consumer maps fail to typecheck until
 * they handle it).
 */
export interface Finding {
    sid: string;
    code: RuleId;
    severity: Severity;
    /**
     * UTF-16 code-unit offsets into the verse text.
     */
    start: number;
    end: number;
    score: number | null;
}

/**
 * How loud a finding is. Maps 1:1 to the editor\'s annotation severity.
 */
export type Severity = "error" | "warning" | "info";

/**
 * Stable, machine-readable rule identity — a **closed set**.
 * Internally a cheap enum discriminant (zero per-finding
 * allocation); each variant serialises to its dotted code string
 * (e.g. `\"lex.excess-h-whitespace\"`) only at the wasm/IPC
 * boundary. The closed set is the typed surface consumers key
 * config and localisation off: Rust via [`RuleId::ALL`] +
 * exhaustive `match`; TS via the `Tsify` string union.
 */
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse";

/**
 * The return type. TS: `Finding[]`.
 */
export type Findings = Finding[];

/**
 * Which rules to run. `rules` maps a rule code to a flag; omit a rule to
 * keep it enabled (default-on). TS: `{ rules?: Partial<Record<RuleId,
 * boolean>> }` — `RuleId` is the same closed union carried on findings,
 * so the consumer\'s config and localisation maps key off one set.
 */
export interface SousConfig {
    rules?: Partial<Record<RuleId, boolean>>;
}

/**
 * `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
 */
export type VrefMap = Record<string, string>;


/**
 * Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
 * optional parallel map; `config` optionally disables rules (omitted ⇒
 * all rules run). Returns findings with UTF-16 ranges.
 */
export function analyze_vref(target: VrefMap, source?: VrefMap | null, config?: SousConfig | null): Findings;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_vref: (a: any, b: number, c: number) => any;
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
