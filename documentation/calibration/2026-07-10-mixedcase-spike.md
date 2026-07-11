# Calibration SPIKE: mixed-case word (`wOrd` internal capitals)

- **Date:** 2026-07-10.
- **Status:** SPIKE — exploratory. **Nothing frozen.** No production rule,
  `RuleStats`, `CasingConfig`, or `signals/casing.rs` was touched (all read-only,
  per the task). The harness is example-scoped, additive, and self-contained; the
  Wilson bound is the harness-local `sig_wilson_lb` copy the `--signatures` spike
  introduced, and the sweep constants (`PACKET_KS = {8,16,32}`, `PACKET_FLOORS =
  {0.80,0.90,0.95,0.98}`, `REF_K = 32`, `REF_FLOOR = 0.95`) and `rarity_abs` are
  shared with the casing spike so numbers are comparable.
- **Claim under test (plan rule 3):** a word written in a **mixed-case shape**
  (`wOrd` — internal capitals, neither lowercase, Titlecase, nor ALLCAPS) is a
  slip *unless it is a convention*, and the recurrence machinery must excuse the
  conventions (McX name shapes, `LORD`-adjacent forms, intra-word class-prefix /
  clitic orthographies) **without a hardcoded list**.
- **Harness:**
  - per-corpus — `cargo run --release -p ssc-core --example calibrate -- --mixedcase corpora/vref/<id>.txt`
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --mixedcase corpora/vref`
    (1,504 corpora, 359.2M cased tokens; ~22 s on 8 cores).
- **Build note (in-flight tree):** the concurrent rare-glyph production rule
  leaves `ssc-core` (lib) non-compiling mid-session (`RuleStats::GlyphInventory`,
  `RuleSites::RareGlyph`, `RuleId::RareGlyph` half-wired). Per the task, the fleet
  numbers below were produced in a throwaway git worktree checked out at the last
  clean commit (`ac44183`) with this working-tree `calibrate.rs` copied in and the
  run pointed at the (untracked, local) `corpora/vref`. The six synthetic
  extraction tests (`cargo test -p ssc-core --example calibrate mixedcase`) pass
  there; re-run them once the lib compiles again.

## What "mixed" means here (the extraction, pinned by synthetic tests)

- **Token unit** = the plain UAX #29 letter-run word (`tokenize` + the existing
  `is_letter_token` filter). This deliberately does **not** hyphen-merge like
  `casing::compound_words`, so `Obed-Edom` is two Titlecase tokens, never one
  mixed one — matching the plan's "letter-run tokens" unit for rule 3. (Cost:
  apostrophe-medial words like `ng'ombe` are skipped by `is_letter_token`; they
  are almost never mixed-shape, an acceptable scoping choice.)
- **Shape** is read off the sequence of *cased* letters only (marks and caseless
  letters skipped, so an intra-word caseless glyph cannot manufacture a shape):
  `Lower` (all lower), `AllCaps` (all upper — incl. a lone `I`/`A`), `Title`
  (first upper, all the rest lower), else `OtherMixed`. **A single cased letter is
  never OtherMixed** (single-letter guard). `OtherMixed` therefore *always*
  carries an internal capital — that is exactly the `wOrd` phenomenon.
- Consequence for `LORD`: pure `LORD` is **AllCaps**, never a candidate. Only its
  inflections (`LORDs`, `GOD's`, Hebrew `HaMelech`, Indonesian `TUHANlah`) are
  OtherMixed. So the "LORD-adjacent" convention is precisely the inflected /
  clitic-attached all-caps name.

Tests pinned: `plain_shapes`, `single_letter_is_never_mixed`,
`caseless_has_no_shape`, `convention_shapes_are_othermixed` (McDonald, kiSwahili,
iPhone, LORDs OtherMixed; LORD AllCaps), `combining_marks_and_caseless_do_not_
manufacture_mixing`, `first_cased_axis`.

## The two evidence routes (as measured)

For every OtherMixed occurrence:

- **Route A — within-word** (the ADR 0051 two-factor, one more shape lane):
  `score = dominance(word's not-other-mixed share) × rarity_abs(other-mixed count, k)`.
  A word that is dominantly some clean shape with a stray mixed occurrence scores
  high; a word that is *dominantly* OtherMixed (a convention like `HaElohim`) has
  `dominance ≈ 0` and is silent. **A hapax mixed word is structurally silent
  here** (not_other = 0 ⇒ dominance 0) — this is the gap route B is meant to fill.
