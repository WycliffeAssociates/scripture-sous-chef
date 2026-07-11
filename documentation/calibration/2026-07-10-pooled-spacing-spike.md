# Calibration SPIKE — pooled class-conditioned spacing (plan rule 2, Design A vs B)

- **Date:** 2026-07-10
- **Status:** SPIKE — measurement only. **Nothing ships; no knobs frozen.** No
  production code touched — both designs are re-derived from public APIs in the
  calibration harness (`crates/core/examples/calibrate.rs`, new `--pooled-spacing`
  mode; harness-local `pool_*` / `PClass` / `BCat` / `Pool*`; unit tests in
  `mod pooled_tests`). The shipped `punct.spacing-anomaly` (ADR 0054 per-side
  rule) is run unchanged for the regression baseline.
- **Scope:** the user's class-conditioned pooled model — "the typist chooses the
  SPACE, not the neighbour; condition on content, judge the choice" — measured
  **head-to-head** against a rival immediate-context multinomial, over the same
  sites, samples, and reference constants:
  - **Design A (class-conditioned binary).** Per `(mark, side, class)` a binary
    *attached*-vs-*spaced*, where the class is the fused-Class of the **first
    non-whitespace neighbour** on that side {Letter, Number, Punct} — crossing
    verse (and book) seams to reach the next/prev verse's edge grapheme
    (book-ordered), the seam reading as an **ordinary spaced observation** (no
    forcedness, repo CLAUDE.md). Quote is **merged into Punct** in the model; a
    quote/non-quote sub-split rides inside Punct **as data only**. A site is
    judged by its most specific pool that holds a Wilson-dominant convention
    (class pool → top-level all-class fallback); Wilson self-gates thin pools.
  - **Design B (immediate four-way category).** Each side reads its **immediate**
    context {letter, number, ws, punct} — whitespace is terminal, never looked
    past. Verdict per `(mark, side)`: mode-dominance (Wilson lower bound of the
    modal category's share) × recurrence on the observed category's count; flag
    non-modal occurrences above floor.
- **Reference constants** (the shipped ADR 0050/0054 family, both designs, all
  pools): z **1.96**, volume-scaled knee **k=32 + 40/10k** on the pool's own N,
  floor **0.5** — the production `side_verdict` shape.
- **Harness:**
  - per-corpus — `cargo run --release -p ssc-core --example calibrate -- --pooled-spacing corpora/vref/<id>.txt`
  - fleet — `cargo run --release -p ssc-core --example calibrate -- --pooled-spacing corpora/vref`
    (1,504 corpora, **139.0M** side-observations; **~20 s analyze** on 8 cores).
- **Candidate domain:** GC `Po` minus quotes (ADR 0033), lone scalars only — as
  shipped. Plus a **separately-reported Pd lane** (dashes; a pragmatic dash set,
  the fused table carries no `Pd` bit). Pd is domain-widening evidence, **not** a
  decision this spike makes.

---

## 0. Headline

- **Design A wins decisively on every criterion.** At the reference constants
  the fleet lands: **shipped 9,644 → Design A 27,772 (+18,128) → Design B 95,232
  (+85,588).** Design A is the same order of magnitude as shipped plus the
  genuine new coverage the conditioning buys; Design B is ~10× shipped and ~3.4×
  Design A, dominated by content-rarity false positives (§4, §5).
- **Design A reproduces every shipped win.** Six-corpus regression:
  A-operational keeps **100%** on all six (engwebster 4/4, kmr-IQ 20/20, udu
  0/0, ne_udb 76/76, pa_ulb 25/25, mya 15/15). The Letter pool alone reproduces
  all but **one** site (mya EZK 48:30), for a legible reason: that side's class
  is **Punct** (a `။` before another mark), not Letter, so it is judged by the
  punct/top route, not the Letter pool (§3).
- **The make-or-break answer is yes — conditioned granularity is real coverage,
  not silent theory** (§1). The **Number pool** reaches a Wilson-dominant
  convention in **887/1,504** corpora and actually flags ≥1 site in **334**; the
  **Quote sub-pool** is dominant in **1,352/1,504** and flags in **1,037**.
- **Both design-B predictions confirmed with real samples** (§5): (a) Design B
  **cannot** judge spaced-side-vs-content (`Sam 118: 26` vs `verse. 26` are both
  just "ws" to B) — Design A flags the mis-spaced numeric colon, B is blind; (b)
  Design B **over-flags rare-content attachments** (`144,000`'s comma, `666`'s
  period) as non-modal categories where Design A's thin/majority pool stays
  silent. B's flagged sides are **33,791 number** + **66,508 punct** — almost all
  legitimate content, not misplacement.
- **The quote sub-split is mark-dependent** (§1): for the period, quote-adjacent
  and other-punct **diverge** (`."` attaches 77% vs other-punct spaces 71%) —
  real evidence for splitting quotes out later; for `,` and `:` they **track**
  (both ~88–98% spaced). Merged stays for now; the divergence is logged.
- **The histogram is the familiar shape:** one colossal silent spike
  (Design A: 138,942,475 of 139,015,010 side-obs in `[0, 0.025)`, 99.95%) plus a
  thin, near-flat anomaly tail. Design B's tail is visibly fatter (only 99.81%
  silent). Floor/knee is a pure sensitivity dial, as everywhere in this stack.

