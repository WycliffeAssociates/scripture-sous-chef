# ADR 0021: Domain-tailored grapheme segmenter (fast path + UAX-#29 fallback) over one fused static table

- **Date:** 2026-07-01
- **Status:** Accepted, **amended by [ADR 0022](0022-fused-table-category-and-script.md)**
  — the fused byte is widened `u16` → `u32` and gains General_Category-group
  bits + a script lane (flat table 128 KB → 256 KB). The segmenter, fast-path
  claim, and gates below are unchanged; only the table's width and contents grow.
- **Amends:** [ADR 0020](0020-char-classification-fused-classbits-table.md)
  (the per-analyze `ClassBits` trie — retired here in favour of one fused
  static table).
- **Builds on:** [ADR 0019](0019-shared-tokenization-and-per-char-cost.md)
  (shared per-analyze caches), [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (three-phase `analyze_stateful`), [ADR 0010](0010-pure-analyzer-contract-v1-reset.md)
  (pure-analyzer contract).

## Context

Grapheme-level rules are coming: repeated-character runs already need
user-perceived characters, and letter **n-gram / bigram rarity** ("how rare is
this pair of letters") is on the roadmap. Both count over **grapheme
clusters**, not scalars, and every finding must highlight a **whole cluster**.
The Rust-idiomatic answer is `unicode-segmentation`'s
`grapheme_indices(true)`, which casing already uses.

But that cursor is the residual hotspot after ADRs 0019/0020: on a full Bible
it is ~70–150 ms of general UAX-#29 machinery, most of it spent proving trivial
things about ASCII and simple base+mark text where grapheme boundaries are
obvious. A spike (`scratchpad/classify-spike`, modes `seg-verify` / `seg-bench`
/ `seg-cost`) measured a hand-rolled alternative against Unicode 17.0 (the
version `unicode-segmentation` 1.13.x implements):

- **Correctness — two gates, both green:**
  - **GraphemeBreakTest.txt: 766/766** UAX-#29 cases pass.
  - **Differential: 1,185/1,185 corpora** — our boundaries are byte-identical
    to `unicode-segmentation` on every verse we have.
- **Speed (segmentation only, incl. materializing spans):** **2.7–4.9×** the
  oracle walk across *every* script — Latin, Cyrillic, Ethiopic, Thai, Tamil,
  Devanagari, Malayalam, Myanmar, Khmer, CJK, Japanese.
- **Cost of the two structures (the crux for the architecture):**
  - the classification **table** is **fixed ~68 KB** (does not grow with the
    corpus); building it is a one-time cost (~0 when baked).
  - the **spans** are bulk: materializing every cluster of a full Bible is
    **22–34 MB** (11–17 MB packed) — but the largest single verse holds only
    **~1–5 KB** of spans.

### Why the fast path is safe

Every scalar carries grapheme-break bits derived from the UCD 17.0 property
files (`GraphemeBreakProperty`, `emoji-data`, `DerivedCoreProperties` for InCB):

- `EXTENDER` = GCB ∈ {Extend, SpacingMark, ZWJ} — glue to the previous cluster.
- `COMPLEX` = the cases the trivial rule cannot handle — GCB ∈ {Prepend,
  Control, CR, LF, Regional_Indicator, L, V, T, LV, LVT} ∪ Extended_Pictographic.

The **only** claim the fast path makes: *a non-`COMPLEX` base owns itself plus
its trailing `EXTENDER`s.* A `COMPLEX` char defers the whole cluster to
`unicode-segmentation` verbatim. Because no non-`COMPLEX` char can join
*forward* (every forward-joiner — Hangul jamo, RI, pictographic, Prepend — is
`COMPLEX`), breaking before a complex char after a simple base is always
correct. **GB9c (Indic conjuncts) is handled inline**: `InCB=Consonant` is a
normal fast base, and a ~8-line state machine continues the cluster across a
consonant only when a linker (virama) sat between them with only InCB marks in
the gap. This keeps Devanagari/Malayalam/Myanmar/Khmer on the fast path (the
conservative "route all consonants to fallback" variant regressed Khmer to
0.83×; inline GB9c took it to 3.76× with fallback dropping 85% → 13%).

**Domain tailoring, stated plainly:** scripture has ~zero emoji, zero flags,
and one astral char across all 1,185 corpora. We deliberately do **not**
optimize the emoji-ZWJ / Regional-Indicator paths — they fall to the oracle,
which is fine because they essentially never occur. The fast path is tuned for
the scripts scripture is actually written in.

### Why a static Unicode-property table is now unavoidable

ADR 0020 chose a **per-analyze** trie and explicitly rejected a resident static
table (the ~128 KB blob is download/RAM weight for web/mobile). That reasoning
rested on a property of casing: its bits (`is_alphabetic`, `is_lowercase`, …)
are **std predicates, computable per-char at build time from nothing**. So a
per-analyze trie could fill itself from `std` with no resident data.

Grapheme-break bits are **not** computable from `std` (no `InCB` /
`Extended_Pictographic` / GCB accessor exists) and `unicode-segmentation` does
not expose them. They can only come from **committed Unicode property data,
resident in the binary**. Once that data must be resident to segment at all,
the per-analyze trie's whole justification collapses — and keeping a *second*
per-analyze table for casing beside it is pure duplication.

## Decision

1. **Hand-roll a domain-tailored grapheme segmenter** in a self-contained
   `crates/core/src/grapheme.rs`: `segment(text, table, &mut Vec<GSpan>)`, the
   fast path above with inline GB9c, deferring `COMPLEX` clusters to
   `unicode-segmentation`. `unicode-segmentation` stays a dependency as **both
   the fallback and the correctness oracle** — worst case, everything routes to
   it and we are exactly as correct as today, only slower.

2. **Two permanent correctness gates.** A committed `#[test]` runs the
   UCD `GraphemeBreakTest.txt` conformance suite plus hand-built synthetic
   clusters (per our synthetic-tests rule). The **whole-corpus differential vs
   `unicode-segmentation`** stays a calibration/spike run (corpora are
   gitignored). Neither gate green ⇒ it does not ship. This is what licenses a
   hand-roll despite "prefer grapheme iteration over mark tables."

3. **Fuse casing + grapheme-break bits into one `Class(u16)` over a single
   static table, built once** — retiring ADR 0020's per-analyze trie. The table
   is expanded at first use (`OnceLock`) into a flat BMP array from a **compact
   committed range table** (generated offline from UCD 17.0 + std casing), so
   the `.wasm` download grows only by the ranges (~tens of KB), not a 128 KB
   blob; the flat table is a ~128 KB **heap** allocation for process life.
   Astral scalars (vanishingly rare) take a binary-searched range fallback.
   This removes the per-analyze classification rebuild entirely.

4. **Hoist the pass per verse, not per corpus.** In `analyze_stateful`'s
   per-verse phase, each verse is segmented **once** into a reused buffer and
   its `&[GSpan]` slice handed to every grapheme rule for that verse, then
   dropped. Peak span memory is O(largest verse) ≈ **~5 KB**, never the
   corpus-wide 22–34 MB. A stateful grapheme rule (bigram) segments in its own
   `reduce` pass the same way. The **table** is the only "up front" shared
   structure; the **spans** are transient.

5. **`GraphemeRule` trait** (per-verse: `check(text, &[GSpan]) -> Vec<Span>`),
   mirroring `TokenRule`/`CharClassRule`. **`casing` and
   `lex.repeated-character-run` migrate to consume the shared pass**; casing's
   own grapheme-cursor walk and ADR 0020's `CharClass` build both go away.

6. **Not stateful, not persisted.** The table is a process constant, not
   per-project `Stats`; the spans are transient (20–30 ms/Bible to rebuild —
   negligible). Neither enters the shell-held incremental `Stats` flow
   (ADR 0017). Incremental re-segmentation of only edited verses is a possible
   future optimization, explicitly out of scope here.

## Alternatives considered (and why not)

- **Keep `unicode-segmentation` everywhere (status quo):** the baseline and the
  fallback. 2.7–4.9× slower on every script; the residual hotspot 0019/0020
  surfaced. Kept as the safety net, not the hot path.
- **Retain ADR 0020's per-analyze fused trie (add grapheme bits to it):**
  preserves 0020's ~0-resident philosophy (compact ranges ~30 KB resident +
  a few-KB per-analyze trie). Viable, and the drop-in if a constrained build
  ever objects to the 128 KB resident table — the `get` API is identical. Not
  the default: it keeps the per-analyze rebuild 0020's follow-up wanted to
  delete, for a RAM saving that modern targets don't need.
- **Materialize a corpus-wide `GraphemeCache` (all spans up front):** 22–34 MB
  for a full Bible. Rejected — the per-verse transient buffer is ~5 KB and
  re-segmentation is 20–30 ms; there is no reason to hold the bulk.
- **Optimize the emoji-ZWJ / flag paths in the fast path:** rejected as
  anti-domain — ~zero occurrences in scripture; the added complexity would be
  pure risk against the gates. They route to the oracle.
- **Aggressive `COMPLEX` (route all Indic consonants to fallback):** simplest
  correct start, but regressed Khmer to 0.83× (85% fallback). Inline GB9c is
  the ~8 lines that buy Devanagari/Malayalam/Myanmar/Khmer their 3.8–4.9×.

## Consequences

- **Segmentation 2.7–4.9× faster on every script**, and grapheme rules share
  one pass per verse instead of walking independently — so even a script at
  ~1× per-pass becomes a system win once ≥2 rules consume it.
- **One `Class(u16)` lookup** answers both "what are this scalar's casing bits"
  and "what are its grapheme-break bits"; the per-analyze `CharClass` build is
  gone.
- **`.wasm` grows by the compact ranges (~tens of KB)**; ~128 KB resident heap
  for process life (escalation hatch: swap to the per-analyze trie behind
  `get`, no caller change).
- **Findings still highlight whole clusters** — `GSpan` ranges are
  grapheme-aligned by construction, so a bigram finding spanning two clusters
  can never split one.
- Reserved `clinging` bit from ADR 0020 survives the u8→u16 widening.

## Follow-up (not in this ADR)

- **Batched fallback / single persistent `GraphemeCursor`** for `COMPLEX`-heavy
  input — a "never slower than the oracle on *any* document" guarantee. Not
  needed for scripture (worst residual fallback 13% still yields 3.76×); add
  only if a non-scripture consumer needs it.
- **Letter bigram/trigram rarity rule** — the first stateful consumer of the
  shared grapheme pass.
- Data-hygiene items carried from ADR 0020's audits remain open.
