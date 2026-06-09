# Execution brief: proportionality — the first cross-map rule (v0.0.3)

- **Date:** 2026-06-09
- **Audience:** a fresh agent thread executing autonomously.
- **Status:** plan, not yet started. This is the **next** run (preferred
  over the deterministic batch in
  `2026-06-09-deterministic-rule-expansion.md`, which follows later).

You are implementing **proportionality** (`prop.length-ratio`, vision
`SSC-PROP-001`): the first rule that reads **across** the VerseMap rather
than within a single verse, and the first to use `score` + structured
`args` + a per-rule config knob. It is graduation-order item #2 (config →
proportionality) from `v1-reset-design.md`. Read this whole brief, then
the cited contracts, before writing code.

## What it is

For each verse present in **both** the target and a reference (`source`),
the target's length relative to the reference's is informative: a verse
that is 3× or ⅓ the reference length is often a misplaced verse number,
an omission, or a gross over/under-translation. We flag verses whose
target/reference length ratio is a **robust outlier within its book**.

This is **deterministic** (a formula, not a learned model), so it fits the
library's spirit — it is *not* the speculative statistical tier. The only
"statistics" is a median + MAD over a few hundred per-book ratios.

## Contract (read first)

- `documentation/adrs/0010-...md` — pure analyzer; byte `Span` into the
  given text; `sid`-anchored; no rendered messages (code + structured
  args only). The `source` param already exists for exactly this.
- `documentation/adrs/0011-...md` — **statefulness ladder.** Ship
  proportionality in **Mode A** (stateless: `source` passed each call,
  per-book distribution rebuilt each call — microseconds for a book).
  Resident immutable reference (A+) and incremental target (B) are
  **future**, gated on measurement. Do not build resident state now.
- `documentation/adrs/0012-...md` — `RuleId` is a closed enum defined by
  the `define_rule_ids!` macro in `diagnostics.rs`; `Config` enable/disable
  exists. Adding `prop.length-ratio` is one line in that macro.
- `documentation/methods.md` §3.4 — **use median + MAD, not mean + stddev**
  (robust to the one bad verse that would otherwise poison the threshold).
  `documentation/vision.md` §8 (`SSC-PROP-001`, default `Warn`), §9
  (defaults: `|z| > 2.5`, min 50 verses/book), §12.5 (length = graphemes).

## The math

- Group target sids by book (`BookId`, already on `Sid`).
- For each sid in **target ∩ source**, `ratio = graphemes(target_text) /
  graphemes(source_text)` (grapheme count via `unicode-segmentation`;
  skip if either side is empty).
- Per book: if fewer than `min_verses` ratios (default 50), **skip the
  book** — too little distribution to judge (vision §9). Otherwise compute
  `median` and `MAD = median(|ratio_i − median|)`.
- Robust z: `z = 0.6745 · (ratio − median) / MAD` (the 0.6745 makes MAD a
  stddev-equivalent). Guard `MAD == 0` (a book of identical ratios → no
  outliers; skip).
- Flag verses with `|z| > threshold` (default 2.5).

## New contract surface this rule introduces (all additive)

1. **`score: Some(..)`** — first rule to populate it. Map `|z|` into a
   bounded confidence (e.g. saturating at some `|z|`), documented.
