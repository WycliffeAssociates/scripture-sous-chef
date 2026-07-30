# Plan — source-paired tier: calibration harness, length-ratio calibration, untranslated-words rule

- Date: 2026-07-30
- Status: committed (owner decision 2026-07-30 — "this is next"; the
  sensitivity/preset-derivation discussion depends on it)
- Subsumes (deleted 2026-07-30, in git history):
  `ideas/discussing/2026-07-29-length-ratio-calibration.md`,
  `ideas/candidates/2026-07-29-untranslated-words-rule.md`. Also relates:
  ADR 0067 (substrates), ADR 0059 (intentional-drift template).

## Carried state (from the subsumed docs)

`prop.length-ratio` today: shipped long ago, statistically sound
(per-verse grapheme ratio vs source; median + MAD robust-z per book and
per project; **z 3.5, min 50 verses**); migrated to the substrate lane
(WP7c) with an exact delta; ~0.08 ms warm — live-typing warnings are
already the architecture, no perf work owed. The `calibrate` CLI already
accepts `<target-vref-file> [<source-vref-file> [z]]` for a **single**
pair — the existing single-pair path is Phase A's loading precedent.
What has never happened: any paired sweep; the rule has produced zero
findings ever. UI framing intent: keep the MAD-z gate internal, present
per-book percent terms.

## Why now

1. `prop.length-ratio` shipped long ago, does the right statistics, and
   has produced **zero findings ever** — every fleet sweep ran
   `source = None`. It is the only shipped rule with no calibration story.
2. The preset-derivation work (evidence-bar slider) needs per-rule
   fleet-measured anchors; length-ratio can't contribute anchors until a
   paired survey exists. This plan feeds that one.
3. Untranslated-words is the last engine-shaped gap on the PO checklist
   (triage refreshed 2026-07-30), and it calibrates on the same harness —
   one prerequisite, two rules served.

## Non-goals

- No alignment, no romanization, no fuzzy matching (edit-distance thread
  lives in its own candidate).
- No quote work, no census changes.
- No new visualization dependencies — second self-contained HTML template
  on the `fleet_report_template.html` pattern (verified hand-rolled, no
  bundled chart lib).
- No changes to `oracle.rs` in Phases A–B (harness is a survey cluster,
  gate-neutral by construction).

## Phase A — the paired harness (calibrate survey cluster)