- **Route B — corpus fallback for hapax words:**
  `score = dominance(corpus-wide not-other-mixed share) × rarity_abs(exact-form count = 1, k)`.
  Since a hapax's count is 1, `rarity = 1` at every `k`, so **route B is
  knee-independent** and gated only by the corpus dominance floor.

Both are reported as **separate columns** so they can be adjudicated
independently.

## Headline numbers (fleet, 1,504 corpora)

Mixed-case is **rare**: 684,645 OtherMixed tokens out of 359.2M cased
(**0.19%**). Seven of the clean major Latin corpora — `eng-web`, `eng-kjv`,
`engwebster`, `deu1912`, `spaRV1909`, `nld`, `porblt` — have **zero** OtherMixed
tokens. Where it appears it is either a real interior-cap slip or convention/
run-on noise.

| metric | value |
| --- | --: |
| cased letter-run tokens | 359,188,750 |
| OtherMixed tokens | 684,645 (0.191%) |
| **Route A** surfaced @ ref (k=32, floor 0.95) | **950** across 540 corpora |
| **Route B** surfaced @ ref (hapax-fallback) | **15,439** |

### Route A volume sweep (surfaced OtherMixed sites)

| floor \ k | 8 | 16 | 32 |
| --: | --: | --: | --: |
| 0.80 | 1,844 | 2,256 | 2,775 |
| 0.90 | 1,088 | 1,370 | 1,614 |
| 0.95 | 756 | 756 | 950 |
| 0.98 | 479 | 479 | 479 |

### Route B volume sweep (knee-independent — gated only by corpus-dominance floor)

| floor \ k | 8 | 16 | 32 |
| --: | --: | --: | --: |
| 0.80 | 24,115 | 24,115 | 24,115 |
| 0.90 | 22,403 | 22,403 | 22,403 |
| 0.95 | 15,439 | 15,439 | 15,439 |
| 0.98 | 11,009 | 11,009 | 11,009 |

The `k` columns for route B are identical by construction (hapax rarity ≡ 1).

## Route 4 — position does NOT matter (assumption verified, not imported)

Unlike initial-case, a mid-word capital is position-independent, and the fleet
confirms it: the OtherMixed rate is essentially flat across the sentence seam.

| position | OtherMixed rate | count |
| --- | --: | --: |
| forced (book-initial / after bare terminal) | 0.1848% | 111,425 / 60,287,596 |
| mid-flow | 0.1918% | 573,220 / 298,901,154 |
| **ratio forced / mid** | **0.964** | — |

**Conclusion:** do **not** import casing's forced-position/censoring machinery
into the mixed-case rule. The forced/mid split is measured here only to prove it
is irrelevant (ratio ≈ 1). This is the opposite of ADR 0051's initial-case
finding, where position is the whole story.

## Histogram — a giant ≈0 spike plus a thin, near-flat tail

Route-A score over all 684,645 OtherMixed sites at k=32:

- **[0.000, 0.025): 656,802 sites — 95.9%.** The recurrence collapse: every
  convention (dominantly-OtherMixed word) and every hapax (dominance 0) lands
  here. This is the analog of spacing's ≈0 mass and it is huge.
- A **thin, roughly flat tail** from 0.025 to 1.0 (~300–1,400 sites per
  0.025-bucket), with a slight uptick in the top bucket ([0.975, 1.0) = 552).

So mixed-case is **closer to spacing's bimodal shape than to casing's fat mid**:
the recurrence factor does almost all the work, and the surviving tail is thin.
The floor is still a real dial *within* that thin tail (route A 756→950 across
0.95, 479 at 0.98), but there is no fat homograph mid-mass to fight — because
"internal capital" has no graded-ambiguity analog the way German noun-casing does.

## Noisiest corpora and the storm shape

| corpus | route-A | route-B | other% | corpusDom | hapax types |
| --- | --: | --: | --: | --: | --: |
| yer | 9 | 852 | 4.85% | 0.9507 | 852 |
| WA-ss-reg | 6 | 848 | 4.04% | 0.9584 | 848 |
| WA-ndc-x-chiducua-reg | 1 | 802 | 3.69% | 0.9620 | 802 |
| kyq | 0 | 800 | 3.52% | 0.9641 | 800 |
| engojb (Orthodox Jewish Bible) | 0 | 224 | 0.97% | 0.9901 | 224 |
| ron1924 (Romanian Cyrillic) | 0 | 71 | 0.017% | 0.9998 | 71 |

