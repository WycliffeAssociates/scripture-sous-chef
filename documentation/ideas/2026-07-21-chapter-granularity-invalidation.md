# Idea — chapter-granularity invalidation: two roads, one invariant

Date: 2026-07-21. Status: **superseded/absorbed by
`../plans/2026-07-22-granularity-spine-plan.md` — historical rationale only;
do not dispatch either road.** The plan adopts typed boundary-state replay to
convergence and replaces this document's open road/seam choices.

Originally consolidated the stream-state-checkpointing item from the
60fps doubtful doc (which now points here), the chapter-granularity queued
spike, and the 2026-07-21 conversation that split the question in two.

## Motivation (the frame-slice budget)

Warm whole-corpus re-analyze is already 5.2–18.9 ms (3JN/MAT/PSA, en_ulb,
ADR 0062 numbers) — fine in isolation, but sous is **one slice of the
editor's per-frame pie** (backup buffers, USFM linting, and everything else
compete for the same 16.67/33.33 ms), on hardware potentially much slower
than the bench machine. The goal isn't literally 60fps for sous alone; it's
shrinking sous's slice as far as the architecture allows. The dominant
remaining warm cost is the **edited book's full re-walk from verse 1**
(`PrepCache` invalidates at whole-book granularity, ADR 0060) — for a
150-chapter Psalms, one keystroke pays for the whole book.

**Ceiling measured (2026-07-21,
`../calibration/2026-07-21-warm-path-profile.md`):** the edited-book
re-walk is ~14.8 ms of PSA's 19.55 ms warm call (**76%** — the entire
ladder spread), so chapter granularity could take PSA to the ~4.5 ms fixed
floor; 3JN is already at that floor (re-walk ~0.11 ms) and gains nothing.
Judge measured ~free (0.04–0.05 ms, v1 defaults), confirming this rework
attacks the right term for long books — and that the **fixed floor
(~4.4 ms of residency logistics: per-call re-hashing, clean-book product
cloning, regrouping)** is a separate, cheaper lever that benefits every
book and should probably be taken first.

**All-rules caveat (same day, same doc):** under an everything-on config
the warm ladder is 43.7–76.6 ms and **judge dominates (~39–45 ms fixed,
80% of the small-book call)** — chapter granularity still attacks the
re-walk term (now ~43% on PSA) but lands on a judge-dominated floor it
cannot touch. For all-rules configs, judge incrementalization (see the
calibration doc's named targets) outranks this idea; for the shipped
v1 defaults, the original ranking stands.

## The invariant that shapes everything

**Chapters are not discourse units** — same doctrine as verses (repo
CLAUDE.md; the book is the unit, ADR 0042). Discourse state genuinely
crosses chapter seams: casing's pending terminal, bracket-balance's LIFO
stack, duplicate-word's tail, spacing's cross-seam neighbor classes — and
real text demands it (the pericope adulterae spans John 7:53–8:11). So no
design here may *reset* state at a chapter boundary. Chapters are usable
only as **addresses** — places to observe/summarize state, never places to
zero it.

## Road 1 — entry-state-keyed chapter memoization (cheap; no rule rewrites)

Key insight: a chapter's walk products are a **pure function of (chapter
text, entry state)**, where entry state is the small stream-order bundle as
it stood at the chapter's start. So:

- Checkpoint each stream-order listener's state at chapter boundaries
  (small, hashable — a pending-terminal flag, a usually-empty bracket
  stack, a dedup tail…).
- Cache per chapter, keyed by `(chapter content hash, entry-state
  fingerprint)`.
- An edit in chapter N: re-walk N from its stored entry state; if N's newly
  computed **exit** state matches the entry-state key of N+1's cache entry,
  everything downstream is provably unchanged — stop. "Replay until
  re-convergence" expressed as a cache hit.

