# Rare-glyph spike — raw scalar inventory is not yet a shippable rule

- **Date:** 2026-07-10
- **Status:** SPIKE — candidate extraction and scoring domain are not frozen.
- **Harness:** `cargo run --release -p ssc-core --example calibrate -- --glyphs corpora/vref`
- **Scope:** raw scalar inventory plus visible candidate lanes L/N/P/S. Every
  scalar is tallied for the future census; only candidate lanes enter the
  recurrence sweeps. This is measurement code, not a live rule or `RuleStats`
  schema change.

## What ran

`calibrate --glyphs <file-or-directory>` now reports:

- raw scalar inventory and a per-corpus rare-glyph table;
- L/N/P/S type-count histograms;
- absolute recurrence knees and rate-shaped knees, both at raw rarity >= 0.95;
- lane-split noisiest corpora and review samples;
- a dependency-free base+combining-mark preflight table.

The full 1,504-corpus vref fleet completed in 69 seconds (release build).

## Fleet result

Candidate opportunities: L 1,936,394,870; N 4,320,431; P 89,755,233; S
1,209,875.

| Knee | Total sites | L | N | P | S |
| --- | ---: | ---: | ---: | ---: | ---: |
| Absolute K=32 | 12,202 | 7,836 | 1,816 | 2,229 | 321 |
| Rate 0.25/10k | 9,184 | 7,056 | 878 | 1,065 | 185 |
| Rate 2/10k | 47,217 | 44,870 | 891 | 1,271 | 185 |
| Rate 10/10k | 258,795 | 254,082 | 992 | 3,536 | 185 |

The rate form is materially more permissive than the absolute form, almost
entirely in L. It cannot be considered a replacement knee until the letter
candidate domain has a discriminator stronger than raw scalar frequency.

## Why raw letters fail

The noisiest K=32 corpora are CJK inventories: `cmnfeb` has 757 L sites,
`cmncbt` 751, `cmncbs` 700, `jpn1965` 334, and `kor` 310. These are ordinary
rare Han/Hangul glyphs, not copy errors. The review samples also show normal
capitalized names/headings (`Xerxes`, `Wahebe`, Bible acrostic labels), rare
but genuine diacritics, and literal alphabet examples. A scalar's low count
does not mean its writing system rejects it.

Therefore **do not ship an L lane** from this model. It remains useful census
data and may become eligible only with a measured writing-system discriminator
(for example, a bounded-alphabet gate), not a hardcoded script allow-list.

## Other lanes

- **N:** samples are mostly verse/reference digits, superscript conventions,
  and mixed numeral presentation. Keep this as calibration/census data; do not
  treat number rarity as a body-text typo signal yet.
- **P:** does expose plausible paste/style artifacts (one-off dash forms and
  bracketed fragments), but also legitimate Tibetan signs and bracketed
  editorial text. It needs focused sample adjudication before a punctuation
  lane can be proposed.
- **S:** surfaces likely wreckage (`=`, `>`, stray backticks, emoji) alongside
  legitimate Greek/Coptic and language-specific symbols. It likewise needs a
  narrower ownership decision before it can be scored live.

## Normalization seam

The M exclusion is necessary: the fleet contains heavy decomposed usage,
including `a` + U+0331 (1,590,685 occurrences in 59 corpora) and many Indic,
Malayalam, Myanmar, and Telugu base+mark pairs. The harness intentionally does
not claim equivalence with precomposed forms: the workspace has no declared
Unicode-normalization dependency. Adding one is a separate approval; until
then the preflight table only identifies the affected corpora.

## Next decision

The accumulator is validated for the future census. Before an ADR or live
`uni.rare-glyph` rule, choose a defensible candidate restriction for each lane
and review the lane-specific samples. No scoring constants, default state, or
wire schema are frozen by this spike.

## Round 2 — L-only closure, small knee, lexical concentration

Round two implements the agreed L-only stack in the same `--glyphs` harness:

1. **Alphabet closure:** `hapax letter-token occurrences / all letter-token
   occurrences`, where a letter token is a UAX #29 word made only of letters
   and marks. The report sweeps the self-disable threshold; no script list or
   threshold is frozen.
2. **Small absolute knee:** raw letter count <= 1, 2, 3, 4, or 5. No rate
   knee participates.