**Every storm is a route-B storm.** The noisy corpora are convention-rich
orthographies: a lowercase class-prefix / clitic fused to a Capitalized name
(Bantu concord `baYuda`, `naTata`, `waYahathi`; Hebrew construct `HaElohim`,
`HaMelech`; Romanian Cyrillic `ТатэлМеу`, `КэчТатэл`; Indonesian `TUHANlah`). In
these corpora OtherMixed is still a small *fraction* of all cased tokens (so
`corpusDom` sits at 0.95–0.99, above the floor), yet the mixed shape is
**productive morphology, not error**. Route A correctly leaves the hapax members
silent; route B surfaces them because corpus dominance is undiscriminating.

## Convention adjudication — what recurrence excuses vs what needs more

Genuine review samples (major corpora, ref cell). `Up1`/`lo1` = first cased
letter upper/lower; `FORCED`/`mid` = position.

**Route A flagged — real interior-cap slips (high quality):**

```
WA-es-419-ulb PSA 140:6  [DIos]  Up1 mid  dom0.999 other1 wtot4606 score0.999   (DIOS/Dios slip)
WA-es-419-ulb EXO 14:20  [asÍ]   lo1 FRC  dom0.998 other1 wtot2539 score0.998   (así — interior Í)
WA-sw-ulb     ECC 11:9   [MUngu] Up1 mid  dom0.999 other1 wtot4424 score0.999   (Mungu)
WA-fr-ulb     JHN 15:21  [ILs]   Up1 FRC  dom0.996 other1 wtot1408 score0.996   (Ils)
swhulb        EXO 6:29   [MIsri] Up1 mid  dom0.991 other1 wtot613  score0.991   (Misri)
tglulb        REV 15:8   [kanIyang] lo1 mid dom0.999 other1 wtot7309 score0.999 (kaniyang)
vie1934       ACT 17:23  [CHúA]/[THù] Up1 dom0.99+ (all-caps name w/ un-shifted diacritic)
```

**Excused by route-A recurrence (the conventions, no hardcoding):** any mixed
form that recurs ≥2× with its shape collapses `dominance → ~0` and goes silent —
`HaElohim ×419`, `HaMelech ×1130`, `TUHANlah ×22`, Spanish `queEL ×3`,
`FIlisteos ×3`, Bantu `yaYahweh ×2`, French run-ons `fitDavid ×2`/`queMoïse ×2`,
Romanian `ТатэлМеу ×4`/`КэчТатэл ×2`. **This is the rule working as designed:
recurrence excuses the convention with no name list.**

**Needs something else (route-B leaks) — two classes:**

1. **Missing-space run-ons** — `deJésus`, `porJonatán`, `maisDieu`, `saJuda`,
   `niJesus`, `deRoma`, `nuestroDIOS`. These are *real defects* but a spacing /
   word-boundary phenomenon (a dropped space that manifests as a mixed shape),
   not "this word is miscased." One-phenomenon-one-finding argues these belong to
   the attachment-signatures / spacing lane, not mixed-case.
2. **Productive-morphology hapaxes** in convention-rich corpora — engojb
   `HaMaarechet ×1`, `HaShaloshim ×1`; Bantu `waYahathi ×1`, `naHadadezeri ×1`;
   Romanian `КэчИоан ×1`. These recur *as a class* (the `Ha-`/`wa-`/`Кэч-`
   prefix) but not as an exact form, so route A (correctly) can't reach them and
   leaves them silent — but route B surfaces them as false positives.

## The hapax-fallback verdict — route B does NOT earn its volume

Route B surfaces **16× more** than route A (15,439 vs 950), and inspection shows
that volume is **almost entirely** the two leak classes above — missing-space
run-ons and productive-morphology hapaxes — not genuine one-off miscasings. The
root cause is the same multinomial-dominance-is-1 problem the rare-glyph spike
hit: the corpus-wide "not-other-mixed" share is ≈0.95–1.0 for *every* corpus,
including the ones with a live mixed-case convention (engojb 0.9901,
ron1924 0.9998), so it supplies almost no discrimination and simply passes
through every hapax above the floor.

Crucially, **route A already does the safe thing for hapaxes: it stays silent.**
In the clean Latin corpora where a hapax `wOrd` would be a genuine catch, there
are essentially no hapax OtherMixed tokens to begin with (`eng-*`, `deu1912`,
`spaRV1909`, `nld`, `porblt` = 0 OtherMixed total). So the fallback buys almost
no true positives while importing a large false-positive storm.

