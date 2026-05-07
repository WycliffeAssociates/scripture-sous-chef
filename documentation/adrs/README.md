# Architecture Decision Records

Short records of non-obvious decisions: what we chose, why, what we
considered, and what it forecloses. Each ADR is dated and immutable
once accepted; changes happen by writing a new ADR that supersedes
the old one (link both directions).

ADRs are for decisions a future reader (you, in six months) would
reasonably second-guess without the context that made the decision
obvious at the time. Don't write an ADR for "use Rust" or "use the
existing BK-tree module" — those are derivable from the codebase.
Do write one for "Noisy-OR factors stay plain in Phase A even though
the architecture supports sub-cluster routing," because the choice
isn't visible from the code alone.

## Index

| #    | Date       | Title                                                              | Status   |
| ---- | ---------- | ------------------------------------------------------------------ | -------- |
| 0001 | 2026-05-07 | [Lane separation: per-token, verse, family](0001-lane-separation.md) | Accepted |
| 0002 | 2026-05-07 | [Phase A keeps factors plain; sub-clusters deferred](0002-plain-factors-phase-a.md) | Accepted |
| 0003 | 2026-05-07 | [Source co-rarity abstain: drop from product, not 0.7](0003-source-corarity-abstain.md) | Accepted |
| 0004 | 2026-05-07 | [char_ngram_backoff: one factor, bigram+trigram, no 4-grams](0004-ngram-backoff-one-factor.md) | Accepted |
| 0005 | 2026-05-07 | [Verse-NCD source mirror: arithmetic subtraction](0005-ncd-source-mirror-subtraction.md) | Accepted |
| 0006 | 2026-05-07 | [Verse length bucketing: graphemes, empirical quintiles](0006-verse-length-quintiles.md) | Accepted |
| 0007 | 2026-05-07 | [Source proper-noun match via BK-tree edit-distance](0007-source-proper-noun-bktree.md) | Accepted |
| 0008 | 2026-05-07 | [Multi-provenance surfacing: one verse entry, lanes in metadata](0008-multi-provenance-surfacing.md) | Accepted |

## Format

Each ADR has six fields:

- **Date** — when accepted
- **Status** — Proposed / Accepted / Superseded (by NNNN) / Rejected
- **Context** — what problem we're solving and what's true at decision time
- **Decision** — the choice, in one or two sentences
- **Rationale** — why this and not the alternatives
- **Consequences** — what becomes easy, what becomes hard, what's foreclosed

When superseding, the new ADR cites the old; the old's status changes
to "Superseded by NNNN" with a link.
