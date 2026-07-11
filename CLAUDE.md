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

## Feature routing: rules first, census adopts later

New check ideas start in **statistics mode** — a scored, convention-learned
rule (the `StatefulRule` shape, calibrated on the fleet). The census /
inventory report (absolute mode) is never the primary implementation of an
error-shaped check: rules judge, the census counts. A census lane appears
either by mirroring a shipped rule's extractor or because triage explicitly
adjudicated the item as house-style/census-only. Anything that would need a
threshold to be useful belongs in a rule — the census stays knob-free.
