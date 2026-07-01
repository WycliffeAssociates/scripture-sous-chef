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
 * A flag candidate: a lowercase token observed after a terminal glyph.
 * Retained so `judge` can emit findings without re-scanning the text.
 *
 * `sid` is a `Copy` [`Sid`] natively — building it costs nothing in the hot
 * `reduce` loop and `judge` reads it back directly — yet it still crosses
 * the wasm boundary as the canonical `\"GEN 1:1\"` **string** (via
 * [`sid_as_string`] + the tsify `type` override), so `Stats` round-trips as
 * a typed value the shell holds opaquely with no hand-rolled wrapper
 * (ADR 0017). The string is materialised only when serde actually
 * serialises — never on the native analysis path.
 */
export interface LowerSite {
    sid: string;
    /**
     * Byte offsets of the lowercase grapheme within its verse.
     */
    start: number;
    end: number;
    glyph: string;
}

/**
 * Cached casing statistics, keyed by book code (e.g. `\"GEN\"`) so an edit
 * supersedes only its book. The corpus-wide `P(upper | glyph)` is the sum
 * of the per-book counts, derived at `judge` time.
 */
export interface CasingStats {
    per_book: Record<string, BookCasing>;
}

/**
 * Cached proportionality statistics: the raw ratios keyed by book code, so
 * an edit supersedes only its book and the median/MAD is derived at `judge`.
 */
export interface ProportionalityStats {
    per_book: Record<string, RatioObs[]>;
}

/**
 * Counts behind `P(upper | glyph) = upper / total` for one terminal glyph.
 */
export interface Tally {
    upper: number;
    total: number;
}

/**
 * Findings plus the corpus [`Stats`] to cache for incremental re-analysis.
 */
export interface Analysis {
    findings: Finding[];
    /**
     * Caller-opaque cache: hold it and pass it back as `prior` next call.
     */
    stats: Stats;
}

/**
 * How loud a finding is. Maps 1:1 to the editor\'s annotation severity.
 */
export type Severity = "error" | "warning" | "info";

/**
 * One book\'s contribution: the per-glyph counts, the lowercase flag
 * candidates, and the cased-letter tally that drives the emergent gate.
 */
export interface BookCasing {
    counts: Record<string, Tally>;
    lower_sites: LowerSite[];
    cased_letters: number;
    total_letters: number;
}

/**
 * One delimiter seen inside a `punct.bracket-balance` window: which verse
 * (`sid` as the canonical `\"GEN 1:1\"` string), its glyph, whether it opens
 * or closes, and whether the matcher paired it. The whole list lets a
 * reviewer see the full bracket context of the window and decide what is
 * actually missing — not just stare at the lone orphan. `sid` is a string
 * (not the byte-offset `Span` other findings use) because each observation
 * lives in a *different* verse; the orphan\'s own precise range is carried
 * on the `Finding`.
 */
export interface DelimObservation {
    sid: string;
    glyph: string;
    role: DelimRole;
    matched: boolean;
}

/**
 * One verse\'s target/reference ratio, retained so `judge` can derive the
 * distribution and emit findings without the text. Wire-friendly (canonical
 * `sid` string, `f32` ratio, `u32` byte length for the finding range).
 */
export interface RatioObs {
    sid: string;
    ratio: number;
    len: number;
}

/**
 * Partial overrides for `case.sentence-initial-lowercase`\'s knobs. Omitted
 * fields keep core\'s calibrated defaults (`threshold` 0.99,
 * `min_samples` 200).
 */
export interface CasingOverrides {
    threshold?: number;
    min_samples?: number;
}

/**
 * Partial overrides for `prop.length-ratio`\'s knobs. Omitted fields keep
 * core\'s calibrated defaults (`z_threshold` 3.5, `min_verses` 50).
 */
export interface ProportionalityOverrides {
    z_threshold?: number;
    min_verses?: number;
}

/**
 * Per-rule cached statistics — a **closed** union like `FindingArgs`, one
 * variant per stateful rule. The orchestration treats it opaquely; each
 * rule reduces into / judges from its own variant.
 */
