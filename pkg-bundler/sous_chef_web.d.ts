/* tslint:disable */
/* eslint-disable */
/**
 * A positional batch of lazy args, parallel to the requested indices
 * (duplicates and `null`s preserved in order). TS: `(FindingArgs | null)[]`.
 */
export type FindingsArgsOut = (FindingArgs | null)[];

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
 * How loud a finding is. Maps 1:1 to the editor\'s annotation severity.
 */
export type Severity = "error" | "warning" | "info";

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
 * One existing-chapter-run replacement from JS. TS: `{ slug, chapter, keys,
 * texts }`. Every key must parse to `slug` and `chapter`; the run must already
 * exist. Whole-chapter insertion/removal/reorder is a whole-book update.
 */
export interface ChapterUpdateIn {
    slug: string;
    chapter: string;
    keys: string[];
    texts: string[];
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
 * Stable, machine-readable rule identity — a **closed set**.
 * Internally a cheap enum discriminant (zero per-finding
 * allocation); each variant serialises to its dotted code string
 * (e.g. `\"lex.excess-h-whitespace\"`) only at the wasm/IPC
 * boundary. The closed set is the typed surface consumers key
 * config and localisation off: Rust via [`RuleId::ALL`] +
 * exhaustive `match`; TS via the `Tsify` string union.
 */
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse" | "hyg.invalid-codepoint" | "hyg.replacement-run" | "prop.length-ratio" | "struct.source-marker-leftover" | "struct.merge-conflict-marker" | "punct.adjacency-anomaly" | "lex.duplicate-word" | "lex.punct-only-token" | "uni.combining-mark-without-base" | "uni.redundant-zero-width-space" | "uni.mixed-script-in-token" | "lex.repeated-character-run" | "uni.mixed-numeral-systems" | "punct.bracket-balance" | "punct.spacing-anomaly" | "case.sentence-initial-lowercase" | "case.inconsistent-word-casing" | "uni.rare-glyph" | "case.mixed-case-word" | "uni.mixed-normalization";

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
export type FindingArgs = { kind: "length-ratio"; ratio_pct: number; scope: LengthRatioScope } | { kind: "bracket-window"; window: DelimObservation[]; measure: BracketMeasure; majority: number; total: number } | { kind: "spacing-convention"; mark: string; left: SpacingSide | null; right: SpacingSide | null } | { kind: "casing-convention"; glyph: string | null; quoted: boolean; upper: number; total: number } | { kind: "word-casing"; word: string; upper: number; total: number } | { kind: "punct-only-rate"; count: number; units: number } | { kind: "adjacency-evidence"; pattern: string; k: number; lead_n: number; books: number; corpus: number } | { kind: "script-mix-evidence"; k: number; n: number; books: number; corpus: number } | { kind: "repeat-evidence"; ch: string; run: number } | { kind: "duplicate-word"; first_sid: string } | { kind: "rare-glyph"; glyph: string; count: number } | { kind: "mixed-case-word"; word: string; other: number; total: number } | { kind: "normalization"; affected: number; example: string };

/**
 * The analysis-input set every entry point takes as one typed object:
 * the complete target corpus, an optional parallel reference, and an
 * optional config (omitted ⇒ `Config::v1_defaults()`). A single typed
 * object rather than positional args because the shape exceeds
 * `(required, optional?)` — an optional before another optional is a
 * footgun positionally (owner decision, progress Entry 11). Shared by the
 * `Galley` constructor and stateless [`analyze_vref`]: one wire shape.
 * TS: `{ target: VrefCorpus, source?: VrefCorpus, config?: SousConfig }`.
 */
export interface GalleyArgs {
    target: VrefCorpus;
    source?: VrefCorpus;
    config?: SousConfig;
}

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
 * The closed, output-level classification of a rule\'s semantic inputs, used
 * by content identity and persisted-findings validation. It
 * describes which inputs may affect a rule\'s *findings* — never its substrate
 * or cache implementation. Rules never inspect it; the closed registry and
 * the generated wire schema do. It is an enum, not a bool, so a future
 * non-silent absence behavior or new input kind forces an explicit
 * exhaustive decision rather than silently entering the reference-removal
 * salvage path.
 *
 * The generated JS schema spells these `\"target-only\"` and
 * `\"target-and-reference-silent-when-absent\"`.
 */
export type InputDependency = "target-only" | "target-and-reference-silent-when-absent";

/**
 * The lazy args of one finding, cloned out of the resident `Galley` on the
 * low-volume detail path (§A.3.3). Absence (a no-interpolation rule) is
 * `null`, matching the record\'s cleared `has_args` bit. TS: `FindingArgs |
 * null`.
 */
export type FindingArgsOut = FindingArgs | null;

/**
 * The result of a resident mutation, as a JS string union `\"unchanged\" |
 * \"changed\"` (generated by Tsify). Mirrors `ssc_core::MutationEffect`; the
 * wrapper uses it to stale its published lazy-args lookup on `\"changed\"`
 * without re-deriving equality. TS: `MutationEffect`.
 */
export type MutationEffect = "unchanged" | "changed";

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
     * Analyze the resident corpus and return the packed findings buffer
     * (§A.1), the same wire shape as the stateless [`analyze_vref`] — a
     * 32-byte header plus one 16-byte record per finding, crossing wasm→JS as
     * one `Uint8Array` (transfer it worker→main with
     * `postMessage(bytes, [bytes.buffer])`). Decode with `decodeFindings(bytes,
     * keys)`; open a finding's full detail with [`finding_args`](Galley::finding_args)
     * under the header's `analysis_id`. Publishes the new `(analysis_id, args
     * table)` only after the pack succeeds; a pack failure leaves the previous
     * publication untouched (§3.3 `EngineCurrentWireStale`).
     */
    analyze(): Uint8Array;
    /**
     * Census (absolute inventory) over the resident corpus, serialized to the
     * ADR 0058 JSON string, exactly like the stateless [`census`].
     */
    census(example_cap?: number | null): string;
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
     */
    expectedAnalysisId(): bigint;
    /**
     * The target-only content identity (target + config + engine stamp,
     * excluding the reference), as a JS `bigint`. Same pure/analysis-free
     * lifecycle as [`expected_analysis_id`](Galley::expected_analysis_id); its
     * only use is the reference-present -> reference-absent persisted-findings
     * salvage (`ExpectedAnalysisIdentity.targetContextId`).
     */
    expectedTargetContextId(): bigint;
    /**
     * The lazy args of one finding from the last successful [`analyze`](Galley::analyze),
     * addressed by that analyze's `analysis_id` (the header value) and the
     * record `index`. `null` for a no-interpolation rule. Throws if no analyze
     * has succeeded, `analysis_id` is not the current publication's, or `index`
     * is out of range (§A.3.3). The `analysis_id` marshals as a JS `bigint`.
     */
    findingArgs(analysis_id: bigint, index: number): FindingArgsOut;
    /**
     * Batch form of [`finding_args`](Galley::finding_args): the lazy args for
     * `indices`, positionally parallel (duplicates and `null`s preserved). The
     * **whole batch** is validated before anything is cloned — one bad index
     * rejects the entire request (§A.3.3).
     */
    findingsArgs(analysis_id: bigint, indices: Uint32Array): FindingsArgsOut;
    /**
     * Whether a reference (source) corpus is currently resident — the
     * canonical presence bit for persistence validation
     * (`ExpectedAnalysisIdentity.hasReference`). Analysis-free.
     */
    hasReference(): boolean;
    /**
     * Seed the handle from a single typed args object (`{ target, source?,
     * config? }`; `config` omitted ⇒ `Config::v1_defaults()`, exactly like
     * the stateless exports). The first `analyze` is a full cold pass.
     */
    constructor(args: GalleyArgs);
    /**
     * Remove books by slug. Unknown slugs are no-ops; returns the number
     * removed (`0` means unchanged). A positive count stales the wire
     * publication (§3.1).
     */
    removeBooks(slugs: string[]): number;
    /**
     * Reseed the whole corpus (project switch, git pull). Books absent from the
     * new corpus leave the prior and cache before it is adopted. Returns the
     * `MutationEffect` — `"unchanged"` when the new corpus equals the current.
     */
    replaceCorpus(target: VrefCorpus): MutationEffect;
    /**
     * Replace the optional reference (source) corpus. The prior is retained;
     * provenance stales the same-slug target books whose source changed on the
     * next analyze. Returns the `MutationEffect`.
     */
    replaceSource(source?: VrefCorpus | null): MutationEffect;
    /**
     * Replace one complete book in place, or append it if its slug is new.
     * Atomic (all-or-nothing): a rejected block leaves the handle unchanged.
     * Returns the `MutationEffect` — `"unchanged"` for a byte-identical no-op.
     * Does not analyze.
     */
    updateBook(block: BookUpdateIn): MutationEffect;
    /**
     * Replace exactly one existing `(slug, chapter)` run. Atomic; a rejected
     * block leaves the handle unchanged. Returns the `MutationEffect`. Does
     * not analyze.
     */
    updateChapter(block: ChapterUpdateIn): MutationEffect;
    /**
     * Swap the config. Required (not optional): a config change is explicit,
     * never an accidental reset to defaults. Equal config ⇒ `"unchanged"`;
     * otherwise the prep cache clears and the prior is retained (provenance
     * decides what re-tallies).
     */
    updateConfig(config: SousConfig): MutationEffect;
}

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
 */
export function analyze_vref(args: GalleyArgs): Uint8Array;

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