**Recommendation for the rule design: ship route A only; hapaxes stay silent.**
If a hapax route is ever wanted, its corpus-level factor must be *shape-class
recurrence-aware* (does this corpus routinely produce OtherMixed forms of this
prefix/clitic class?), not a flat not-other-mixed dominance — the flat factor is
proven here to be non-discriminating.

## Interplay with the rare-glyph name-shape logic (titlecase consistency)

The round-5 rare-glyph discount defines **titlecase** as "uppercase first + ≥1
following lowercase" (`letter_word_shapes` in the same harness). This spike's
`Title` is stricter: "uppercase first **and all the rest lowercase**." The gap is
exactly the OtherMixed set: `McDonald`, `HaMelech`, `FIls` are *titlecase* to
rare-glyph (first-upper, has-a-lower) but **OtherMixed** here. That is the correct
division of labour — rare-glyph only needs "is this a name-shaped container?" to
excuse a rare *glyph*, whereas mixed-case needs the finer "is the interior
irregular?" — but the two definitions must be documented as intentionally
different, not accidentally divergent, when both rules ship. Recommend a shared
`case_shape` helper at rule time with rare-glyph consuming `shape != Lower &&
first_upper` and mixed-case consuming `shape == OtherMixed`.

## Boundary predicate vs casing v2 (one phenomenon, one finding)

Split of all OtherMixed occurrences by first cased letter:

- **first-upper: 81,037** (`McDonald`, `LORDs`, `DIos`, `FIls`) — casing is
  **blind** to these: `case.sentence-initial-lowercase` and
  `case.inconsistent-word-casing` both fire only on a lowercase word-start. These
  are unambiguously mixed-case's own. Of the 950 ref-flagged route-A sites,
  **657 are first-upper** — the bulk of the clean signal is casing-invisible.
- **first-lower: 603,608** (`wOrd`, `kiSwahili`, `asÍ`, `ajilI`) — these overlap
  casing's lowercase-site domain. At a *forced* position casing's
  sentence-initial-lowercase would fire on the initial lowercase, but the actual
  defect is the *interior* capital (`asÍ` after a terminal: the `a` is fine, the
  `Í` is the slip). 430 of the 950 ref-flagged route-A sites are forced.

**Proposed boundary predicate:** the OtherMixed shape is mixed-case's phenomenon
in full; casing should **skip OtherMixed tokens** in its lowercase-site rules so a
first-lower mixed word is reported once (interior-capital finding) rather than
twice (spurious sentence-initial-lowercase on the incidental initial). This is a
casing-side carve-out to settle at ADR time; the spike only sizes it (≤430
forced first-lower ref sites fleet-wide).

## What this spike answers for the rule design

1. **Shape extraction is clean and cheap.** OtherMixed = has-both-cases and not
   Title/AllCaps ⇒ always an interior capital; single-letter and caseless guards
   pinned by tests. Token unit is the plain letter-run (no hyphen merge).
2. **Route A (within-word) is the rule.** ~950 sites @ ref across 540 corpora,
   high-quality real slips; recurrence excuses every convention (Bantu concord,
   Hebrew construct, `TUHANlah`, run-ons that recur) with **no hardcoded list**.
3. **Route B (hapax corpus-fallback) does not earn its volume** — 16× the volume,
   almost entirely missing-space run-ons and productive-morphology hapaxes;
   corpus not-other-mixed dominance is non-discriminating (≈1 everywhere).
   Recommend **hapaxes stay silent**.
4. **Position is irrelevant** (forced/mid ratio 0.964): do not import the
   censoring machinery; a mid-word capital is position-independent.
5. **Histogram is spacing-like** (95.9% at ≈0 + a thin flat tail), so the floor
   is a modest dial, not a load-bearing discriminator; no fat homograph mid-mass.
6. **Absolute knee** (as ADR 0051): the surviving TPs are min=1 hapax-shape slips
   at rarity 1; `k` only moves the mid-tail. No rate knee needed (word-scale
   denominators).
7. **Boundary:** first-upper OtherMixed is casing-invisible (mixed-case's alone);
   first-lower OtherMixed overlaps casing — settle a casing-side skip of
   OtherMixed tokens so the interior-capital phenomenon is reported once.
8. **Titlecase definition** must be a shared, documented helper with rare-glyph
   (their `Title` is looser on purpose).

Nothing is frozen. The knee, floor, the route-B decision, the casing boundary
carve-out, and whether missing-space run-ons are re-routed to the spacing lane
are all open for the rule-design decision / ADR.