export type RuleStats = { Casing: CasingStats } | { Proportionality: ProportionalityStats };

/**
 * Stable, machine-readable rule identity — a **closed set**.
 * Internally a cheap enum discriminant (zero per-finding
 * allocation); each variant serialises to its dotted code string
 * (e.g. `\"lex.excess-h-whitespace\"`) only at the wasm/IPC
 * boundary. The closed set is the typed surface consumers key
 * config and localisation off: Rust via [`RuleId::ALL`] +
 * exhaustive `match`; TS via the `Tsify` string union.
 */
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse" | "hyg.invalid-codepoint" | "prop.length-ratio" | "struct.source-marker-leftover" | "struct.merge-conflict-marker" | "punct.repeated-punct" | "lex.duplicate-word" | "lex.punct-only-token" | "uni.combining-mark-without-base" | "uni.mixed-script-in-token" | "lex.repeated-character-run" | "uni.mixed-numeral-systems" | "punct.placeholder-leftover" | "punct.bracket-balance" | "punct.space-before-punct" | "case.sentence-initial-lowercase";

/**
 * Structured message arguments — the additive payload ADR 0010 §6
 * anticipated. A **closed** discriminated union, like `RuleId`: rules
 * whose localised message interpolates values add a variant here, and
 * the consumer\'s ICU layer renders from it. Never a rendered string.
 * Deterministic no-interpolation rules carry `None` on the finding.
 *
 * Not `Copy`: the `BracketWindow` payload owns a `Vec`. Findings are
 * collected into `Vec`s and never copied on a hot path, so this costs
 * nothing real (ADR 0016).
 */
export type FindingArgs = { kind: "length-ratio"; ratio_pct: number; scope: LengthRatioScope } | { kind: "bracket-window"; window: DelimObservation[] } | { kind: "duplicate-word"; first_sid: string };

/**
 * The return type. TS: `Finding[]`.
 */
export type Findings = Finding[];

/**
 * What `analyze_stateful` returns and the shell threads back. It is a
 * strongly-typed value across the wasm boundary, but **treated as opaque**:
 * the caller holds and round-trips it and should not depend on its shape.
 * To drop a book (e.g. it was deleted from the project), call
 * [`Stats::remove_book`] and omit those verses from the next `map` —
 * supersede only *replaces* the books you supply, it never removes.
 */
export interface Stats {
    rules: Partial<Record<RuleId, RuleStats>>;
}

/**
 * Whether an observed delimiter opens or closes.
 */
export type DelimRole = "open" | "close";

/**
 * Which distribution flagged a `prop.length-ratio` verse, with the robust
 * z-score(s) that did. Modelled so a scope cannot exist without its
 * score(s): `Both` carries both, the single scopes carry one. The sign of
 * `z` is informative (negative = shorter than the median).
 */
export type LengthRatioScope = { Book: { z: number } } | { Project: { z: number } } | { Both: { book_z: number; project_z: number } };

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
    casing?: CasingOverrides;
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

/**
 * Stateful analyze (ADR 0017). Same as [`analyze_vref`] but returns the
 * corpus `Stats`; pass it back as `prior` along with only the edited
 * verses in `target` to re-analyze incrementally — the changed books
 * supersede their prior entries and stateful rules re-judge the whole
 * corpus from the cache. Omit `prior` (and pass the whole corpus) on the
 * first call.
 */
export function analyze_vref_stateful(target: VrefMap, source?: VrefMap | null, config?: SousConfig | null, prior?: Stats | null): Analysis;

/**
 * Drop a book from cached `Stats` (e.g. it was removed from the project),
 * returning the updated stats — the sanctioned deletion path so callers
 * don't mutate the opaque value's internals. `book` is a 3-letter USFM code
 * (e.g. `"GEN"`); an unknown code is a no-op.
 */
export function stats_remove_book(stats: Stats, book: string): Stats;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_vref: (a: any, b: number, c: number) => any;
    readonly analyze_vref_stateful: (a: any, b: number, c: number, d: number) => any;
    readonly stats_remove_book: (a: any, b: number, c: number) => any;
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