Properties: sequential semantics fully preserved (this is memoization, not
reordering); the pericope case needs zero special-casing (John 7's exit
state simply carries the open discourse into 8); stats fold per-chapter
tallies in book order (sums are associative — the stats side was never the
hard part); **judge is untouched** (already global-per-call, comparing
tallies against thresholds, cost scaling with vocabulary not characters);
worst case (state ripples to book end) degrades gracefully to exactly
today's whole-book behavior.

**The first thing any spike should measure** (cheap, fleet-scale, no engine
change): **ripple distance** — instrument the walk to record, per chapter
seam, whether each listener's state at the seam is at its "neutral"
value, and simulate how far a mid-chapter perturbation propagates before
exit states re-converge. If the common case converges at or before the next
seam (plausible: most state resets at most sentence boundaries), Road 1's
payoff is roughly `book_len / chapter_len` on warm edits. If state chains
persistently (quote-heavy corpora with long-open brackets?), the win
shrinks — better to know from a survey than an implementation.

Also carried from the checkpointing thread, still unverified: whether
judge's cost genuinely stays small as vocabulary grows across the fleet, or
could itself become the next bottleneck once reduce shrinks.

## Road 2 — order-independent chapter contributions (expensive; per-rule rework)

True map/reduce: each chapter mapped with **no** entry state, any order,
parallel — the classic parallel-scan/monoid trick. It is *possible*:
brackets are the textbook case (a chapter summarizes to "unmatched closers
I need, unmatched openers I provide" and summaries compose); cross-seam
punctuation exports boundary context a stitch step joins; casing exports
its first observation *conditional* on whatever entry state materializes,
resolved at stitch time. But the price is that **every stream-order
listener must define a boundary summary and a stitch operator, each with
its own correctness argument** — an oracle-gated engine rework of ADR 0057
scale.

What Road 2 buys over Road 1 is exactly one thing: **intra-book
parallelism** of the walk. Priced honestly: book-level `par_iter` already
exists natively, wasm is serial regardless (the threaded-rayon wasm spike
was tried and rejected — `galley-resident-handle` branch tip), and
incremental invalidation gets nothing from it (replay-to-convergence is
inherently sequential-from-the-edit). Pinned design note if Road 2 is ever
built: use rayon `fold/reduce` per work unit, **not** a shared
`Arc<Mutex<…>>` sink — see
`rejected/2026-07-21-verse-parallel-mutex-sink.md`.

## The relationship (why Road 1 first is almost certainly right)

Road 1 needs no rule rewrites, preserves every invariant by construction,
and attacks the actual latency driver (invalidation scope). Road 2
subsumes Road 1's benefit only by paying for parallelism nobody currently
needs. They aren't exclusive — Road 1's chapter checkpoints are also the
natural observation points Road 2's summaries would formalize — but Road 1
is the one with a plausible near-term spike.

## Open questions for the discussion this doc is parked on

- Is the ripple-distance survey worth running now, or does this whole
  thread wait until a real editor profile shows the edited-book re-walk
  dominating a frame?
- Checkpoint granularity: chapter is the natural address (matches the
  editor's patch unit), but verse-level checkpoints are the same design
  with more addresses — is there any reason to prefer them? (More
  checkpoint storage, finer replay bounds; probably not worth it.)
- `PrepCache` shape: per-chapter entries change the cache's key structure
  (`(book, chapter, entry_state)`) — how does that interact with the
  per-book `Tally` provenance (ADR 0062) and `remove_book`? (Likely
  cleanly — tallies stay per-book, assembled from chapter parts — but it
  needs a real design pass.)

## Relates to

- ADR 0060 (whole-book `PrepCache` granularity — what Road 1 refines),
  ADR 0062 (warm numbers; `Tally` provenance), ADR 0042 (book as unit),
  ADR 0057 (the scale of rework Road 2 implies).
- `doubtful/2026-07-17-60fps-dream-list-open.md` (the checkpointing item
  this absorbs; its SIMD-prefilter and hash-bench items remain there).
- `rejected/2026-07-21-verse-parallel-mutex-sink.md` (the Road 2 pinned
  note).
- `../plans/2026-07-21-packed-findings-wire-plan.md` (the send-side half of
  the same frame-slice budget).
