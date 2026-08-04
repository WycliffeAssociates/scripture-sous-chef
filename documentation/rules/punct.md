# `punct.*` — Punctuation integrity

The `punct.` namespace now holds exactly one rule: `bracket-balance`, the
corpus-relative book-stream matcher in `bracket_balance.rs`.

`punct.adjacency-anomaly` and `punct.spacing-anomaly` were both **retired** and
absorbed into `uni.nonletter-usage-anomaly`. Their extractors survive in
`punctuation.rs` with no rule of their own, read by the census's `punct.runs` and
`punct.mark-spacing` lanes: the adjacent-punctuation run walk with its
per-lead-glyph opportunity count, and the per-mark per-side class-conditioned
attached/spaced walk. Nothing in that file judges any more.

---

## `punct.bracket-balance` — unbalanced paired brackets

> **Severity** Info · **Default** on · **Scope** substrate-backed (book stream, corpus-relative verdicts) · **Knobs** `window_verses` (default 16), `confidence_z` (default 1.96), `emit_score_min` (default 0.5) · **Source** `bracket_balance.rs` · **ADR** 0016, 0037

**Flags** — Bracket events that don't fit the corpus's own pairing behaviour,
matched with a LIFO stack across the **whole book stream** (verses in
canonical order, no distance cutoff — verses anchor findings, they never
bound analysis). The inventory is the UCD `BidiBrackets.txt` pairs plus a
documented supplement (U+FD3E/FD3F ornate parens, which pair as text brackets
but are excluded from BidiBrackets on a bidi-mirroring technicality) — so
`﴾﴿`, CJK corners `「」`, fullwidth `（）`, and Tibetan `༺༻` get balance
checking alongside ASCII `()[]{}`. Each finding anchors the offending
delimiter, carries a `score`, and carries the delimiter inventory within
`window_verses` of the anchor (`FindingArgs::BracketWindow`) so a reviewer
sees the whole bracket context.
- stray closer: `…then a stray) closer` — scored by its family's pairing dominance
- opener never closed in the book → flagged at the opener
- crossed nesting: `a ([b) c]` → both crossed events surface
- a matched pair spanning more than `window_verses` verses, in a corpus whose
  pairs are otherwise short → flagged at the opener

**Clean** — `a (b [c] {d}) e`; an aside opened in v1 and closed in v3; a
25-verse speech paren in a corpus (kmr-IQ, ayn) where long spans are routine
— the long-span verdict learns that as the corpus's own convention; gux_reg's
`]`-used-as-a-letter (a legacy font-hack orthography: `ku ]inbiagu`) — the
family's pairing dominance is ~0, so every event self-suppresses.

**Why it matters** — A missing bracket can change meaning — especially the
editorial `[ ]` that mark disputed text. But a deterministic matcher assumes
a universal bracket identity, and ADR 0016's version demonstrated three
failures of that assumption: gux's letter-`]` (376 false findings), kmr/ayn
speech parens longer than the window (both halves orphaned by the
circuit-breaker), and total silence on non-ASCII pairs. The corpus itself
knows which glyphs it pairs and how far; the verdicts now read that.

**Verdict model (ADR 0037)** — Two corpus-relative verdicts, both
`evidence::dominance` (Wilson lower bound), judged per open-glyph family:
- An **orphan** (unmatched or crossed event) scores the family's corpus-wide
  *pairing dominance* — `dominance(matched_events, events, z)`. A corpus
  that pairs `(` 99.9% of the time makes a stray `(` a ~0.99 finding; a
  never-paired glyph scores ~0 and is silent.
- A **matched pair spanning more than `window_verses`** scores the family's
  *short-span dominance* — `dominance(short_pairs, pairs, z)`, anchored at
  the opener. A 25-verse `(…)` in a corpus of short pairs surfaces; a corpus
  of routinely-long spans establishes that as its convention and stays
  silent.

Findings are emitted at or above `emit_score_min`.

**Config** — `window_verses` (u16, default **16**) is no longer a matching
circuit-breaker: it is the **long-span bar** for the second verdict and the
reported-inventory radius. The default clears the longest legitimate
editorial brackets with margin — the *pericope adulterae* (JHN 7:53–8:11)
and the longer ending of Mark (MRK 16:9–20) run 11–12 verses.
`confidence_z` / `emit_score_min` are the suite-standard pair. Frozen
2026-07-06: the redesign took the 106-corpus survey from 1,114 findings to
579, with gux 376 → 0, kmr-IQ 126 → 89, ayn 78 → 70, no corpus rising, and a
sharply top-heavy score histogram (291 at 1.0, 35 at the floor).

**Nuance & ADR ties** — Quotes are **excluded**: they're direction-ambiguous,
and their book-scope balance is deferred (ADR 0011) — brackets are the
unambiguous warm-up for that matcher. Severity is **Info**, now with a score
(pre-0037 findings carried none). The reference corpus is irrelevant —
brackets are intrinsic to the target. The rule is a typed observation
substrate with ordered book-local delimiter state; its complete corpus
aggregate is retained residently and re-derived from dirty chapters. The
inventory is generated into `charclass_table.rs` (`BRACKET_PAIRS`) from a
committed trimmed `BidiBrackets.txt` by `cargo xtask gen-charclass-table`.

**Open issues / future work** — Quote balancing remains the deferred harder
sibling (ADR 0011). Known accepted residual (ADR 0037): a missing closer
plus a coincidental stray same-family closer can silently pair across a long
span; the long-span verdict recovers the short-pair-convention cases, and in
a corpus with no dominant span convention the cost is at most one missed
coincidence where the window design gave two uninspectable orphans. Its
per-family tallies are the resident substrate's aggregates; any future
quote-balance rule must declare its own boundary state and substrate contract.