---

## 1. Per-pool volume census + the make-or-break question

Fleet-summed Design-A pools (raw counts mix conventions — a shape check; `*` =
that pool holds a Wilson-dominant convention at the floor). Selected major marks:

| mark | L.letter | R.letter | R.number | R.punct | R.punct **quote / other** sub-split |
| --- | --- | --- | --- | --- | --- |
| `.` | attached 100% | spaced 100% | **spaced 77%** | attached 70% | **quote attached 77% / other spaced 71%** ⟵ diverge |
| `,` | attached 100% | spaced 100% | **attached 69%** | spaced 88% | quote spaced 88% / other spaced 90% (track) |
| `:` | attached 98% | spaced 89% | **attached 94%** | spaced 97% | quote spaced 98% / other spaced 96% (track) |
| `?` | attached 98% | spaced 99% | spaced 98% | attached 80% | quote attached 95% / other spaced 72% (diverge) |
| `।` | attached 76% | spaced 100% | spaced 99% | attached 82% | quote attached 88% / other spaced 85% (diverge) |
| `¿` | spaced 100% | attached 100% | — | attached 100% | quote attached 93% / other spaced 76% (diverge) |

Two conventions genuinely coexist for the content pools: **`.`/`:` before a
number split** (`.` 77% spaced = cross-references `verse. 3`, 23% attached =
decimals `7.8`; `:` 94% attached = chapter:verse `1:1`), and the pool learns
which per corpus. That split is exactly the signal Design B cannot see (§5a).

### MAKE-OR-BREAK — is conditioned granularity real coverage?

> For a pool to earn its slot it must both **hold a convention** in enough
> corpora and **actually flag** in some.

| conditioned pool | corpora Wilson-dominant | of which flag ≥1 site |
| --- | ---: | ---: |
| **Number** (per mark, either side) | **887 / 1,504** | **334** |
| **Quote sub-pool** (inside Punct) | **1,352 / 1,504** | **1,037** |

**Answer: real coverage, not silent theory.** The Quote convention is nearly
universal and flags in two-thirds of the fleet; the Number convention is more
selective (decimals/refs are corpus-dependent) but is dominant in 59% of corpora
and flags in 334. Both clear the bar. Book-edge seam-crosses that found no
neighbour: **43,138 sides of 139.0M — 0.03%, negligible**, as predicted.

---

## 2. What the pooled model newly flags vs the shipped rule (ref cell)

Counts: shipped **9,644**, Design A **27,772**, Design B **95,232**. Design A's
+18,128 is the sum of the new pools the shipped rule structurally could not see.
All samples below are single occurrences against a strong pool majority (score
≈ 1.0), with verse context; `count/N` is the pool's descriptive share.

**Digit pools (`7. 800` / decimals / refs)** — the shipped rule abstained on
digit neighbours entirely:

