# Doubtful — 60fps "dream list" items still open

Date: 2026-07-17. Status: **not buried, not committed to** — plausible
ideas from the same "ignore everything we've built, what would get this to
60fps" exercise that didn't get rejected but also haven't been spiked or
scheduled. See `documentation/ideas/rejected/2026-07-17-60fps-dream-list.md`
for the items from the same exercise that *were* rejected. The two spikes
that ran from the same exercise are both closed: the word-break fast path
**landed** (ADR 0064), and the fold cache was rejected
(`rejected/2026-07-17-fold-cache.md`).

## SIMD as an ASCII/common-case fast-reject prefilter

Not "vectorize the binary search" (rejected — see the rejected doc; binary
search doesn't vectorize cleanly, and the target line is already small).
The narrower, still-plausible version: since `ALetter` + `WSegSpace` alone
cover ~85% of all scalars fleet-wide (per the word-break survey), a cheap
vectorized check — "is this whole chunk of bytes an ordinary ASCII
letter/space run, skip the real table lookup entirely" — could handle the
bulk of scalars, falling to the per-scalar lookup only for the minority that
need it. Never spiked. Given tape-build's overall small absolute share of
per-verse cost, this is a low-priority "someday, if curious" item (the
word-break work it once queued behind has since landed, ADR 0064 — that
changes nothing about this item's low priority).

## Stream-state checkpointing — promoted out of this doc (2026-07-21)

The checkpointing idea (snapshot stream-order listener state at seam
addresses; replay from an edit only until state re-converges) grew into its
own proposal with options:
`../2026-07-21-chapter-granularity-invalidation.md` — "Road 1"
(entry-state-keyed chapter memoization) is this item's direct descendant,
and that doc also carries the still-unverified judge-cost-vs-vocabulary
question this section flagged.

## xxh3 vs. rapidhash, benchmarked on our actual key shapes

Raised alongside the casing.rs `FxHashMap` switch: `xxh3` is already used
correctly (2 call sites, both whole-buffer content-fingerprinting — book
content hash in `PrepCache`, the enabled-rule-set fingerprint), and
`FxHashMap` is the established internal-hot-path-map hasher (ADR 0057, now
also used in casing's trust-building maps). Whether `rapidhash` would beat
either on this codebase's actual workload (short-to-medium word-string keys
for casing's maps, whole-book buffers for content-hashing) is genuinely
unresolved — external benchmarks for these ultra-fast hashers are
notoriously sensitive to input-size distribution and CPU microarchitecture,
so a README win elsewhere doesn't necessarily transfer here. Cheap to
resolve with a targeted criterion bench on real key shapes; not done yet.
(The word-break work it once queued behind landed as ADR 0064.)
