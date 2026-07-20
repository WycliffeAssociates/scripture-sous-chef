# ADR 0051: Casing rebuilt on a word lexicon — two-factor scores, two rules from one module

- **Date:** 2026-07-10
- **Status:** Accepted
- **Supersedes:** the scoring model of [ADR 0035](0035-casing-recast-on-dominance.md)
  (per-glyph dominance as the site score). The reduce→judge forwarding
  (ADR 0044), tape walk (ADR 0045), and book fan-out (ADR 0042) mechanics carry
  over unchanged.
- **Builds on:** [ADR 0050](0050-spacing-minority-recurrence-factor.md) (the
  dominance × rarity two-factor shape and its linear recurrence knee),
  `evidence::dominance` (ADR 0035), and the verse-invariant doctrine in
  `CLAUDE.md` / `documentation/overview/methods.md` §0.1.

## Context

`case.sentence-initial-lowercase` scored every lowercase-after-terminal site
with one number per terminal glyph: the Wilson dominance of "uppercase follows
this glyph" corpus-wide (ADR 0035). Every site sharing a glyph scored
identically, so the fleet histogram near the top was the distribution of
*per-glyph dominance across corpora* — a smooth slope with no gap, on which the
0.98 floor sat unanchored (2026-07-09 fleet survey: 40.0M candidates, ~19–22K
sites per 0.025-bucket right below the floor, 17.5K surfaced above it).

Two confounds were structural, not tunable:

1. **Position corrupts the lexicon.** A capitalized word after a terminal is
   uninformative — the position explains it — so no per-word convention could
   be learned from sentence starts.
2. **The lexicon corrupts the habit.** Proper nouns at sentence starts inflate
   the apparent capitalize-after-terminal convention. Measured on the fleet:
   the naive per-glyph habit exceeds the lexicon-restricted one by a median of
   +0.03 and up to **+0.9997** — whole corpora whose "sentence-start
   convention" was nothing but proper nouns.

A spike (committed `d3c70ae`, `5a25734`; measurements in the
[2026-07-09 calibration doc](../calibration/2026-07-09-casing-two-factor-spike.md))
validated the replacement on all 1,504 vref corpora.

## Decision

**Generative model.** An occurrence's case is the OR of two causes: the
position forces uppercase, or the word is intrinsically capitalized. Censoring
is one-directional — uppercase at a forced position is uninformative about the
word; lowercase is informative everywhere.

**Forced positions** are structural: a token following an attached terminal
glyph (the ADR 0035 pending-terminal machine, which carries across verse
seams), plus the book-initial token. **Never verse-initial** — verses are
addressing, not discourse (`CLAUDE.md` invariant).

**Word unit:** UAX #29 word segmentation (`token::tokenize`), with adjacent
tokens joined by a single letter-flanked hyphen (U+002D / U+2010) merged into
one compound. Pure-number tokens are dropped. (Bare letter-runs split
`Bar-jesus` into a spurious lowercase `jesus` — the largest artifact class in
the first spike round.)

**Estimation order** (the identification strategy — each factor is estimated
without conditioning on the other's noise):

1. *Per-word intrinsic profile* from mid-flow occurrences: case-folded word →
   upper/lower tallies.
2. *Positional habit*, per glyph = Wilson dominance of uppercase among
   forced-position occurrences of words the lexicon says are intrinsically
   lowercase. This is the decontaminated version of ADR 0035's number.
3. *Soft censoring*, one re-pass: forced-position uppercase re-enters the
   word's profile at weight `1 − habit` (in a no-habit corpus the pool comes
   back; in a strong-habit corpus the observation is honestly near-worthless).
   When `terminal_strength` (shortlist 2/3) lands, the discount becomes
   `terminal_strength × habit` — same slot, no second mechanism now.