3. **Lexical concentration:** discount only where every occurrence is inside
   one case-folded letter-word type and that type occurs at least twice.

The full fleet completed in 130 seconds with compact per-corpus summaries
after each worker discarded its raw inventory.

At a representative 0.5% closure threshold, 207 of 1,504 corpora open their
L lane. Factor kill rates (sites) are:

| Small knee | Base | Closure killed | Lexical killed | Retained |
| --- | ---: | ---: | ---: | ---: |
| <= 1 | 3,710 | 1,924 (51.9%) | 48 (1.3%) | 1,738 |
| <= 2 | 7,836 | 3,764 (48.0%) | 2,264 (28.9%) | 1,808 |
| <= 3 | 11,883 | 5,573 (46.9%) | 4,397 (37.0%) | 1,913 |
| <= 4 | 16,355 | 7,505 (45.9%) | 6,849 (41.9%) | 2,001 |
| <= 5 | 20,480 | 9,120 (44.5%) | 9,249 (45.2%) | 2,111 |

The concentration discount does meaningful work once the knee permits two or
more occurrences; it cannot affect hapaxes under its deliberately strict
repeated-word contract. Closure does the larger first cut, including the
open-inventory populations that made round one unusable. The threshold sweep
remains the decision surface: at 0.1%, only 26 corpora open; at 1.0%, 404;
at 2.0%, 711; at 5.0%, 1,042.

This is encouraging but not a frozen rule: inspect retained samples and choose
the closure threshold before any ADR or production `RuleStats` work. N remains
census-only, and P/S remain pending separate sample adjudication.

## Round 3 — closure is a letter-SCALAR share, not a word share

Round two's closure gate was a bug of intent: it measured *vocabulary* closure
(hapax letter-WORD-token share), so a closed-alphabet but morphologically rich
corpus — Bantu, Sanskrit, Hebrew — looked "open" purely because it mints many
never-repeated word forms, and its L lane was wrongly silenced. Round three
replaces the gate with **alphabet closure**: over all letter (GC L) scalar
occurrences, the share belonging to scalar types seen exactly once
(`hapax L-scalar types / total L-scalar occurrences`). It reads straight off the
glyph inventory the harness already builds; no word walk feeds the gate. The
lexical-concentration discount keeps its unchanged word-type machinery.

The fleet re-swept in 86 seconds (release). Closure thresholds now use finer
low-end steps (0.001%…2%) because scalar closure values are ~100× smaller than
the round-2 word-hapax shares; the absolute knee sweeps ≤1 through ≤8.

### Corpora that open the L lane, by scalar-closure threshold

| Closure ≤ | Corpora open | Corpora closed |
| --- | ---: | ---: |
| 0.001% | 1,412 | 92 |
| 0.01% | 1,496 | 8 |
| 0.05% | 1,503 | 1 |
| 0.1% | 1,503 | 1 |
| 0.2% and up | 1,504 | 0 |

The metric separates writing systems as intended, but the discriminating band
is narrow and low. The eight corpora still closed at **0.01%** are exactly the
Han/Hangul fleet — `cmnfeb` (0.135%), `jpn1965` (0.037%), `cmncbt` (0.033%),
`cmncbs` (0.031%), `cmn-cu89t` (0.029%), `cmn-cu89s` (0.027%), `kor` (0.013%),
plus Blackfoot `bla` (0.011%). CJK closure *is* materially nonzero and sits at
the very top of the ranking (as predicted), but in absolute terms it is still
small because the denominator — total L-scalar occurrences — is enormous
(hundreds of thousands to >1M per corpus). Above ~0.05% even `cmnfeb` opens, so
the gate stops discriminating CJK; below ~0.001% it starts closing genuine
closed-alphabet corpora. **0.01% is the sweet spot** that opens the
Latin/Cyrillic/Ethiopic majority while keeping the five named CJK corpora
(and their siblings) closed.

### Factor kill-rates crossed with the absolute knee (sites)

At the **0.01%** threshold (1,496 corpora open):

