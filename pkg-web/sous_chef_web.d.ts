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
 * One rule\'s human-facing card (ADR 0038, amended by ADR 0070): plain-language title, what a
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
     */
    verdict: string;
    /**
     * `\"fixed\"` or `\"mapped\"`; independent of the rule\'s verdict class.
     */
    review_control: string;
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
 * Partial overrides for `lex.untranslated-word`\'s knobs (Phase C/D, source-
 * paired tier plan). Omitted fields keep core\'s provisional defaults —
 * **not yet calibrated** (Phase D\'s job; see
 * `documentation/calibration/` for the running calibration doc). The rule
 * ships default-OFF (`Config::v1_defaults()` disables it) until Phase D
 * adjudicates default-on/off.
 */
export interface UntranslatedWordsOverrides {
    corpus_gate_share?: number;
    word_recurrence_k?: number;
    run_bonus?: number;
    emit_score_min?: number;
}

/**
 * Partial overrides for `prop.length-ratio`\'s knobs. Omitted fields keep
 * core\'s calibrated defaults (`z_long`/`z_short` 3.5, `min_verses` 50).
 * The two thresholds are separate knobs (ADR 0069, asymmetric spread): the
 * UI\'s fine-tune panel exposes them as two trims, \"longer than typical\" /
 * \"shorter than typical\".
 */
