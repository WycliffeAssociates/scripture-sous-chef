# ADR 0013: Proportionality — the first cross-map rule, and the contract surface it grows

- **Date:** 2026-06-09
- **Status:** Accepted

## Context

[ADR 0010](0010-pure-analyzer-contract-v1-reset.md) reserved a `source`
parameter on `analyze` for source-relative rules and named a future
additive `args` field on `Finding`. [ADR 0011](0011-statefulness-incrementality-strategy.md)
chose proportionality (`SSC-PROP-001`, vision §8) as the first rule that
reads **across** the `VerseMap`, and fixed its statefulness strategy:
ship Mode A, escalate only on measurement. [ADR 0012](0012-ruleid-closed-enum-config-surface.md)
made `RuleId` a closed enum with a `Config` whose value type was `bool`,
anticipating that richer per-rule config "arrives with proportionality."

This ADR records the decisions made when proportionality actually landed
(`prop.length-ratio`, graduation-order item #2). It is the first rule to
populate `score`, the first to carry structured `args`, and the first
with config knobs — so its choices set the pattern every scored /
knob-bearing rule follows.

## Decision

1. **The statistic is median + MAD, not mean + stddev** (methods §3.4).
   Per book, over the verses present in both target and source:
   `ratio = graphemes(target) / graphemes(source)` (grapheme count per
   vision §12.5; verses empty on either side are skipped), then
   `z = 0.6745 · (ratio − median) / MAD`, flagging `|z| > z_threshold`.
   The 0.6745 makes MAD a stddev-equivalent so the threshold reads in
   z-score units. `MAD == 0` (a book of identical ratios) means "no
   outliers": the book is skipped, not divided by. Books with fewer than
   `min_verses` shared verses are skipped — too little distribution to
   judge.

2. **Mode A only** (per ADR 0011): the reference is passed each call and
   the per-book distribution is rebuilt each call. No `AnalysisContext`,
   no resident reference, no patch channel. The one shape requirement the
   resident path (A+/B) needs later is honoured for free: ratios are
   `sid`-keyed and grouped by `BookId`.

3. **`FindingArgs` is a closed, typed discriminated union** in
   `diagnostics.rs` (`Option<FindingArgs>` on `Finding`, `None` for
   no-interpolation rules), serde-tagged with `kind` and `Tsify`-exported
   so the consumer's ICU layer gets a typed payload:
   `{ kind: "length-ratio", ratio_pct, robust_z }`. Future scored rules
   add variants. The generic `BTreeMap<&str, f64>` alternative was
   rejected: a closed union matches the `RuleId` philosophy (exhaustive
   consumer handling, compiler-enforced) where a string-keyed map can
   silently drift.

4. **Per-rule config graduates as a typed sub-config, not a generic
   value type.** `Config` keeps `rules: BTreeMap<RuleId, bool>` for
   enable/disable and gains `proportionality: ProportionalityConfig
   { z_threshold, min_verses }` — one small typed struct per knob-bearing
   rule, additively. Defaults live in core. (`Config` consequently drops
   `Eq` — thresholds are `f32`.) The wasm boundary exposes partial
   overrides (`ProportionalityOverrides`, all fields optional) so a TS
   consumer can set one knob without restating the others.

5. **Knob-bearing project rules are constructed from `Config` in the
   registry** (`project_rules(&Config)`), so `ProjectRule::check` keeps
   its pure `(target, source) -> Vec<Finding>` signature instead of
   threading `&Config` through every rule. Disabled-rule skipping stays
   in `analyze_with_config`, unchanged.

6. **`score` maps `|z|` linearly onto a bounded confidence:** 0.5 at the
   firing threshold, saturating at 1.0 at twice the threshold. It orders
   findings for the editor's confidence chip; it is *not* a calibrated
   probability.

7. **Single reference for v1.** The one `source` map passed to `analyze`
   is the reference; multi-reference ensembling stays future work
   (vision §11 #16). `source = None` ⇒ the rule returns nothing.

8. **`Finding.range` spans the whole verse** (`0..text.len()`): the
   finding is about the verse as a unit; `sid` carries identity and the
   editor highlights the verse.

9. **Default `z_threshold` is 3.5, not vision §9's 2.5.** Calibration
   (`documentation/calibration/2026-06-09-proportionality.md`) showed
   verse-length ratios are strongly fat-tailed: at 2.5 the borderline
   findings are ±30% — ordinary cross-language verbosity — and 2.5–5% of
   all verses flag. At 3.5 the borderline is ±40–50% and worst-book
   volume on a clean published pair is 33 (≈1/chapter). `min_verses = 50`
   confirmed. Severity Warn (vision §8).

## Rationale

- **Median+MAD over mean+stddev:** one gross outlier (the very verse the
  rule exists to catch) inflates a stddev enough to hide itself; it
  barely moves a median/MAD. Robustness is the point of the rule.
- **Deterministic, not the speculative statistical tier:** the "model" is
  a formula over a few hundred per-book ratios — microseconds to rebuild,
  which is also why Mode A needs no measurement to justify (ADR 0011's
  ladder is entered at its cheapest rung, escalation gated on evidence).
- **Typed `args` / typed sub-config:** both surfaces are consumed by an
  exhaustively-typed TS consumer; closed sets turn engine growth into
  consumer compile errors instead of silent runtime drift (the ADR 0012
  payoff, extended to payloads and knobs).
- **Book as the grouping unit:** translation register varies per book
  (genealogy vs narrative); a corpus-wide distribution would flag whole
  books, and per-chapter buckets are too small to estimate from (also
  the ADR 0011 reasoning for the book as the future invalidation unit).

## Consequences

- `Finding` gains `args: Option<FindingArgs>`; existing rules emit
  `None`. Wire format is additive (`skip_serializing_if` on `None`), but
  the wasm TS `Finding` type grows a field — pre-alpha consumer updates
  its mapping (tracked in `scripture-editor-proto-2`: localization entry
  for `prop.length-ratio` consuming `{ratio_pct}`).
- Every future scored/interpolating rule follows this recipe: `RuleId`
  variant + `FindingArgs` variant + (if knobbed) a typed sub-config
  struct + registry construction from `Config`.
- The calibration harness (`crates/core/examples/calibrate.rs`) stays as
  dev tooling — it reads files and naively strips USFM, which is fine
  for measuring volume and forbidden in the library path (ADR 0010).
- Released as tag **v0.0.3**.

## References

- Execution brief: `documentation/plans/2026-06-09-proportionality.md`
- Calibration: `documentation/calibration/2026-06-09-proportionality.md`
- [ADR 0010](0010-pure-analyzer-contract-v1-reset.md), [ADR 0011](0011-statefulness-incrementality-strategy.md), [ADR 0012](0012-ruleid-closed-enum-config-surface.md)
- `documentation/methods.md` §3.4; `documentation/vision.md` §8 (`SSC-PROP-001`), §9, §12.5
