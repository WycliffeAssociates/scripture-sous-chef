# Doubtful — follow-ons from the wire-format/interning investigation

Date started: 2026-07-18. Status: **living doc for this thread** — Will
explicitly asked for these to accumulate in one place as they surface rather
than being scattered, since this was a rapid brainstorm session. Nothing
below is committed-to; append new items here rather than opening new files,
until/unless one of them earns its own spike-and-verdict doc the way
`rejected/2026-07-17-fold-cache.md` did.

Companion material already landed elsewhere, not repeated here:
- `documentation/calibration/2026-07-18-findings-wire-format-survey.md` —
  the measured wire-format spike (items 1+3 from the queued-spikes list:
  serialization histograms, packed `Finding`). That doc's own "Reading"
  section already flags the packed-buffer-diffing idea as a next step; the
  first item below refines it further.
- The 5-item ranked queued-spikes list (Galley diff, packed `Finding`,
  adaptive word-frequency caching, chapter-granularity invalidation) lives
  in agent memory (`project_two_perf_spikes_queued`), not the repo — several
  items below are direct extensions of #4 (adaptive word-frequency caching)
  and #5 (chapter-granularity invalidation) from that list.

## Galley diff needs a deletion tombstone, and packed records suit SIMD comparison

If `Galley` ever ships a diff instead of the full finding set (queued-spikes
item 2), and findings are a flat packed buffer instead of a JS object array
(item 3), a plain "here are the changed 16-byte records" diff can't
represent *removal* — a fixed record can't signal its own absence the way a
missing key does in an object map. Needs an explicit tombstone (a marker
that a given slot/site is now clear, not just "unchanged since last diff").

Separately: once findings are fixed-width records, comparing an old buffer
against a new one for equality is exactly the shape SIMD wants — wide `!=`
across aligned chunks — much cheaper than walking two JS object arrays or
hashing them. Both of these are refinements of the same not-yet-spiked idea
already flagged in the wire-format survey (combining queued items 2 and 3),
not new spikes on their own.

## Stats/PrepCache going "binary looking" — real for part of it, blocked for the rest

Prompted by asking whether `PrepCache`/`RuleStats` could get the same
packed-bitfield treatment as `Finding` (queued item 3), with rule-id
occupying a few fixed high bits and the remaining bits interpreted
per-rule (a manual tagged union via bitfields), SIMD-reduced by first
grouping records by rule id so each group is bit-for-bit uniform (the
standard columnar/vectorized-engine trick for heterogeneous rows).

This splits cleanly into two answers, not one:
- **`PrepCache`'s actual contents** — mostly `Vec<Finding>`/`Token`/
  `SiteAddr`, i.e. exactly what queued item 3 already targets. If `Finding`
  gets packed, `PrepCache` shrinks as a side effect, free, no separate idea
  needed.
