# ADR 0063: `uni.mixed-normalization` — a deterministic, corpus-scoped NFC-mixing finding

- **Date:** 2026-07-16
- **Status:** Accepted — implementation landed on `mixed-normalization-warning`
  (Phases A–F, full test/oracle/wasm gate green). The rule ships
  **default-off** — a deliberate deviation from this plan's original
  default-on ruling, adjudicated after the measured warm-path cost (see
  Consequences) did not clear the owner's "negligible" bar even after the
  `NORM_RELEVANT` prefilter closed most of an initial regression.
- **Builds on:** ADR 0010 (pure analyzer contract), ADR 0021 (grapheme
  segmenter), ADR 0057 (event-stream engine, internal-hot-path-map
  pattern), ADR 0060 (`PrepCache`), ADR 0061 (`Corpus`/`KeyIdx` addressing),
  ADR 0062 (resident `Galley` + per-book `Tally`)
- **Supersedes:** the scored/probabilistic design in the
  `uni.mixed-normalization` idea doc (deleted 2026-07-20 per the ideas
  lifecycle) and its vendored-decomposition-table plan

## Context

A translation corpus can write the same abstract character in more than one
raw Unicode encoding — most commonly precomposed vs. decomposed Latin
diacritics (`é` U+00E9 vs. `e` + COMBINING ACUTE), but also ASCII/singleton
equivalences (plain `K` vs. KELVIN SIGN U+212A) and Indic composition
exclusions (Bengali `U+09DF` vs. its decomposed `U+09AF U+09BC`). Canonically
equivalent strings render identically but break exact-match search,
de-duplication, token identity, and cross-corpus tooling.

The original idea (2026-07-11) proposed a scored, calibrated rule with a
house-vendored decomposition table. A throwaway spike
(`examples/nfc_spike.rs`, `examples/nfc_fleet.rs`) established that the
useful condition is **binary**, not scored: a supplied corpus either mixes
two raw forms under one NFC key or it doesn't. There is no threshold,
recurrence knee, or language-specific convention to learn — mixing is rare
(69/1504 corpora in the spike, using a shortcut that skipped ASCII and
both-NFC-and-NFD forms) and, where it fires, precise.

The plan additionally required the resident `Galley` (ADR 0062) as a
prerequisite, since a "one finding per corpus" contract is only honest once
the analysis scope is genuinely the whole project.

## Decision