| Knee | Base | Closure-killed | Lexical-killed | Retained |
| --- | ---: | ---: | ---: | ---: |
| ≤1 | 3,710 | 1,770 (47.7%) | 277 (7.5%) | 1,663 |
| ≤2 | 7,836 | 4,044 (51.6%) | 935 (11.9%) | 2,857 |
| ≤3 | 11,883 | 6,189 (52.1%) | 1,445 (12.2%) | 4,249 |
| ≤4 | 16,355 | 8,777 (53.7%) | 1,773 (10.8%) | 5,805 |
| ≤5 | 20,480 | 11,302 (55.2%) | 1,973 (9.6%) | 7,205 |
| ≤6 | 25,490 | 14,284 (56.0%) | 2,225 (8.7%) | 8,981 |
| ≤7 | 29,963 | 16,797 (56.1%) | 2,526 (8.4%) | 10,640 |
| ≤8 | 34,947 | 19,733 (56.5%) | 2,878 (8.2%) | 12,336 |

The eight still-closed CJK corpora alone account for ~half of every base row's
sites (round one already showed `cmnfeb` = 757 L sites at K=32), which is why
closure removes 48–56% of sites even while closing only eight corpora. As the
threshold rises the closure cut shrinks (9–11% at 0.05%, 0% at 0.2%) and the
lexical discount becomes the dominant filter (up to ~57% at knee ≤8), because
opening the CJK corpora dumps their storm back into the pool.

### Sanity checks

- **Flip-open corpora (the headline win).** 1,297 corpora were *closed* under
  round two's word-hapax gate (>0.5%) yet *open* under round-three scalar
  closure (≤0.1%) — precisely the agglutinative / inflected class the round-2
  bug silenced. Top flips: `WA-kan-x-koungaru-reg` (word-hapax 32.2% →
  scalar 0.0005%), `arp` (Arapaho, 29.3% → 0.0023%), `eskNT` (Inuktitut,
  28.4% → 0.0001%), the entire `san*` Sanskrit fleet (~25% → ~0.0002%),
  `hbo`/`hebwlc`/`hboWLC` (Hebrew, ~25.6% → ~0.0000%), and a long tail of
  `WA-*-reg` Bantu corpora. These are corpora whose alphabets are closed but
  whose word inventories are open; the scalar gate now sees them correctly.

- **Retained-set knee-insensitivity does NOT hold in round 3.** Round two's
  "flatness" (1,738 → 2,111 across knees 1–5) was an artefact of almost nothing
  being open (only 207 corpora at its 0.5% word threshold). With most lanes now
  open, retained grows roughly linearly with the knee at every closure
  threshold (e.g. 1,663 → 12,336 across knees 1–8 at 0.01%). The knee is a real
  volume lever now, not a plateau — it must be chosen on signal quality, not on
  an assumed flat region. The ≤1–5 range is not settled.

- **Round-2's knee-≤1 lexical kills — mechanism confirmed.** They are
  case-folding merges: a hapax-cased letter scalar (count == 1) whose one
  occurrence sits inside a word that case-folds to a *repeated* type. Almost all
  are uppercase glyphs folding into a repeated lowercase word — e.g.
  `WA-es-419-ulb` Ü (count 1) → `vergüenza` (108 tokens), `WA-fr-ulb` Ç →
  `ça` (12), `WA-ihi-reg` Q → `que` (29), `WA-bil-reg` C → `centurion` (3). A
  handful are the lowercase partner (a `p`/`P` split where one case is the
  hapax). The discount fires exactly as designed. Round three surfaces more of
  them (277 at knee ≤1) only because it opens far more corpora than round two
  did; the mechanism is identical.

- **Retained-sample quality is a genuine mix.** The retained review table
  (30 of 4,218 leads at closure ≤0.1%, knee ≤3, non-lexical) shows two
  populations. **Strong signal:** Latin letters embedded in non-Latin scripts —
  `F`/`a` inside Ethiopic (`WA-am-ulb`: `ያቃጥላFቸው`), `l`/`p` inside Bengali/
  Assamese (`WA-as-ulb`), `o` inside Telugu (`WA-bgd-x-pawari-reg`:
  `ఆనందoతో`), `c`/`i` inside Arabic (`WA-ayn-ulb`) — these are clear
  script-intrusion typos. **Name/loan noise:** `Q`/`q` in transliterated names
  (`Quirinius`, `Aquila`), `J` in `Jesus`, `x` in `Alexander` — real but not
  errors, and not lexical-discounted because each name form is itself a hapax
  word (word-type count 1, so the repeated-word contract never triggers).
  Adjudication is warranted: the intrusion class is exactly the Hawaiian-`q`
  story the rule is for; the name class argues either for a proper-name carve-out
  or for accepting that single-occurrence loan names are a residual.