2. **Structured `args` on `Finding`** — the additive field ADR 0010 §6
   anticipated, needed for the interpolated message ("this verse is
   {ratio_pct}% of the reference length"). **Recommended shape:** a typed,
   `Tsify`-able discriminated union in `diagnostics.rs`:
   ```rust
   pub enum FindingArgs { LengthRatio { ratio_pct: f32, robust_z: f32 } }
   ```
   on `Finding` as `Option<FindingArgs>` (`None` for the existing rules).
   Wire it through the wasm `Finding`. (Alternative: a generic
   `BTreeMap<&'static str, f64>` — less typed; prefer the enum to match the
   closed-set philosophy.) This is the keystone decision — get it right;
   future scored rules add variants.
3. **Per-rule config knob** — proportionality has a threshold, so this is
   where `Config`'s value graduates from `bool` to a small per-rule
   config, **additively**. Recommended: keep `rules: BTreeMap<RuleId,
   bool>` for enable/disable and add a separate typed field, e.g.
   `Config { rules, proportionality: ProportionalityConfig }` where
   `ProportionalityConfig { z_threshold: f32 = 2.5, min_verses: usize =
   50 }`. Avoid reworking the whole config into a generic per-rule struct
   — one typed sub-config per knob-bearing rule keeps it simple and typed
   for both Rust and TS. Defaults live in `core` (vision §9).

## Implementation

- Add `ProjectLengthRatio` → `"prop.length-ratio"` to `define_rule_ids!`.
- New `signals/proportionality.rs` implementing `ProjectRule`
  (`check(target, source) -> Vec<Finding>`). If `source` is `None`, return
  empty (the rule needs a reference). Use the single provided `source` map
  as the reference (multi-reference ensembling is future — vision §11 #16).
- `Finding.range` = the whole verse span (`Span { start: 0, end:
  text.len() }`) — the finding anchors the verse; `sid` carries identity.
- Register in `rule::project_rules()`.
- Honor `Config`: `analyze_with_config` already skips disabled project
  rules; thread the `ProportionalityConfig` to the rule (e.g. give
  `ProjectRule::check` access to `&Config`, or construct the rule from
  config in the registry — pick the cleaner; document in the ADR).
- wasm: extend `Finding` with the optional `args`; surface
  `ProportionalityConfig` in `SousConfig` (`Tsify`, optional, sensible
  defaults). The `.d.ts` should show the `FindingArgs` union and the
  config fields.

## Calibration gate (before shipping)

Reference pairs exist in `corpora/` (methods §0): e.g. `bem_reg` /
`fij-x-saqani_reg` / `acz_reg` as target vs `en_ulb` as source (≈99.3%
sid coverage). Run proportionality over a pair and:
1. Confirm it flags **gross** outliers (eyeball the top |z| verses — they
   should look like real length anomalies, not noise).
2. Confirm a clean pair yields **bounded** findings (vision §10) — if it
   floods, the default `z_threshold` is too low or MAD handling is off.
3. Tune the default threshold if needed; record counts + the chosen
   default in `documentation/calibration/2026-06-09-proportionality.md`.

Do not ship a threshold that floods a clean reference pair.

## Done criteria & release

- `cargo test -p ssc-core` green: an outlier fires with the right `sid` +
  `score` + `args`; a uniform-ratio corpus produces nothing; `source =
  None` produces nothing; a book under `min_verses` is skipped; `MAD == 0`
  is handled.
- `cargo build -p ssc-core --no-default-features` clean.
- `npm run build:wasm` clean; `.d.ts` shows `FindingArgs` and the
  proportionality config.
- Calibration report committed; clean pair not flooded.
- **ADR 0013**: proportionality — median+MAD (vs mean+stddev), Mode A
  (with A+/B deferred per ADR 0011), the `FindingArgs` additive field, the
  per-rule `ProportionalityConfig` graduation, single-reference for v1,
  grapheme length, min-verses skip. Index it.
- Update `vision.md` §8 to mark `SSC-PROP-001` shipped.
- Commit, **tag `v0.0.3`**, push (workspace Cargo stays `0.1.0`; tag is the
  release ref).

## Statefulness — explicitly OUT of this run (by design)

Incremental / resident stats ("update the stats a chapter at a time") are
**not in scope here**, for two reasons:

1. **No pressure yet.** Proportionality's per-book median+MAD is a re-sort
   of a few hundred ratios — microseconds. Mode A (rebuild every call) is
   already sub-frame at whole-NT scale. ADR 0011's discipline is *ship
   Mode A, measure, escalate only when forced* — and nothing here forces
   it.
2. **The incremental unit would be the BOOK, not the chapter.**
   Proportionality's distribution is per-book (you need a book's whole set
   of ratios to compute its median+MAD), so the natural invalidation unit
   is the book: a verse/chapter edit re-sorts *that book's* ratios. A
   chapter is too fine a unit for this particular statistic. (Counting-type
   stats — hapax, n-gram tallies — are the ones that partition to chapter/
   verse grain; proportionality does not.)

The one requirement this run must honor so the resident path (A+/B) slots
in **later without a rewrite**: keep the rule **`sid`-keyed and
book-groupable** (it naturally is — group by `BookId`, ratios keyed by
`sid`). That's all. Build no `AnalysisContext`, no patch channel, no
resident reference here.

## Notes

- "Reading discourse across the VerseMap" is the general capability the
  `ProjectRule` path provides; proportionality is its first user. Later
  cross-verse/discourse rules (e.g. book-scope quote balance) use the same
  path — but those that need a corpus *model* or per-edit state are still
  deferred per ADR 0011.
- Downstream (not this run): the consumer adds a localization entry for
  `prop.length-ratio` that consumes `args` (the `{ratio_pct}` message) and
  decides severity/surfacing. Tracked in `scripture-editor-proto-2`.
