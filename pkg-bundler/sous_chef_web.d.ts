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
    /**
     * Structured args for the consumer\'s interpolated message (the
     * `FindingArgs` closed union); `None` for no-interpolation rules.
     */
    args: FindingArgs | null;
}

/**
 * How loud a finding is. Maps 1:1 to the editor\'s annotation severity.
 */
export type Severity = "error" | "warning" | "info";

/**
 * Partial overrides for `prop.length-ratio`\'s knobs. Omitted fields keep
 * core\'s calibrated defaults (`z_threshold` 3.5, `min_verses` 50).
 */
export interface ProportionalityOverrides {
    z_threshold?: number;
    min_verses?: number;
}

/**
 * Stable, machine-readable rule identity — a **closed set**.
 * Internally a cheap enum discriminant (zero per-finding
 * allocation); each variant serialises to its dotted code string
 * (e.g. `\"lex.excess-h-whitespace\"`) only at the wasm/IPC
 * boundary. The closed set is the typed surface consumers key
 * config and localisation off: Rust via [`RuleId::ALL`] +
 * exhaustive `match`; TS via the `Tsify` string union.
 */
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse" | "prop.length-ratio" | "struct.source-marker-leftover" | "punct.repeated-punct" | "lex.duplicate-word" | "lex.punct-only-token" | "uni.combining-mark-without-base" | "uni.mixed-script-in-token" | "lex.repeated-character-run" | "uni.mixed-numeral-systems" | "punct.placeholder-leftover" | "punct.bracket-balance" | "punct.space-before-punct" | "case.sentence-initial-lowercase";

/**
 * Structured message arguments — the additive payload ADR 0010 §6
 * anticipated. A **closed** discriminated union, like `RuleId`: rules
 * whose localised message interpolates values add a variant here, and
 * the consumer\'s ICU layer renders from it. Never a rendered string.
 * Deterministic no-interpolation rules carry `None` on the finding.
 */
export type FindingArgs = { kind: "length-ratio"; ratio_pct: number; robust_z: number };

/**
 * The return type. TS: `Finding[]`.
 */
export type Findings = Finding[];

/**
 * Which rules to run, plus per-rule knobs. `rules` maps a rule code to a
 * flag; omit a rule to keep it enabled (default-on). TS: `{ rules?:
 * Partial<Record<RuleId, boolean>>, proportionality?: … }` — `RuleId` is
 * the same closed union carried on findings, so the consumer\'s config
 * and localisation maps key off one set.
 */
export interface SousConfig {
    rules?: Partial<Record<RuleId, boolean>>;
    proportionality?: ProportionalityOverrides;
}

/**
 * `{ sid -> text }` as it arrives from JS. TS: `Record<string, string>`.
 */
export type VrefMap = Record<string, string>;


/**
 * Analyze a vref text map. `target` is `{ sid -> text }`; `source` is an
 * optional parallel map; `config` overrides the shipped defaults
 * (omitted ⇒ `Config::v1_defaults()`: language-agnostic rules on,
 * convention-dependent rules off). Returns findings with UTF-16 ranges.
 */
export function analyze_vref(target: VrefMap, source?: VrefMap | null, config?: SousConfig | null): Findings;
