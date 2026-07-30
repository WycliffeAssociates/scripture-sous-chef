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

1. `eng-asv`/`eng-kjv` **LEV 6:8** (z=55.2/47.9, `both`) —
   **CORRECTED on steward prescreen (2026-07-30): versification offset,
   not register.** The kjv/asv slice at "LEV 6:8" carries the content of
   *English* Lev 6:15 ("he shall take of it his handful…") — those vref
   files follow Hebrew numbering in this chapter (Hebrew 6:8 = English
   6:15) while `en_ulb` follows English ("Then Yahweh spoke to Moses,
   saying"). A *sustained* offset run has no adjacent opposite-sign pair,
   so the adjacent-shear detector sees only shear *boundaries*, not
   offset *segments* — a known limitation, noted for Phase D.
2. `eng-asv`/`eng-kjv` **NUM 17:11** (z=41.0/36.0, `both`) — same
   diagnosis: the slice is English Num 16:46's content ("Take a
   censer…"; Hebrew Num 17 = English 16:36ff).
3. `WA-es-419-ulb` **LEV 18:2** (z=29.6) — Spanish verbosity vs. `en_ulb`.
4. `eng-kjv` **1CH 6:17** (z=28.4) — 1CH 6 is a known Hebrew/English
   offset site (also in the §6 shear list); offset suspected.
5. `eng-asv` **PSA 44:4**, **NEH 7:69**, **NUM 1:46** (z=27–28) —
   PSA 44:4 is suspect for the Hebrew Psalm-title offset
   (superscription counted as v1) and NEH 7:69 is a known
   versification variant; NUM 1:46 is verified genuine and striking:
   KJV/ASV spell out the total in words ("six hundred thousand and
   three thousand...") where `en_ulb` writes digits ("603,550") — a
   *formatting* difference the rule correctly reads as a length
   outlier, a distinct pattern from genuine over/under-translation.
6. `eng-kjv` **1JN 5:7** (z=26.3) — the Comma Johanneum, see §6.
   Verified genuine (textual tradition).

**Prescreen conclusion:** after book-grain quarantine and adjacent-shear
exclusion, the top of the outlier list is *still* dominated by
versification artifacts (sustained offset segments) in the
archaic-English tier-2 pairs. This does not indict the rule — tier-1
same-ecosystem pairs share versification by construction — but any
future segment-grain shear detector should subsume these, and tier-2
clean-negative rates should be read with this contamination in mind.

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

## 9. Phase B2 — asymmetric double-MAD spread (2026-07-30, owner-adjudicated, accepted)

Owner adjudication after §8 (above): z=3.5 confirmed, AND split the
spread measurement by direction — a symmetric MAD mis-sizes the squeezed
(bounded-at-zero) short tail against the open-ended long tail. Full
design, the collapse property this uncovered, the per-side data floor
that fixes it, and the oracle-pairing correction (§1 above's "zero
findings ever" framing was true only for `--fleet`, not the oracle
dump — `prop.length-ratio` has been firing in the oracle gate all along)
are in `documentation/adrs/0069-length-ratio-asymmetric-spread.md`. This
section is the paired-harness half of that ADR's evidence; **the owner
reviewed the full package (drift table, floor design, catch-rate deltas)
and accepted it as-is on 2026-07-30 — ADR 0069 is Accepted and this
change is landed.**

### Per-side floors (both vocabularies)

Median non-quarantined book floor at z=3.5, long side vs. short side,
full 23-pair fleet:

| | long-side floor (median %) | short-side floor (median %) |
|---|---|---|
| tier-1 (18 pairs, 496 books) | 68.3% | 61.8% |
| tier-2 (5 pairs, 288 books) | 40.6% | 39.1% |

The two sides are close at the median (this fleet's books are not
wildly skewed) but not identical, and individual books vary more — see
each pair's `<id>.floors.tsv` for the full per-book, per-z, per-side
breakdown that feeds this median.

### Finding-count deltas at 3.5/3.5 (real rule, 4 tier-1 pairs)

Same pairs as §2 (`amo`, `bbm`, `bsj` vs. `en_ulb`; `kiz` vs. `sw_ulb`,
the Swahili-source pair):

| pair | Phase B (symmetric MAD) | B2 (asymmetric + floor) | Δ |
|---|---|---|---|
| amo vs en_ulb | 217 | 184 | −15.2% |
| bbm vs en_ulb | 108 | 83 | −23.1% |
| bsj vs en_ulb | 127 | 128 | +0.8% |
| kiz vs sw_ulb | 209 | 196 | −6.2% |

Fleet-wide (all 23 pairs, oracle gate): total findings 92,731 → 86,131
(WA-subset, default config), `prop.length-ratio` 47,598 → 40,998
(−13.9%). Full table, per-rule confirmation (only `prop.length-ratio`
moved), and the no-floor-vs-floor+fallback byte-identical result are in
ADR 0069.

### Seeded-chop catch-rate change

Combined (real OR-gate) catch rate, aggregated across the same 4 pairs,
at z=3.5 — chops are short-side faults, so this is the direct read on
"did the short side get more or less sensitive":

| | Phase B | B2 | read |
|---|---|---|---|
| tail-chop 10% | 0% | 0% | unchanged (still undetectable, as expected) |
| tail-chop 20% | 1% | 1% | unchanged |
| tail-chop 30% | 12% | **16%** | improved |
| tail-chop 50% | 30% | **34%** | improved |
| source-paste | 0% | 0% | unchanged (still length-ratio's structural blind spot) |
| clean (false-positive) rate @3.5 | 2.03% | **1.83%** | improved (noise went DOWN, not up) |

**Read**: the short side got more sensitive (higher catch rate on real
chop faults) while overall noise decreased — the opposite of a
recall/precision trade-off, which is the signature of the OLD symmetric
MAD having been miscalibrated in a way asymmetric measurement corrects,
not of a knob simply being loosened. Long-side noise specifically was
not isolated as its own number in this pass (the clean-rate table
doesn't split by sign), but the aggregate noise decrease is inconsistent
with a long-side noise increase large enough to matter.

### Status

Owner-adjudicated and accepted 2026-07-30. ADR 0069 is Accepted; this
change is committed (`core(prop): asymmetric double-MAD spread +
per-side thresholds (ADR 0069)`).
