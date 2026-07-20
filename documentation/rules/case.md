# `case.*` — Casing (word lexicon, two-factor, stateful)

Source: `crates/core/src/signals/casing.rs`.

Three rules from this family. Two share one per-word case lexicon (ADR 0051,
superseding ADR 0035's per-glyph dominance); the third,
[`case.mixed-case-word`](#casemixed-case-word--an-interior-capital-slip)
(ADR 0055), rides its own compact shape table for the *interior*-capital
phenomenon.

**The casing pair** (`case.sentence-initial-lowercase`,
`case.inconsistent-word-casing`) model an occurrence's case as the OR of two
causes: the position forces uppercase, or the word is intrinsically
capitalized. Censoring is one-directional — uppercase at a forced position is
uninformative about the word (weighted by the terminal class's learned trust —
ADR 0052); lowercase is informative everywhere. Both score a
lowercase site as `dominance × rarity` and share one config (`Config.casing`:
`emit_score_min` 0.95, `recurrence_k` 32, `confidence_z` 1.96, `trust_gate` 0.90).
All three rules ship **default OFF**.

Forced positions are structural: a word right after a *bare* attached terminal
glyph (the pending-terminal machine, carried across verse seams), plus the
book-initial word. **Never verse-initial** — verses are addressing, not
discourse (`CLAUDE.md`). The pair's word unit is a UAX #29 token with letter-
flanked hyphen compounds merged (`Bar-jesus` is one word), pure-number tokens
dropped; `case.mixed-case-word` uses the *unmerged* letter-run token instead
(see its section).

---

## `case.sentence-initial-lowercase` — lowercase after a casing-convention terminal

> **Severity** Info · **Default** OFF · **Scope** stateful (per-book word table) · **Knobs** `emit_score_min` (0.95), `recurrence_k` (32), `confidence_z` (1.96), `trust_gate` (0.90) · **ADR** 0017, 0035, 0051, 0052

**Flags** — A forced-position lowercase word-start, scored by how established
the corpus's capitalize-after-this-terminal habit is (measured only on words
the corpus itself writes lowercase) times how rare *this* word's lowercase-
after-that-terminal form is:
- `he said. and then` where the corpus reliably capitalises after `.` → the
  `and` after the period
- a lowercase first word of the book, or a lowercase word after `?`/`!`/`:`
  where the corpus reliably capitalises there
- across a verse seam: a period ending verse N forces the first word of verse
  N+1 (the old per-verse rule could never see this)

**Clean (learned silent)** — Everything in a caseless script (no cased
word-start ever accumulates a habit — silence by construction, not a script
list); lowercase after a glyph the corpus doesn't treat as a casing boundary;
a word the corpus *itself* writes lowercase after that terminal many times (the
recurrence factor collapses it — a second convention, not a slip); a corpus
whose only capitalize-after-terminal evidence is proper nouns (the
lexicon restriction sees no habit for common words, so nothing surfaces);
verse-initial words with no preceding terminal (continuations, not forced).

**Why it matters** — It does **not** assert "a sentence starts uppercase." It
learns, per terminal glyph, how reliably *this translation* capitalises the
next word — restricted to words it otherwise writes lowercase, so names
starting sentences don't set the habit — and flags a lowercase site only where
that habit is strong and the word doesn't itself recur lowercase there.

**Verdict model (ADR 0051)** — `score = habit(glyph) × rarity(word's
forced-lowercase recurrence)`. `habit(glyph)` is the Wilson lower bound of
"uppercase follows this glyph" over forced occurrences of intrinsically-
lowercase words (the decontaminated ADR 0035 number). `rarity = 1 − min(minority
− 1, k)/k` (ADR 0050 absolute knee). This retires ~64% of ADR 0035's surfaced
set as recurring second-conventions and exposes corpora whose "sentence-start
convention" was pure proper-noun confound.

**Trust gate (ADR 0052)** — Before a forced site is scored at all, its
**boundary class** — the terminal glyph *plus* whether a close-quote intervened
(`.` and `."` are separate classes, each earning its own trust) — must clear a
learned `trust ≥ trust_gate` (0.90); `habit` is measured per class, not per bare
glyph. `trust` is a noisy-OR of two witnesses: the lexicon-restricted
capitalize-after habit (the `habit` above) and a case-free word-reshuffle witness
(does the class's following-word distribution diverge from the corpus baseline,
guarded by its agreement with the reference terminal's aftermath). A class that
clears the gate is scored with the *unchanged* `habit × rarity` — trust never
multiplies into the score (three honest ~0.97 factors would compound a confident
finding under the floor; multiplicative wiring eroded 373 genuine findings). Below
the gate the positional channel is silent for that class. The gate sits in a
measured plateau (identical fleet totals for every `trust_gate ∈ [0.50, 0.95]`)
and is deliberately below the 0.95 emit floor so the two constants can't be
conflated. Caseless scripts and thin/rare classes contribute 0 trust and
self-silence. This wiring took the fleet from 3,547 to 4,005 findings (+519
newly-policeable quote-context sites, +373 readmitted erosion victims).

**Config** — `emit_score_min` (0.95), `recurrence_k` (32), `confidence_z`
(1.96), `trust_gate` (0.90); see `documentation/reference/config.md`. **Stricter:** raise
the floor or lower `recurrence_k`. **Looser:** the reverse.

---

## `case.inconsistent-word-casing` — a usually-capitalized word written lowercase

> **Severity** Info · **Default** OFF · **Scope** stateful (per-book word table) · **Knobs** shared `Config.casing` · **ADR** 0051 (new rule), 0052

**Flags** — A lowercase occurrence of a word this translation almost always
capitalizes, scored by how dominantly it capitalizes that exact word times how
rare the lowercase form is:
- `yesu` where the corpus writes `Yesu` 1,315 of 1,316 times → score 0.995
- a proper noun slipped in lowercase mid-sentence (`jesus`, `jérusalem`)
- in noun-capitalizing orthographies (German, Danish), a common noun written
  lowercase — individually correct, but high volume (a per-language stance,
  deferred)

**Clean (learned silent)** — A word the corpus writes both ways (no dominant
capitalized convention); a word it writes lowercase (the expected case — the
silent quadrant); a lowercase form that itself recurs (the recurrence factor
collapses it); caseless scripts.

**Why it matters** — The first casing coverage of mid-flow text (ADR 0035 was
blind everywhere except after terminals). It is judged against how *this
translation* writes that exact word, not a dictionary — so it adapts to each
project's proper-noun set and orthography.

**Verdict model (ADR 0051)** — `score = dominance(word's soft-censored
capitalized share) × rarity(word's lowercase recurrence)`. The dominance is the
Wilson lower bound of the word's mid-flow uppercase share, with forced-position
uppercase re-entering at weight `1 − trust × habit` (soft censoring weighted by
the terminal class's learned trust — ADR 0052; a capital after a *distrusted*
mark is not position-explained and re-enters the word's profile — one pass):
in a no-habit corpus a word capitalized only at sentence starts still earns a
profile; in a strong-habit corpus the position explains its capitals. A
both-quadrant site (forced-position lowercase of a capitalized word) may also
fire `case.sentence-initial-lowercase` — corroboration across observables.

**Honest limits (accepted, ADR 0051)** — Rare homographs of frequently-
capitalized words leak through (a predicate adjective `almighty` vs the noun
`Almighty`); counts cannot encode grammar. The score is a ranker, not a
classifier; the floor buys precision at the head. Noun-capitalizing
orthographies are high-volume by design — a deferred per-language gate, not a
scoring defect.

**Config** — shares `Config.casing`. **Stricter:** raise `emit_score_min` or
lower `recurrence_k`. **Looser:** the reverse.

---

**Stats / ADR ties** — Per book, the word table stores raw case tallies (mid-
flow upper/lower and forced upper/lower split by **boundary class** — the mark
plus its adjacent-close-quote context, ADR 0052 — not the bare glyph); the
lexicon classification, per-class habit, `terminal_strength` trust, the gate, and
soft censoring are all judge-time arithmetic over the merged table, so
book-supersede stays sound and `reduce` stays one walk. The trust's second
witness needs a second word-level aggregate — per-class following-word (juror)
counts and the baseline word-start distribution (ADR 0052). Only uncased-only
words are pruned (the sole per-book-safe drop — see the module doc for why
dropping either case mass is unsound across the merge). Findings are recovered
from the forwarded reduce sites, or by re-walking a book counted from the prior
(ADR 0044). See ADR 0017 (stateful shape), 0042 (book fan-out), 0044
(reduce→judge sites), 0050 (two-factor precedent), 0052 (mark-trust gate), and
the 2026-07-09 casing calibration doc.

---

## `case.mixed-case-word` — an interior-capital slip

> **Severity** Info · **Default** OFF · **Scope** stateful (per-book word shape table) · **Knobs** `emit_score_min` (0.95), `recurrence_k` (32), `confidence_z` (1.96) · **ADR** 0017, 0050, 0055

**Flags** — A word written with a capital letter *inside* it (`wOrd`, `DIos`,
`MUngu`, `FIls`, `asÍ`) — an OtherMixed shape: it has both cases and is neither
Titlecase nor ALLCAPS — where this translation almost always writes that exact
word in a clean shape. `score = dominance(word's not-other-mixed share) ×
rarity(this mixed form's recurrence)`.

**Clean (learned silent)** — A word the translation itself writes OtherMixed
repeatedly (the recurrence factor collapses it — a convention, not a slip):
`McX` name shapes, `LORD`-inflected forms (`TUHANlah`), Bantu class prefixes
(`baYuda`), Hebrew construct forms (`HaElohim ×419`) — **excused with no
hardcoded list**. A word dominantly written OtherMixed (a live convention) has
dominance ≈ 0 and stays silent. A **hapax** mixed word stays silent (it has no
clean-shape mass, so dominance is 0). Single cased letters (`I`, `A`) are never
OtherMixed; caseless scripts have no shape.

**Why it matters** — The Shift-key-slip catch, judged against how *this
translation* writes that exact word — not a dictionary. Its bulk (657 of the
950 fleet reference sites) is *first-upper* mixed words (`DIos`, `McDonald`),
which the casing pair is blind to (they fire only on lowercase word-starts).
**Position is irrelevant** — a mid-word capital is position-independent, so
this rule imports none of the casing pair's forced-position / trust / censoring
machinery (verified on the fleet, not assumed).

**Token unit** — the plain UAX #29 **letter-run** word, *not* casing's
hyphen-merged unit: `Obed-Edom` is two Titlecase tokens, never one mixed one.

**Boundary (ADR 0055)** — one phenomenon, one finding: a *first-lower* mixed
word (`asÍ`) overlaps the casing pair's lowercase-site domain, so
`case.sentence-initial-lowercase` and `case.inconsistent-word-casing` **skip
OtherMixed tokens** — the interior-capital defect is reported here, once, not
twice as a spurious lowercase-start finding.

**Not this rule** — Missing-space run-ons (`deJésus`) are real defects but a
*spacing* phenomenon; they surface as hapax mixed forms, so this rule stays
silent on them (they belong to the spacing / attachment lane).

**Config** — `Config.mixed_case` (`emit_score_min` 0.95, `recurrence_k` 32,
`confidence_z` 1.96). **Stricter:** raise `emit_score_min` or lower
`recurrence_k`. **Looser:** the reverse.

**Stats / ADR ties** — Per book, a word→four-shape-count table
(`lower`/`title`/`allcaps`/`other`), raw and mergeable; dominance and the
recurrence knee are judge-time sums over the merged table. Every **cased** word
is kept — mixed-only pruning is unsound because a candidate's clean-shape mass
(which drives dominance) is spread across books (see the module doc). Judge
forwards no sites and re-scans for spans (ADR 0044), like `uni.rare-glyph`. The
shape classifier and the titlecase name-shape helper live in
`signals::case_shape`, shared with `uni.rare-glyph` (whose `is_titlecase_name`
is intentionally looser — see ADR 0055). Shipped by the
[mixed-case spike](../calibration/2026-07-10-mixedcase-spike.md); the production
rule reproduces its 950-finding reference count exactly.
