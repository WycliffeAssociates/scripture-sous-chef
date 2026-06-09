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