Ship `uni.mixed-normalization` as a **deterministic, corpus-scoped**
rule with no knobs. It ships **default-off** (see Consequences for why this
deviates from the plan's original default-on ruling) — toggled through the
same `Config.rules` map every rule uses, with identical detection once
enabled.

### Detection semantics

The unit of comparison is one extended grapheme cluster (the repository's
existing UAX #29 segmenter, ADR 0021), keyed by its NFC form via
`unicode-normalization` — not a house-vendored table, since correct NFC
needs canonical ordering, recursive decomposition, singleton mappings, and
composition exclusions, and a partial reimplementation would disagree with
JS `String.prototype.normalize` at the wasm boundary.

**No unsafe skip.** Every distinct raw grapheme form is recorded, including
plain ASCII and forms that are already both NFC and NFD:

- Skipping ASCII would miss `K` mixing with KELVIN SIGN `U+212A` (which
  normalizes to `K`).
- Skipping an `is_nfc(raw) && is_nfd(raw)` cluster would miss the Bengali
  composition-exclusion case: `U+09AF U+09BC` is itself both NFC and NFD,
  while composition-excluded `U+09DF` normalizes to it — the exclusion
  case is unrepresentable if the decomposed form is treated as "already
  fine and skippable."

**Majority, tie-break, and anchor.** For each NFC key with ≥2 raw forms
("mixed"), the majority form is chosen by: greater corpus-wide count; on a
tie, earlier first occurrence in caller-presented corpus order (ADR 0061 —
never canonical book order); on a further tie (defensive), lexicographically
smaller raw UTF-8 bytes. `affected` sums every mixed key's minority count.
The one emitted finding anchors at the corpus-wide earliest non-majority
occurrence across every mixed key; its `example` is that key's NFC form as a
`String` — **not `char`**, since composition exclusions and multi-mark
clusters can be more than one scalar (the repository's one documented
exception to "single glyphs are `char`" — see
`documentation/rules/messaging-and-fixes.md`).

Cardinality is capped at one finding per supplied corpus. `source` is
ignored entirely (mixing is intrinsic to the target).

### Wire contract

```rust
MixedNormalization => "uni.mixed-normalization",   // RuleId
Normalization { affected: u32, example: String },  // FindingArgs, kind: "normalization"
```

Severity `Warning`, `score: None`, verdict `Deterministic`, **default-off**
(present in `Config::v1_defaults`'s disabling list; toggled through the same
`rules` map every rule uses — no typed sub-config, since there is nothing to
tune).

### Runtime architecture

Registering a `ProjectRule` alone does not make it run in production: the
fused `analyze_stateful` path emits project findings from explicit
per-listener blocks (`if plan.bracket`, `if plan.duplicate`), so this rule
needed both:

1. A direct `ProjectRule::check` (for calibration/direct callers), driving
   the shared listener via `stream::drive_book` — the same pattern as
   `bracket_balance::match_book`.
2. The fused walk: `WalkPlan.normalization`, `project_needs()` (requires
   graphemes), a `NormalizationAcc` listener parallel to bracket/duplicate,
   `BookOut.normalization`, and a content-keyed `PrepCache` product
   (`BookEntry`/`CachedWalk.normalization`, `has_walk_lanes`) — **not** a
   `RuleStats`/`Tally` entry. This is a pure per-book product of the text
   alone; it does not enter `rules_fp` or the serialized `Stats` digest.

Both paths share one accumulator/emitter (`NormalizationAcc` → `finish()` →
`BookNormalization`, merged by `emit()`), so they cannot drift — proven by a
dedicated direct-vs-fused equivalence test.

### The `NORM_RELEVANT` prefilter

The no-unsafe-skip contract above means `NormalizationAcc::verse()` must
touch **every** grapheme cluster in the corpus, not just mixed ones — a real,
measured cost (see Consequences). The fix is a safe-superset prefilter, not
a relaxation of the contract: bit 29 (`NORM_RELEVANT`) of the existing fused
per-scalar `Class` bitfield (ADR 0020/0022) marks every scalar that could
*possibly* participate in a canonical-equivalence collision. A grapheme
cluster is hash-counted only if at least one of its scalars carries the bit;
every other cluster is *provably* a canonical singleton — `NFC(raw) == raw`
for every possible raw form built only from unmarked scalars — so skipping
it cannot miss a real mixing case.

The bit is computed (`xtask/src/gen_charclass_table.rs`, using
`unicode_normalization::char::{decompose_canonical, canonical_combining_class}`
directly — not a `UnicodeData.txt` decomposition-column scan, which is empty
for Hangul's algorithmic decomposition and would silently miss all 11,172
syllables) as three rules, deliberately narrower than "mark every
decomposition-target scalar":

1. every scalar whose own canonical decomposition differs from itself
   (composed accented letters, Kelvin/Ohm/Angstrom-style singletons, Hangul
   syllables);
2. every scalar with nonzero canonical combining class (any combining
   mark) — this alone gates any cluster containing a decomposed accent's
   mark, or a pure mark-reordering case, without needing to mark the base
   letter too;
3. decomposition **target** scalars, but only when that decomposition's
   entire output has combining class zero — canonical singleton targets
   like plain `K` (Kelvin's target), plain `;` (a target of GREEK QUESTION
   MARK `U+037E` — a real Unicode singleton this project hadn't previously
   catalogued), and Hangul Jamo. An ordinary accent target like plain `e`
   (target of `é`) is deliberately **not** marked — rule 2 already gates
   that cluster via the mark, so marking the base letter too would only
   widen the candidate set without closing any gap.

Verified against real Unicode data before implementation (a dispatched
research pass, not just reasoning): swept all ~1.1M scalars, cross-checked
the resulting union against every one of `unicode-normalization`'s actual
961 composable pairs — zero gaps. Guarded permanently by an exhaustive
completeness test (`charclass::tests::norm_relevant_bit_equals_closure_over_all_scalars`,
mirroring the file's existing full-sweep pattern for `CONTROL`/`ZW_FORMAT`/
`INVALID_CP`) plus named fixtures and a selectivity assertion over ordinary
ASCII. `NormalizationAcc::verse()` reads the bit off the same tape the
grapheme segmenter already built, advancing a cursor in lockstep with the
grapheme spans — no extra pass over the text.

The lower-level `analyze_stateful` API's partial-target semantics are
unchanged: like every `ProjectRule`, this rule treats the target supplied on
that call as its corpus and does not merge absent books from `Stats`. The
resident `Galley` (ADR 0062) is what makes "one finding per corpus" an
honest contract — it always analyzes its complete resident corpus.

### Fleet evidence vs. the spike

The production detector is deliberately more complete than the spike
(records ASCII and both-NFC-and-NFD forms the spike's shortcut skipped), so
its fleet counts move. On the WA-scope (251-corpus) subset: 45 corpora gained
a `uni.mixed-normalization` finding — a materially higher rate than the
spike's full-fleet 69/1504 would extrapolate to, entirely attributable to
the completeness fix, not a false-positive class. Spot-checked rows are
plausible real cases: Latin diacritic mixing (à/í/õ/ñ), Devanagari/Bengali/
Gurmukhi nukta forms, and Arabic shadda+kasra ordering — matching the
spike's anticipated evidence classes (Latin compose/decompose, Hebrew
canonical mark reordering, Indic composition exclusions).

## Rejected-for-now

- **NFD as an alternative fix target.** NFC is the one explicit downstream
  fix; NFD would only be worth adding if a future editor product decision
  values preserving a decomposed house style over NFC interoperability.
- **Folding into `uni.rare-glyph`.** ADR 0053 already records a residual
  where a normalization-variant scalar surfaces mislabeled as "rare." This
  rule gives that residual its own honest finding but does **not**
  coordinate suppression with `uni.rare-glyph` — a scalar that is merely a
  normalization variant of a common grapheme can still surface in both
  rules today. Coordinating them needs a separately reviewed ownership
  predicate (§14 follow-up).
- **A census normalization lane.** Absolute composed/decomposed/neither
  counts for fleet inspection is a separate, not-yet-scoped census
  addition; this rule's deterministic finding payload does not carry that
  reporting detail.
- **A house-vendored decomposition table**, per the original idea — see
  Context.

## Oracle adjudication

`Stats` must remain byte-identical regardless of default status
(non-stateful product). Because the rule ships default-**off**, the
`default`-config WA-scope dump is **byte-identical to the pre-change
baseline with no filtering needed at all** — the rule contributes zero rows
under the shipped default. The `all`-config dump (which enables every rule)
gained 45 new rows across the WA-scope 251 corpora; filtering those out
leaves every pre-existing finding byte-identical. Both were reconfirmed
after every implementation phase, including four rounds of internal
accumulator optimization and the final default-on → default-off flip — the
detector's observable behavior never moved once implemented, only its
internal data structures and its default did.

## Consequences

- **Editor gate (§11 in the plan).** This repository ships the detector,
  catalog, wasm wire, and generated packages, but cannot truthfully ship the
  editor's one-click project fix yet: the live editor
  (`scripture-editor-proto-2`) still calls stateless `analyze_vref` per book,
  so cross-book mixing is invisible there and `SousFinding` doesn't preserve
  `args`. The first JS consumer can dispatch on the closed rule id alone
  (`if (finding.code === "uni.mixed-normalization") { verses =
  verses.map(t => t.normalize("NFC")) }`) without needing `affected`/
  `example` — those are presentation-only. Publication/adoption is gated on
  the editor first adopting a whole-project resident `Galley` **and**
  explicitly enabling the rule (it ships default-off — see below); until
  then, core commits may merge but the end-to-end fix must not be
  advertised.
- **Dependency.** `unicode-normalization` promoted from a throwaway
  dev-dependency to a real workspace dependency. Default features are just
  `["std"]` — no surprises. `Cargo.lock` needed no textual change (already
  resolved the same version).
- **Wasm size:** the `unicode-normalization` dependency itself added
  +145,682 raw bytes (~12.4%) / +68,424 gzip bytes to both `pkg-web` and
  `pkg-bundler`; the `NORM_RELEVANT` table growth (§ below) added a further
  +28,005 raw / +10,111 gzip. Total from the pre-dependency baseline:
  **+173,687 raw bytes (+14.8%), +78,535 gzip bytes (+18.8%)**
  (1,172,170 → 1,345,857 raw; 416,727 → 495,262 gzip). The owner confirmed
  this is acceptable: full-fidelity NFC tables (composition, decomposition,
  canonical ordering, exclusions) are inherently data-heavy, and a partial
  table was explicitly rejected (Context).
- **Memory:** the retained per-book product is a flat map of distinct raw
  forms, grouped by NFC key at `finish()` (not per-occurrence). Measured
  cardinality: `WA-en-ulb` (full English Bible, 31,086 verses) — 82
  distinct raw forms / 82 NFC keys (zero mixed); `WA-as-ulb` (the spike's
  worst measured corpus, 31,083 verses) — 1,542 distinct raw forms grouped
  into 1,529 NFC keys (13 mixed). Trivially small either way; this rule
  does not dominate memory. `NORM_RELEVANT` adds no width to the existing
  BMP table (a spare bit in an already-resident `u32`).
- **Warm-path performance — why the rule ships default-off.** `cargo bench
  -p ssc-core -- analyze` (criterion, serial, `v1_defaults`, en_ulb) found a
  real regression against ADR 0062's measured `cached_edit_*` band
  (5–25 ms): the naive nested-`BTreeMap` implementation measured 37–48 ms
  for `cached_edit_PSA` (was 18.9 ms), driven by an unconditional
  lookup-or-insert per grapheme cluster across the **entire** verse text —
  not just mixed occurrences, per the no-unsafe-skip contract above. Four
  rounds of behavior-preserving optimization (confirmed byte-identical via
  full test suite + oracle re-dump after each): `BTreeMap` → `FxHashMap`
  (ADR 0057's internal-hot-path-map pattern); flattening the accumulator to
  one map keyed by raw form, deferring NFC-key computation from every
  *occurrence* to once per *distinct* raw form in `finish()`; a 128-slot
  direct-addressed array for single-ASCII-byte clusters (measured no
  statistically significant benefit, and **removed** once the prefilter
  below made it redundant — after `NORM_RELEVANT`, only a tiny
  singleton-target subset of ASCII like `K`/`;` ever reaches the map at
  all); and finally the `NORM_RELEVANT` prefilter, which skips the large
  majority of clusters entirely (measured candidate rate on `en_ulb`'s PSA:
  ~0% — this corpus has essentially no combining marks or decomposable
  characters at all).

  Net result: `cached_edit_PSA` 37–48 ms → **24.97–25.16 ms** (mean
  25.07 ms), reproducible on a clean re-run. Against the 18.9 ms baseline
  that is **~+33%**, essentially at the 5–25 ms band's upper edge — a
  substantial, real recovery from the initial ~+150% regression, but
  **not comfortably inside the band**. `cached_edit_MAT`: 16.8–17.2 ms
  (baseline 13.1 ms, ~+30%). `cached_edit_3JN`: 6.5–6.8 ms (baseline
  5.2 ms, ~+27%). Profiling (`samply`) before the prefilter traced the
  cost to the hash/map operations themselves — cloning on cache hits and
  `is_nfc`/`.nfc()` calls were both measured and ruled out as dominant
  costs.

  **Owner/reviewer adjudication:** ~+30% on the default keystroke path,
  landing at rather than comfortably inside the historical band, does not
  clear the "negligible" bar the default-on decision was contingent on. Per
  the pre-agreed fallback, the rule ships **default-off** — a cold,
  explicit-opt-in check (add it to your project's `rules` map, or run it
  through the census/calibration surfaces) rather than a fifth round of
  hot-path optimization. This is recorded here as a deliberate deviation
  from the plan's original default-on ruling (repo convention for
  intentional behavior changes: recorded with the measured numbers and the
  adjudication, not silently reverted). Revisit only if a demonstrably
  cheaper detection design emerges — not by layering further tricks onto
  this one.

## Relates to

- Plan: `documentation/plans/completed/2026-07-14-mixed-normalization-plan.md` (this
  ADR is its expected 0063).
- Superseded idea: the `uni.mixed-normalization` idea doc (deleted
  2026-07-20 per the ideas lifecycle; the plan + this ADR are the record).
- ADR 0053's residual note now points here (does not claim rare-glyph
  coordination has landed).
- Downstream handoff: see the cross-repo handoff note (§11) for the exact
  live seams the editor implementation must address.
