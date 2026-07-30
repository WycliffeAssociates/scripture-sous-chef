# `prop.length-ratio` paired survey — Phase B calibration

- Date: 2026-07-30
- Scope: Phase B of `documentation/plans/2026-07-30-source-paired-tier-plan.md`
  (length-ratio calibration). Measurement only — no engine change, no
  default change. The adjudication at the end is a recommendation.
- Harness: `crates/core/examples/calibrate/survey/paired.rs`
  (`--paired-survey`, `--seed-faults`), reading
  `documentation/calibration/corpora-pairs.tsv`.
- **Fidelity note.** Every firing decision below comes from the shipped
  rule itself (`signals::proportionality::length_ratio_findings`, the real
  `judge`), harvested via a single near-zero-threshold pass per corpus and
  re-thresholded arithmetically for every swept z (`harvest_real_verdicts`
  in `paired.rs`). No z, median, or MAD this file computes ever decides
  whether a verse fires — only the floors table's descriptive stats are
  self-computed, and they are never a gate.

## 1. The fidelity correction, confirmed against the real fleet

Phase A's smoke run used a book-only re-derivation and undercounted:
small books (under `min_verses` = 50) got no verdict at all. The real
rule's `judge` fires on **either** of two channels — book z or project z
(the whole corpus's pooled distribution) — and Phase B measures both.

Fleet-wide, book-only vs. real-rule (both channels) findings at z=3.5,
same 23 pairs:

| | total findings @ z=3.5 |
|---|---|
| Phase A (book-only, smoke) | 4,896 |
| Phase B (real rule, book ∨ project) | 6,073 |
| ratio | **1.24×** |

The delta is uneven — `amo` alone goes 98 → 217 (2.2×) because a larger
share of its books sit under `min_verses`. Concretely, in `eng-kjv` vs.
`en_ulb`:

- **MAL** (49 paired verses, under `min_verses`): 0 findings. Every
  verse's ratio genuinely sits within typical range for this pair — a
  true negative, not a coverage gap.
- **OBA** (21 paired verses, under `min_verses`): 1 finding, scope
  `project` — its own book channel never judges (too few verses), but the
  verse is still caught because the project channel is corpus-wide. This
  is the exact class of correction the owner called out.

**Small-book coverage statement**: the project channel does what it is
supposed to. A book too small to establish its own distribution is not
silently unprotected — it rides the corpus-wide distribution instead. The
two example books above show both sides of that: OBA had something to
catch and the project channel caught it; MAL had nothing to catch and
correctly stayed silent. No fleet evidence surfaced in this survey that
the dual-channel design misbehaves (over-fires or under-fires) for small
books — the Phase A plan's n-weighted-blend fallback is not indicated.

## 2. Seeded-fault curves (real rule, 4 tier-1 pairs)

Ground truth: `amo` (en_ulb), `bbm` (en_ulb), `bsj` (en_ulb), `kiz`
(**sw_ulb** — the Swahili-source pair). 120 verses seeded per pair (20
per fault kind), fixed seed, `--seed-faults`. Numbers below are the
**combined** (real OR-gate) catch rate, aggregated across the 4 pairs.

### Catch rate by fault magnitude, at z=3.5

| fault | caught / seeded | rate |
|---|---|---|
| tail-chop 10% | 0/80 | 0% |
| tail-chop 20% | 1/80 | 1% |
| tail-chop 30% | 10/80 | 12% |
| tail-chop 50% | 24/79 | 30% |
| whole-verse delete | 0/0 | n/a — see note |
| source-verse paste | 0/80 | 0% |

**Confirms the owner's prior exactly**: 10–20% chops are not detectable
(measured: 0–1%). Notable new findings:

- **Whole-verse deletion is invisible to this rule by construction**: an
  emptied target verse never pairs (`pair_verses` skips zero-grapheme
  sides, mirroring the production rule's own skip), so it never reaches
  `judge` at all — not a miss, a structural blind spot. This is exactly
  the untranslated-verse case Phase C/D's untranslated-words substrate is
  shaped to see instead.
- **Source-verse paste (0% caught)**: pasting the *source-language* verse
  wholesale over the target is not a length outlier when the two
  languages have comparable verbosity (English↔English/French/Bantu in
  this sample) — length-ratio cannot distinguish "right length, wrong
  language" from "right length, right language". This is
  untranslated-words' reason to exist, not a length-ratio defect.

### z-sweep (tail-chop 50%, the only magnitude with real signal)

| z | caught (combined) | clean flag rate (combined) |
|---|---|---|
| 2.0 | 73/79 (92%) | 9.49% |
| 2.5 | 55/79 (70%) | 5.41% |
| 3.0 | 32/79 (41%) | 3.27% |
| **3.5** | **24/79 (30%)** | **2.03%** |
| 4.0 | 20/79 (25%) | 1.34% |
| 4.5 | 16/79 (20%) | 0.94% |
| 5.0 | 15/79 (19%) | 0.73% |
| 5.5 | 15/79 (19%) | 0.58% |
| 6.0 | 7/79 (9%) | 0.43% |

Book-only vs. combined at z=3.5, tail-chop 50%: 24/79 vs 24/79 — for
*this specific* fault/magnitude the project channel adds nothing beyond
what book already catches (the seeded verses' own books are all
`min_verses`-eligible in these 4 pairs). The channel's real-fleet value
shows up in the fleet-wide finding count (§1), not this particular
fault curve.

## 3. Floors table (both vocabularies)

Per-pair, per-book, per-project floors live in
`<pair-id>.floors.tsv` (full z-sweep, both channels represented — book
rows and a `project` row). Aggregated headline (median across all
non-quarantined judgeable books, 18 tier-1 pairs / 5 tier-2 pairs):

| z | tier-1 book floor (median %) | tier-2 book floor (median %) |
|---|---|---|
| 2.0 | 37.0% | 23.0% |
| 2.5 | 46.3% | 28.7% |
| 3.0 | 55.5% | 34.5% |
| **3.5** | **64.8%** | **40.2%** |
| 4.0 | 74.0% | 45.9% |
| 4.5 | 83.3% | 51.7% |

Project-channel floors at z=3.5 range 30–83% across pairs (median ≈54%),
generally tighter than tier-1 book floors because the pooled sample is
larger.

**Correction to the owner's prior**: the prior estimate was "roughly
2–3× longer/shorter" (100–200% off typical) for the meaningful band. The
measured floors are **tighter than that** — at z=3.5 a book typically
needs only ~40–65% deviation from its own typical ratio to fire, not
100%+. The rule is more sensitive in percent terms than the prior
assumed. This does not by itself argue for moving z (see §6) — it means
the UI percent-label framing ("z=3.5 ≈ ~65% off-typical") should use
these measured numbers, not the prior's guess.

## 4. Fleet finding-rate histogram (bimodality read)

Findings per 1,000 paired verses, all 23 pairs, real rule at z=3.5:

- range: 11.9–36.2 per 1k, median 16.5 per 1k
- **Read: unimodal, not bimodal.** Every pair — tier-1 field data and
  tier-2 clean-negative pseudo-pairs alike — clusters in a 12–30/1k band.
  The two outliers on the high side are `eng-kjv` (36.2/1k) and
  `eng-asv` (35.1/1k), both archaic-register English against the
  modern-register `en_ulb` — expected, since register drift itself
  produces real (if benign) length differences. There is no clean/dirty
  split visible at this stage; that is itself useful evidence that the
  rule doesn't have a "mostly silent, occasionally very loud" failure
  mode across this fleet.

## 5. Multi-source sensitivity

Same target, two declared sources, real rule at z=3.5
(`multi-source-sensitivity.tsv`):

| target | source A | source B | flagged A | flagged B | overlap | Jaccard | shared books | mean \|Δfloor%\| |
|---|---|---|---|---|---|---|---|---|
| bbm | en_ulb | fr_ulb | 108 | 95 | 43 | 0.269 | 27 | 9.15 |
| bsj | en_ulb | bn_ulb | 127 | 122 | 57 | 0.297 | 27 | 15.81 |
| gux-x-gourmantche | en_ulb | fr_ulb | 301 | 142 | 79 | 0.217 | 27 | 9.53 |

**Read**: which verses fire is meaningfully source-dependent (Jaccard
0.22–0.30 — most flagged verses do NOT agree across sources), and the
per-book floor itself shifts by 9–16 percentage points on average when
the source changes. This is a real, expected property of a rule that
measures against a *specific* source's verbosity, not a defect — but it
means length-ratio's findings should be read as "outlier relative to
THIS source," not as an absolute translation-quality signal. `gux`'s much
larger flagged-A count (301 vs 142) is consistent with the versification
quarantine already isolating 3 books in the en_ulb pairing that don't
reappear as quarantined against fr_ulb (see that pair's `books.tsv`) —
worth a closer look in Phase D triage, not resolved here.

## 6. Versification shear (new, first-class signal)

Chapter-grain shear (adjacent verses, `|z| > 5`, opposite signs) detected
in `<pair-id>.shear.tsv`, excluded from all finding counts above:

| pair | shear pairs |
|---|---|
| eng-kjv vs en_ulb | 59 |
| eng-asv vs en_ulb | 88 |
| WA-es-419-ulb vs en_ulb | 18 |
| WA-pt-br-ulb vs en_ulb | 5 |
| WA-fr-ulb vs en_ulb | 3 |
| WA-nyf-x-rabai-reg vs sw_ulb | 6 |
| WA-kiz-reg vs sw_ulb | 2 |
| WA-ema-x-emai-reg vs en_ulb | 1 |

`eng-kjv` vs `en_ulb`'s 59 pairs span 31 distinct book:chapter sites,
including the exact case the owner's own smoke run flagged
(**2CH 2:13/2:14**, |z|=39.8/−7.0 — KJV's verse 13 absorbs what en_ulb
splits starting at 14) and the **Comma Johanneum** (1JN 5:7, |z|=26.3,
in the tier-2 triage dump below — KJV's longer trinitarian clause vs.
en_ulb's shorter modern-critical-text rendering is a real, well-known
textual-tradition difference, not noise). Other sites: GEN 32, EXO 8,
LEV 6 (two adjacent shear pairs in the same chapter), NUM 17, DEU 23
(three sites) and 29, 1SA 21 and 24, 2SA 19, 1KI 5 and 22, 2KI 12, 1CH 6
and 12, 2CH 2 and 14, NEH 4 and 10, PSA 18/40/61/92, ISA 9, DAN 4 and 6,
HOS 2, JOL 3, MIC 5, SNG 7, EZK 21, ECC 5 — the full 31-site list is in
`eng-kjv__vs__WA-en-ulb.shear.tsv`.

This confirms the owner's ruling: shear is a real, actionable signal (a
PO can point a translator at exactly these chapter boundaries to check
versification), and it is a *different* failure mode than a translation
length problem — mixing the two into one finding count would blur both.

## 7. Tier-2 triage dump (top 40 by \|z\|, model-prescreened)

`triage-top40.tsv`, excluding shear/quarantine. Top of the list (for the
owner's spot-adjudication):

1. `eng-asv`/`eng-kjv` **LEV 6:8** (z=55.2/47.9, `both`) — "meal offering"
   (ASV/modern) vs. `en_ulb`'s much shorter paraphrase; likely a real
   register/verbosity difference, not damage.
2. `eng-asv`/`eng-kjv` **NUM 17:11** (z=41.0/36.0, `both`) — same pattern.
3. `WA-es-419-ulb` **LEV 18:2** (z=29.6) — Spanish verbosity vs. `en_ulb`.
4. `eng-kjv` **1CH 6:17** (z=28.4).
5. `eng-asv` **PSA 44:4**, **NEH 7:69**, **NUM 1:46** (z=27–28) — NUM
   1:46 is a striking case: KJV/ASV spell out the total in words
   ("six hundred thousand and three thousand...") where `en_ulb` writes
   digits ("603,550") — a *formatting* difference the rule correctly
   reads as a length outlier, worth flagging to the owner as a distinct
   pattern from genuine over/under-translation.
6. `eng-kjv` **1JN 5:7** (z=26.3) — the Comma Johanneum, see §6.

The full 40 rows (verse key, both text slices, fraction, z, scope) are
in the TSV for the owner's pass; the model-prescreen above is a reading
aid, not an adjudication.

## 8. Recommendation — owner adjudicates

**z=3.5 is not contradicted by this survey and I recommend confirming
it, but the evidence is mixed enough that this is a judgment call, not a
clean verdict:**

- **For confirming 3.5**: the clean-flag rate at 3.5 (≈2% on tier-1 pairs
  with real translation noise, and the tier-2 triage sample above shows
  the top hits are largely *real* differences — register, script/number
  formatting, textual tradition — not rule malfunction) suggests 3.5 is
  already sitting in a reasonable precision/recall zone. Moving to 3.0
  roughly doubles the clean-flag rate (3.27% vs 2.03%) for a catch-rate
  gain that's modest at these fault magnitudes (41% vs 30% of 50%-chops).
- **Against confirming 3.5 without changes**: the fleet-wide finding
  count is 24% higher than anyone has seen before (this rule has
  produced zero findings ever, per the plan's own framing) — z=3.5 was
  originally calibrated sourceless-blind. Nothing here proves 3.5 is
  wrong, but nothing here proves it's right either; it has simply never
  been measured against real sources until this survey.
- **The versification-shear carve-out changes the denominator**: 8 of 23
  pairs have shear pairs (up to 88 for eng-asv) now excluded from finding
  counts. Any future re-pin should be measured post-shear-exclusion, as
  this survey already does.

No default change and no engine change are made by this document. If the
owner confirms 3.5, that is a no-op (the shipped default is already
3.5). If the owner wants to re-pin, that is an intentional behavior
change requiring its own ADR with the measured drift, per ADR 0059 — the
numbers in §1 and §2 are exactly the drift figures that ADR would need.
