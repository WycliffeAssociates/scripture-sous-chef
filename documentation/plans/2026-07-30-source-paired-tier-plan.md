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
   - **Loading resolved (2026-07-30):** every pair member exists as a
     plain vref file in `corpora/vref/` (`WA-<target>-reg.txt`,
     `WA-en-ulb.txt`, `WA-sw-ulb.txt`, `WA-fr-ulb.txt`, `WA-bn-ulb.txt`,
     `WA-pt-br-ulb.txt`, `WA-es-419-ulb.txt`) — no repo-format loading
     needed anywhere in this plan.
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

**Adjudications carried in from Phase A's smoke run (2026-07-30):**

- **Fidelity requirement:** Phase A's harness re-derived *book-only*
  statistics; the shipped judge fires on either of two channels (book z
  and project z, scope recorded) — so the smoke catch rates are
  pessimistic and small books (MAL/OBA-class, under min_verses) are in
  fact covered by the project channel. Phase B measures the **real
  rule's** verdicts, both channels, scope reported.
- **Small-book fallback held in reserve:** if calibration shows the
  dual-channel design misbehaving, the fallback is an n-weighted blend
  (book yardstick weighted by the book's verse count, corpus yardstick
  otherwise) — an engine change, oracle-gated + ADR, only if the data
  demands it.
- **Owner prior on the operating band:** ~2.5–4 deviations; in percent
  terms roughly 2–3× longer/shorter for cross-language pairs. 10–20%
  chops are not expected to be detectable (measured: they aren't).
  Floors are reported in BOTH vocabularies (z and percent-of-typical).
- **Versification shear is a first-class signal, not noise** (owner
  ruling): the kjv smoke run surfaced Hebrew-vs-English numbering shear
  (reciprocal long/short adjacent verses within known chapters) and the
  Comma Johanneum. The book-grain quarantine stays; Phase B adds a
  chapter-grain shear detector (adjacent verses with opposite-sign
  extreme z) reported in its own section — actionable for a PO in its
  own right.

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

## Phase B2 — asymmetric spread (owner-adjudicated 2026-07-30)

Phase B outcome: **z=3.5 confirmed** (no-op, evidence recorded in the
calibration doc). Additionally adjudicated: split the spread measurement
by direction — the ratio distribution is squeezed against zero on the
short side and open-ended on the long side, so one symmetric "usual
difference" mis-sizes one tail. Change `prop.length-ratio`'s judge to
two one-sided MADs (deviations above the median measured separately from
deviations below — "double MAD") and two thresholds (`z_long`,
`z_short`), both defaulting 3.5 so behavior is initially unchanged in
spirit (values shift where the tails are actually asymmetric). No compat
shim (pre-alpha). UI language: "flags at N× the usual
longer/shorter-than-typical difference" — two trims on the fine-tune
panel. Engine change → full oracle discipline + ADR recording the
measured drift + Phase B key tables re-run.

**Premise correction (2026-07-30, found by the B2 gate itself):** the
oracle dumps are NOT sourceless — `oracle_source` (oracle.rs) pairs
every dump corpus against `WA-en-ulb.txt` when present (pre-existing,
documented). `prop.length-ratio` contributes ~47.6k findings to today's
WA default dump; "zero findings ever" was true only of the `--fleet`
survey path. B2 is therefore a real adjudicated behavior change (ADR
0059 pattern), and Phase C's pin-move will likewise show real findings,
not zero drift. Additionally adjudicated into B2: naive double-MAD has a
single-deviation-side collapse (a side with one strict deviation pins
its z at 0.6745 and can never fire) — the design gains a per-side data
floor with pooled-symmetric fallback before drift is measured for
adjudication.

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

## Phase D — untranslated-words calibration + defaults — COMPLETE (2026-07-30)

1. Same harness, paste faults as ground truth; sweep the judge knobs
   (ceiling, knee, run bonus) judge-only; triage sample as in Phase B.
2. Adjudicate: default-on or default-off, knob defaults, and the anchor
   table rows this rule contributes to preset-derivation.
3. Deliverable: calibration doc + (if default-on) the adjudicated oracle
   re-pin for the default config; catalog card, localization key, docs.

**Closed out**: added a case-shape excusal (`CopiedToken.
proper_noun_shaped`, via `signals::case_shape`) on top of the shipped
gates — owner-approved 2026-07-30 with two survival criteria encoded as
unit tests. Measured, adjudicated drift: WA-251 all-config dump
430 → 55 findings (−87.2%); 23-pair manifest 625 → 284 (−54.6%); zero new
findings anywhere (excusal only shrinks the candidate set); real catches
(gaz-ulb English pastes, zga-x-mahanji Swahili catches, omt-reg's
half-translated-draft class) all confirmed still live; genealogy/name-
list false positives removed wholesale. `run_bonus` re-examined against
a new partial-paste seed fault (the MAT 9:15 shape, non-saturating
unlike the whole-verse paste fault) and kept at its 0.5 default — it
sits at the recall/noise knee. Rule stays **default-off** for now (owner
decision). Full packet: `documentation/calibration/
2026-07-30-untranslated-word-calibration.md`.

## Open questions (owner)

- Tier-1 target hygiene: the Tech_Advance repos are real field data —
  findings there may be *true positives*; triage will tell us whether to
  quarantine any pair from the clean-negative denominator.
- Whether Phase B's re-pin (if any) waits for the preset-derivation
  anchor-table work or lands independently.
- rmn: drop from tier 1 vs run against en_ulb behind the versification
  guard.
