# Rejected — 60fps "dream list" items

Date: 2026-07-17. Status: **rejected** (or, for one entry, out of this
repo's scope). Origin: a conversational exercise — "ignore everything we've
built, what would get this to run at 60fps" — deliberately unconstrained,
most of it never spiked in code. See `documentation/ideas/doubtful/` for the
two items from the same exercise that are still open.

## Single fused automaton merging grapheme-break + word-break ("4-radical")

**Claim considered:** instead of two independent boundary walks (the
existing grapheme walk, plus a hypothetical word-break walk, both reading
the same per-scalar `Class` tape) sharing classification but running as
separate passes, merge them into one automaton emitting both boundary types
from a single traversal — justified by word boundaries always being a
*coarser* refinement of grapheme boundaries (a word boundary is never placed
mid-cluster).

**Rejected because the trade is bad, not because it's impossible.** The
thing merging saves is a second linear pass over the tape — and the tape is
small (a `TapeEntry` is 12 bytes; even a long 200-char verse is ~2.4 KB,
comfortably inside L1 cache) and already reused/hot across a book's verses,
so a second pass over already-cache-resident data is close to free. The
thing merging costs is a joint automaton whose combined state space
(grapheme-state × word-state) needs its own correctness proof against *both*
conformance suites simultaneously — strictly harder than two independently-
provable walks that share no mutable state. Saving a near-free second pass
in exchange for a materially harder correctness argument is a bad trade.
Confirmed conversationally, not by an actual benchmark of "walk twice vs.
walk once over a resident tape" — that specific micro-question was never
spiked (see the doubtful doc's note on this), but the reasoning didn't
depend on getting that number exactly right: even a measurable-but-small
difference wouldn't flip the conclusion given how much harder the merged
automaton is to prove correct.

Note: the *non*-radical version of this idea ("one classify, two
independent consumers" — `4-modest`) isn't rejected and isn't a separate
open item either — it turned out to be identical to whatever the word-break
fast-path work already is, once that spike found zero new `Class` bits were
needed. There's nothing left to file separately; it was subsumed by the
word-break investigation
(`documentation/calibration/2026-07-17-word-break-fast-path-survey.md`),
which has since landed as ADR 0064.

## GPU / WebGPU compute-shader classification

**Claim considered:** upload the corpus's scalar stream to a GPU buffer once
and classify codepoints in parallel via a compute shader.

**Rejected on data-scale grounds, never spiked.** A whole Bible is ~4 MB of
text. Data-transfer overhead to and from the GPU would almost certainly
dwarf any compute time saved at this scale — this isn't a workload with
enough raw throughput need to justify the round-trip. Named in the original
brainstorm as the most "wild" idea, explicitly not because it looked
promising.

## Statistical pre-filtering before the deterministic pipeline

**Claim considered:** a cheap bloom-filter/learned-classifier gate that
predicts "this verse almost certainly has no findings" and skips the full
deterministic rule pipeline for the predicted-clean majority — the trick
real-world spell-checkers use (bloom-filter a "known good" word list, only
run the expensive check on a miss).

**Rejected on philosophical grounds, not perf grounds.** This project's
foundational commitment (repo `CLAUDE.md`, the oracle-gated engine-rework
discipline, "the census stays knob-free") is full determinism and
explainability — every finding traceable to a deterministic rule, no
statistical short-circuit deciding what never gets checked. A pre-filter
that skips analysis based on a probabilistic guess is a different paradigm
than anything else in this codebase and would cost something real
(occasional false-negative skips) that the project isn't set up to accept
implicitly. Not spiked, and not worth spiking unless that foundational
stance changes.

## SIMD-vectorized binary search over the classification table

**Claim considered:** vectorize `class_of`'s table lookup (currently a
binary search over the generated range table) to speed up the tape-build
stage.

**Rejected on two independent grounds.** First, a correction from a prior
conversation with another agent: binary search is a sequence of
*data-dependent* branches, which is exactly the shape SIMD doesn't help
with directly — you can't vectorize a single binary search the way you'd
vectorize a uniform per-byte comparison. Second, even if it worked, tape
build is already the *smallest* line in the per-verse cost breakdown
(0.37-0.48 µs/verse against several µs for tokenization), so a large
*relative* win there would still be a small *absolute* one. The narrower,
still-plausible version of this idea (a cheap ASCII/common-case fast-reject
prefilter, not a vectorized binary search) is filed as open, not rejected —
see the doubtful doc.

## Decoupling typing latency from analysis completeness (LSP-style background re-analysis)

**Claim considered:** show stale-but-instant findings immediately, run the
real analysis pass in the background, converge within ~100-300ms — the
pattern rust-analyzer/tsserver use to stay responsive on large codebases
instead of blocking a keystroke on a full resync.

**Out of this repo's scope, not rejected on merit.** This is a client/editor
scheduling concern — which process runs when, what the UI shows while
waiting — not something `ssc-core`'s analysis engine has any say over. Noted
here so it doesn't get re-proposed as an `ssc-core` change; if it's worth
doing, it belongs in the editor repository's own architecture discussion.
