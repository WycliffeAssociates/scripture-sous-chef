# `punct.*` — Punctuation integrity

The `punct.` namespace spans two source files: `punctuation.rs`
(`adjacency-anomaly` and `spacing-anomaly` — both corpus-relative and stateful)
and `bracket_balance.rs` (`bracket-balance`, the corpus-relative book-stream
matcher).

---

## `punct.bracket-balance` — unbalanced paired brackets

> **Severity** Info · **Default** on · **Scope** project (book stream, corpus-relative verdicts) · **Knobs** `window_verses` (default 16), `confidence_z` (default 1.96), `emit_score_min` (default 0.5) · **Source** `bracket_balance.rs` · **ADR** 0016, 0037

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
brackets are intrinsic to the target. The rule stays a `ProjectRule`
(whole-map, non-incremental); family statistics are recomputed per call. The
inventory is generated into `charclass_table.rs` (`BRACKET_PAIRS`) from a
committed trimmed `BidiBrackets.txt` by `cargo xtask gen-charclass-table`.

**Open issues / future work** — Quote balancing remains the deferred harder
sibling (ADR 0011). Known accepted residual (ADR 0037): a missing closer
plus a coincidental stray same-family closer can silently pair across a long
span; the long-span verdict recovers the short-pair-convention cases, and in
a corpus with no dominant span convention the cost is at most one missed
coincidence where the window design gave two uninspectable orphans. If the
rule ever converts to stateful reduce/merge/judge, the per-family tallies are
already the right aggregates.

---

## `punct.adjacency-anomaly` — corpus-relative repeated / mixed punctuation

> **Severity** Info · **Default** on · **Scope** stateful (aggregate-only) · **Knobs** `convention_rate`, `confidence_z`, `breadth_convention_rate`, `breadth_z`, `breadth_min_books`, `length_gain_slope`, `emit_score_min` · **Source** `punctuation.rs` · **ADR** 0024, 0031

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

## `punct.spacing-anomaly` — corpus-relative attachment signatures

> **Severity** Info · **Default** off · **Scope** stateful (aggregate-only) · **Knobs** `emit_score_min` (default 0.5), `confidence_z` (default 1.96), `minority_recurrence_k` (default 32), `minority_rate_per_10k` (default 40) · **Source** `punctuation.rs` · **ADR** 0029, 0033, 0050, 0054

**Flags** — A separator-punctuation mark (GC `Po` minus quotes, ADR 0033 —
`. , ; : ? !` and equally danda `।`, Arabic `۔ ، ؟ ؛`, Ethiopic `። ፤ ፥`,
Burmese `။ ၊`, Khmer `។`) written in a **per-side spacing form rare for that
mark in this corpus**, with a continuous `score`. Each mark carries **two
conditional binary conventions** — is it *attached* (a letter neighbour) or
*spaced* (whitespace, or the verse/book seam) on its **left**, and independently
on its **right** (ADR 0054 amendment — per-side factorization, superseding the
same-day 16-cell joint model and the ADR 0029 before-only binary). A punct/digit
neighbour **abstains** on that side — the attached-vs-spaced question does not
apply:
- a spaced `,` in a corpus that attaches commas (English) → spaced-left is a rare
  form → high score
- an *attached* `?` in a corpus that spaces `? !` (French, `pa_ulb`) → high score
- **new after-side coverage** the before-only rule could never see: `word,word`
  (comma attached-right against a spaced-right convention), `away!Why` (`!`
  attached-right), a verse-leading `.word` (`.` spaced-left via the seam)
- a swapped Spanish `¿` used with a letter to its left and a space to its right
  (`así¿ no`) violates **both** sides against its spaced-left / attached-right
  opening convention → one finding carrying both

