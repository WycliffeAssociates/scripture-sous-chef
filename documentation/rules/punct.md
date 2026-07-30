# `punct.*` — Punctuation integrity

The `punct.` namespace spans two source files: `punctuation.rs`
(`adjacency-anomaly` and `spacing-anomaly` — both corpus-relative substrates)
and `bracket_balance.rs` (`bracket-balance`, the corpus-relative book-stream
matcher).

---

## `punct.bracket-balance` — unbalanced paired brackets

> **Severity** Info · **Default** on · **Scope** substrate-backed (book stream, corpus-relative verdicts) · **Knobs** `window_verses` (default 16), `confidence_z` (default 1.96), `emit_score_min` (default 0.5) · **Source** `bracket_balance.rs` · **ADR** 0016, 0037

**Flags** — Bracket events that don't fit the corpus's own pairing behaviour,
matched with a LIFO stack across the **whole book stream** (verses in
canonical order, no distance cutoff — verses anchor findings, they never
bound analysis). The inventory is the UCD `BidiBrackets.txt` pairs plus a
documented supplement (U+FD3E/FD3F ornate parens, which pair as text brackets
but are excluded from BidiBrackets on a bidi-mirroring technicality) — so
`﴾﴿`, CJK corners `「」`, fullwidth `（）`, and Tibetan `༺༻` get balance
checking alongside ASCII `()[]{}`. Each finding anchors the offending
delimiter, carries a `score`, and carries the delimiter inventory within
`window_verses` of the anchor (`FindingArgs::BracketWindow`) so a reviewer
sees the whole bracket context.
- stray closer: `…then a stray) closer` — scored by its family's pairing dominance
- opener never closed in the book → flagged at the opener
- crossed nesting: `a ([b) c]` → both crossed events surface
- a matched pair spanning more than `window_verses` verses, in a corpus whose
  pairs are otherwise short → flagged at the opener

**Clean** — `a (b [c] {d}) e`; an aside opened in v1 and closed in v3; a
25-verse speech paren in a corpus (kmr-IQ, ayn) where long spans are routine
— the long-span verdict learns that as the corpus's own convention; gux_reg's
`]`-used-as-a-letter (a legacy font-hack orthography: `ku ]inbiagu`) — the
family's pairing dominance is ~0, so every event self-suppresses.

**Why it matters** — A missing bracket can change meaning — especially the
editorial `[ ]` that mark disputed text. But a deterministic matcher assumes
a universal bracket identity, and ADR 0016's version demonstrated three
failures of that assumption: gux's letter-`]` (376 false findings), kmr/ayn
speech parens longer than the window (both halves orphaned by the
circuit-breaker), and total silence on non-ASCII pairs. The corpus itself
knows which glyphs it pairs and how far; the verdicts now read that.

**Verdict model (ADR 0037)** — Two corpus-relative verdicts, both
`evidence::dominance` (Wilson lower bound), judged per open-glyph family:
- An **orphan** (unmatched or crossed event) scores the family's corpus-wide
  *pairing dominance* — `dominance(matched_events, events, z)`. A corpus
  that pairs `(` 99.9% of the time makes a stray `(` a ~0.99 finding; a
  never-paired glyph scores ~0 and is silent.
- A **matched pair spanning more than `window_verses`** scores the family's
  *short-span dominance* — `dominance(short_pairs, pairs, z)`, anchored at
  the opener. A 25-verse `(…)` in a corpus of short pairs surfaces; a corpus
  of routinely-long spans establishes that as its convention and stays
  silent.

Findings are emitted at or above `emit_score_min`.

**Config** — `window_verses` (u16, default **16**) is no longer a matching
circuit-breaker: it is the **long-span bar** for the second verdict and the
reported-inventory radius. The default clears the longest legitimate
editorial brackets with margin — the *pericope adulterae* (JHN 7:53–8:11)
and the longer ending of Mark (MRK 16:9–20) run 11–12 verses.
`confidence_z` / `emit_score_min` are the suite-standard pair. Frozen
2026-07-06: the redesign took the 106-corpus survey from 1,114 findings to
579, with gux 376 → 0, kmr-IQ 126 → 89, ayn 78 → 70, no corpus rising, and a
sharply top-heavy score histogram (291 at 1.0, 35 at the floor).

**Nuance & ADR ties** — Quotes are **excluded**: they're direction-ambiguous,
and their book-scope balance is deferred (ADR 0011) — brackets are the
unambiguous warm-up for that matcher. Severity is **Info**, now with a score
(pre-0037 findings carried none). The reference corpus is irrelevant —
brackets are intrinsic to the target. The rule is a typed observation
substrate with ordered book-local delimiter state; its complete corpus
aggregate is retained residently and re-derived from dirty chapters. The
inventory is generated into `charclass_table.rs` (`BRACKET_PAIRS`) from a
committed trimmed `BidiBrackets.txt` by `cargo xtask gen-charclass-table`.

