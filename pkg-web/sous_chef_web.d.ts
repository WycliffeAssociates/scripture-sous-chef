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
 * An ordered, duplicate-preserving vref corpus as it arrives from JS:
 * parallel `keys`/`texts` arrays in caller-presented order (a `Corpus` is a
 * duplicate-preserving structure, not a map — unlike the retired
 * `VrefMap(Record<string, string>)`, this shape cannot silently collapse a
 * duplicate ref). TS: `{ keys: string[], texts: string[] }`.
 */
export interface VrefCorpus {
    keys: string[];
    texts: string[];
}

/**
 * Cached casing statistics, keyed by book so an edit supersedes only its book.
 */
export interface CasingStats {
    per_book: Record<string, BookCasing>;
}

/**
 * Cached mixed-case statistics, keyed by book so an edit supersedes only its
 * book. Corpus-wide profiles are the sums over books, derived at `judge`.
 */
export interface MixedCaseStats {
    per_book: Record<string, BookMixedCase>;
}

/**
 * Cached mixed-script aggregates, keyed by book so an edit supersedes only its
 * book. Corpus-wide counts are the sums over books, derived at `judge`.
 */
export interface MixedScriptStats {
    per_book: Record<string, BookMixedScript>;
}

/**
 * Cached proportionality statistics: the raw ratios keyed by book, so
 * an edit supersedes only its book and the median/MAD is derived at `judge`.
 */
export interface ProportionalityStats {
    per_book: Record<string, RatioObs[]>;
}

/**
 * Cached punct-only-token aggregates, partitioned by book so incremental
 * analysis can supersede one book without retaining occurrence sites.
 */
export interface PunctOnlyTokenStats {
    per_book: Record<string, BookPunctOnlyToken>;
}

/**
 * Cached punctuation-adjacency aggregates, keyed by book code so an edit
 * supersedes only its book. Corpus-wide `k` and `N_start` are the sums over
 * books, derived at `judge`.
 */
export interface PunctuationAdjacencyStats {
    per_book: Record<string, BookPunctuationAdjacency>;
}

/**
 * Cached rare-glyph statistics, keyed by book so an edit supersedes only its
 * book. Corpus-wide quantities are the sums over books, derived at `judge`.
 * Doubles as the future glyph-census accumulator (ADR 0053).
 */
export interface RareGlyphStats {
    per_book: Record<string, BookGlyphs>;
}

/**
 * Cached repeated-run aggregates, partitioned by book so incremental analysis
 * can supersede one book without retaining occurrence sites.
 */
export interface RepeatedCharacterRunStats {
    per_book: Record<string, BookRepeatedCharacterRun>;
}

/**
 * Cached spacing aggregates, keyed by book code so an edit supersedes only its
 * book. Corpus-wide counts are the sums over books, derived at `judge`.
 */
