# Calibration SPIKE: word-level casing, two-factor score

- **Date:** 2026-07-09; **updated 2026-07-10** (post-hyphen-fix re-run — all
  headline numbers below are the post-fix fleet; see the dated section at the
  end for the before/after delta).
- **Status:** SPIKE — exploratory. **No knob is frozen**, no rule/`RuleStats`/
  `CasingConfig` was touched. The measurements below inform the casing-rule
  rebuild (next-checks-shortlist item 4); they are not a decision.
- **Core change:** `walk_book_experimental` (+ `WordObsExperimental`,
  `PosClassExperimental`, `FirstCaseExperimental`) alongside `walk_book` in
  `crates/core/src/signals/casing.rs` — emits one observation per **word**
  (span, position class, first-letter case), reusing `walk_book`'s exact
  pending-terminal state machine. The word unit is the repo's UAX #29
  tokenizer (`token::tokenize`) with adjacent tokens joined across a
  word-internal hyphen (U+002D / U+2010 flanked by letters) merged into one
  compound — see the post-hyphen-fix section. Every spike symbol carries the
  `_experimental` suffix.
- **Harness:**
  - per-corpus — `cargo run --release -p ssc-core --example calibrate -- --casing <corpus>`
    (habit table, censoring shadow, current-rule fate, knee sweeps, histogram, samples);
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --casing corpora/vref`
    (1,504 corpora, ~17.3M verses; ~25s on 8 cores; all tables aggregated to
    stdout, including the floor-decision packet — per-channel volume grids,
    tracked anchors, near-floor samples, German storm).
- **Reference setting** (used for the "surfaced" volume, samples, hard-vs-soft
  diff, noisiest ranking, current-rule fate): absolute knee **k = 32**, floor
  **0.5**, `confidence_z = 1.96` — the ADR 0050 analog, chosen only so numbers
  are comparable across sections. It is **not** a proposed default.

## Generative model (the settled design)

An occurrence's case = `OR(position-forces-uppercase, word-is-intrinsically-capitalized)`.
Censoring is one-directional: **uppercase at a forced position is uninformative**
about the word (discard from the lexicon), lowercase is informative everywhere,
mid-flow observations are all informative.

**Forced positions** are structural, defined before any casing knowledge:
(a) a word immediately following a *bare* attached terminal glyph — the same
`pending: Option<(char, bool)>` machine `walk_book` uses, carried across verse
seams, with the intervening-punctuation boundary (`."`, `...`) left unpoliced
(it falls to mid-flow, exactly as the live rule declines to police it); and
(b) the book-initial word. **Verse-initial is NOT forced** — verses are
reference plumbing; discourse flows across the seam.

A "word" is a UAX #29 word token (repo `token::tokenize`), with adjacent tokens
joined across a single word-internal hyphen merged into one compound. UAX #29
keeps apostrophes word-internal (`ng'ombe` is one word) but splits at hyphens,
so the merge is what keeps `Bar-jesus` a single word whose first letter is `B`.
(Earlier the spike used a bare letter-run, which split hyphenated compounds and
apostrophe words; the *post-hyphen-fix* section quantifies what that cost.)

### Estimation

- **Step 1 — per-word intrinsic profile from mid-flow occurrences only.** Key =
  case-folded word. Recorded per word: `midflow_upper`, `midflow_lower`
  (the intrinsic profile) and `forced_upper`, `forced_lower` (for the positional
  channel and the soft-censoring re-estimate).