**Clean (learned silent)** — The dominant form on each side of each mark, any
side with **no dominant form** (a near-even split scores below the floor on its
own — no tie special-case), a side seen in one form only, and a **rare form that
recurs at scale** (ADR 0050): engwebster's spaced `; : ? !` period typography,
kmr-IQ's 1,289 spaced ` ،`. A recurring minority is the text's *second
convention*, not a slip. The old hard-coded exclusions are silent by
**abstention** (ADR 0054 amendment) — no exclusion list and no flaggable
punct/digit combos: numeric `1:1` colons abstain on **both** sides (digit-
flanked), cluster tails (`?!`'s `!`) abstain on their punct side, and
quote-adjacent `,"`/`."` abstain on the quote side (quote-adjacency returns to
unjudged-by-structure until the boundary-class work). Verse-leading/-final marks
read the seam as whitespace (`spaced`), pooled with their mid-verse twins.

**Why it matters** — Whether a mark is spaced or attached, and on which side, is
a *per-mark convention*, not a universal rule. The predecessor
`punct.space-before-punct` flagged all whitespace-before-punct as a typo and
fired **6159 times** on `pa_ulb`, where spacing `? !` is the norm. This rule
learns each mark's signature distribution and flags only the rare ones, in
**every** direction — including the missing-space-*after* the before-only ADR
0029 rule structurally could not catch.

**Score — two factors: dominance × rarity (ADR 0048/0050/0054)** — Per mark, sum
the per-book four counters `[l_attached, l_spaced, r_attached, r_spaced]`. Each
side is scored independently over its judged occupancy `N_side` (only the
occurrences whose neighbour on that side is a letter or whitespace; punct/digit
abstains). Each form holding `count` of `N_side` scores:

```
dominance = wilson_lower_bound(N_side − count, N_side, confidence_z)   // the majority
K         = minority_recurrence_k + minority_rate_per_10k · N_side / 10 000
rarity    = 1 − min(count − 1, K) / K
score     = dominance × rarity
```

- **dominance** is the conservative share held by the side's *majority* (a
  binary's complement **is** its one opposing form, so this recovers ADR 0029's
  literal opposing-convention meaning). The dominant form has a tiny complement
  ⇒ score ≈ 0 ⇒ silent; a rare one ⇒ ≈ 1. A side with no dominant form stays
  quiet with no special-case tie handling.
- **rarity** (ADR 0050, retained under per-side denominators by the ADR 0054
  amendment knee re-sweep) is a linear recurrence knee on the form's count whose
  width grows with the side's volume: at large `N_side` the flag boundary is a
  *rate*, while thin marks get the absolute base `k`. A form seen once is
  `rarity = 1` (a rare slip); one recurring past the knee is `rarity = 0` (a
  second convention).

An occurrence is scored on each side independently; one violating **both** sides
is a single finding carrying both. A deliberate dynamic follows from the product:
**fixing occurrences raises the score of the remaining ones** (rarity climbs back
toward 1) — clean-as-you-go sharpens the signal.

**Presentation** — `FindingArgs::SpacingConvention { mark, left, right }` where
each present side is a `SpacingSide { form, count, total }` — the violated side's
observed minority form (`"attached"` / `"spaced"`), its `count`, and that side's
`total = N_side` (ADR 0048). A side that abstained or was not violated is absent.
The span highlights the violated side's *neighbourhood*: the crossed whitespace
run where a space **is**, or the attached neighbour grapheme where a space
**belongs** (`,w` for a run-together-after comma, ` ,` for a spaced-before one),
unioned across both sides when both fire.

**Config** — `emit_score_min` (default **0.5**) is the emission floor on the
two-factor score; `minority_recurrence_k` (default **32**) and
`minority_rate_per_10k` (default **40**) are the volume-scaled recurrence knee
(the rate term is required to keep ne_udb's verse-final dandas near the floor —
ADR 0054); `confidence_z` (default 1.96) is an advanced Wilson-confidence knob.
There is deliberately no `convention_rate` and no `min_samples`. Constants
carried from ADR 0050 and re-verified under the per-side denominators (ADR 0054
amendment knee re-sweep); the knee is a pure sensitivity dial.

**Nuance & ADR ties** — Context classification per side: a neighbour cluster with
an alphabetic scalar (incl. a decomposed base + combining letter) is `attached`;
crossed whitespace **or** the verse/book **seam** (which reads as whitespace,
never its own category — repo `CLAUDE.md`: a terminal is never attached across a
seam, ADR 0054's no-edge ruling) is `spaced`; a punct/digit neighbour
**abstains** (the question does not apply). Quotes stay out of the candidate mark
set (ADR 0033) and, as a *neighbour*, abstain (they no longer read `punct` — the
16-cell model's quote-adjacency findings were the chief driver of its finding
storm). The two-step redesign took the fleet count from 3,928 (old before-only
binary) up to 115,883 (the same-day 16-cell joint model) and back to **9,644**
(the per-side amendment at shipped defaults) — the intended
broadening (genuine after-side coverage the before-only rule could not see); the
rule is default-off. See ADR 0029 (dominance), 0033 (separator class), 0050
(recurrence knee), and 0054 (joint signatures + the per-side amendment).

**Open issues / future work** — The 16-cell model's rare-*context* digit
false-positive class is **retired** by the amendment: digit neighbours now
abstain, so a digit-flanked mark in a digit-sparse corpus is never judged. A
`mark × script` fallback dimension is deferred until calibration shows both
buckets carry evidence. Quote-specific attachment rides the parked quote work
(ADR 0039); until then the quote side abstains.
