# Idea — grapheme-cluster interning as an *enabler* (spiked; gated on a customer)

Date: 2026-07-21, consolidating the interning threads from the (dissolved)
2026-07-18 wire-format/interning doubtful doc. Status: **open — spiked and
premise-validated, deliberately not scheduled.** The missing piece is not
more mechanism evidence; it's a **customer with a measured need** and the
big-picture cost/benefit that follows from naming one.

## What it is

Assign small dense integer ids to the grapheme clusters (UAX #29 units, via
`unicode-segmentation` — house convention) that actually occur in a corpus,
so word-shaped state is keyed/stored as fixed-width id sequences instead of
variable-width UTF-8 strings. Ids are discovered at runtime per corpus
(cluster identity is combinatorially unbounded — no pregenerated table can
exist, unlike `charclass_table.rs`, which works only because codepoint
classification is a fixed function of the UCD).

## What the spike established (2026-07-18 — full data in
`../calibration/2026-07-18-grapheme-interning-survey.md`, bench preserved at
`spike-bench/archive/2026-07-18-grapheme-interning-bench/`)

- **Premise held:** per-script cluster vocabularies are small and heavily
  reused (75–6,517 distinct clusters; ≥99.57% interner hit-rate everywhere).
- **Hebrew, not CJK, is the boundary case** (6,517 clusters — niqqud/
  cantillation combinations, not Han count, blow up the alphabet).
- **No crate beats plain `FxHashMap`** on speed (`lasso`/`string-interner`
  win only on memory). If built: no new dependency.
- **The rules that would consume this are bicameral-only** (casing family);
  the relevant alphabets are 75–192 clusters ⇒ **`u8` ids suffice**.
- Conversion costs **22–60 ns/grapheme**, dominated by UAX #29 segmentation,
  not hashing — a real per-ingest tax any design pays.

## The honest framing: this is not an optimization, it's a representation

The spike killed the diet arguments: hashing strings is already fast, and
whether fixed-width beats UTF-8 on memory is script-dependent (win for
diacritic-heavy text, wash-or-loss for ASCII). What interning actually does
is convert variable-width identity into **small fixed-width numeric
identity** — the *precondition* for structures that are impossible over
strings:

1. **Binary / fixed-width `RuleStats`.** Word-keyed rule state (casing's
   trust model, any word-frequency tracking) cannot be packed while its keys
   are `String`s. Pure-counter stats need no interning; word-keyed stats
   need exactly this. (This absorbs the dissolved doubtful doc's
   "Stats/PrepCache going binary" item — note its own caveat still stands:
   **no measurement exists that stats accumulation/storage costs anything
   today**, and re-shaping `RuleStats` is an oracle-gated engine change.)
2. **Tries over a small dense alphabet.** With ≤256 symbols, a
   fold-as-you-walk trie makes case-insensitive bucketing *structural*: fold
   grapheme-ids before walking, so the trie node for a folded word holds the
   observed cased variants + counts — the two-level `FoldedKey →
   [(CasedForm, count)]` structure the proper-noun/casing work wants,
   without a second map joined by pointers. Real early-exit on shared
   prefixes, no per-word hashing.
3. **Wide/columnar operations** — comparing, sorting, deduplicating word
   state as integer sequences.

Known design wrinkles, unchanged from the spike thread: full case folding
can change sequence length (ß → "ss"), so folded and cased forms aren't
length-paired (a trie handles variable-length fine; flat packing must not
assume 1:1). Word identity (fold) and the per-occurrence casing observation
(don't fold) need **two representations** — read how `casing.rs`'s existing
interner/model already separates these before designing anything. Word
n-grams do **not** benefit: their locations are already free (span pairs),
their cardinality is *more* hapax than words, and interning fixes substrate
cost, not cardinality.

## What it would take / cost / buy (the big-picture skeleton)

- **Take:** per-script symbol table discovered at ingest (amortizable
  per-language across the fleet, not per corpus); an intern pass on the
  word-observation path; dual fold/cased representations; migration of
  casing's string-keyed maps; oracle-gated (touches how stateful rules keep
  state — byte-identical findings required).
- **Cost:** 22–60 ns/grapheme ingest tax; table memory (small); real
  complexity in the fold/identity split; an engine-rework-scale review
  burden if `RuleStats` shapes change.
- **Buy:** the three representations above — *if* one of them has a workload
  that wants it. Not speed, not memory, per se.

## The gate (the fold-cache lesson, encoded)

The fold cache also had a validated mechanism (5.5× fleet-confirmed call
reduction) and still died end-to-end because the underlying cost was a
too-small slice of `analyze()`. So: **schedule this only when a customer
shows up in a profile or a design actually blocks on it** — candidate
customers, in likelihood order: word-keyed `RuleStats` packing (needs a
measurement that stats storage/accumulation matters), the casing fold-bucket
trie (needs the proper-noun two-level work to be scheduled), adaptive
word-frequency caching (the old queued item 4 — itself unstarted and
marginal by its own notes).

**First step if a customer appears:** the head-to-head the spike stopped
short of — today's string-keyed casing/word storage vs a `u8`-id fixed-width
representation, speed and memory, net of conversion, on both a
diacritic-heavy and an ASCII corpus.

## Relates to

- `../calibration/2026-07-18-grapheme-interning-survey.md` (the spike).
- `rejected/2026-07-17-fold-cache.md` (the lesson the gate encodes).
- `../plans/2026-07-21-packed-findings-wire-plan.md` (its non-goals point
  here for word-keyed stats packing).
- ADR 0051 (casing lexicon — the main prospective consumer).