export interface PunctuationSpacingStats {
    per_book: Record<string, BookPunctuationSpacing>;
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
 * Forced-position first-letter tallies after one key. Raw and mergeable.
 */
export interface ForcedTally {
    upper?: number;
    lower?: number;
}

/**
 * How loud a finding is. Maps 1:1 to the editor\'s annotation severity.
 */
export type Severity = "error" | "warning" | "info";

/**
 * One book\'s aggregate contribution. Raw-text run counts include candidates
 * outside UAX #29 tokens; the word map includes only token types whose folded
 * form contains a run. Folding before that gate lets `Eee` establish the same
 * word convention as `eee` without storing general word frequencies.
 */
export interface BookRepeatedCharacterRun {
    lexical_units: number;
    cluster_runs: Record<string, number>;
    run_words: Record<string, number>;
}

/**
 * One book\'s aggregate contribution: per-lead-glyph run-start opportunity
 * counts and per-exact-pattern occurrence counts. **No sites** — spans are
 * re-derived from the text at `judge`, so this stays a few KB even on a
 * ZWSP-/punctuation-pervasive corpus. Patterns keyed by their exact run string
 * (`\",,\"`, `\"?!?\"`, `\"፤፤\"`), so `??`/`???`/`????` stay distinct.
 */
export interface BookPunctuationAdjacency {
    lead_opportunities: Record<string, number>;
    pattern_counts: Record<string, number>;
}

/**
 * One book\'s aggregate contribution: per-signature mixed-token counts and
 * per-script token counts (how many tokens contain each script at all — the
 * dominant-script denominator\'s raw material). **No sites.**
 */
export interface BookMixedScript {
    signature_counts: Record<string, number>;
    script_tokens: Record<string, number>;
}

/**
 * One book\'s aggregate contribution: whitespace-unit count and per-chunk
 * candidate counts, keyed by the exact chunk text.
 */
export interface BookPunctOnlyToken {
    lexical_units: number;
    chunks: Record<string, number>;
}

/**
 * One book\'s contribution: the full scalar inventory (census substrate) plus
 * word-level detail confined to locally-rare letter glyphs.
 */
export interface BookGlyphs {
    /**
     * Every scalar in the book (ADR 0053 census substrate).
     */
    inventory?: Record<string, number>;
    /**
     * `glyph → word → eligible occurrences of the glyph in that word`, for
     * letter glyphs whose per-book eligible count is ≤ [`RARE_CAP`]. \"Eligible\
     * = inside a single-script letter token (mixed-script tokens are owned by
     * `uni.mixed-script-in-token`).
     */
    rare?: Record<string, Record<string, number>>;
    /**
     * The container words referenced by `rare`: book-local token count + shape.
     */
    words?: Record<string, WordInfo>;
}

/**
 * One book\'s contribution: the per-word shape table.
 */
export interface BookMixedCase {
    words?: Record<string, ShapeProfile>;
}

/**
 * One book\'s contribution: the pruned word table plus the cased-word-start
 * count that drives the emergent gate.
 */
export interface BookCasing {
    words: Record<string, WordStats>;
    /**
     * Cased word-start observations in the book — the emergent gate input,
     * counted before pruning.
     */
    cased_starts: number;
}

/**
 * One book\'s per-mark **per-side per-class tallies**: the twelve counters
 * above, one set per mark (ADR 0054 2nd amendment, replacing the `[u64; 4]`
 * per-side table). **No sites** — spans re-derive from the text at `judge`, so
 * this stays a few dozen bytes per mark even corpus-wide.
 */
export interface BookPunctuationSpacing {
    per_mark: Record<string, number[]>;
}

/**
 * One case-folded word type\'s raw shape counts within one book. Raw and
 * mergeable — no dominance, no censoring — so book-supersede holds.
 */
export interface ShapeProfile {
    lower?: number;
    title?: number;
    allcaps?: number;
    other?: number;
}

/**
 * One container word\'s book-local facts: token count, and the titlecase /
 * forced shape of its (last-seen) occurrence. Only consulted for hapax
 * containers, which occur once, so last-seen is unambiguous there.
 */
export interface WordInfo {
    tokens?: number;
    titlecase?: boolean;
    forced?: boolean;
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
 * One rule\'s human-facing card (ADR 0038): plain-language title, what a
 * finding is, why it might deserve an eyeball, the enable question behind a
 * language-dependent toggle, and how its verdict works. `code` is the same
 * closed `RuleId` union carried on findings, so a UI can join cards to
 * findings and key translations off it.
 */
export interface RuleCard {
    code: RuleId;
    title: string;
    what: string;
    why: string;
    enable_question: string | null;
    /**
     * `\"deterministic\"` | `\"corpus-relative\"` | `\"source-relative\"`.
     * Corpus-relative rules carry scores and honour the sensitivity dial.
     */
    verdict: string;
}

/**
 * One verse\'s target/reference ratio, retained so `judge` can derive the
 * distribution and emit findings without the text. `local_idx` is
 * book-local (the per-book map already carries the slug); rebased to a
 * global `KeyIdx` only at `judge` time, against the current call\'s
 * `BookGroup::base`. `f32` ratio, `u32` byte length for the finding range.
 */
export interface RatioObs {
    local_idx: number;
    ratio: number;
    len: number;
}

/**
 * One violated side of a `punct.spacing-anomaly` finding (ADR 0054 2nd
 * amendment — the pooled class-conditioned model): the observed minority `form`
 * (`\"attached\"` or `\"spaced\"`) against the neighbour-content pool `class`
 * (`\"letter\"`, `\"number\"`, or `\"punct\"`) that judged it, how many of the mark\'s
 * occurrences **in that pool** take this form (`count`), and the pool\'s judged
 * occupancy `N_pool` (`total`). `count / total` is the descriptive rate the
 * Wilson-bound `score` deliberately isn\'t (ADR 0048).
 */
export interface SpacingSide {
    form: string;
    class: string;
    count: number;
    total: number;
}

/**
 * One whole-book update block from JS. TS: `{ slug, keys, texts }`. Chapter
 * or verse edits are the caller\'s to roll up to their whole book before
 * sending — the book is the invalidation unit.
 */
export interface BookUpdateIn {
    slug: string;
    keys: string[];
    texts: string[];
}

/**
 * One word\'s raw case tallies within one book. Mid-flow upper/lower (the
 * intrinsic profile), forced upper/lower split by the *bare* terminal glyph
 * (`after_glyph`) and by the *quote-context* glyph (`after_quote`, the `.\"`
 * classes ADR 0051 discarded to mid-flow), and book-initial. All raw — no
 * censoring, no trust — so book-supersede holds.
 */
export interface WordStats {
    mid_upper?: number;
    mid_lower?: number;
    book_initial?: ForcedTally;
    after_glyph?: Record<string, ForcedTally>;
    after_quote?: Record<string, ForcedTally>;
}

/**
 * Partial overrides for `case.mixed-case-word`\'s corpus-relative score.
 * Omitted fields keep core\'s defaults (ADR 0055): `emit_score_min` 0.95,
 * `recurrence_k` 32, `confidence_z` 1.96.
 */
export interface MixedCaseOverrides {
    emit_score_min?: number;
    recurrence_k?: number;
    confidence_z?: number;
}

/**
 * Partial overrides for `lex.punct-only-token`\'s corpus-relative score.
 * Omitted fields keep core\'s calibrated defaults (ADR 0030).
 */
export interface PunctOnlyTokenOverrides {
    convention_rate_per_10k?: number;
    confidence_z?: number;
    emit_score_min?: number;
}

/**
 * Partial overrides for `lex.repeated-character-run`\'s corpus-relative score.
 * Omitted fields keep core\'s calibrated defaults (ADR 0028).
 */
export interface RepeatedCharacterRunOverrides {
    convention_rate_per_10k?: number;
    word_recurrence_k?: number;
    confidence_z?: number;
    emit_score_min?: number;
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
 * Partial overrides for `punct.adjacency-anomaly`\'s knobs. Omitted fields
 * keep core\'s defaults (`convention_rate` 0.5, `confidence_z` 1.96,
 * `emit_score_min` 0.5). See ADR 0024.
 */
export interface PunctuationAdjacencyOverrides {
    convention_rate?: number;
    confidence_z?: number;
    emit_score_min?: number;
}

/**
 * Partial overrides for `punct.spacing-anomaly`\'s knobs. Omitted fields keep
 * core\'s defaults (ADR 0029, 0050): `emit_score_min` 0.5 (the emission floor
 * on the two-factor score), `confidence_z` 1.96 (an advanced calibration
 * knob), `minority_recurrence_k` 32 (the recurrence knee\'s absolute base),
 * and `minority_rate_per_10k` 40 (the knee\'s opportunity-proportional
 * allowance: `K = k + r·N/10 000` over the mark\'s total occurrences `N`).
 */
export interface PunctuationSpacingOverrides {
    emit_score_min?: number;
    confidence_z?: number;
    minority_recurrence_k?: number;
    minority_rate_per_10k?: number;
}

/**
 * Partial overrides for `uni.mixed-script-in-token`\'s corpus-relative score.
 * Omitted fields keep core\'s calibrated defaults (ADR 0047).
 */
export interface MixedScriptOverrides {
    convention_rate?: number;
    confidence_z?: number;
    breadth_convention_rate?: number;
    breadth_z?: number;
    breadth_min_books?: number;
    emit_score_min?: number;
}

/**
 * Partial overrides for `uni.rare-glyph`\'s corpus-relative score. Omitted
 * fields keep core\'s calibrated defaults (ADR 0053): `closure_threshold`
 * 0.0001 (the alphabet-closure gate — an advanced writing-system knob),
 * `recurrence_k` 2 (the sensitivity dial), `emit_score_min` 0.5.
 */
export interface RareGlyphOverrides {
    closure_threshold?: number;
    recurrence_k?: number;
    emit_score_min?: number;
}

/**
 * Partial overrides for the casing pair (`case.sentence-initial-lowercase`
 * and `case.inconsistent-word-casing`, which share one config). Omitted
 * fields keep core\'s calibrated defaults (ADR 0051/0052): `emit_score_min`
 * 0.95, `recurrence_k` 32, `confidence_z` 1.96, `trust_gate` 0.90.
 */
export interface CasingOverrides {
    emit_score_min?: number;
    recurrence_k?: number;
    confidence_z?: number;
    trust_gate?: number;
}

/**
 * Per-book provenance for a rule-count set: the hashes of the target text,
 * the same-slug source book, and the enabled counting-rule set the counts were
 * tallied from. A book re-tallies iff its current `Tally` differs from the one
 * recorded in [`Stats::tallied`] — staleness is proven from content, never
 * declared by the caller.
 *
 * The hash fields serialize as fixed-width lowercase hex strings (32 chars for
 * each u128, 16 for the u64) so the wire stays JSON-safe and deterministic and
 * never emits a JS `number` for a value past 2⁵³.
 */
export interface Tally {
    /**
     * `book_hash` of the target text these counts were tallied from.
     */
    text: string;
    /**
     * `book_hash` of the same-slug source book at tally time, or [`SOURCE_NONE`]
     * when no source (or no such book) existed. A target book\'s keys all parse
     * to its own slug and proportionality pairs by key, so a book\'s counts
     * depend on exactly one source book — its own slug.
     */
    source: string;
    /**
     * `rules_fp` of the enabled counting-rule set at tally time — records WHICH
     * rules\' contributions exist for this book. Text hashes alone cannot: a
     * prior built with a rule disabled has no counts for it even though every
     * text hash matches.
     */
    rules: string;
}

/**
 * Per-rule cached statistics — a **closed** union like `FindingArgs`, one
 * variant per stateful rule. The orchestration treats it opaquely; each
 * rule reduces into / judges from its own variant.
 *
 * What each variant caches varies: proportionality\'s per-verse ratios are
 * sparse; punctuation adjacency and repeated-character-run cache only
 * **aggregate counts** (never per-occurrence sites — those re-derive from the
 * text at `judge`). Casing (ADR 0051) caches a per-book **word case table** —
 * larger, but raw and mergeable, with the lexicon and per-glyph habit derived
 * at `judge`; both casing rules share it and it round-trips like the others.
 * Zero-width space carries no variant here: it is judged per-verse and
 * deterministically by `uni.redundant-zero-width-space` (ADR 0027), which needs
 * no corpus statistics.
 */
export type RuleStats = { Casing: CasingStats } | { Proportionality: ProportionalityStats } | { PunctuationAdjacency: PunctuationAdjacencyStats } | { PunctuationSpacing: PunctuationSpacingStats } | { RepeatedCharacterRun: RepeatedCharacterRunStats } | { PunctOnlyToken: PunctOnlyTokenStats } | { MixedScript: MixedScriptStats } | { GlyphInventory: RareGlyphStats } | { MixedCase: MixedCaseStats };

/**
 * Stable, machine-readable rule identity — a **closed set**.
 * Internally a cheap enum discriminant (zero per-finding
 * allocation); each variant serialises to its dotted code string
 * (e.g. `\"lex.excess-h-whitespace\"`) only at the wasm/IPC
 * boundary. The closed set is the typed surface consumers key
 * config and localisation off: Rust via [`RuleId::ALL`] +
 * exhaustive `match`; TS via the `Tsify` string union.
 */
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse" | "hyg.invalid-codepoint" | "hyg.replacement-run" | "prop.length-ratio" | "struct.source-marker-leftover" | "struct.merge-conflict-marker" | "punct.adjacency-anomaly" | "lex.duplicate-word" | "lex.punct-only-token" | "uni.combining-mark-without-base" | "uni.redundant-zero-width-space" | "uni.mixed-script-in-token" | "lex.repeated-character-run" | "uni.mixed-numeral-systems" | "punct.bracket-balance" | "punct.spacing-anomaly" | "case.sentence-initial-lowercase" | "case.inconsistent-word-casing" | "uni.rare-glyph" | "case.mixed-case-word";

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
export type FindingArgs = { kind: "length-ratio"; ratio_pct: number; scope: LengthRatioScope } | { kind: "bracket-window"; window: DelimObservation[]; measure: BracketMeasure; majority: number; total: number } | { kind: "spacing-convention"; mark: string; left: SpacingSide | null; right: SpacingSide | null } | { kind: "casing-convention"; glyph: string | null; quoted: boolean; upper: number; total: number } | { kind: "word-casing"; word: string; upper: number; total: number } | { kind: "punct-only-rate"; count: number; units: number } | { kind: "adjacency-evidence"; pattern: string; k: number; lead_n: number; books: number; corpus: number } | { kind: "script-mix-evidence"; k: number; n: number; books: number; corpus: number } | { kind: "repeat-evidence"; ch: string; run: number } | { kind: "duplicate-word"; first_sid: string } | { kind: "rare-glyph"; glyph: string; count: number } | { kind: "mixed-case-word"; word: string; other: number; total: number };

/**
 * The catalog plus the shared sensitivity dial: labelled `emit_score_min`
 * stops, identical for every corpus-relative rule (they all emit the same
 * score unit). Higher value = fewer, surer findings.
 */
export interface RuleCatalog {
    cards: RuleCard[];
    sensitivity_stops: SensitivityStop[];
}

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
    /**
     * Per-book provenance ([`Tally`]): what text, which same-slug source book,
     * and which enabled counting-rule set each book\'s counts came from. A book
     * re-tallies iff its current `Tally` differs from this record — staleness
     * is proven from content, never declared. Serialized with the stats wire
     * in deterministic (`BTreeMap`) order.
     */
    tallied: Record<string, Tally>;
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
 * Which of `punct.bracket-balance`\'s two corpus conventions a finding
 * broke — so the consumer knows which descriptive sentence the counts in
 * [`FindingArgs::BracketWindow`] belong to. `Pairing`: the family is closed
 * at all (`majority` = matched delimiter events); `ShortSpan`: the family\'s
 * pairs close within the window (`majority` = pairs closing in-window).
 */
export type BracketMeasure = "pairing" | "short-span";

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
    punctuation_adjacency?: PunctuationAdjacencyOverrides;
    punctuation_spacing?: PunctuationSpacingOverrides;
    repeated_character_run?: RepeatedCharacterRunOverrides;
    punct_only_token?: PunctOnlyTokenOverrides;
    mixed_script?: MixedScriptOverrides;
    rare_glyph?: RareGlyphOverrides;
    mixed_case?: MixedCaseOverrides;
}

export interface SensitivityStop {
    emit_score_min: number;
    label: string;
}


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
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Analyze the resident corpus; findings carry UTF-16 ranges, the same wire
     * shape as the stateless [`analyze_vref`].
     */
    analyze(): Findings;
    /**
     * Census (absolute inventory) over the resident corpus, serialized to the
     * ADR 0058 JSON string, exactly like the stateless [`census`].
     */
    census(example_cap?: number | null): string;
    /**
     * Seed the handle. `source` is an optional parallel corpus; `config`
     * omitted ⇒ `Config::v1_defaults()`, exactly like the stateless exports.
     * The first `analyze` is a full cold pass.
     */
    constructor(target: VrefCorpus, source?: VrefCorpus | null, config?: SousConfig | null);
    /**
     * Remove books by slug. Unknown slugs are no-ops; returns the number removed.
     */
    remove_books(slugs: string[]): number;
    /**
     * Reseed the whole corpus (project switch, git pull). Books absent from the
     * new corpus leave the prior and cache before it is adopted.
     */
    replace_corpus(target: VrefCorpus): void;
    /**
     * Batch replace/insert whole books. Atomic (all-or-nothing): a rejected
     * batch leaves the handle unchanged. Does not analyze.
     */
    update_books(batch: BookUpdateIn[]): void;
    /**
     * Swap the config. Required (not optional): a config change is explicit,
     * never an accidental reset to defaults. Equal config ⇒ no-op; otherwise
     * the prep cache clears and the prior is retained (provenance decides what
     * re-tallies).
     */
    update_config(config: SousConfig): void;
    /**
     * Swap the source corpus. The prior is retained; provenance stales the
     * same-slug target books whose source changed on the next analyze.
     */
    update_source(source?: VrefCorpus | null): void;
}

