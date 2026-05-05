# research/

Two states for everything in here: **live** (drives current or upcoming
work) or **archived** (historical, may not reflect current code).

## Layout

- `papers/` — external PDFs by topic. Reference, never edited in place.
- `proposed/<dated-topic>/` — research briefs, syntheses, and decision
  notes for work the engine doesn't do yet but is being considered. New
  rounds get a date-prefixed folder so the timeline is obvious.
- `archived/<round-name>/` — finished or superseded artifacts. Kept for
  context, not for guidance.

Documentation that actually describes the engine as it is — file
formats, replay model, config — lives in `documentation/`, not here.

## Active

- `proposed/2026-05-05_unsupervised-morphology/` — current round's
  research brief on unsupervised morphological segmentation. Synthesis
  pending.
- `proposed/sil-audit/` — punctuation/script/edit-metric techniques
  catalogued from SIL's `silnlp`. Decision-deferred per round; some
  primitives already lifted (e.g. `crates/core/src/punctuation_class.rs`).

## Archived

- `archived/initial-refactor-5-5-2026/` — the round that produced the
  current statistical chassis (content-addressed identity, Noisy-OR
  aggregation, Fisher/Dunning, posterior store, compression-texture).
  Contains the agent reports that fed the synthesis, the synthesis
  itself, the implementation plan that drove the refactor, and the
  code review that closed it out.

## Conventions

- A file in `proposed/` is a candidate, not a commitment.
- Moving a file from `proposed/` to `archived/` means the round
  it represents has shipped or been set aside.
- `documentation/` and code comments are authoritative for "what the
  engine does today." Files here describe what we're considering or
  what we considered.