- **Step 2 — corpus positional habit**, per terminal glyph, two ways:
  - **naive** = Wilson dominance of uppercase over *all* forced-position words
    after that glyph (the current rule's estimate);
  - **lexicon-restricted** = the same over words the Step-1 lexicon calls
    intrinsically lowercase (Wilson-lower-bound of the lower share > 0.5). This
    removes the proper-noun confound: names are capitalized after a period for
    word reasons, not positional ones.

### Judge-time 2×2 (every lowercase site)

| | mid-flow lowercase | forced-position lowercase |
| --- | --- | --- |
| **word intrinsically capitalized** | **INTRINSIC** anomaly | **BOTH** (corroboration) |
| **word lexicon-lowercase** | expected — silent | **POSITIONAL** anomaly |

- INTRINSIC score = `dominance(word midflow upper/total) × rarity(lowercase recurrence)`.
- POSITIONAL score = `lexicon-restricted habit(glyph) × rarity(this word's forced-position lowercase recurrence)`.
  This kills "the corpus itself writes *und* lowercase after periods 500×".
- BOTH reports both scores; the site is tagged both-quadrant.

### Recurrence knee — two shapes swept

- **(a) absolute** (ADR 0050): `rarity = 1 − min(minority−1, k)/k`, `k ∈ {8,16,32,64,128}`.
- **(b) rate-scaled**: minority replaced by minority per 1k opportunities;
  `rarity = 1 − min(rate, k_rate)/k_rate`, `k_rate ∈ {0.5,1,2,4,8,16}` per-1k.
  Opportunities = the word's total occurrences (intrinsic) or its forced-position
  occurrences (positional).

Floor swept over `{0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 0.98}` (0.95 added
for the floor-decision packet's candidate band).

### Censoring variants

- **hard** — forced uppercase discarded; the intrinsic profile is midflow-only.
- **soft** — forced uppercase re-enters the intrinsic profile weighted by
  `(1 − positional_habit)`, a single re-estimate after Step 2 (no EM). The habit
  used is the pooled lexicon-restricted forced-uppercase dominance
  (approximation: one scalar per corpus, not per-glyph — keeps the profile to
  four counts per word; dominated by `.`-class terminals anyway).

## Headline numbers (fleet, 1,504 corpora)

### Fate of the current rule's surfaced set

The live rule surfaces **17,357** sites fleet-wide at its shipped floor 0.98
(score = per-glyph Wilson dominance). Under the new positional score, at the
reference setting:

| fate | sites | share |
| --- | --: | --: |
| **die — minority recurrence** ("corpus writes it lowercase after terminals N×") | 11,146 | 64.2% |
| survive as a clean positional anomaly | 3,471 | 20.0% |
| die — word unclassifiable (neither intrinsic-cap nor lexicon-lower) | 2,325 | 13.4% |
| both-quadrant (proper noun lowercased at a forced position) | 415 | 2.4% |
| die — positional habit below floor (proper-noun confound) | 0 | 0.0% |

The **recurrence factor is the dominant death cause** — exactly the
capitalize-after-terminal confound the design predicted. Habit-death is
negligible *for this set* because a naive dominance ≥ 0.98 already implies the
corpus strongly capitalizes that glyph, so its lexicon-restricted habit is high
too; the confound shows up in the **delta** (below) and as recurrence-death, not
as habit-death at the 0.98 floor.

### New-model surfaced volume vs the current 17.5K

At the reference setting (abs k=32, floor 0.5, hard): **104,264** surfaced
across 1,308 corpora — intrinsic 52,861, positional 37,205, both 14,198. This is
far larger than 17,357 because floor 0.5 is far looser than 0.98; at floor 0.98
absolute only **1,116** surface. Volume is entirely a knee/floor decision (see
sweeps) — not frozen here.

### Naive vs lexicon-restricted habit delta (proper-noun confound)

Per-corpus pooled `naive_dom − lexicon_dom`, over 1,322 corpora with a habit:

| p10 | p50 | p90 | max |
| --: | --: | --: | --: |
| +0.009 | **+0.045** | +0.106 | +0.9997 |

For most corpora the proper-noun confound inflates the apparent
"capitalize-after-terminal" habit by ~4.5% (median). The long right tail (max ≈ 1.0) is the
important class: corpora that capitalize **only proper nouns** after a period,
with no sentence-start convention for common words — where the naive habit is a
near-total artifact of names. The lexicon restriction is what separates these
from genuine capitalizers.

### Censoring shadow (cap words whose uppercase evidence is ≥90% forced)

Fraction of all-position-capitalized words that hard censoring cannot see
(their uppercase evidence is almost entirely forced-position):

| | p50 | p90 | max |
| --- | --: | --: | --: |
| TYPES | 2.0% | 7.5% | 91.5% |
| TOKENS | 1.2% | 21.9% | 92.6% |

Small for most corpora (scripture proper nouns recur mid-sentence — "the God of
Abraham"), but a long tail of corpora lose most of their capitalized types to
the midflow-only view. Soft censoring recovers part of this: fleet soft-surfaced
**101,418** vs hard 104,264, with **3,507** verdicts (3.4% of the surfaced set)
differing between hard and soft.

### Word-table cardinality (future word-level `RuleStats` sizing)

Per corpus, confirming the idea-doc's warning that word-type maps "won't be a
few KB":

| | p50 | p90 | max |
| --- | --: | --: | --: |
| word types | 12,648 | 30,228 | 111,981 |
| approx table bytes (key + `WProfile` + overhead) | 620 KB | 1.60 MB | 7.6 MB |

The four-`u32` profile is ~47–49 B/type in practice. A per-book (not per-corpus)
partition, or a frequency-gated table (drop hapax types), will be needed if this
becomes a live aggregate. (The worst-case bytes roughly halved from the pre-fix
15.0 MB: hyphen-heavy scripts — Vietnamese transliterations like
`Nê-bu-cát-nết-sa` — no longer explode into one type per syllable.)

## Knee sweeps (fleet surfaced sites)

### (a) absolute knee — rows = k, columns = floor

| k\floor | 0.30 | 0.40 | 0.50 | 0.60 | 0.70 | 0.80 | 0.90 | 0.95 | 0.98 |
| --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 8 | 73,642 | 56,576 | 41,901 | 28,726 | 19,024 | 10,967 | 4,698 | 2,665 | 1,116 |
| 16 | 116,863 | 91,045 | 66,038 | 44,550 | 28,519 | 15,897 | 6,244 | 2,665 | 1,116 |
| **32** | 182,383 | 142,591 | **104,264** | 69,821 | 43,902 | 23,976 | 8,998 | 3,483 | 1,116 |
| 64 | 281,334 | 221,743 | 161,525 | 106,598 | 66,546 | 36,060 | 13,400 | 4,788 | 1,345 |
| 128 | 419,072 | 336,701 | 248,594 | 160,831 | 99,990 | 53,477 | 19,971 | 7,166 | 1,910 |

### (b) rate knee — rows = per-1k minority cutoff, columns = floor

| r\floor | 0.30 | 0.40 | 0.50 | 0.60 | 0.70 | 0.80 | 0.90 | 0.95 | 0.98 |
| --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 0.5 | 41 | 27 | 21 | 14 | 10 | 1 | 0 | 0 | 0 |
| 1.0 | 194 | 163 | 106 | 62 | 27 | 14 | 1 | 0 | 0 |
| 2.0 | 650 | 505 | 371 | 256 | 161 | 61 | 14 | 0 | 0 |
| 4.0 | 1,613 | 1,282 | 1,015 | 764 | 490 | 247 | 60 | 14 | 0 |
| 8.0 | 4,735 | 3,594 | 2,785 | 1,922 | 1,262 | 745 | 233 | 52 | 10 |
| 16.0 | 12,119 | 9,689 | 7,754 | 5,655 | 3,461 | 1,803 | 703 | 210 | 29 |

**The rate knee is the wrong shape for word-level anomalies at the spacing-scale
cutoffs.** Word opportunities are small (tens–hundreds per type, not the tens of
thousands a punctuation mark accrues), so a genuine single-slip proper noun
(seen ~100×, one lowercase) already sits at ~10/1k — above the entire swept
range — and is silenced. To reach the absolute-k=32 volume (~107k) the rate
cutoff would have to be pushed far past 16/1k. The absolute knee, which treats a
hapax minority as rarity 1 regardless of denominator, matches the word-level
intuition ("*Yesu* written *yesu* once is an anomaly however often *Yesu*
appears"). The rate knee remains the right tool for the mark-level spacing rule,
where denominators are huge; it does not transfer to words.

## Histogram shape — NOT as cleanly bimodal as spacing

Score distribution at the reference knee (abs k=32) over all **24,751,274**
classifiable lowercase sites (~7.2M fewer than the pre-fix 31,977,065 — the
hyphen/compound fragments that inflated the token count are gone):

- **[0.000, 0.025): 23,806,220 sites — 96.2%.** The overwhelming mass:
  correctly-lowercase common words in mid-flow, plus recurrence-collapsed second
  conventions. This is the analog of spacing's ≈0 collapse.
- A **broad, slowly-declining tail** from 0.05 to 1.0. Unlike spacing (which
  became sharply bimodal — a ≈0 spike plus a tight 0.8–1.0 cluster), casing has a
  **fat, continuous mid-mass**: roughly uniform ~4k–10k sites per 0.025-bucket
  across [0.30, 0.70], only gently thinning toward 1.0 (~1.4k in [0.975, 1.0)).

The mid-mass is real, not a scoring artifact: words carry **graded** case
ambiguity that marks do not — German noun/adjective homographs (*alter* the
adjective vs *Alter* the noun, dominance 0.675), transliteration variants, and
words that are genuinely capitalized part of the time. The recurrence factor
collapses the systematic second-conventions (that is the [0,0.025) mass) but a
true continuum of per-word ambiguity survives. **Consequence: the floor still
does volume policy in casing**, whereas in spacing the recurrence factor made it
nearly floor-insensitive across [0.5, 0.9]. Casing will need either a stricter
floor, a second discriminator, or acceptance that the surfaced set is
floor-tunable rather than bimodally self-selecting.

## Noisiest corpora (reference setting)

| corpus | surfaced | intrinsic | positional | both | pooled delta |
| --- | --: | --: | --: | --: | --: |
| dan1931 (Danish) | 1,290 | 1,055 | 103 | 132 | +0.119 |
| deutkw (German) | 896 | 614 | 221 | 61 | +0.049 |
| ukr1871 / ukr1996 / ukrfb | 600 | 46 | 543 | 11 | +0.036 |
| polubg (Polish) | 556 | 42 | 508 | 6 | +0.039 |
| deuelo (German) | 554 | 388 | 119 | 47 | +0.040 |
| engojb (Orthodox Jewish Bible) | 524 | 482 | 14 | 28 | +0.056 |
| deu1951 / deuelbbk / deu1912 (German) | 454–510 | (intrinsic-heavy) | | | ~+0.04 |

Two legible storm shapes: **noun-capitalizing languages** (German, Danish) flood
the *intrinsic* channel — every common noun written lowercase is an anomaly by
this model, which is arguably correct for those orthographies but is a huge
volume; and **positional-heavy Slavic** corpora (Ukrainian, Polish) flood the
*positional* channel. engojb is transliterated-Hebrew casing churn. No single
knee tames both storm shapes — the intrinsic storm needs a per-word gate, the
positional storm a habit/recurrence gate.

## Sample surfaced findings (major-language corpora, reference knee)

Format: `sid  quadrant  [word]  glyph  dom  min  opp  rarity  score | context`.
`^book/∅` = book-initial (forced, no glyph).

```
swhulb  LUK 8:44  intrinsic  [yesu]  ^book  dom 0.995 min 1 opp 1316 rar 1.000 score 0.995 | nyuma ya yesu na kugusa pindo
WA-fr-ulb JHN 13:2 intrinsic [jésus] ^book  dom 0.994 min 1 opp 1219 rar 1.000 score 0.994 | le dessein de trahir jésus.
WA-sw-ulb EXO 19:8 intrinsic [musa]  ^book  dom 0.993 min 1 opp 902  rar 1.000 score 0.993 | Kisha musa akaja kutoa taarifa
WA-es-419-ulb 2KI 15:32 intrinsic [judá] ^book dom 0.993 min 1 opp 850 rar 1.000 score 0.993 | rey de judá.
deu1912 2CH 34:5 intrinsic [jerusalem] ^book dom 0.993 min 1 opp 778 rar 1.000 score 0.993 | reinigte also Juda und jerusalem,
ind     JHN 1:40  intrinsic  [petrus] ^book  dom 0.975 min 1 opp 265  rar 1.000 score 0.975 | dengan kata 'petrus' dalam bahasa
spaRV1909 1SA 7:8 intrinsic  [filisteos] ^book dom 0.976 min 1 opp 237 rar 1.000 score 0.976 | de mano de los filisteos.
vie1934 MAT 24:24 intrinsic  [christ] ^book  dom 0.987 min 2 opp 555  rar 0.969 score 0.956 | nhiều christ giả và tiên tri giả
fraLSG  ACT 19:13 intrinsic  [juifs]  ^book  dom 0.965 min 3 opp 250  rar 0.938 score 0.904 | Quelques exorcistes juifs ambulants
deu1912 2SA 24:8  intrinsic  [land]   ^book  dom 0.994 min 1 opp 893  rar 1.000 score 0.994 | durchzogen das ganze land und kamen  (German noun lowercased)
deu1912 EZK 31:1  intrinsic  [jahr]   ^book  dom 0.977 min 1 opp 247  rar 1.000 score 0.977 | im elften jahr, am ersten Tage      (German noun lowercased)
porblt  MAT 24:24 intrinsic  [messias] ^book dom 0.902 min 2 opp 70   rar 0.969 score 0.873 | Pois falsos messias e falsos profetas
eng-web 2MA 14:5  intrinsic  [meeting] ^book dom 0.962 min 1 opp 147  rar 1.000 score 0.962 | into a meeting of his council
eng-web 3MA 6:9   intrinsic  [gentiles] ^book dom 0.959 min 1 opp 134 rar 1.000 score 0.959 | abhorred, lawless gentiles.
engwebster MIC 5:7 intrinsic [gentiles] ^book dom 0.957 min 1 opp 129 rar 1.000 score 0.957 | shall be among the gentiles
# hyphenated compounds now stay whole (case = first letter of the head):
vie1934 2CH 35:10 intrinsic  [lê-vi]  ^book  dom 0.985 min 1 opp 388  rar 1.000 score 0.985 | và người lê-vi cứ theo ban thứ   (was: "lê" + "vi")
vie1934 2KI 25:1  intrinsic  [ba-by-lôn] ^book dom 0.981 min 1 opp 306 rar 1.000 score 0.981 | Nê-bu-cát-nết-sa, vua ba-by-lôn
engwebster 1CH 26:15 intrinsic [obed-edom] ^book dom 0.754 min 1 opp 20 rar 1.000 score 0.754 | To obed-edom southward

WA-en-ulb LAM 1:22 positional [deal]  '.' dom 1.000 min 2 opp 3   rar 0.969 score 0.968 | come before you. deal with them
eng-kjv SIR 7:5   positional [justify] '.' dom 0.999 min 1 opp 1   rar 1.000 score 0.999 | justify not thyself before
WA-es-419-ulb 1SA 13:10 positional [pronto] '.' dom 0.998 min 1 opp 9 rar 1.000 score 0.998 | pronto como él terminó de ofre
WA-fr-ulb LUK 4:31 positional [descendit] '.' dom 0.989 min 1 opp 1 rar 1.000 score 0.989 | descendit à Capernaüm
fraLSG  COL 2:22  positional [préceptes] '!' dom 0.985 min 1 opp 1  rar 1.000 score 0.985 | préceptes qui tous deviennent
tglulb  ACT 26:4  positional [nga]   '.' dom 0.998 min 1 opp 1   rar 1.000 score 0.998 | nga, nalalaman nang lahat
nld     GEN 6:19  positional [mannetje] ':' dom 0.948 min 1 opp 1 rar 1.000 score 0.948 | te behouden: mannetje en wijfje
ind     DEU 14:12 positional [rajawali] ':' dom 0.938 min 1 opp 1 rar 1.000 score 0.938 | tidak boleh dimakan: rajawali

eng-kjv 2MA 1:25  both       [almighty] ',' dom 0.961 min 1 opp 98  rar 1.000 score 0.961 | the only just, almighty, and everlasting
ron1924 PSA 46:5  both       [сфынтул] ',' dom 0.953 min 2 opp 97  rar 0.969 score 0.923 | четатя луй Думнезеу, сфынтул локаш
# residual hyphen class — a hyphen followed by a SPACE is NOT merged (correct):
tglulb  1CH 16:38 both       [edom]  '-' dom 0.963 min 1 opp 103 rar 1.000 score 0.963 | Kasama din si Obed- edom   (source text has "Obed- edom" — a real spacing defect, not a tokenizer artifact)

# near-floor (deu1912) — the mid-mass, genuine homographs:
deu1912 PHM 1:9   intrinsic  [alter]  ^book  dom 0.675 min 9 opp 48  rar 0.750 score 0.506 | ein alter Paulus   (adj "alter" vs noun "Alter")
```

## Artifacts / review flags (spike, not the rule)

1. **Hyphenated-compound splits — RESOLVED (2026-07-10).** The former letter-run
   word unit split *Merib-baal → merib + baal*, *A-hi-giô → giô*,
   *Bar-jesus → jesus*, so the trailing part surfaced as an anomaly. The word
   unit is now the UAX #29 tokenizer with hyphen-joined compounds merged, so
   these are single words. A **residual** class survives and is *correct*: a
   hyphen followed by a space (`Obed- edom`, tglulb 1CH 16:38) is a real spacing
   defect in the source, not a tokenizer artifact, and is left visible.
2. **Noun-capitalizing orthographies** (German, Danish) make *every* lowercase
   common noun an intrinsic anomaly. Correct by the model, but a volume storm;
   the live rule will need a per-word gate or per-language stance.
3. **Quote/apostrophe as terminal glyphs.** `walk_book`'s "first punct after a
   letter is the terminal" treats `'`, `-`, `:` as terminal candidates in the
   *gaps between words*; their naive habit is near zero so they never surface
   positionally, but they do generate forced-position observations. Harmless to
   volume; flagged for awareness. (Word-*internal* apostrophes — `ng'ombe`,
   `god's` — are no longer split: UAX #29 keeps them inside the word.)
4. **Homograph mid-mass** (*alter*, *dies*) is genuine ambiguity, not error —
   the reason the floor cannot be made bimodally self-selecting.

## What this spike answers for the rebuild

- The **recurrence factor is validated for casing**: it retires 64% of the
  current rule's set as recurring second-conventions, and preserves genuine
  single-slip proper nouns at rarity 1 (the *Yesu → yesu* case, score 0.995).
- The **absolute knee, not the rate knee**, is the right recurrence shape for
  word-level opportunities.
- The **lexicon restriction is load-bearing**: the naive/lexicon habit delta
  exposes corpora whose "capitalize after period" signal is pure proper-noun
  confound (delta → 1.0).
- **Casing is not bimodal like spacing** — the floor remains a real dial; a
  second discriminator (or per-channel floors) is the open question.
- **Soft censoring moves ~3.4% of verdicts** and recovers the ~2%-median (up to
  91%) capitalized-type censoring shadow; whether that is worth the second pass
  is a rule-design call.
- **Word-table sizing** (median ~12.6k types / ~620 KB, worst 7.6 MB) argues for
  per-book partitioning and/or frequency gating before this becomes a live
  aggregate.

Knobs are **NOT frozen.** The knee shape (absolute), the floor, the
hard-vs-soft choice, and the intrinsic-storm gate for noun-capitalizing
languages are open for the rule-design decision. The hyphen-tokenization fix is
**done** (2026-07-10; below).

## Post-hyphen-fix re-run (2026-07-10)

The spike's word unit was a bare letter-run, which fragmented hyphenated
compounds and (in principle) apostrophe words. It is now the repo's UAX #29
tokenizer (`token::tokenize`) with adjacent tokens joined across a single
word-internal hyphen (U+002D / U+2010 flanked by letters) merged into one
compound. UAX #29 already keeps apostrophes word-internal (`ng'ombe`, `god's`
stay whole); the merge is what keeps `Bar-jesus` one word whose first letter is
`B`. `walk_book`'s pending-terminal state machine is unchanged — it now runs
over the *gaps between* word tokens rather than over every grapheme. All
headline numbers above are the post-fix fleet; this section records the delta.

**What was tokenization artifact.** The artifact concentrated in the
**both-quadrant** channel — a hyphen tail like `Bar-jesus → jesus` is both
intrinsically-capitalized (as *Jesus*) and forced-position (after `-`). At the
reference setting the both channel fell **19,799 → 14,198 (−28%)**; net surfaced
fell only **106,998 → 104,264 (−2.6%)** because the fix also *creates* legitimate
whole-compound intrinsic findings (Vietnamese transliterations `lê-vi`,
`ba-by-lôn`; `obed-edom`) — intrinsic rose 50,408 → 52,861. So the fix is not a
volume cut; it is a **quality shift** out of the spurious both channel into
genuine compound-level intrinsic anomalies.

| metric | pre-fix | post-fix |
| --- | --: | --: |
| surfaced @ ref (abs k=32, floor 0.5, hard) | 106,998 | 104,264 |
| — intrinsic / positional / both | 50,408 / 36,791 / 19,799 | 52,861 / 37,205 / 14,198 |
| current-rule set, die-recurrence share | 65.0% | 64.2% |
| classifiable lowercase sites (histogram total) | 31,977,065 | 24,751,274 |
| word-types p50 / max | 11,852 / 119,689 | 12,648 / 111,981 |
| approx table bytes max | 15.0 MB | 7.6 MB |

The **classifiable-site count dropped ~7.2M (−23%)**: fragmenting compounds
inflated the token count, most sharply in hyphen-heavy scripts. The same effect
**halved the worst-case word-table bytes** (Vietnamese `Nê-bu-cát-nết-sa` no
longer explodes into one type per syllable).

**New/changed classes a parametric review should watch** (see the floor-decision
packet, §*Sample near-floor*): (a) hyphenated transliterated proper nouns now
surface as *whole-compound* intrinsic anomalies (correct — `Lê-vi` written
`lê-vi`); (b) a hyphen followed by a **space** (`Obed- edom`) is deliberately
**not** merged and remains a visible both-site — that is a real source spacing
defect, not an artifact.

### Floor-decision packet

Settled: absolute knee, soft censoring, all marks kept (no colon special-case),
rules ship default-off. Open: per-channel floors and `k`. All figures below are
the post-fix fleet (harness `--casing corpora/vref`, packet section).

**1 — Per-channel surfaced volume** (fleet-wide `total (affected corpora; top-5
share)`). The intrinsic and positional channels are independent volume knobs.

*intrinsic:*

| floor \ k | 8 | 16 | 32 |
| --: | --: | --: | --: |
| 0.80 | 4,139 (1022; 11%) | 6,201 (1098; 10%) | 9,935 (1163; 8%) |
| 0.90 | 1,596 (744; 10%) | 2,231 (821; 10%) | 3,292 (913; 10%) |
| 0.95 | 866 (521; 10%) | 866 (521; 10%) | 1,231 (605; 9%) |
| 0.98 | 396 (297; 9%) | 396 (297; 9%) | 396 (297; 9%) |

*positional:*

| floor \ k | 8 | 16 | 32 |
| --: | --: | --: | --: |
| 0.80 | 6,087 (614; 7%) | 8,623 (672; 7%) | 12,312 (721; 7%) |
| 0.90 | 2,841 (452; 10%) | 3,660 (510; 10%) | 5,177 (583; 9%) |
| 0.95 | 1,664 (360; 15%) | 1,664 (360; 15%) | 2,068 (410; 13%) |
| 0.98 | 675 (246; 19%) | 675 (246; 19%) | 675 (246; 19%) |

*both-quadrant:*

| floor \ k | 8 | 16 | 32 |
| --: | --: | --: | --: |
| 0.80 | 741 (445; 7%) | 1,073 (533; 5%) | 1,729 (661; 5%) |
| 0.90 | 261 (201; 10%) | 353 (251; 9%) | 529 (313; 8%) |
| 0.95 | 135 (111; 15%) | 135 (111; 15%) | 184 (146; 12%) |
| 0.98 | 45 (40; 22%) | 45 (40; 22%) | 45 (40; 22%) |

Volume is spread across many corpora (top-5 share 5–19%) — no single corpus
dominates any cell. `k` only bites at floors ≤ 0.90: at 0.95/0.98 a hapax
minority (rarity 1 at every `k`) is the surviving mass, so `k ∈ {8,16,32}` are
near-identical there.

**2 — Anchor fates** (post-fix; `dom·rarity` at k=32, and the floors it clears).
`i`/`p` = intrinsic/positional channel factors `(dom, minority, opportunities)`.

| anchor | verdict | quad | dom | min | score@k32 | alive @ floors |
| --- | --- | --- | --: | --: | --: | --- |
| swhulb LUK 8:44 *yesu* | TP | intrinsic | 0.995 | 1 | 0.995 | 0.80–0.98 |
| WA-fr-ulb JHN 13:2 *jésus* | TP | intrinsic | 0.994 | 1 | 0.994 | 0.80–0.98 |
| spaRV1909 1SA 7:8 *filisteos* | TP | intrinsic | 0.976 | 1 | 0.976 | 0.80–0.95 |
| vie1934 MAT 24:24 *christ* | TP? | intrinsic | 0.987 | 2 | 0.956 | 0.80–0.95 |
| eng-web 3MA 6:9 *gentiles* | TP-ish | intrinsic | 0.959 | 1 | 0.959 | 0.80–0.95 |
| eng-kjv SIR 7:5 *justify* | TP (pos) | positional | 0.999 | 1 | 0.999 | 0.80–0.98 |
| WA-en-ulb LAM 1:22 *deal* | TP (pos) | positional | 1.000 | 2 | 0.968 | 0.80–0.95 |
| fraLSG ACT 19:13 *juifs* | **FP** | intrinsic | 0.965 | 3 | 0.904 | 0.80–0.90 |
| porblt MAT 24:24 *messias* | **FP** | intrinsic | 0.902 | 2 | 0.873 | 0.80 |
| ind DEU 14:12 *rajawali* | **FP** | positional | 0.938 | 1 | 0.938 | 0.80–0.90 |
| nld GEN 6:19 *mannetje* | **FP** | positional | 0.948 | 1 | 0.948 | 0.80–0.90 |
| deu1912 PHM 1:9 *alter* | **FP** | intrinsic | 0.675 | 9 | 0.506 | dead ≥ 0.80 |

The key separations are all clean **at floor 0.95 with k = 32**: the
French-adjective FP *juifs* (0.904), the Portuguese-plural FP *messias* (0.873),
and the German homograph FP *alter* (0.506) die on the intrinsic side; the
list-colon FPs *rajawali* (0.938) and *mannetje* (0.948) die on the positional
side; and **every** tracked TP survives. The load-bearing detail is `k`: the two
min=2 TPs — intrinsic *christ* and positional *deal* — only clear 0.95 at k=32
(*christ* 0.864 → 0.925 → 0.956 at k=8/16/32; *deal* 0.875 → 0.937 → 0.968),
whereas the min=1 FPs are k-flat (rarity 1 at every k). So k=32 lifts the
genuine two-occurrence slips over 0.95 without lifting the single-occurrence
colon FPs — a strictly better separation than k=8 or k=16, which would kill
*christ* and *deal* along with the FPs.

**3 — Near-floor review samples** (major corpora, 0.90 & 0.95 bands): see the
harness `packet 3` output. Highlights the reviewer should sanity-check:
`0.90 band` — Tagalog `!`-forced verbs `bubuksan`/`salakayin` (0.913), Romanian
vocative `доамне` (“Lord!”, both-quadrant 0.911), Portuguese `seol`/`sheol`
(transliteration, 0.910–0.914), Indonesian list-colon `getah`/`piring` (0.909);
`0.95 band` — the merged compound `obed-edom` (tglulb, `-`, 0.963 — the
*post-space* residual class), German `hause` (0.960, the `zu Hause` idiom
lowercased), `almighty`/`gentiles`/`isaac` proper-noun-ish caps. No *new*
false-positive class appears that the hyphen fix created beyond the intended
whole-compound intrinsic findings.

**4 — German/Danish noun storm** — surfaced count by floor at k=32:

| corpus | 0.50 | 0.80 | 0.90 | 0.95 | 0.98 |
| --- | --: | --: | --: | --: | --: |
| dan1931 | 1,290 | 511 | 194 | 52 | 8 |
| deutkw | 896 | 209 | 97 | 43 | 15 |
| deu1912 | 454 | 115 | 40 | 11 | 2 |

Floor **alone** tames the storm by ~1–2 orders of magnitude (dan1931 1,290 → 52
from 0.50 → 0.95), but does not eliminate it — the survivors at 0.95 are
high-confidence lowercased nouns (`land`, `jahr`, `hause`), which are *correct*
findings for German orthography, just high-volume. A per-word or per-language
gate remains the real lever for these; the floor is a blunt instrument here.

### Recommendation (open to the owner)

**Intrinsic floor 0.95, positional floor 0.95, k = 32.**

Rationale: on the tracked anchor set this is the *only* cell that separates
cleanly — every FP (*juifs*, *messias*, *alter*; *rajawali*, *mannetje*) dies
and every TP (*yesu*, *jésus*, *filisteos*, *christ*, *gentiles*; *justify*,
*deal*) lives. Floor does the discrimination (0.95 is the knee that clears the
homograph/adjective/plural FP band at ~0.87–0.95 while leaving the proper-noun
TPs at ≥0.956), and k=32 is what keeps the two genuine two-occurrence slips
(*christ*, *deal*) above that floor — at k=8/16 they fall with the FPs, costing
real recall for no FP-suppression gain, because the FPs at this floor are all
k-flat min=1 sites. The volume is reviewable: ~1.2k intrinsic + ~2.1k positional
+ ~0.2k both ≈ **3.5k sites fleet-wide** (vs 104k at floor 0.5), spread across
~600 corpora with a 9–13% top-5 share, so no single corpus dominates. A symmetric
floor keeps the mental model simple; the data give no reason to split the two
channels' floors. The one thing floor cannot fix is the **German/Danish noun
storm** — even at 0.95 dan1931 still surfaces 52 (correct-but-high-volume
lowercased nouns); that belongs to a per-word/per-language gate, not the floor,
and should be deferred rather than pushed onto the floor value.
