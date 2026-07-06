# ADR 0023: U+200B is orthography-dependent — a corpus-relative anomaly, not hygiene

- **Date:** 2026-07-01
- **Status:** Accepted
- **Builds on:** [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (pure core),
  [ADR 0012](0012-ruleid-closed-enum-config-surface.md) (closed unions),
  [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md) (segmenter).
- **Note:** an earlier revision built this as a stateful rule (ADR 0017); it is
  now a stateless project rule (see Decision 2 / Consequences).
- **Amends:** the hygiene rule's treatment of U+200B only.
- **Amended by:** [ADR 0025](0025-drop-joiner-flagging-from-hygiene.md) —
  Decision 1's parenthetical (hygiene keeps "the script-aware ZWNJ/ZWJ") no
  longer holds; hygiene now skips the joiners too.

## Context

`hyg.zero-width-misuse` treated U+200B ZERO WIDTH SPACE as universally invalid.
That is wrong: ZWSP is a legitimate, orthography-dependent word/line-break aid
in Khmer, Lao, Thai and Myanmar (which lack inter-word spaces), and optional but
real in Japanese. Treating it as an error produced hundreds of thousands of
false findings in exactly the corpora that use it correctly (one per word). A
fixed predicate cannot separate a convention from a slip; only the corpus can.

But an *isolated* ZWSP in a corpus that otherwise never uses it, or a ZWSP in a
context (script pair) unlike the corpus's usual one, is still worth a reviewer's
glance. So the signal is real — it just isn't a hygiene assertion.

## Decision

1. **Hygiene no longer judges U+200B.** `hyg.zero-width-misuse` skips it and
   keeps flagging every other zero-width/bidi/format control (BOM, WJ, LRM/RLM,
   embeddings/overrides, and the script-aware ZWNJ/ZWJ). The broad format
   predicate is redocumented as a *candidate* identifier whose callers decide
   legitimacy — not an "always invalid" predicate.
2. **A new `uni.zero-width-space-anomaly` rule** scores each U+200B's
   *conformance surprise* at `Severity::Info` with a continuous `score ∈ [0, 1]`.
   It ships **default-disabled** until the calibration note freezes its knobs;
   graduation to default-on is a separate, deliberate decision. It is a
   **stateless project rule** — computed over the supplied map in one pass,
   holding nothing between calls (see Consequences for why not stateful).
3. **Two learned factors, multiplied.** In one pass over the map it counts:
   boundary opportunities `N` (inter-grapheme positions, both verse edges
   included), ZWSP total `Z`, and per-ordered-context counts `C(ctx)`, then
   composes
   `evidence = 1 - global_strength(Z, N) · context_strength(C(ctx), Z)`, where
   `strength` is the Wilson lower bound of the rate over the convention rate,
   clamped (`crate::shrinkage`). The corpus scope *is* the supplied map.
4. **The global factor is a low "uses-ZWSP-at-all" gate, not a "uses-it-heavily"
   measure.** Both factors must be high to suppress, so `global_convention_rate`
   is calibrated *low*: any corpus that uses ZWSP at a steady rate saturates it
   and cedes discrimination to the per-context factor, while a corpus with
   essentially no ZWSP keeps the global factor near zero and surfaces the lone
   occurrence.
5. **Context = the ordered `(left, right)` neighbour kinds** around the ZWSP,
   coarse by design: a **letter** carries its *full* Unicode `Script` (Latin ≠
   Khmer ≠ Han — so "ZWSP in the wrong script" is a distinct, rare context);
   everything else collapses to `Whitespace` (redundant-separator shape),
   `ZeroWidthControl` (an adjacent standalone zero-width char — the doubled-ZWSP
   shape), or `OtherNonLetter` (punctuation / symbol / digit); a verse edge is
   `Boundary`. **No look-through** — immediate adjacency is used, so a
   `Khmer ZWSP SPACE Khmer` sequence stays `(Khmer, Whitespace)` rather than
   being laundered into `(Khmer, Khmer)`. Full `Script` is read directly from
   `unicode_script` on the (rare) ZWSP neighbours — no fused-table change, and
   no curated script list (so untracked scripts are still distinguished).

## Rationale

- **Multiplicative composition, not "context alone suppresses."** We rejected
  letting a strongly-conventional context suppress regardless of global
  prevalence: a one-off ZWSP in an otherwise ZWSP-free corpus is 100% of that
  corpus's ZWSP, so its lone context looks maximally "typical" — context-alone
  suppression would silence precisely the anomaly we want (user story 4). Global
  prevalence *must* gate. The moderate-global worry (an optional-use language
  stuck at moderate Info) is a *calibration* concern about the gate's height,
  resolved by keeping it low — not a reason to change the formula.
- **The confidence lower bound `z`, not the rate knobs, is load-bearing at the
  anomaly end.** A context seen once or twice against a large `Z` scores high
  because its conservative rate ≈ 0 regardless of `context_convention_rate`; the
  rate knob only sets "how small a share still counts as established." Calibrate
  `z` against the small-count cases first. This also makes context fragmentation
  (splitting one convention across ordered script contexts) low-risk: a
  fragmented-but-real context still carries enough count for a non-trivial bound.
- **Monotonicity is stated over realizable edits.** `strength` is monotone in
  `k` (up) and `n` (down); composed, adding an occurrence of an *existing*
  context lowers that context's evidence (both factors rise), and a *new* rare
  context scores high. Evidence is **not** monotone in raw `Z` for a fixed
  context — global familiarity rises while that context's share falls, an
  intentional tradeoff — so no test or claim asserts that.
- **Computed in one pass, no state (start simple).** An earlier revision made
  this stateful (ADR 0017) and cached per-occurrence sites, but the site array
  dominated the wire (~12 MiB on km_ulb) and no consumer exercises
  incrementality yet. It is now a **stateless project rule**: aggregate the map,
  score, emit — nothing cached. When it graduates it will move to the
  aggregate-only stateful shape `punct.adjacency-anomaly` uses (cache tiny
  per-book counts, re-scan `target` to emit), which is where the `Script`
  serialisation cost lands; stateless lets us defer that.

## Consequences

- The 330k/17k hygiene ZWSP storms on ZWSP-using corpora go to zero; a conforming
  corpus's dominant contexts fall below the emission floor and serialise no
  findings.
- **No `Stats`, no wire cost** — being stateless, the rule serialises nothing,
  so the ~12 MiB site-payload problem an earlier (stateful-with-sites) revision
  had simply does not exist here. The tradeoff is the **incremental carve-out**
  below.
- **Incremental carve-out (a real contract note).** Because it scores over the
  map it is handed, an incremental `analyze_stateful` call that supplies only an
  edited book would score ZWSP *book-locally*, not corpus-wide — so
  **consumers must pass the full corpus when this rule is enabled.** ADR 0017's
  incremental guarantee is explicitly *not* extended to it yet. This is
  acceptable because it is default-off/experimental; graduation resolves it by
  moving to aggregate-only stateful (per-book counts cached, `target` re-scanned
  at emit — the shape `punct.adjacency-anomaly` already uses), which restores the
  guarantee without caching sites.
- **Limitations (stated so they are known, not surprises):**
  - `boundary_opportunities` counts both verse edges, so the global rate is
    *per-position-including-edges*; for many-short-verse corpora the edges dilute
    the raw rate. Harmless, but `global_convention_rate` is calibrated on this
    same basis.
  - The rule keeps **no** hardcoded script allow-list. The asymmetry with
    `punct.adjacency-anomaly` (which keeps `...`/`--`/`?!` exclusions) is *not*
    principled: it is that punctuation ships default-on and needs conservative
    training wheels now, while this rule ships default-off pending calibration.
  - Corpus counts cannot distinguish a systematic *misuse* from a convention;
    both go silent when common. This never rises to error/hygiene semantics.
