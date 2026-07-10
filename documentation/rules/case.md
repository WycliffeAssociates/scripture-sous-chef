# `case.*` — Casing (word lexicon, two-factor, stateful)

Source: `crates/core/src/signals/casing.rs`.

Two rules from one module and one shared per-word case lexicon (ADR 0051,
superseding ADR 0035's per-glyph dominance). An occurrence's case is modelled
as the OR of two causes: the position forces uppercase, or the word is
intrinsically capitalized. Censoring is one-directional — uppercase at a forced
position is uninformative about the word; lowercase is informative everywhere.
Both rules score a lowercase site as `dominance × rarity` and share one config
(`Config.casing`: `emit_score_min` 0.95, `recurrence_k` 32, `confidence_z`
1.96). Both ship **default OFF**.

Forced positions are structural: a word right after a *bare* attached terminal
glyph (the pending-terminal machine, carried across verse seams), plus the
book-initial word. **Never verse-initial** — verses are addressing, not
discourse (`CLAUDE.md`). The word unit is a UAX #29 token with letter-flanked
hyphen compounds merged (`Bar-jesus` is one word), pure-number tokens dropped.

---

## `case.sentence-initial-lowercase` — lowercase after a casing-convention terminal

> **Severity** Info · **Default** OFF · **Scope** stateful (per-book word table) · **Knobs** `emit_score_min` (0.95), `recurrence_k` (32), `confidence_z` (1.96) · **ADR** 0017, 0035, 0051

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

**Config** — `emit_score_min` (0.95), `recurrence_k` (32), `confidence_z`
(1.96); see `documentation/config.md`. **Stricter:** raise the floor or lower
`recurrence_k`. **Looser:** the reverse.

---

## `case.inconsistent-word-casing` — a usually-capitalized word written lowercase

> **Severity** Info · **Default** OFF · **Scope** stateful (per-book word table) · **Knobs** shared `Config.casing` · **ADR** 0051 (new rule)

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
uppercase re-entering at weight `1 − habit(glyph)` (soft censoring, one pass):
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
flow upper/lower and forced upper/lower split by terminal glyph); the lexicon
classification, per-glyph habit, and soft censoring are all judge-time
arithmetic over the merged table, so book-supersede stays sound and `reduce`
stays one walk. Only uncased-only words are pruned (the sole per-book-safe
drop — see the module doc for why dropping either case mass is unsound across
the merge). Findings are recovered from the forwarded reduce sites, or by
re-walking a book counted from the prior (ADR 0044). See ADR 0017 (stateful
shape), 0042 (book fan-out), 0044 (reduce→judge sites), 0050 (two-factor
precedent), and the 2026-07-09 casing calibration doc.
