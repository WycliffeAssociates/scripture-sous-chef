# ADR 0042: The stateful phase fans out per book — books-shaped rules, one shared grouping, judge on the token cache

> **⚠ Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md).**
> Independent-book parallelism remains a scheduling option; this record's
> stateful-phase grouping and token-cache execution are historical.

- **Date:** 2026-07-07
- **Status:** Superseded by [ADR 0067](0067-typed-observation-substrates-resident-galley.md)
- **Extends:** [ADR 0018](0018-parallelism-behind-a-feature.md) (the
  `parallel` feature and its serial-identical contract — extended from the
  per-verse phase to the stateful phase) and [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (observe/judge — the phase this restructures). Follows [ADR 0041](0041-stateful-phase-hot-path-cleanup.md)
  (the per-char hot-path cleanup that left execution structure as the
  remaining cost).

## Context

After ADR 0041, the profile is structural: on source-relative full-corpus
runs the stateful reduce/judge phase dominates analyze time and runs
entirely serially — ADR 0018's rayon covers only the per-verse check phase.
Three structural wastes, visible in the code rather than any single hot
symbol:

1. **Every stateful rule rebuilt the by-book grouping itself.** All six
   reduces (and casing's judge) called `verse::by_book` — up to 7 full
   groupings of the same corpus per analyze.
2. **`RepeatedCharacterRun` tokenized the corpus twice** — once in reduce,
   again in judge's re-scan — and the shared `TokenCache` (built for token
   rules) was invisible to the stateful phase, so nothing could be shared.
   The UAX-29 word scan was the #1 self-time leader (15.4 %) after ADR 0041.
3. **Nothing in the phase exploited book independence**, even though the
   book is already the unit of everything here: stats supersede at book
   granularity, casing's sentence detection crosses verse seams *within* a
   book (never across), and proportionality's distributions pool per book.

## Decision

**The book is the execution unit of the stateful phase.**

1. **One grouping, computed once.** `analyze_stateful` builds
   `verse::by_book(target)` once (`Books<'_>`) and hands it to every rule
   for both phases. `StatefulRule::reduce`/`judge` now take `&Books<'_>`
   instead of `&VerseMap`, plus the optional shared `TokenCache`.

2. **`map_books` is the one parallelism site.** A crate-internal helper runs
   a closure over every book and collects outputs in book order — rayon
   under the `parallel` feature, a plain iterator otherwise. Rules call it
   and stay `cfg`-free; the wasm build compiles the serial branch and pulls
   no rayon (ADR 0018's contract, unchanged). Reduces fan book reductions
   out through it; judges keep their corpus-aggregate derivation serial
   (cheap, needs all books' stats) and fan only the re-scan.

3. **Determinism is structural, not asserted.** Books are disjoint; an
   indexed parallel collect preserves `BTreeMap` key order; every judge
   already sorts its findings; `analyze_stateful` sorts the merged stream.
   Serial and parallel builds therefore produce identical output — verified
   by building the playground survey cache under both and comparing all 106
   corpora: every finding, rule count, and verse count identical (the only
   differing JSON fields are the run's own metadata — `elapsed_ms`,
   `parallel`, `threads`). Plus zero survey movers against the pre-change
   baseline.

4. **Project rules join the fan-out.** `ProjectRule`/`ProjectTokenRule`
   also take `&Books<'_>` — bracket-balance's per-book LIFO matching and
   duplicate-word's per-book walk (both already book-independent; the tail
   carry and bracket stack never cross a book) fan through the same
   `map_books`. This removed the last serial corpus pass: profiling showed
   bracket-balance's `match_book`+fold as ~17 % of the parallel run's
   busiest thread *and* the source of the all-config sweep's multi-second
   run-to-run variance (a serial section between parallel phases whose
   duration floated with scheduling). After the change, consecutive
   all-config sweeps agree within ~0.2 s.

5. **Judge reads the token cache.** The cache-build heuristic counts
   repeated-character-run as two tokenization passes when enabled (reduce +
   judge), so a default config now builds the cache and the corpus is
   tokenized **once** per analyze instead of three times (mixed-script
   inline + rcr reduce + rcr judge). Both rcr phases fall back to inline
   tokenization when no cache exists — behavior identical either way.

### Why per-book scan parallelism (not per-verse)

Casing's judge re-scan is *not* verse-independent: `walk_book` carries a
pending sentence-terminal across verse seams. The book is the largest unit
that is always independent and the smallest that is always correct — and at
~66 books × 2 phases × 6 rules, rayon has ample work-stealing granularity
against Psalms-sized skew.

### Rejected: pooling all rules into two rule×book barriers

Flattening the sequential per-rule loop into two pools (all reduces, serial
merge seam, all judges — rule-level `par_iter` nesting with each rule's
per-book fan) was implemented and measured (2026-07-07): across seven
sweep samples it was parity-to-slightly-worse (`default` min 6.02 s vs
5.68 s; `all` min 7.73 s vs 7.03 s). Each rule's own per-book fan already
saturates the workers between barriers — the idle in wait-profiles sits at
the final joins and the genuinely serial sections, not between rules — and
interleaving six rules' working sets costs cache locality. Reverted to the
simple loop; findings were identical throughout (the experiment ran the
full zero-movers/findings-identity gauntlet before being measured out).

### Rejected: chapter-granularity work units

Chapters would shrink the skew ceiling (Psalm 119 vs Psalms) but break the
semantics the book unit exists for: bracket pairs legitimately cross `\c`
(kmr speech-parens span dozens of verses), casing's pending terminal
currently crosses chapter seams, and the deferred quote-balance rule (ADR
0039) polices discourse that spans chapters by construction (John 14–17).
It would also fork stats keys to `(book, chapter)` — wire churn for no
incremental benefit, since the shell supplies whole books. The book stays:
the smallest unit that is always correct.

### Rejected: folding observation into the per-verse phase (“4b”)

Making the parallel per-verse check phase also emit per-rule observations
would require every rule's book stats to be a commutative monoid over
verses. Casing's cross-verse seam already violates that, and — decisively —
verses are **navigation milestones, not discourse units**: rules legitimately
read a book as a sequence, so verse-shaped observation is semantically wrong,
not just risky. Book granularity also matches the editor's real update unit;
sub-book incremental updates are not on the table.

## Consequences — measured

Sweep = 106 corpora, 1.54 M verses, 1 timed pass, `en_ulb` source, recorded
under samply, same session as ADR 0041's numbers (whose “after” is this
table's “before”). Build-flag note: this work surfaced that the playground's
`ssr` feature forwards `ssc-core/parallel`, so the “before” column — like
every prior sweep — already had ADR 0018's per-verse rayon on. “Parallel
after” is therefore the like-for-like comparison; “serial after” is a
**pure-serial build** (no rayon anywhere — the wasm editor's execution
shape) whose exact “before” was never measured (it would be modestly slower
than the before column, which enjoyed per-verse parallelism):

| Config | before (`ssr` build) | parallel after (like-for-like) | pure-serial after |
|---|---|---|---|
| `default` time/pass | 34.5 s | **9.9 s (3.5×)** | 30.6 s |
| `default` verses/sec | 44.7 k | **156.2 k** | 50.5 k |
| `all` time/pass | 41.7 s | **20.6 s (2.0×)** | 39.1 s |
| `all` verses/sec | 37.0 k | **74.9 k** | 39.5 k |

The serial column reflects the restructure alone (one shared grouping,
single tokenization) — and it *beats* the before column despite giving up
the per-verse rayon the before build had: the wasm editor gets a real
speedup from this change, not just neutrality. The parallel column adds the
book fan-out on native builds (playground, survey/CI loops — the
`refresh-survey`/`survey-diff` regression loop being the big daily winner).

Session total (this ADR + 0041, `ssr` builds): `default` 55.7 s → 9.9 s
(**5.6×**), `all` 69.8 s → 20.6 s (**3.4×**), findings-per-analysis constant
at 421 / 778 throughout. Two follow-ups landed after the first cut push it
further: ADR 0041 §5 (per-word `.find()` off the critical path — `default`
6.8 s, `all` ≈10.5 s) and §4 above (project rules fanned — `default`
**5.7 s**, `all` **7.6 s**, recorded under samply; plain runs 6.2 / 7.1).
Final standing vs the session baseline: **`default` 9.8×, `all` 9.2×.**

Criterion spot-checks (serial, default features — the wasm-shaped build;
change vs post-0041, total vs the session's HEAD baseline):

| bench | post-0041 | after | Δ | session total |
|---|---|---|---|---|
| `analyze/full_bible` | 428 ms | 393 ms | −8 % | 652 → 393 ms (−40 %) |
| `analyze/full_devanagari` | 960 ms | 725 ms | −24.5 % | 1 160 → 725 ms (−37 %) |
| `proportionality/nt_vs_bible` | 5.6 ms | 5.5 ms | −2 % | 32.6 → 5.5 ms (−83 %) |

Devanagari gains most, as predicted: the UAX-29 tokenizer was the
non-Latin tax, and this change cut full-corpus tokenizations from three to
one.

The incremental path now has its own benches pinning ADR 0017's promise —
`analyze/incremental_edit_{3JN,MAT,PSA}`: cached corpus `Stats` as prior,
one edited verse re-supplying its whole book (the supersede unit), serial
build. **3JN 137 µs · MAT 11.2 ms · PSA 19.4 ms**, vs 371 ms for the full
pass. Two readings worth pinning: cost is linear in the *edited book's*
text (~8–10 µs/verse) and independent of corpus size beyond it, and the
3JN floor shows the judges' corpus-aggregate re-derivation is nearly free —
so any future rule that accidentally re-scans the whole corpus in judge
will show up here as a step change, not a shrug. (`analyze/nt` was statistically flat this run — p = 0.71, small-corpus
variance; `analyze/nt_rayon` is the legacy bench-local per-verse probe whose
retirement its own TODO already calls for now that the library parallelizes
for real.)

Verified: 210 ssc-core tests green serial **and** `--features parallel`;
clippy clean; `ssc-wasm` compiles for `wasm32-unknown-unknown` with tests
green; survey-diff zero movers; serial-vs-parallel survey caches
byte-identical.

## What becomes easy / hard

- **Easy:** any future stateful rule inherits fan-out by construction — it
  writes a per-book closure and never sees rayon. The `Books` view is also
  where a future shared grapheme cache would slot if segmentation ever
  re-emerges as a leader.
- **Hard(er):** a rule whose statistic genuinely crosses books (corpus-order
  effects) would not fit `map_books` and would need its own serial pass —
  which is exactly the friction such a rule *should* face, given the
  supersede model assumes book independence.