### Round-3 verdict

Letter-scalar closure is the correct gate shape and fixes the round-2
over-silencing (1,297 corpora recovered). But nothing is frozen: the
discriminating threshold band is narrow (~0.01%), the knee is now a live volume
lever rather than a plateau, and the retained set still mixes true
script-intrusion signal with single-occurrence proper-name noise. Choose the
threshold/knee from this surface, and decide the proper-name residual, before an
ADR or `RuleStats` work. N stays census-only; P/S still await separate
adjudication.

## Round 4 — proper-noun-shape discount for the hapax-name residual

Round 3 named a residual the lexical-concentration discount structurally cannot
reach: a rare glyph whose sole containing word type is itself a **hapax**
(occurs once). `Q` in `Quirinius`, `J` in `Jesus`, `x` in `Alexander` — the
containing name is unique, so the concentration contract (which requires the
type to recur ≥2) never fires, and the glyph survives into the retained set as
name noise rather than a typo. Round 4 adds a fourth, measurement-only kill
column that targets exactly this class.

### The branch

The discount applies to a rare letter glyph when **all** hold:

1. its sole containing word type is a hapax (one word type carries every
   occurrence, and that type's token count is 1);
2. that lone occurrence is **capital-initial** (uppercase first letter, using
   the casing machinery's own test); and
3. the occurrence is at a **non-forced** position — mid-flow, reusing the casing
   walk's forced definition: book-initial or a word that consumed a bare
   attached terminal is forced; **verse-initial is not** (`CLAUDE.md`).

A capital at a forced position is capitalised for position reasons — its shape
says nothing — so no discount there (conservative: the flag survives).
Bicameral-only falls out for free: caseless scripts have no uppercase first
letter, so condition 2 is never met and the branch never fires for them. The
harness replicates the casing pending-terminal machine (`glyph_advance_gap` +
book-ordered `letter_word_shapes`), carrying pending state across verse seams
and resetting per book, keyed by the same lowercase letter-token key the lexical
machinery already uses. This is spike measurement code; no production rule,
`RuleStats`, or wire change.

### Kill table at the representative 0.01% closure threshold

Sites, four factors stacked (closure → lexical concentration → proper-noun
shape → retained), across the absolute knee ≤1…≤8:

| Knee | Base | Closure | Lexical | Proper-noun | Retained |
| --- | ---: | ---: | ---: | ---: | ---: |
| ≤1 | 3,710 | 1,770 (47.7%) | 277 (7.5%) | **396 (10.7%)** | 1,267 |
| ≤2 | 7,836 | 4,044 (51.6%) | 935 (11.9%) | **400 (5.1%)** | 2,457 |
| ≤3 | 11,883 | 6,189 (52.1%) | 1,445 (12.2%) | **400 (3.4%)** | 3,849 |
| ≤4 | 16,355 | 8,777 (53.7%) | 1,773 (10.8%) | **400 (2.4%)** | 5,405 |
| ≤5 | 20,480 | 11,302 (55.2%) | 1,973 (9.6%) | **400 (2.0%)** | 6,805 |
| ≤6 | 25,490 | 14,284 (56.0%) | 2,225 (8.7%) | **400 (1.6%)** | 8,581 |
| ≤7 | 29,963 | 16,797 (56.1%) | 2,526 (8.4%) | **400 (1.3%)** | 10,240 |
| ≤8 | 34,947 | 19,733 (56.5%) | 2,878 (8.2%) | **400 (1.1%)** | 12,336 |

The proper-noun kill is a **fixed ~400-site residual, knee-insensitive** (396 at
≤1, 400 from ≤2 on) — unlike closure, lexical, and retained, which all grow with
the knee. That is the signature of the class it targets: a capital-initial hapax
name contributes its rare glyph essentially once, so raising the knee adds
nothing. It cleanly excises the round-3 residual — retained at ≤3 drops from
4,249 (round 3) to 3,849, the whole difference being these 400 name sites.

### Sample (a) — sites the discount kills (adjudication)

20 diverse leads (closure ≤0.1%, knee ≤3). The overwhelming majority are the
intended Quirinius class — genuine names carrying a locally rare glyph:

- `J` → `Jesus` (WA-auh LUK 9:47), `Q` → `Quiriniusi`/`Qwirini` (WA-bem,
  WA-auh LUK 2:2), `x` → `Alexandre` (WA-bbm), `q` → `Aqila` (WA-ekp),
  `R` → `Roma` (WA-bnx), `è` → `Kulène` (Cyrene, WA-bnx), `Ṣ` → `Ṣur` (Tyre,
  WA-bil), `X` → `Xatche` (WA-bwg), `à` → `Alàtalla` (a divine name; the rare
  glyph is mid-name, the container's titlecase shape carries the discount),
  `È`/`V` → `Ève`/`EVE` (Eve, WA-fr, WA-ekp), `h` → `Ruth` (WA-ach).

These are exactly right to discount.

**Wrongly-eaten risk (real, small, one shape).** Three of the twenty are *not*
names — they are stray glyphs that trivially satisfy "capital-initial" because
the whole token is one capital or all-caps, not because it is a titlecase name:

- **Lone single-capital tokens.** `Q` alone as a one-letter word at verse start
  (WA-dje MAT 11:4: `Q Yesu tuu ga …`) and a stray Latin `I` alone at the end of
  a Devanagari verse (WA-dso ACT 1:13: `… रेलाए I`). A one-letter uppercase token
  is capital-initial and, standing alone once, a hapax — so the shape branch eats
  it, but a lone stray capital is far more likely an artifact/script-intrusion
  than a name.
- **All-caps common word with a stray diacritic.** Spanish `Ö` in `YÖ`
  (WA-es-419 ZEC 3:4) — this is the pronoun `YO` ("I") with a wrong umlaut, an
  all-caps *common* word, not a name; the discount wrongly eats a genuine typo.

All three share one property the genuine names do not: the container is a
single-letter or fully-uppercase token, never a titlecase (initial-upper,
rest-lower) name shape. **Recommendation for round 5:** tighten condition 2 from
"capital-initial" to "titlecase shape" (uppercase first letter *and* at least
one following lowercase letter). Every genuine name in the sample is titlecase
and would still be discounted; the three risky sites (and all-caps forms like
`EVE`/`DOVE`) would fall back to retained — the safe direction, since retained
means "still flagged". The cost is a handful of legitimate all-caps names
staying flagged, which is the conservative failure mode.

### Sample (b) — survivors after all four factors

30 retained leads (closure ≤0.1%, knee ≤3, non-lexical, non-proper-noun). As
predicted, this set is dominated by **lowercase script-intrusion typos** — the
signal the rule exists for — which the shape discount correctly does not touch:

- Latin `F`/`a` inside Amharic (WA-am-ulb: `ያቃጥላFቸው`, `ሠaርዊታቸውን`), `l`/`p`
  inside Assamese (WA-as-ulb: `উত্তৰl`, `স্বমেহনp`), `o` inside Telugu
  (WA-bgd-x-pawari: `ఆనందoతో`), `c`/`i` inside Arabic (WA-ayn-ulb).

These are lowercase, so condition 2 fails and they stay flagged — exactly right.
The set also contains capitals the guards *correctly* spared: `QMunu` after a
colon (WA-bez 2CO 9:6) is at a forced position (the colon is a bare terminal),
so no shape discount — and it reads like a real `Q`-stuck-to-`Munu` slip that
should stay flagged; `x` in `Alexander` (WA-atg, WA-bem) is retained because `x`
appears across more than one word type, so it is not a sole-container hapax.

### Round-4 verdict

The proper-noun-shape discount does the job round 3 scoped for it: a clean,
knee-insensitive ~400-site excision of the hapax-name residual, leaving the
lowercase script-intrusion signal untouched. It is safe enough to consider for
the eventual rule, but **not frozen**: the "capital-initial" test leaks on
single-capital and all-caps tokens (lone `Q`/`I`, Spanish `YÖ`), wrongly eating
a small number of real typos. Adopt the titlecase-shape tightening (round 5)
before this factor enters an ADR or `RuleStats`. Closure threshold, knee, and
the other three factors remain unfrozen; N stays census-only; P/S still await
separate adjudication.
