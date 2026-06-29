# ADR 0016: Bracket balance — book-scope, windowed, with a delimiter inventory

- **Date:** 2026-06-29
- **Status:** Accepted

## Context

`punct.bracket-balance` shipped in the deterministic batch ([ADR 0014](0014-deterministic-rule-batch.md))
as a `PerVerseRule`: a LIFO stack matcher born and discarded inside each
verse. Brackets, like quotes, legitimately span verse boundaries —
a parenthetical aside opens in one verse and closes in another — so the
open half and the close half are each flagged as unbalanced when examined
alone.

Measuring en_ulb (footnote-stripped, i.e. what onion's vref projection
actually hands core) made the cost concrete:

- **Book-scope balance is perfect:** 0 unmatched openers, 0 stray closers
  across all 66 books.
- The per-verse rule produced **24 findings, every one a false positive** —
  12 genuine cross-verse asides × 2 halves each (the "~24" the old
  doc-comment noted, which is why it was `Info`, not `Warning`).
- Real prose asides span **1–3 verses**. But the ULB also wraps whole
  **disputed passages** in editorial `[ ]`: the *pericope adulterae*
  (JHN 7:53–8:11) and the longer ending of Mark (MRK 16:9–20) run
  **11–12 verses**. These set the real floor on any window, not the asides.

So on the cleanest corpus the rule had a 100% false-positive rate and zero
true findings. Brackets are also the **unambiguous** sibling of quotes
(`(` always opens, `)` always closes — no direction heuristic), which is
why [ADR 0011](0011-statefulness-incrementality-strategy.md) could defer
book-scope *quote* balance but not for a technical reason that applies
here. Brackets are the warm-up for that engine.

## Decision

1. **Match at book scope, not verse scope.** `BracketBalance` becomes a
   `ProjectRule` (the second cross-map rule, after proportionality). It
   groups the `VerseMap` by `BookId` and walks each book's verses in
   canonical order — free, because `VerseMap` is a `BTreeMap<Sid, _>`
   ordered by `(book, chapter, verse)`. The stack carries `(glyph, sid,
   offset, verse-index)`; it resets at each book boundary. Quotes stay
   excluded.

2. **A `window_verses` circuit-breaker, default 16.** An opener unmatched
   for more than the window is reported as an orphan and **dropped**, so
   one missing closer can't mis-pair with every later bracket in the book
   (bounding blast radius — the reason for the window, *not* aside
   detection). The default is set by the disputed-passage brackets
   (12 verses), not the prose asides (≤3); 16 clears them with margin.
   It is a config knob (`BracketBalanceConfig`), like proportionality's.

3. **Each finding carries the window's full delimiter inventory.** A new
   `FindingArgs::BracketWindow { window: Vec<DelimObservation> }` lists
   every delimiter seen within ±`window_verses` of the orphan — its `sid`
   (canonical string), glyph, open/close role, and matched/unmatched
   status. A reviewer sees the whole bracket context and decides what is
   actually missing, rather than staring at the lone orphan. The orphan's
   own precise location stays on the `Finding.range` (byte offsets,
   UTF-16-projected at the wasm boundary as usual).

4. **`Finding` and `FindingArgs` drop `Copy`.** The inventory owns a `Vec`.
   Findings are only ever collected into `Vec`s and never copied on a hot
   path, so this costs nothing real; the call sites that relied on `Copy`
   (a destructure in the proportionality test, the wasm boundary, the
   calibrate example) bind by reference or `clone()`.

5. **Severity stays `Info`.** The per-verse `Info` level existed to soften
   the aside false positives this change eliminates; promoting to `Warning`
   waits until the rule has run against more corpora than en_ulb.

## Consequences

- **en_ulb: 0 bracket-balance findings** (was 24). At book scope the rule
  now surfaces *real* signal elsewhere — e.g. es-419_ulb's accented
  characters mangled into `[`+vowel (`pidi[o` for `pidió`), a genuine
  data-corruption catch the per-verse rule buried under aside noise.
- The pure per-verse `scan_bracket_balance` and its `PerVerseRule` impl are
  deleted (no compat shim — pre-alpha). The rule moves to its own module
  `signals/bracket_balance.rs`, mirroring `proportionality.rs`'s split as
  the first cross-map rule; `punctuation.rs` stays pure per-verse.
- `RuleId::BracketBalance` / `"punct.bracket-balance"` identity is
  unchanged — only the scope and payload change.
- The `FindingArgs` union and the `DelimObservation`/`DelimRole` types
  cross the wasm boundary via `Tsify`; consumers get a typed
  `bracket-window` variant on the closed `FindingArgs` union.
- Sets the template for book-scope **quote** balance (ADR 0011, deferred):
  same windowed walk, plus the direction heuristic quotes need and
  brackets don't.