**Open issues / future work** — Quote balancing remains the deferred harder
sibling (ADR 0011). Known accepted residual (ADR 0037): a missing closer
plus a coincidental stray same-family closer can silently pair across a long
span; the long-span verdict recovers the short-pair-convention cases, and in
a corpus with no dominant span convention the cost is at most one missed
coincidence where the window design gave two uninspectable orphans. Its
per-family tallies are the resident substrate's aggregates; any future
quote-balance rule must declare its own boundary state and substrate contract.

---

## `punct.adjacency-anomaly` — corpus-relative repeated / mixed punctuation

> **Severity** Info · **Default** on · **Scope** substrate-backed (aggregate evidence) · **Knobs** `convention_rate`, `confidence_z`, `breadth_convention_rate`, `breadth_z`, `breadth_min_books`, `length_gain_slope`, `emit_score_min` · **Source** `punctuation.rs` · **ADR** 0024, 0031

**Flags** — A repeated or mixed punctuation run that is **neither frequent nor
widespread** in the corpus, amplified by run length, with a continuous `score`:
- `end., next` in an English corpus of clean periods → high score
- `wait,, what`, `what?!? yes` where the corpus doesn't otherwise double/mix
- non-ASCII mixed wrecks like ur-deva `?।` and `,।` — visible since the
  mixed-run separator class widened to GC `Po` (ADR 0033)

