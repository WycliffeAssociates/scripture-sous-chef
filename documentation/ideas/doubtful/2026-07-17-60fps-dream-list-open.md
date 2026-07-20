# Doubtful — 60fps "dream list" items still open

Date: 2026-07-17. Status: **not buried, not committed to** — plausible
ideas from the same "ignore everything we've built, what would get this to
60fps" exercise that didn't get rejected but also haven't been spiked or
scheduled. See `documentation/ideas/rejected/2026-07-17-60fps-dream-list.md`
for the items from the same exercise that *were* rejected, and the two live
spikes in `documentation/calibration/` for the ones actively in progress
(word-break fast path) or already resolved (fold cache — rejected, folded
into that doc).

## SIMD as an ASCII/common-case fast-reject prefilter

Not "vectorize the binary search" (rejected — see the rejected doc; binary
search doesn't vectorize cleanly, and the target line is already small).
The narrower, still-plausible version: since `ALetter` + `WSegSpace` alone
cover ~85% of all scalars fleet-wide (per the word-break survey), a cheap
vectorized check — "is this whole chunk of bytes an ordinary ASCII
letter/space run, skip the real table lookup entirely" — could handle the
bulk of scalars, falling to the per-scalar lookup only for the minority that
need it. Never spiked. Given tape-build's overall small absolute share of
per-verse cost, this is a low-priority "someday, if curious" item, not
something worth chasing ahead of the word-break work.

## Stream-state checkpointing to bound book re-walk to the affected suffix

Today, editing anything in a book forces a full re-walk of that book from
verse 1 — every listener, full substrate rebuild — because `PrepCache`
invalidates at whole-book granularity (ADR 0060). Listeners that carry
state across verse seams within a book (casing's pending terminal,
bracket-balance's LIFO stack, duplicate-word's tail, rare-glyph's
forced-position machine, spacing's cross-seam neighbour classes — the
"stream-order" rule class, per `stream.rs`'s own module doc) are, in
principle, checkpointable: snapshot each such listener's state at every
verse boundary, and re-walking from an edit only needs to replay forward
until the newly-computed state re-converges with what was checkpointed
before the edit. Worst case is still bounded by book length (state resets
at book boundaries, never crosses them — repo `CLAUDE.md`), but the common
case (most stream-order state resets frequently — a pending-terminal flag
at most sentence boundaries, a bracket stack usually empty between
well-formed pairs) could be much cheaper than "replay to the end of the
book," let alone "replay the whole book from verse 1."

This does **not** need to also solve corpus-wide statistical verdicts
(Wilson-interval/proportionality/majority-tally rules, where an edit
anywhere can in principle flip a verdict anywhere else) — that concern was
raised and then walked back in the same conversation: `analyze_stateful`
already separates the expensive, book-scoped **reduce** phase from a
**judge** phase that is *already global on every call*, comparing
already-computed tallies against thresholds rather than re-walking text.
That's cheap because it scales with vocabulary/tally size, not corpus size
in characters, and it's what already handles "a change anywhere can affect
a verdict anywhere" — checkpointing stream-order state would be a pure
optimization *on reduce*, orthogonal to judge, not blocked by it. The one
thing worth confirming before trusting that reasoning at scale: whether
judge's cost genuinely stays cheap as vocabulary size grows across the
fleet, or whether it could itself become a bottleneck somewhere — not
verified either way.

Never spiked; a real candidate for a future engine-rework-scale
investigation (would need the full oracle-gate discipline per `CLAUDE.md`,
since it changes how the engine executes), not a small change.

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
resolve with a targeted criterion bench on real key shapes; not done yet,
not prioritized ahead of the word-break work.
