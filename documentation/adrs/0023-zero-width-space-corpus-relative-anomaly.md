# ADR 0023: U+200B is orthography-dependent — a corpus-relative anomaly, not hygiene

- **Date:** 2026-07-01
- **Status:** Accepted
- **Builds on:** [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (pure core),
  [ADR 0017](0017-stateful-rules-stats-returning-analyze.md) (reduce/merge/judge),
  [ADR 0012](0012-ruleid-closed-enum-config-surface.md) (closed unions),
  [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md) (segmenter).
- **Amends:** the hygiene rule's treatment of U+200B only.

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
2. **A new stateful rule `uni.zero-width-space-anomaly`** (ADR 0017 shape)
   scores each U+200B's *conformance surprise* at `Severity::Info` with a
   continuous `score ∈ [0, 1]`. It ships **default-disabled** until the
   calibration note freezes its knobs; graduation to default-on is a separate,
   deliberate decision.
3. **Two learned factors, multiplied.** `reduce` counts, per book: boundary
   opportunities `N` (inter-grapheme positions, both verse edges included), ZWSP
   total `Z`, and per-ordered-grapheme-context counts `C(ctx)`. `judge` sums
   over books and composes
   `evidence = 1 - global_strength(Z, N) · context_strength(C(ctx), Z)`, where
   `strength` is the Wilson lower bound of the rate over the convention rate,
   clamped (`crate::shrinkage`).
4. **The global factor is a low "uses-ZWSP-at-all" gate, not a "uses-it-heavily"
   measure.** Both factors must be high to suppress, so `global_convention_rate`
   is calibrated *low*: any corpus that uses ZWSP at a steady rate saturates it
   and cedes discrimination to the per-context factor, while a corpus with
   essentially no ZWSP keeps the global factor near zero and surfaces the lone
   occurrence.
5. **Context = the ordered `(left, right)` grapheme classes** around the ZWSP:
   the first script-bearing scalar of each neighbour (so a trailing combining
   mark can't hide its base script), else a category (whitespace / punctuation /
   symbol / numeric / another ZWSP / other), and `Boundary` at a verse edge.
   Untracked scripts collapse to `Other`; `ScriptTag` is **not** expanded for
   this rule.

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
- **Judge is not O(total ZWSP).** `judge` aggregates from per-book per-context
  *counts* and floor-gates before touching sites, so it is
  O(books·contexts + emitted sites). A suppressed common context contributes one
  count and its sites are never iterated — so the O(project) recompute worry does
  not materialise, and no change to the `StatefulRule` contract is needed.

## Consequences

- The 330k/17k hygiene ZWSP storms on ZWSP-using corpora go to zero; a conforming
  corpus's dominant contexts fall below the emission floor and serialise no
  findings.
- **All sites are stored; emission is complete (no site cap).** An earlier
  revision capped retained sites per context per book, but review showed that is
  lossy: a context that is *frequent in absolute count yet rare relative to its
  denominator* clears the floor and must emit every occurrence, so a cap silently
  drops valid findings (the "common ⇒ never surfaces" premise is false). We
  therefore store every site and emit for every occurrence above the floor —
  `judge` aggregates from `sites.len()` and floor-gates before iterating, so its
  *cost* is still O(books·contexts + emitted sites). The consequence is **wire
  size**: a ZWSP-pervasive corpus with the rule enabled serialises one site per
  occurrence (~12 MiB of `Stats` on km_ulb). This is **unpaid on shipped
  defaults** (the rule is default-off) but is a real **graduation gate**: the
  sanctioned non-lossy fix — deferred until graduation actually needs it — is a
  `FindingArgs` "bounded sample + true count" shape (like `BracketWindow`),
  **not** a lossy per-site cap and **not** a `StatefulRule` contract change.
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