- **`RuleStats`** — a closed enum specifically because different rules keep
  genuinely different-shaped state. Pure counters/small bounded tallies
  could plausibly go binary. Anything holding unbounded string-keyed data
  (casing's trust model, any word-frequency tracking) can't — packing that
  needs an interned string/symbol table first, which is a materially bigger
  lift (basically re-deriving `Corpus`/`KeyIdx`-style addressing, but for
  words) than extending the `Finding`-packing idea. That lift is exactly
  the same unresolved cost already sitting in queued item 4's own notes:
  *"no int-only representation avoids needing to hash/slice a word's
  actual text at least once ... interning ... helps storage, not lookup
  cost."* Whichever of item 4 or this gets measured first answers both.
- Also flagged, not yet acted on: turning `RuleStats` from a type-checked
  closed enum into a bit-tagged union is an engine-behavior change, not a
  small patch — would need the repo's oracle-gate/ADR discipline if it
  ever actually gets built, same as any structural engine rework.
- No measurement exists yet that stats aggregation costs anything today —
  everything measured this thread is about the wire-format *send* path,
  not internal accumulation cost. Unlike the wire-format work, this one
  hasn't earned a "worth chasing" verdict yet.

## Grapheme/codepoint-level interning instead of word-level interning

The word-frequency-caching idea (queued item 4) is expensive specifically
because most words are hapax legomena (high cardinality, low reuse) — caching
gains little. Reframe: intern individual **grapheme clusters** (UAX #29
units, via `unicode-segmentation` — not raw codepoints, not hand-rolled mark
tables, per this codebase's standing convention) instead of whole words. Per
non-CJK script, the grapheme-cluster vocabulary is plausibly small (tens to a
couple hundred) and highly reused — the *opposite* regime from word-hapax.
CJK is the expected exception (thousands of distinct Han characters, no small
closed set) — this project already treats CJK as its own lane elsewhere
(ADR 0047's mixed-script rule leaves it un-collapsed), so a script-conditional
design (small-alphabet fast path, CJK falls back to hash/trie) would fit
existing convention rather than invent a new one.

**Pregeneration doesn't transfer from `charclass_table.rs`, and this is worth
being explicit about** — that table works because codepoint classification is
a fixed function of Unicode properties alone (context-free, same for every
corpus, precomputable once from the UCD). Grapheme-cluster *interning* is
asking a different question — "assign a dense small id to whatever subset of
graphemes actually appears in this corpus" — which isn't knowable until real
text is read, so it isn't precomputable the same way. It's worse than that at
the cluster level specifically: unlike codepoints (a fixed enumerable set),
grapheme clusters are combinatorially unbounded (a base letter plus any number
of combining marks is a distinct cluster) — there is no "give everything a
number" table that could even in principle cover cluster identity. Only the
constituent codepoints can be pregen-classified (already done); cluster
identity has to be assembled at runtime by combining those, one cluster at a
time — the real options are runtime discovery (cheap in practice because the
real alphabet turns out small) with the discovery cost amortized per
language/script across the fleet rather than per corpus, not a build-time
table.

"Pattern matching on interned numbers" (small-int alphabet, trie-shaped
lookup) is a legitimate structure once the alphabet is genuinely small — real
early-exit on shared prefixes, no per-word hashing. It's a bigger structural
lift than swapping a hash function though, closer to queued items 3/5b's size
than item 4's.

**Case-folding is a real open problem, not a detail**: full Unicode case
folding isn't always 1-codepoint-in-1-codepoint-out (German ß → "ss"),
breaking a clean cluster→single-int mapping (a trie handles variable-length
sequences fine, but it's a design wrinkle). More fundamentally, word identity
(for bucketing occurrences together) and the per-occurrence casing
observation (the actual signal the casing rule needs) are in tension — folding
case is exactly what identity-matching wants and exactly what the casing
signal doesn't want destroyed. Needs two parallel representations, not one —
worth reading how `casing.rs`'s existing `WordInterner`/`Model` already
separates these (if it does) before designing this from scratch.

**Status: SPIKED 2026-07-18, results in.** Full methodology, tables, and
reading live in
`documentation/calibration/2026-07-18-grapheme-interning-survey.md`; the
reproducible standalone bench project (Cargo.toml/main.rs, plus the
canonical 20-trial result TSV) is preserved at
`documentation/calibration/2026-07-18-grapheme-interning-bench/` — it was
never a git worktree, just a standalone Cargo crate built in the session's
ephemeral scratchpad and copied here afterward so it survives; it
path-depends on the real `ssc-core` crate read-only, purely to reuse its
corpus loader. Condensed bottom line:

- The small-alphabet/high-reuse premise held (75-6,517 distinct clusters by
  script, ≥99.57% hit-rate everywhere) — **but Hebrew, not CJK, turned out
  to be the actual boundary case** (6,517 clusters, over 2x the largest CJK
  corpus — Masoretic niqqud/cantillation combinations, not Han character
  count, is what blows up the alphabet).
- **No off-the-shelf crate beat a plain `FxHashMap` baseline** on speed
  (which itself mirrors `casing.rs`'s existing interner shape) — `lasso`
  and `string-interner` only win on memory. If this is ever built, no new
  dependency is needed.
- **Casing (the actual motivating rule) only applies to bicameral scripts**
  — Hebrew and CJK almost certainly don't run it at all. The scripts that
  matter for this idea are closer to Belarusian (75) and Vietnamese
  (192) — both under 256, so **`u8`, not `u16`, is the more likely right
  grapheme-id width** for the relevant subset.
- **Still the central open gap**: no head-to-head yet between today's raw
  string-keyed word/casing storage and a fixed-width grapheme-id-sequence
  representation, on either memory or speed, net of the measured conversion
  cost (22-60ns/grapheme, dominated by UAX #29 segmentation, not hashing).
  Whether fixed-width beats today's UTF-8 strings on memory is
  script-dependent — likely a real win for diacritic-heavy text (more bytes
  per grapheme today), likely a wash or a loss for plain ASCII (already 1
  byte/char).

**Follow-on questions surfaced after the spike, all reasoning-only, none
spiked:**
- **Word bigrams/trigrams probably don't benefit from this idea, and may be
  a harder version of the same problem.** A bigram/trigram's *location* is
  already free today (a pair of offsets/spans, no string needed) — the hard
  part was never storage, it's determining whether two n-grams are the same
  recurring pattern, which needs comparing content. Word n-grams are
  typically *more* hapax than single words (combinatorially larger space),
  so if word-level interning was already marginal, n-gram interning faces a
  steeper version of the same wall. Grapheme-interning fixes substrate cost,
  not cardinality — it doesn't change how many distinct things there are to
  track.
- **Proper nouns / case-fold genuinely need a two-level structure, and
  that's true regardless of representation.** A case-insensitive bucket
  identity (to group "The"/"the"/"THE") plus, per bucket, the actual
  observed cased forms and their counts — `FoldedKey → Vec<(CasedFormKey,
  count)>`. Grapheme-interning changes what gets hashed/compared, not the
  need for this structure. It also inherits the earlier-flagged wrinkle
  that case-folding can change sequence *length* (ß → "ss"), so a folded
  key and its original-cased counterpart aren't guaranteed to be the same
  length.
- **Rough trie sketch** (unbuilt, unspiked): fold grapheme-ids *before*
  walking, so the trie itself is the case-insensitive bucket structure —
  no separate pointer-based join between variants needed. Children indexed
  by the next folded grapheme-id (small array or small map, given the
  bicameral-relevant alphabets are 75-192 wide); a node marking "complete
  folded word" holds the variant/count payload from above. Open even in the
  sketch: dense fixed-size child array (wastes memory on sparse branching)
  vs. a small per-node map (extra indirection) — undecided.

## Parallel-execution granularity: rayon fold/reduce vs. a shared-mutex sink

Distinct from queued item 5 (which is about *cache-invalidation*
granularity — how much re-walking one edit forces). This is about *parallel
execution* granularity — if a future redesign made rule stats genuinely
order-independent (stream-order state becomes a property counted then
associatively combined, rather than requiring sequential book-order walks —
the same prerequisite item 5b already depends on), could work be
parallelized all the way down to verse level, and would that actually be
cheaper?

Two shapes to compare: (a) `par_iter().fold(...).reduce(...)` — each verse
produces a small local list, lists get glued together pairwise as work
completes; rayon's adaptive splitting means this is realistically low
hundreds of merge points for a whole Bible, not one per verse, so the
"reduce" cost scales with split count, not verse count. (b) A shared
`Arc<Mutex<Vec<Finding>>>` every thread pushes into directly, no separate
merge step — but even locking once per verse (not per finding) is ~31,000
lock/unlock pairs through one shared point, and contention specifically
worsens when findings cluster (like real noisy corpora do), not when they're
merely frequent on average.

Prediction, not yet measured: (a) should win, since its shared-resource cost
scales with core count/split depth rather than verse count, and doesn't get
worse under clustering the way (b) does. How to mock it faithfully (Rust/
desktop-only — rayon threads don't exist in wasm without
`SharedArrayBuffer`, so this is disconnected from anything wasm-side):
synthetic ~31,102-verse data (real whole-Bible verse count) with findings
drawn from real per-verse density in the oracle TSVs (~0.01/verse quiet,
~0.17/verse on noisy p99-style corpora), both implementations over identical
input, varying thread-pool size (2/4/8 — contention is a function of how
many cores hammer the lock at once, the one axis a single fast dev machine
under-represents) and density shape (uniform vs. clustered). Not yet
spiked.