**Two rules from one module**, one shared word table, judged on the 2×2 of
(position, word's intrinsic class):

- `case.sentence-initial-lowercase` (**rebuilt, same `RuleId`**): forced-position
  lowercase of a lexicon-lowercase word.
  `score = habit_dominance × rarity(this word's forced-lowercase recurrence)` —
  the recurrence factor silences words the corpus itself writes lowercase after
  periods.
- `case.inconsistent-word-casing` (**new `RuleId`**): mid-flow lowercase of an
  intrinsically-capitalized word.
  `score = dominance(word's mid-flow upper share) × rarity(lowercase-form recurrence)`.
- Both-quadrant sites (forced-position lowercase of an intrinsically-capitalized
  word) break two conventions: both rules may fire — corroboration across
  observables, not double-counting.

`rarity = 1 − min(minority − 1, k) / k`, ADR 0050's linear knee. The 0050
amendment's opportunity-proportional term is **omitted**: word-level
opportunity counts are tens-to-hundreds, where `rate · N / 10 000` vanishes —
and the spike's pure-rate sweep confirmed rate framing surfaces ~nothing at
word scale. Plain absolute knee.

**Frozen knobs:** `emit_score_min = 0.95` (both rules), `recurrence_k = 32`,
`confidence_z = 1.96`. Both rules **ship default-off**. No glyph
special-casing (the colon's list-vs-quote polysemy is accepted noise until
`terminal_strength`; most colon FPs scored 0.93–0.95 and die at the floor).

**Stats:** ADR 0035's per-glyph `CasingStats` dies (pre-alpha, no shims). The
replacement is the first word-level aggregate: per book, the word table above.
Persist only words observed in **both** cases plus aggregate tallies for the
consistent mass — measured unpruned worst case is 7.6 MB/corpus (~49 B/type,
p50 ~0.6 MB), and the mixed-case subset is a small fraction of types. The
future inventory mode reads the same table; if it needs the consistent words
individually, that is its schema change to make.

**Ruled out** (2026-07-09 discussion, recorded in the shortlist): word-before-
terminal statistics as site evidence — legitimate terse finals ("He went in.")
have the same count shape as errors; rarity-shaped evidence measures rarity,
not wrongness, in that channel. The aggregate form survives as
`terminal_strength`'s word-reshuffle witness.

## Rationale — freezing the floor and k

Twelve anchors from the fleet run were adjudicated by parametric review
against the actual verse text (five of them false positives the score cannot
know about: French adjective *juifs*, Portuguese generic-plural *messias*,
German adj/noun homograph *alter*, and the Dutch/Indonesian list-colon pair).
At **floor 0.95, k = 32** the set separates completely: all seven true
positives (from *yesu* 0.995 down to *christ* 0.956) survive; all five false
positives (0.506–0.948) die. `k = 32` is load-bearing at the top: the min=2
true positives (*christ*, *deal*) clear 0.95 only there, while the min=1 false
positives are k-flat — smaller k kills real findings for zero FP suppression.

Honest limits, accepted rather than tuned away:

- **The band above 0.95 is not pure.** Rare homographs of frequently-
  capitalized words leak through: deu1912 *hause* (subjunctive verb, noun
  *Hause* capped 741×), eng-web *almighty* (predicate adjective, noun capped
  97×). Counts cannot encode grammar; these are irreducible corpus-internally
  and are the designed territory of the exhaustive inventory mode (idea
  stage) — the score is a ranker, not a classifier, and the floor buys
  precision at the head, not truth.
- **Noun-capitalizing orthographies storm the intrinsic channel** with
  findings that are individually correct (dan1931: 1,290 at floor 0.5 → 52 at
  0.95). The floor tames volume 1–2 orders of magnitude; the remainder is a
  deferred per-language stance and a type-grouped presentation question, not a
  scoring defect.
- Fleet volume at the frozen knobs: **~3.5K findings** (1.2K intrinsic, 2.1K
  positional, 0.2K both) across ~600 corpora, top-5 corpus share ≤ 15% — no
  storming. Versus 17.5K surfaced by the superseded score, of which 65% died
  to the recurrence factor (the capitalize-after-terminal confound) under the
  new model.

## Consequences

- The word table replaces `CasingStats` on the wire; the wasm `RuleStats`
  schema changes (regenerate packages at implementation).
- Per-book stats stay **raw tallies** (mid-flow and forced counts, uncensored)
  so book-supersede merge semantics hold; soft censoring is judge-time
  arithmetic over the merged table — the habit only exists corpus-wide.
  `reduce` stays one walk; the site scan remains hot-path (30M+ lowercase
  sites fleet-wide), so the implementation gets a `/perf-campaign` pass before
  merge.
- Clean-as-you-go (ADR 0050's dynamic) now applies per word: fixing a word's
  minority occurrences raises the score of those remaining.
- The intrinsic rule is the first casing coverage of mid-flow text — the old
  rule was blind everywhere except after terminals.
- `terminal_strength` gains a concrete consumer contract: multiply into the
  positional score and the censoring discount when it lands (shortlist 2/3).

Sweep grids, anchor tables, storm decompositions, and per-corpus table sizes:
[2026-07-09 calibration doc](../calibration/2026-07-09-casing-two-factor-spike.md)
(post-hyphen-fix section, 2026-07-10).