1. **Pairs manifest** (checked in, e.g. `corpora-pairs.tsv` beside the
   fleet notes): `target-path, source-path, tier, note`.
   - Tier 1 (true pairs, mostly NT): the 15 `Tech_Advance__*` targets
     against their owner-declared sources — en_ulb for
     amo_reg/bbm/bsj/ema-x-emai/gux-x-gourmantche/gux_reg/jid/jni_reg/
     lko/nyn-x-runyaruguru/sbk_reg; sw_ulb for kiz/nyf-x-rabai/
     zga-x-mahanji; rmn-x-yerliroman vs en_ulb carries the versification
     caveat (true source is a Russian ULB we don't hold).
   - Tier 2 (pseudo-pairs, full Bibles, clean negatives + OT coverage +
     high-parametric-knowledge triage): eng-kjv/eng-asv/pt-br_ulb/fr_ulb/
     es-419_ulb against en_ulb.
   - Multi-source targets (bbm+fr, bsj+bn, gux+ar avd where held) run
     under both sources — a free source-sensitivity experiment.
2. **`--paired-survey <pairs.tsv> <out-dir>`**: per pair, per book — verse
   fractions, median, MAD, flag boundaries, findings at default z.
   **Versification guard**: a book whose *median fraction* is an outlier
   against the corpus's other books is quarantined as a pairing artifact
   (rmn-class), reported separately, never counted as findings.
3. **`--seed-faults <pairs.tsv> <out-dir>`**: deterministic fixed-seed
   mutation of targets before analysis — tail-chop at 10/20/30/50%,
   whole-verse deletion, source-verse paste — with a ground-truth manifest
   written beside the dump. Join after analysis → catch-rate and
   clean-verse flag-rate tables. The paste fault is untranslated-words
   ground truth (Phase D) as well as length-ratio's untranslated-verse
   case.
4. **Judge-only z-sweep**: map each corpus once, re-judge per z ∈
   [2.0 … 6.0] (knob isolation — the spine's slider machinery used as a
   harness primitive). Output: the recall/noise curve and per-book
   detection floors.
5. **Report**: second self-contained HTML template — per-book fraction
   scatter with boundaries and findings marked, seeded-fault catch table,
   finding-rate histogram over the paired fleet (bimodality health check).

## Phase B — length-ratio calibration

1. Run both tiers through the harness; read the three instruments:
   seeded-fault curves (primary), fleet finding-rate shape, manual triage
   (top-scored sample from tier-2 high-parametric-knowledge books,
   model-prescreened, owner-adjudicated).
2. Deliverable: `documentation/calibration/2026-XX-XX-length-ratio-paired-survey.md`
   — the curves, per-book floors, source-sensitivity result from the
   multi-source targets, and the adjudication: **z=3.5 confirmed or
   re-pinned**. A re-pin is an intentional behavior change → ADR with
   measured drift per the ADR 0059 template (in principle it drifts
   nothing today, since no fleet corpus loads a source — the ADR would
   record the forward-looking default change).
3. Per-book percent labels for the UI slider ("3.5 ≈ ~38% off-typical in
   Luke") fall out of the floors — recorded for preset-derivation.

## Phase C — untranslated-words substrate

One new substrate (`UntranslatedWords`), zero new machinery — the
proportionality paired-drive precedent end to end: `ChapterView::paired`,
`ObservationInputStamp::with_reference` (both sides stamped → target edits
remap one chapter, source swaps invalidate exactly the changed pairings),
`index_reference_chapters` pairing by verse key. Target tokens off the
shared token lane; the source chapter is tokenized inside `map_chapter`,
chapter-local and transient.

- **map_chapter**: per verse, folded source token set; membership-test
  target tokens (exact match after NFC + case fold — nothing fuzzier).
  Observation: per-verse `(copied, total, longest_run)`, per-word copied
  tallies, capped target-side example spans for maximal copied runs.
  Membership is order-free by design — word order does not transfer
  across languages; the only positions kept are target-side (where the
  run sits in the target text).
- **reduce_chapter**: corpus copied/total share; per-word corpus counts.
- **judge** (knob-isolated, slider-ready), three gates in order:
  1. **Corpus gate** — corpus-wide copied share above a ceiling → silent
     everywhere (creole / related-language case; base rate is the
     disable switch).
  2. **Word excusal** — recurrence knee over per-word counts; recurring
     source-identical words (Jerusalem, amen, loanwords) become
     conventions subtracted from every verse's numerator.
  3. **Site scoring** — excused-adjusted verse fraction × run-length
     bonus; adjacent runs (paste shape) dominate scattered singles.
- **materialize**: findings on target-side run spans (runs ≥ 2 after
  excusal) or the verse for scattered high fraction.
- **Self-gating**: silent when no source is loaded, when the corpus gate
  trips, when tokenization is degenerate for the script, or when
  conventions absorb everything.

**Oracle discipline for Phase C.** A new rule changes the `all`-config
dumps by definition. Before pin (full fleet, both configs) → land the
substrate with the rule **excluded from both oracle configs** and prove
byte-identity (the substrate exists, no behavior moved) → then a single
adjudicated pin-move commit adds it to the `all` config with the drift
recorded (expected: zero findings on the sourceless fleet — the pin move
may in fact be byte-identical there; the paired harness is where its
behavior is actually witnessed). Default-config membership is decided in
Phase D, not here.

**Memory gate (new — we have never measured paired residency).** Every
dhat number to date is sourceless. Before Phase C lands: dhat probe of a
resident Galley with source loaded, target = the largest tier-1 pair, both
configs — that's the paired baseline. After Phase C: repeat; budget is
baseline + O(corpus verses) small observations. The source corpus text
itself is the floor (it must be resident for source edits/re-pairs);
the folded per-verse token sets must *not* be retained — they are
map-transient. If retained state creeps toward "second copy of
everything," that's a design failure, not a tuning problem.

## Phase D — untranslated-words calibration + defaults

1. Same harness, paste faults as ground truth; sweep the judge knobs
   (ceiling, knee, run bonus) judge-only; triage sample as in Phase B.
2. Adjudicate: default-on or default-off, knob defaults, and the anchor
   table rows this rule contributes to preset-derivation.
3. Deliverable: calibration doc + (if default-on) the adjudicated oracle
   re-pin for the default config; catalog card, localization key, docs.

## Open questions (owner)

- Tier-1 target hygiene: the Tech_Advance repos are real field data —
  findings there may be *true positives*; triage will tell us whether to
  quarantine any pair from the clean-negative denominator.
- Whether Phase B's re-pin (if any) waits for the preset-derivation
  anchor-table work or lands independently.
- rmn: drop from tier 1 vs run against en_ulb behind the versification
  guard.
