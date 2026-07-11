# scripture-sous-chef — agent notes

## Domain invariants (get these right)

### Verse markers are reference plumbing, not discourse structure

Verses are a versification/addressing scheme laid over the text. Discourse —
sentences, quotations, punctuation state — flows freely across verse
boundaries. Therefore:

- A verse start is **not** a sentence start. Never treat verse-initial
  position as "position-forced" for casing, and never reset
  sentence/punctuation/quote state at a verse seam.
- The **book** is the real discourse unit. Sentence state resets at book
  boundaries only, and the book is the parallel-walk unit (ADR 0042).
- The one legitimate seam effect is glyph adjacency: a terminal at the start
  of verse N is not "attached" to the last letter of verse N−1 (see
  `walk_book` in `crates/core/src/signals/casing.rs`, which carries its
  pending-terminal state *across* verse seams for exactly this reason).

Agents repeatedly assume verse-initial ≈ sentence-initial. In this codebase
that assumption is wrong every time.

## Oracle-gated engine rework (mandatory for structural changes)

Any change to how the engine *executes* — walk fusion, phase restructuring,
data-shape swaps, statistical-kernel replacements — is gated by the finding
oracle, not by unit tests (tests get rewritten during such work and cannot
be the referee):

1. **Before touching anything**, dump deterministic findings over the full
   vref fleet with `calibrate --dump-findings` (both configs: v1 defaults
   and everything-on) and `--dump-incremental` (echo + snapshot + stats
   digest over a fixed mutation). These dumps are the behavior contract.
2. **Gate every step**: re-dump and diff; byte-identical or the step does
   not land. Commit per gated step.
3. **Intentional behavior changes are not perf work**: they get their own
   ADR recording the measured drift (counts, max delta, samples), user
   adjudication, and a re-pinned oracle — see ADR 0059 for the template.

This pattern rewrote the engine to the event stream, re-keyed rule
internals, and swapped Fisher for G² across 2026-07-10/11 with zero
unadjudicated behavioral movement. It is the precondition for any future
rework of comparable reach.

## Feature routing: rules first, census adopts later

New check ideas start in **statistics mode** — a scored, convention-learned
rule (the `StatefulRule` shape, calibrated on the fleet). The census /
inventory report (absolute mode) is never the primary implementation of an
error-shaped check: rules judge, the census counts. A census lane appears
either by mirroring a shipped rule's extractor or because triage explicitly
adjudicated the item as house-style/census-only. Anything that would need a
threshold to be useful belongs in a rule — the census stays knob-free.
