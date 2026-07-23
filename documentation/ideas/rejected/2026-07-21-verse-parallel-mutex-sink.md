# Rejected — shared-mutex finding sink for verse-level parallelism

Date: 2026-07-21. Status: **rejected on reasoning** (never spiked — and the
scenario it lives in doesn't exist yet, which is the second strike). Origin:
the 2026-07-18 wire-format/interning brainstorm (dissolved doc), adjudicated
in the 2026-07-21 triage.

## What was considered

*If* rule stats ever became order-independent so work could parallelize down
to verse level (the Road 2 rework in
`../2026-07-21-chapter-granularity-invalidation.md`), two shapes for
collecting findings were compared:

- (a) rayon `par_iter().fold(...).reduce(...)` — each work unit builds a
  small local list, lists glue pairwise as work completes; rayon's adaptive
  splitting means low hundreds of merge points for a whole Bible, so merge
  cost scales with split count, not verse count.
- (b) a shared `Arc<Mutex<Vec<Finding>>>` every thread pushes into — even
  locking once per verse is ~31,000 lock/unlock pairs through one contended
  point, and contention specifically *worsens* when findings cluster (as
  they do in real noisy corpora), not just when they're frequent on average.

## Why rejected

1. **(b) loses to (a) on structure, not tuning.** (a)'s shared-resource cost
   scales with core count/split depth; (b)'s scales with verse count and
   degrades under exactly the clustered distributions real corpora produce.
   There's no parameter regime where the mutex shape wins.
2. **The premise is doubly hypothetical.** It requires the order-independent
   stats rework (Road 2 — unscheduled, engine-rework-scale), and its benefit
   is native-desktop-only: wasm has no rayon threads, and the threaded-rayon
   wasm spike was already tried and rejected (`galley-resident-handle`
   branch tip). Serial reduce is also already cheap enough that sub-book
   parallelism has no current customer.

## What survives

One pinned design line, recorded in the chapter-granularity idea: **if the
Road 2 rework ever happens and sub-book parallelism is built, collect via
fold/reduce, never a shared sink.** The detailed mock methodology (synthetic
31,102-verse data, real per-verse finding densities from the oracle TSVs,
2/4/8-thread sweep, uniform-vs-clustered shapes) is preserved in this file's
origin doc via git history (`doubtful/2026-07-18-wire-format-and-interning-followups.md`,
deleted 2026-07-21) should the bench ever be worth running to confirm the
already-confident prediction.