export interface ProportionalityOverrides {
    z_long?: number;
    z_short?: number;
    min_verses?: number;
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
 * Partial overrides for `uni.nonletter-usage-anomaly`\'s corpus-relative score.
 * Omitted fields keep core\'s calibrated defaults — the constants ADR 0071 froze:
 * `emit_score_min` 0.75 (the adjudicated Review Depth midpoint), `rarity_k` 8,
 * `placement_min_pool` 30, placement\'s opportunity-proportional knee
 * `K = 32 + 40·N/10⁴`, sequence\'s `K = 8 + 40·N/10⁴`, and the support gates below
 * which a channel abstains rather than inventing a convention.
 *
 * Setting either `*_rate_per_10k` to `0` makes that knee flat, which is a
 * documented regression rather than a tuning choice: a flat knee silences the
 * slip clouds a large translation accrues with volume.
 *
 * Prefer moving Review Depth to a per-knob override: depth resolves the policy
 * values together, so a hand-set support gate can silently contradict the floor
 * it ships with.
 */
export interface NonletterUsageOverrides {
    emit_score_min?: number;
    rarity_min_exposure?: number;
    rarity_k?: number;
    placement_min_pool?: number;
    placement_k?: number;
    placement_rate_per_10k?: number;
    placement_z?: number;
    sequence_min_leads?: number;
    sequence_k?: number;
    sequence_rate_per_10k?: number;
    sequence_z?: number;
    continuation_min_support?: number;
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
export type RuleId = "lex.excess-h-whitespace" | "hyg.tab-in-body" | "hyg.control-chars" | "hyg.zero-width-misuse" | "hyg.empty-verse" | "hyg.invalid-codepoint" | "hyg.replacement-run" | "prop.length-ratio" | "struct.source-marker-leftover" | "struct.merge-conflict-marker" | "lex.duplicate-word" | "uni.combining-mark-without-base" | "uni.redundant-zero-width-space" | "uni.mixed-script-in-token" | "lex.repeated-character-run" | "uni.mixed-numeral-systems" | "punct.bracket-balance" | "case.sentence-initial-lowercase" | "case.inconsistent-word-casing" | "uni.rare-glyph" | "case.mixed-case-word" | "uni.mixed-normalization" | "lex.untranslated-word" | "uni.nonletter-usage-anomaly";

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
export type FindingArgs = { kind: "length-ratio"; ratio_pct: number; scope: LengthRatioScope } | { kind: "bracket-window"; window: DelimObservation[]; measure: BracketMeasure; majority: number; total: number } | { kind: "casing-convention"; glyph: string | null; quoted: boolean; upper: number; total: number } | { kind: "word-casing"; word: string; upper: number; total: number } | { kind: "script-mix-evidence"; k: number; n: number; books: number; corpus: number } | { kind: "repeat-evidence"; ch: string; run: number } | { kind: "duplicate-word"; first_sid: string } | { kind: "rare-glyph"; glyph: string; count: number } | { kind: "mixed-case-word"; word: string; other: number; total: number } | { kind: "normalization"; affected: number; example: string } | { kind: "untranslated-word"; copied_pct: number; run_len: number } | { kind: "nonletter-usage"; glyph: string; reason: NonletterReason; form: NonletterForm; partner: string; count: number; total: number; also: NonletterReason[] };

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
 * The catalog plus the one continuous Review Depth control description.
 */
export interface RuleCatalog {
    cards: RuleCard[];
    review_depth: ReviewDepthCatalog;
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
 * The form a `uni.nonletter-usage-anomaly` finding\'s primary reason names: the
 * neighbour class for a side reason, the four-state topology for `Topology`, and
 * `None` for the reasons that name no form (rarity, pair, continuation).
 *
 * Start/end are **logical**, never visual left/right, so a finding does not move
 * when text direction does.
 */
export type NonletterForm = "none" | "letter" | "digit" | "spaced" | "neither" | "start-only" | "end-only" | "both";

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
 * The unresolved Review Depth policy. Values are validated at the wasm
 * boundary and never clamped silently: `depth` is an integer in `0..=100`,
 * and each relative adjustment is an integer in `-100..=100`.
 */
export interface ReviewPolicyInput {
    /**
     * `0..=100`; omitted means the current-behavior anchor `50`.
     */
    depth?: number;
    /**
     * Relative per-rule adjustments in `-100..=100`.
     */
    adjustments?: Partial<Record<RuleId, number>>;
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
 * Which of `uni.nonletter-usage-anomaly`\'s channels set a finding\'s score — the
 * **primary reason**, chosen by a fixed priority when several tie. Three
 * independently sufficient channels (absolute rarity, placement, sequence)
 * compose with `max`, and placement/sequence each split into the sub-reasons
 * below, so a consumer can name the exact convention the occurrence stands
 * against rather than a single opaque \"unusual\".
 *
 * Explicitly renamed per variant: these strings are a published localisation
 * surface and must never depend on an inferred naming convention.
 */
export type NonletterReason = "rarity" | "start" | "end" | "topology" | "pair" | "continuation";

/**
 * Which rules to run, plus per-rule knobs. `rules` maps a rule code to a
 * flag; omit a rule to keep it enabled (default-on). TS: `{ rules?:
 * Partial<Record<RuleId, boolean>>, proportionality?: … }` — `RuleId` is
 * the same closed union carried on findings, so the consumer\'s config
 * and localisation maps key off one set.
 */
export interface SousConfig {
    rules?: Partial<Record<RuleId, boolean>>;
    review?: ReviewPolicyInput;
    proportionality?: ProportionalityOverrides;
    casing?: CasingOverrides;
    repeated_character_run?: RepeatedCharacterRunOverrides;
    mixed_script?: MixedScriptOverrides;
    rare_glyph?: RareGlyphOverrides;
    mixed_case?: MixedCaseOverrides;
    untranslated_words?: UntranslatedWordsOverrides;
    nonletter_usage?: NonletterUsageOverrides;
}

export interface ReviewDepthCatalog {
    minimum: number;
    maximum: number;
    default: number;
    label: string;
    strict_label: string;
    exploratory_label: string;
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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_galley_free: (a: number, b: number) => void;
    readonly analyze_vref: (a: any) => [number, number, number, number];
    readonly census: (a: any, b: number) => [number, number, number, number];
    readonly galley_analyze: (a: number) => [number, number, number, number];
    readonly galley_census: (a: number, b: number) => [number, number];
    readonly galley_expectedAnalysisId: (a: number) => bigint;
    readonly galley_expectedTargetContextId: (a: number) => bigint;
    readonly galley_findingArgs: (a: number, b: bigint, c: number) => [number, number, number];
    readonly galley_findingsArgs: (a: number, b: bigint, c: number, d: number) => [number, number, number];
    readonly galley_hasReference: (a: number) => number;
    readonly galley_new: (a: any) => [number, number, number];
    readonly galley_removeBooks: (a: number, b: number, c: number) => number;
    readonly galley_replaceCorpus: (a: number, b: any) => [number, number, number];
    readonly galley_replaceSource: (a: number, b: number) => [number, number, number];
    readonly galley_updateBook: (a: number, b: any) => [number, number, number];
    readonly galley_updateChapter: (a: number, b: any) => [number, number, number];
    readonly galley_updateConfig: (a: number, b: any) => [number, number, number];
    readonly rule_catalog: () => any;
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