**Clean (learned silent)** — `፤፤` (Ethiopic) or `۔۔` (Arabic) in a corpus that
doubles them corpus-wide (established by **frequency**); a modest-frequency
`۔۔۔` ellipsis spread across many books (established by **breadth**); a `::`
that is a corpus's only `:` run form even in few books (frequency). Also the
known-safe `...` / `--` / `?!` / `!?` set and quote runs (`''`, `""`), which
never enter the candidate domain, and identical runs of 3+ `?` — encoding
damage owned by `hyg.replacement-run`, excluded from candidacy so one
phenomenon gets one finding (ADR 0034; `??` stays this rule's to judge).

**Why it matters** — A doubled or mixed cluster is *sometimes* a typo and
*sometimes* an orthographic convention; only the corpus can say which. The rule
keeps the prior conservative candidate extraction but replaces the fixed
Latin-centric allow-list verdict with a corpus-rate one: each exact pattern's
project count `k` is judged against `N_start(a)`, the number of positions where
its lead glyph `a` begins a maximal same-glyph run (a single clean period, a
`..`, and the `.` of a `.,` each count once toward `.`). A pattern that is a
meaningful share of its lead glyph's opportunities is a convention and goes
silent; a rare one surfaces at Info.

**Verdict model (ADR 0031)** — Frequency and breadth are **independent** evidence
of a convention, combined by noisy-OR; run length then amplifies the residual as
an odds multiplier:
```
base  = (1 − freq_strength) · (1 − breadth_strength)
score = odds_amplify(base, 1 + length_gain_slope·(len − 2))
```
`freq_strength = strength(k, N_start(a), …)` (share of lead-glyph run-starts);
`breadth_strength = strength(pattern_books, corpus_books, …)` (share of books,
gated off below `breadth_min_books`). Either axis fully establishing a convention
zeroes `base`, so length can raise an anomaly toward 1 but never resurrect a
convention.

**Config** — `convention_rate` / `confidence_z` (frequency axis; the `z` is
load-bearing where a lead glyph is exclusive to its pattern), `breadth_convention_rate`
/ `breadth_z` (breadth axis, same Wilson primitive), `breadth_min_books`
(dispersion is meaningless below a handful of books), `length_gain_slope` (odds
per extra character; `0.5` ⇒ 8-long ≈ 4× a doubling), `emit_score_min` (surfacing
floor). Calibrated 2026-07-06 — see ADR 0031.

**Nuance & ADR ties** — Exact run strings are distinct patterns (`??` ≠ `???` ≠
`????`); one long run is one event, and its length feeds the amplifier. The
mixed-run pass extends runs through the **separator class** — GC `Po`
(Other_Punctuation) minus quotes (ADR 0033), replacing the literal ASCII
`. , ; : ? !` list that had made a `?।` double-punctuation wreck invisible.
Recurring non-ASCII adjacencies suppress through the same frequency/breadth
axes as their ASCII peers (the widening added +480 findings across 106
corpora, e.g. ur-deva `?।` ×30 and hi `,*`/`;*` footnote asterisks — see the
2026-07-06 calibration report). The `...`/`--`/`?!`/`!?` exclusions stay
hardcoded in v1: a stray `...` in a never-otherwise-ellipsis corpus is
unflaggable (it never enters stats). Severity is **Info** with a score, not a
Warning verdict. See ADR 0024 (frequency verdict), ADR 0031 (breadth +
length), ADR 0033 (separator class), and ADR 0034 (`?`-run exclusion).

**Open issues / future work** — Broadening the candidate domain further
(quotes, brackets, cross-glyph families beyond the `Po` separators) and
relaxing the hardcoded exclusions are deferred, calibration-backed changes.
hi-style footnote-asterisk adjacency (`,*`) rides the sparse-convention
margin; a project whose apparatus leans on it disables per-project or raises
the floor. Corpus-wide systematic corruption (spread across *all* books)
still reads as broad ⇒ suppressed — an ingest-level concern, out of per-verse
scope.

---
## `punct.spacing-anomaly` — pooled class-conditioned spacing

> **Severity** Info · **Default** off · **Scope** substrate-backed (aggregate evidence) · **Knobs** `emit_score_min` (default 0.5), `confidence_z` (default 1.96), `minority_recurrence_k` (default 32), `minority_rate_per_10k` (default 40) · **Review Control** mapped · **Source** `punctuation.rs` · **ADR** 0029, 0033, 0050, 0054 (2nd amendment)

**Flags** — A separator mark (GC `Po` minus quotes — `. , ; : ? !` and equally
danda `।`, Arabic `۔ ، ؟ ؛`, Ethiopic `። ፤ ፥`, Burmese `။ ၊`, Khmer `។`) **or a
dash** (GC `Pd` — hyphen `-`, en/em-dash `– —`, Hebrew maqaf `־`; ADR 0054 2nd
amendment) written in a **per-side spacing form rare for that mark, against
neighbours of the same content class, in this corpus**, with a continuous
`score`. The model (ADR 0054 2nd amendment — the pooled class-conditioned model,
superseding the same-day 16-cell joint model and the per-side first amendment):
*the typist chooses the space, not the neighbour — so condition on the content
and judge the choice.* For each `(mark, side, class)` where `class ∈ {Letter,
Number, Punct}` is the content class of the **first non-whitespace neighbour** on
that side, a binary *attached*-vs-*spaced* is learned **within that pool**. The
judged bit is *whether whitespace was crossed* — the neighbour's class is read
even across the verse/book seam (in book order); a book edge with no neighbour
abstains.

- a spaced `,` in a corpus that attaches commas to letters (English) → the Letter
  pool's spaced-left is a rare form → high score
- an *attached* `?` in a corpus that spaces `? !` (French, `pa_ulb`) → high score
- **content-conditioned coverage** the per-side model could not judge: `verse. 3`
  (a `.` spaced from a **number** where the corpus writes chapter:verse refs
  attached) vs `7.8` (a decimal, attached) — the *same* Number pool, different
  form; `1: 1` (a mis-spaced colon in a `1:1` corpus)
- **after-side coverage**: `word,word` (comma attached-right against a
  spaced-right Letter convention), `away!Why`, a verse-leading `.word`
- a swapped Spanish `¿` used with a letter to its left and a space to its right
  (`así¿ no`) violates **both** Letter pools → one finding carrying both
- a lone spaced hyphen in a hyphenation corpus (medial `-` normally both-attached)

**Clean (learned silent)** — The dominant form of each pool, any pool with **no
Wilson-dominant convention** (a near-even split, or a thin pool — Wilson
self-gates, no min-samples), a pool seen in one form only, and a **rare form that
recurs at scale** (ADR 0050) — the text's *second convention*, not a slip. The
old hard-coded exclusions are now silent **by their pools' conventions**, not by
an exclusion list: a numeric `1:1` colon is judged in the **Number** pool and, if
the corpus always attaches it, its attached sole form scores 0; a cluster tail
(`?!`'s `!`) is judged in the **Punct** pool (its neighbour the `?`) and goes
silent the same way. **No top-level fallback** (user ruling): a side is judged by
its class pool *only* — a pool that holds no convention is silent, never routed
to an all-class bucket. This is what kills the spike's `?)` over-reach: a `?`
before `)` lands in a thin `Punct` pool that holds nothing, so it stays quiet.
Verse-edge marks read the seam as whitespace (`spaced`) with the neighbour class
read across it, pooling with their mid-verse twins.

**Why it matters** — Whether a mark is spaced or attached, on which side, **and
against what kind of neighbour**, is a per-mark convention, not a universal rule.
The predecessor `punct.space-before-punct` fired 6159 times on `pa_ulb`, where
spacing `? !` is the norm. Conditioning on the neighbour's class is what lets the
rule judge a spacing choice *against its content* — a mis-spaced chapter:verse
colon (`Sam 118: 26`) is invisible to a model that only sees "there is
whitespace"; the Number pool sees it is spaced where refs attach.

**Score — two factors: dominance × rarity (ADR 0048/0050/0054)** — Per mark, sum
the per-book **twelve** counters `[side][class][form]` (2 sides × 3 classes × 2
forms). Each `(side, class)` pool is scored independently over its judged
occupancy `N_pool`. A pool holds a convention iff its majority is Wilson-dominant
at the floor. Each form holding `count` of `N_pool` scores:

```
dominance = wilson_lower_bound(N_pool − count, N_pool, confidence_z)   // the majority
K         = minority_recurrence_k + minority_rate_per_10k · N_pool / 10 000
rarity    = 1 − min(count − 1, K) / K
score     = dominance × rarity
```

- **dominance** is the conservative share held by the pool's *majority* (a
  binary's complement **is** its opposing form). The dominant form ⇒ score ≈ 0 ⇒
  silent; a rare one ⇒ ≈ 1. A pool with no dominant form stays quiet, no tie
  handling — and with no fallback, a pool that does not hold flags nothing.
- **rarity** (ADR 0050, retained under per-pool denominators) is a linear
  recurrence knee whose width grows with the pool's volume: at large `N_pool` the
  flag boundary is a *rate*, thin pools get the absolute base `k`. A form seen
  once is `rarity = 1`; one recurring past the knee is `rarity = 0`.

An occurrence is scored on each side independently; one violating **both** sides
is a single finding carrying both. **Fixing occurrences raises the score of the
remaining ones** (rarity climbs back toward 1) — clean-as-you-go sharpens the
signal.

**Presentation** — `FindingArgs::SpacingConvention { mark, left, right }` where
each present side is a `SpacingSide { form, class, count, total }` — the violated
side's observed minority form (`"attached"` / `"spaced"`), the neighbour-content
`class` that judged it (`"letter"` / `"number"` / `"punct"`), its `count`, and
the pool's `total = N_pool` (ADR 0048). So the message names the pool: "`:` is
spaced on the right to a number in only 1 of 214 places." A side that abstained
(book edge), whose pool held no convention, or that was not violated, is absent.
The span highlights the violated side's *neighbourhood*: the crossed whitespace
run where a space **is**, or the attached neighbour grapheme where a space
**belongs** (`,w` for a run-together-after comma, ` ,` for a spaced-before one),
unioned across both sides when both fire.

**Config** — `emit_score_min` (default **0.5**) is the emission floor and the
per-pool Wilson-dominance gate; `minority_recurrence_k` (default **32**) and
`minority_rate_per_10k` (default **40**) are the volume-scaled recurrence knee
(the rate term keeps ne_udb's verse-final dandas near the floor — ADR 0050);
`confidence_z` (default 1.96) is an advanced Wilson-confidence knob. Review
Depth maps the four judging fields through the offline profile; explicit
advanced overrides win afterward. No
`convention_rate`, no `min_samples`. Constants carried from ADR 0050 and
re-verified under the per-pool denominators (ADR 0054 2nd amendment). At the
shipped cell the fleet lands at **27,024 findings across 1,360 corpora**; the
six ADR 0050 regression corpora reproduce **100%** of the previous per-side
rule's findings (140/140, incl. mya's one Punct-pool site).

**Nuance & ADR ties** — Neighbour classification: a cluster with an alphabetic
scalar (incl. a decomposed base + combining letter) → `Letter`; a leading
non-quote numeric scalar → `Number`; everything else — another mark, a **quote**,
a bracket, a symbol — → `Punct` (quote **merged** into Punct, user ruling). The
verse/book seam reads as whitespace (`spaced`), never its own category (repo
`CLAUDE.md`: a terminal is never attached across a seam, ADR 0054's no-edge
ruling), and the neighbour class is read across it in book order. The redesign
took the fleet count from 3,928 (old before-only binary) → 115,883 (same-day
16-cell) → 9,644 (per-side amendment) → **27,024** (pooled model at shipped
defaults) — the last rise being genuine content-conditioned number/punct/dash
coverage the per-side model abstained on. Default-off. See ADR 0029 (dominance),
0033 (`Po` separator class), 0050 (recurrence knee), and 0054 + its second
amendment (the pooled class-conditioned model).

**Open issues / future work** — **Known priced-in false-positive class:
ellipsis adjacency** — the trailing `.` of `...Word` reads Letter/Attached on the
right, so in a spaced-period corpus it can read as a medial run-on
(documented, not fixed; floor/knee-shaped). The period's `."` divergence from
other-punct is logged evidence for a possible future **per-mark quote split**
(the spike's quote sub-tally; `,`/`:` track, so a blanket split is not yet
warranted). A `mark × script` fallback dimension is deferred until calibration
shows both buckets carry evidence.
