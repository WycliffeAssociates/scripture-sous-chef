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

## `punct.spacing-anomaly` — corpus-relative punctuation spacing

> **Severity** Info · **Default** off · **Scope** stateful (aggregate-only) · **Knobs** `emit_score_min` (default 0.5), `confidence_z` (default 1.96), `minority_recurrence_k` (default 32) · **Source** `punctuation.rs` · **ADR** 0029, 0033, 0050

**Flags** — A separator-punctuation mark (GC `Po` minus quotes, ADR 0033 —
`. , ; : ? !` and equally danda `।`, Arabic `۔ ، ؟ ؛`, Ethiopic `። ፤ ፥`,
Burmese `။ ၊`, Khmer `។`) written in the **minority** spacing form for that
mark in this corpus, with a continuous `score`:
- a spaced `,` in a corpus that attaches commas (English) → high score
- an *attached* `?` in a corpus that spaces `? !` (French, `pa_ulb`) → high score
- a spaced ` ،` in kmr-IQ against an attached-comma majority, spaced Burmese
  finals ` ၏` in my_juds against an attached majority — the non-Latin marks
  the old ASCII candidate list never judged

**Clean (learned silent)** — The majority form for each mark (whatever the
corpus does most), any mark with **no dominant convention** (near-50/50 stays
quiet), a mark seen in only one form, and — new in ADR 0050 — a **minority form
that recurs at scale**: engwebster's spaced `; : ? !` (Webster 1833 period
typography, hundreds each), kmr-IQ's 1,289 spaced ` ،`, udu's 2,478 spaced `/`.
A minority recurring past the knee is the text's *second convention*, not a slip.
Cluster tails (`word?!` — the `!` clings to `?`), closing-quote/paren-then-mark
(`word" ,`), verse-leading marks, and numeric `1:1` colons never enter the
opportunity pool.

**Why it matters** — Whether a mark is spaced or attached is a *per-mark
convention*, not a universal rule. The predecessor `punct.space-before-punct`
flagged all whitespace-before-punct as a typo and fired **6159 times** on
`pa_ulb`, where spacing `? !` is the norm — every hit a false positive. This
rule learns each mark's dominant form and flags only deviations, in **both**
directions (the old rule could never catch an errant *attached* mark in a
spacing corpus).

**Score — two factors: dominance × rarity (ADR 0050)** — Per mark, count
word-adjacent occurrences that are `spaced` vs `attached` (`N = spaced +
attached`); let `minority = min(spaced, attached)`. The score of every
minority-form occurrence is:

```
dominance = wilson_lower_bound(max(spaced, attached), N, confidence_z)
rarity    = 1 − min(minority − 1, k) / k        (k = minority_recurrence_k)
score     = dominance × rarity
```

- **dominance** (ADR 0029) is the conservative share held by the *opposing*
  convention, equivalently `1 − upper_bound(minority_share)`. Confidence-monotone:
  at a fixed ratio it rises with `N` toward the observed rate.
- **rarity** (ADR 0050) is a linear recurrence knee on the minority count — the
  same shape `lex.repeated-character-run` uses for `word_recurrence_k`. A minority
  seen once is `rarity = 1` (a rare slip against a strong convention); a minority
  recurring past `k` is `rarity = 0` (a second convention, silent).

An exact tie yields no verdict (silent). A deliberate dynamic follows from the
product: **fixing minority occurrences raises the score of the remaining ones**
(rarity climbs back toward 1) — clean-as-you-go sharpens the signal on what's
left.

**Config** — `emit_score_min` (default **0.5**) is the emission floor on the
two-factor score. Before ADR 0050 the score was dominance alone and this read as
a literal convention share ("≥75% dominant"); with the rarity factor folded in
it is a two-factor cutoff, and it dropped from 0.75 to 0.5 once the recurrence
knee collapsed the mid-mass that had made 0.75 a volume policy rather than a
truth cutoff. `minority_recurrence_k` (default **32**) is the recurrence knee;
`confidence_z` (default 1.96) is an advanced Wilson-confidence knob omitted from
normal UI. There is deliberately no `convention_rate` and no `min_samples`.
Defaults frozen at the [2026-07-09 calibration](../calibration/2026-07-09-spacing-minority-recurrence.md);
`emit_score_min` / `confidence_z` were provisional from 2026-07-06 (ADR 0029).

**Nuance & ADR ties** — Governing neighbour is a *grapheme cluster* containing a
letter, so a decomposed word-final letter (base + combining mark) still counts.
The finding message is direction-neutral ("this mark's spacing differs from the
corpus convention"). Two scorers were rejected en route: `1 − strength(self)`
(confuses insufficient evidence with rarity — fires on 1:1) and signed contrast
(confidence-*inverts* as the corpus grows). The `Po` widening (ADR 0033) had
taken the 106-corpus survey from 2,981 to 12,565 findings, with the caveat that
"the volume *is* the inconsistency count" — a strong convention whose minority
recurs thousands of times (kmr-IQ 2,131 ` ،`, engwebster's spaced period
typography) emitted thousands of findings. ADR 0050 resolves that: the recurrence
factor reads a recurring minority as a *second convention* and silences it, so
those storm corpora collapse to their handful of genuine slips (engwebster
2,209 → a few, kmr-IQ 2,131 → ~11) while ne_udb's strong-convention slips
(`!` 9, `,` 15) are kept. See ADR 0029, 0033, and 0050.

**Open issues / future work** — A `mark × script` fallback dimension (for a mark
that genuinely follows different conventions across scripts) is deferred until
calibration shows both buckets carry enough evidence. Digit-adjacent punctuation
stays out of scope. The recurrence knee cannot distinguish a genuine slip cluster
from an emerging second convention purely by count when the two coincide in
magnitude (ne_udb's `?` minority of 18 is discounted the same way am's
structurally identical `፡` minority of 24 is silenced) — a single knee splits
them as well as one constant can (2026-07-09 calibration margin).