| corpus | sid | mark | pool / form | context |
| --- | --- | --- | --- | --- |
| yby | MAT 23:39 | `:` | number/**spaced** | `Sam 118: 26` (a chapter:verse ref mis-spaced) |
| hmo | 2CH 31:5 | `,` | number/**spaced** | `karoa 10 , vadaeni` (spaced thousands comma) |
| deuelbbk | JDG 20:46 | `.` | number/**spaced** | `tapfere Männer.` (verse-final before a digit next verse) |
| priNT | 2CO 6:18 | `,` | top/attached | `2 Samuel 7.8,14` (attached ref comma, N=23,165) |

**Quote-adjacent (`word ."` vs `word."`)** — deferred by shipped (quote side
abstains):

| corpus | sid | mark | pool / form | context |
| --- | --- | --- | --- | --- |
| frasbl | JHN 4:39 | `:` | punct(quote)/attached | `témoigne :"` (spaced-then-quote is the corpus minority) |
| gax | 1SA 17:37 | `,` | punct(quote)/attached | `jiraatuu ti,»` |
| engbsb | 1CO 15:45 | `;` | top/attached | `living being;"` |
| eng-t4t | 2CH 32:15 | `.` | punct/spaced | `from my power' .”` (space before the closing group) |

**Medial periods (`word.word`, letter attached on the right)** — the phenomenon
the plan named; the shipped left-side rule saw only the (majority) attached-left:

| corpus | sid | mark | context |
| --- | --- | --- | --- |
| cac | 1CH 5:12 | `.` | `vin̈aj Safán.Ix cajnajpax` (missing space after period) |
| tel2017 | LUK 10:7 | `.` | `పాత్రుడు.ఇంటింటికీ` |
| twiasante | MAT 13:44 | `.` | `afuo bi mu.Ɔkataa so` |

Quality is high; these read as genuine run-ons / mis-spacings. A **priced-in FP
sub-class** in this bucket: the trailing `.` of an ellipsis `...Word` reads
`letter/attached` (kpg/kos/twi NUM 21:14 `“...Waheb`) — an ellipsis-adjacency
artifact, not a spacing slip. Floor/knee-shaped, same family as the shipped
rule's known limitations.

---

## 3. Six-corpus regression vs the shipped rule

Join by `(sid, mark byte-offset, side)`. "Shipped findings must be reproduced by
their Letter pools."

| corpus | shipped | A-operational keeps | A-**Letter-pool** keeps | B keeps |
| --- | ---: | ---: | ---: | ---: |
| engwebster | 4 | **4** | 4 | 4 |
| WA-kmr-IQ-badini-reg | 20 | **20** | 20 | 18 |
| udu | 0 | **0** | 0 | 0 |
| WA-ne-udb | 76 | **76** | 76 | 76 |
| WA-pa-ulb | 25 | **25** | 25 | 25 |
| mya | 15 | **15** | **14** | 15 |

**Design A operational reproduces 100% of every shipped win** — nothing
non-zero fell to zero, ne_udb's rate-scaled dandas ride exactly as shipped. The
single Letter-pool "miss" is **mya EZK 48:30 `။`**: shipped flagged its
left-spaced form, but Design A classifies that side's neighbour as **Punct** (a
`၏ ။` mark-before-mark), so it is judged by the punct/top route, not the Letter
pool — the finding is kept, only routed to a different (correct) pool. This is
the intended "report any site whose verdict changes and why": the Letter pool is
the shipped rule's core, and the only divergences are non-letter neighbours the
shipped rule folded into its coarse `spaced` bucket. Design B drops 2 of
kmr-IQ's 20 (content-category modality flips them).

---

## 4. Fleet totals + per-class delta

```
shipped 9,644  →  Design A 27,772  (+18,128)   Design B 95,232  (+85,588)
```

**Design A findings by pool level** (flagged sides; a two-sided site is one
finding, so flagged sides 32,574 > findings 27,772):

| level | letter | number | punct (quote / other) | top-fallback |
| ---: | ---: | ---: | ---: | ---: |
| flagged sides | 11,809 | 1,286 | 9,771 / 4,758 | 4,950 |

The delta over shipped is overwhelmingly the **new pools the shipped rule
abstained on**: quote-adjacent (9,771), other-punct (4,758), number (1,286), and
the top-level all-class fallback (4,950 — which re-admits some punct-adjacency
shipped deliberately abstained on; see the caveat below). The Letter pool
(11,809) is the shipped rule's own territory.

**Design B flagged sides by observed category** — the failure mode in one line:

| letter | number | ws | punct |
| ---: | ---: | ---: | ---: |
| 5,088 | **33,791** | 8,251 | **66,508** |

Design B's mass is **rare content categories** (number, punct), i.e. "this mark
has an unusual *neighbour*", not "this mark is mis-spaced" — the content-rarity
FP class (§5b).

**Hierarchy telemetry (Design A):** class-vs-top **disagreements 25,528**,
**double-flags 13,114**. The two levels genuinely diverge (conditioning changes
the verdict 25.5k times), and the most-specific-pool selection dedupes 13.1k
sites that both levels would otherwise flag — the hierarchy earns its keep.

**Caveat — the top-level fallback can over-reach in thin corpora.** In a corpus
with few punct-adjacent marks (e.g. engwebster's 13 `?`-before-`)`), the Punct
class pool holds no convention, so a `?)` falls to the top-level all-class pool
and is flagged as a rare attached-right — a parenthetical the shipped rule
abstained on. In high-volume corpora the Punct pool holds (`?` R.punct N=541k,
attached 80%) and **shields** it. This is a per-corpus thin-pool artifact of the
fallback, floor/knee-shaped, worth an eye at ADR time.

---

## 5. Histograms, noisiest corpora, and the two design-B predictions

**Histogram (site-side operational scores, ref knee):** Design A **99.95%** in
`[0, 0.025)` (138,942,475 / 139,015,010); Design B **99.81%** (138,794,482 /
139,058,148) — same one-huge-spike-plus-thin-tail shape, B's tail fatter.

**Noisiest new-pool corpora** (number-flag / quote-flag / dash-flag sites):
WA-vi-ulb (13/9/144), tdx (0/4/86), swe (32/3/33), WA-tel-x-piya-reg (22/25/9),
hch (0/27/24), lacNT (0/22/29). Dash-heavy corpora dominate the dash column;
quote-flag volume tracks corpora with a spaced-then-quote minority.

### (a) Design B cannot judge spaced-side-vs-content — **CONFIRMED**

Whenever Design A reads a side as *spaced*, Design B's immediate read is `ws`
(structural: the whitespace/seam is right there), so `7. 8` and `7. Next` are
**identical** to B. Real samples where A flags a spaced-content minority and B is
blind:

| corpus | sid | mark | A pool / form | context |
| --- | --- | --- | --- | --- |
| yby | MAT 23:39 | `:` | number/**spaced** | `Sam 118: 26` |
| mya | EZK 48:30 | `။` | punct/**spaced** | `ဖြစ်၏ ။ မြို့` |
| engjps | JOS 22:34 | `:` | punct/**spaced** | `altar — : 'for it is a witness` |
| cmncbt | ACT 17:23 | `、` | punct/**spaced** | ` 、我在街上走` |

### (b) Design B over-flags rare-content hapax attachments — **CONFIRMED (large)**

Design B flags a *non-modal category* even when the spacing is perfectly
conventional; Design A's conditioning shields it (the content pool holds a
convention, or the mark's attached-left is the global majority). All A-silent,
B-flag:

| corpus | sid | mark | B category (A silent) | context |
| --- | --- | --- | --- | --- |
| snnNT | REV 14:1 | `,` | number | `bainreba, 144,000 ba'icua` (thousands comma) |
| mcdNT | REV 13:18 | `.` | number | `numero 666.` |
| hebwlc | 2SA 8:3 | `׃` | punct | `בִּֽנְהַר־׃` (maqaf-then-sof-pasuq, conventional) |
| engDRA | LEV 7:31 | `.` | punct | `his sons'.` (period after apostrophe) |

**The nuance the samples add to prediction (b):** the user's stated case ("a
single 7.8") is only *sometimes* A-silent — a lone decimal whose class pool is
N=1 falls to the top level, where attached-right is a global minority, so A may
still flag it (same as the medial-period coverage). Design A's real, reliable
win is on **recurring** content (thousands commas, chapter:verse colons): once
the Number pool holds a convention it shields every member, while Design B keeps
flagging each one as a rare category. The `144,000`/`666` samples are exactly
that recurring-content class, and Design A is correctly silent on all of them.

---

## Pd dash lane (separately reported)

Design A **4,079** dash findings, Design B **7,287**. Fleet-summed dash pools
(Design A, `*` = Wilson-dominant):

| dash | L.letter | R.letter | note |
| --- | --- | --- | --- |
| `-` U+002D | attached 97% | attached 99% | hyphenation — attached both sides is the convention |
| `—` U+2014 | **spaced 61%** | **attached 65%** | em-dash: spaced-left/attached-right, mixed |
| `‑` U+2011 | attached 100% | attached 100% | non-breaking hyphen |
| `־` U+05BE | attached 100% | attached 100% | Hebrew maqaf — always attached (conventional) |
| `–` U+2013 | spaced 85% | spaced 83% | en-dash used as a spaced range/aside marker here |

This is the user's en-dash point in fleet form: **a word-medial both-attached
dash is the convention for `-`/`‑`/maqaf and silent; a lone spaced dash in such a
corpus is the anomaly** (hbo ISA 3:15 `מַּה־ לָּכֶם` maqaf spaced; tdx JER 49:32
`boak'an -dafi'e` hyphen spaced). Conversely the em/en-dashes are conventionally
**spaced**, and their pools learn that per corpus. The synthetic tests pin both
shapes (`en_dash_medial_both_attached_is_the_conventional_shape`). Whether to
fold Pd into the live candidate domain is an **adjudication for the ADR**, not
this spike — the volumes and conventions above are the evidence.

---

## Head-to-head verdict

| criterion | Design A (class-conditioned) | Design B (immediate 4-way) | winner |
| --- | --- | --- | --- |
| fleet findings (Po, ref cell) | 27,772 (+18k over shipped) | 95,232 (+86k) | **A** — proportionate |
| reproduces shipped wins | 100% (6/6 corpora) | drops a few (kmr-IQ 18/20) | **A** |
| spaced-side-vs-content | **judgeable** (class conditions the pool) | **blind** (ws is terminal) | **A** (pred a) |
| rare-content over-flag | thin pool self-gates (Wilson) | flags every non-modal content nbr | **A** (pred b) |
| conditioned coverage real? | yes — Number 334, Quote 1,037 corpora flag | n/a (no conditioning) | **A** |
| model complexity | 2 levels + quote sub-split | flat per-side multinomial | B simpler, but loses |

**Design A is the model to carry to an ADR.** It is proportionate, reproduces
every shipped win, buys the new digit/quote/medial-period coverage the plan
asked for, and is the only design that can judge a spacing choice against its
content. Design B's simplicity costs it the whole point: it judges *which
neighbour* is present, not *how the mark is spaced against it*, so it drowns in
content-rarity false positives.

## What this answers for the redesign

1. **The pooled class-conditioned model is sound and measurement-justified.**
   Conditioning on the first-non-ws neighbour's class, with the seam as an
   ordinary spaced observation, reproduces the shipped rule (Letter pool) and
   adds real, clean digit/quote/medial coverage.
2. **Quote stays merged in Punct for now**, with the divergence logged: the
   period's `."` genuinely behaves unlike other-punct (attach vs space), which is
   the concrete evidence for a future quote split — but `,`/`:` track, so a
   blanket split is not yet warranted.
3. **The two-level hierarchy earns its keep** (25.5k class-vs-top disagreements,
   13.1k deduped double-flags) but the **top-level fallback needs a guard**
   against thin-pool over-reach (the `?)` parenthetical class).
4. **Design B is refuted** as a rival: the immediate-context multinomial cannot
   see spacing-against-content and over-flags rare content 3.4× worse.
5. **Knee/floor re-sweep and the Pd domain decision are ADR work**, measured
   here, not frozen. Nothing is frozen.