/**
 * Analyze a vref corpus. `source` is an optional parallel corpus; `config`
 * overrides the shipped defaults (omitted ⇒ `Config::v1_defaults()`:
 * language-agnostic rules on, convention-dependent rules off). Returns
 * findings with UTF-16 ranges.
 */
export function analyze_vref(target: VrefCorpus, source?: VrefCorpus | null, config?: SousConfig | null): Findings;

/**
 * Stateful analyze (ADR 0017). Same as [`analyze_vref`] but returns the
 * corpus `Stats`; pass it back as `prior` along with the corpus (or just the
 * edited books) to re-analyze incrementally. Counting is proof-driven: each
 * supplied book re-tallies only if its content, same-slug source, or enabled
 * rule set differs from the prior's recorded provenance — the caller declares
 * nothing. Omit `prior` (and pass the whole corpus) on the first call.
 */
export function analyze_vref_stateful(target: VrefCorpus, source?: VrefCorpus | null, config?: SousConfig | null, prior?: Stats | null): Analysis;

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
 */
export function census(target: VrefCorpus, example_cap?: number | null): string;

/**
 * The shipped English rule catalog — the reference text a consumer renders
 * (or keys a translation off). Complete by construction: one card per
 * `RuleId`.
 */
export function rule_catalog(): RuleCatalog;

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
    readonly __wbg_galley_free: (a: number, b: number) => void;
    readonly analyze_vref: (a: any, b: number, c: number) => [number, number, number];
    readonly analyze_vref_stateful: (a: any, b: number, c: number, d: number) => [number, number, number];
    readonly census: (a: any, b: number) => [number, number, number, number];
    readonly galley_analyze: (a: number) => any;
    readonly galley_census: (a: number, b: number) => [number, number];
    readonly galley_new: (a: any, b: number, c: number) => [number, number, number];
    readonly galley_remove_books: (a: number, b: number, c: number) => number;
    readonly galley_replace_corpus: (a: number, b: any) => [number, number];
    readonly galley_update_books: (a: number, b: number, c: number) => [number, number];
    readonly galley_update_config: (a: number, b: any) => [number, number];
    readonly galley_update_source: (a: number, b: number) => [number, number];
    readonly rule_catalog: () => any;
    readonly stats_remove_book: (a: any, b: number, c: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
