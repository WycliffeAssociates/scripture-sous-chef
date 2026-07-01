# ADR 0020: Per-character classification via a fused `ClassBits` lookup (per-analyze trie)

- **Date:** 2026-06-30
- **Status:** Accepted

## Context

[ADR 0019](0019-shared-tokenization-and-per-char-cost.md) found the dominant
cost of analysis on non-Latin scripts is **per-character Unicode
classification**, not segmentation. The char-walking rules — `casing`'s
`reduce_book` and `lexical`'s `scan_repeated_character_run` — ask several
std `char` questions per grapheme (`is_alphabetic`, `is_lowercase`,
`is_uppercase`, `is_numeric`, `is_whitespace`, `is_decimal_digit`). Each is a
Unicode-table lookup; std ASCII-fast-paths them, but on non-ASCII text every
call is a table walk and we make ~five per character. On Thai (~90% non-ASCII)
`is_alphabetic` alone was 71% of samples.

A standalone spike measured the candidate structures over real corpus text,
and an audit ran over **all 1,185 corpora** we have (106 stitched/WA + 1,079
eBible):

- **Distinct chars per corpus:** max **2,582** (Chinese); every alphabet is a
  few hundred.
- **Astral (≥ U+10000):** exactly **one** character across all 1,185 corpora
  (a lone 🙏).
- **One fused lookup vs the ~5 std predicates:** 1.4–1.6× on Latin, **5–145× on
  non-Latin** (Thai 758 ms → ~5 ms per pass).
- **Per-analyze build cost:** classify each distinct char once into the table
  — **~4–11 ms for a whole 31k-verse Bible, sub-millisecond per verse**
  (incremental). (An earlier 7–170 ms reading was a spike bug: a `== 0`
  sentinel re-classifying zero-valued combining marks every occurrence; fixed
  by marking classified cells.)
- **Table memory (two-level trie, pages-touched):** ~1.5–3 KB single-script,
  ~22–43 KB for CJK (Han spans ~80 pages) — always far under a flat table.

## Decision

1. **Fuse the per-char boolean properties into one [`Class`] byte** —
   `alphabetic, lowercase, uppercase, whitespace, numeric, decimal-digit`, with
   a bit reserved (e.g. a future `clinging` flag) and a private `COMPUTED`
   marker so a char with no flags still classifies exactly once. One
   `CharClass::get(c) -> Class` replaces the ~five std predicate calls.
   (`script-tag` stays out — wider, and the token rules' concern.)

2. **Back it with a two-level BMP page-table (trie), built per analyze** over
   the text being analyzed (`crates/core/src/charclass.rs`). A block is
   allocated only for a codepoint page the text uses, so a single-script corpus
   is ~1.5–3 KB; astral codepoints take a direct `classify` fallback. The table
   is **stateless** — built from the input, used, dropped — so it holds no
   process-global or per-corpus resident state and respects the pure-analyzer
   contract (ADR 0010): the caller supplies nothing.

3. **Shipped first in `casing`** (the #1 hotspot), which builds one `CharClass`
   over the call's text in `reduce` and reads bits in `reduce_book`. Behaviour
   is identical (the table stores the same std-predicate answers; 127 tests
   pass, all corpus finding counts unchanged).

## Alternatives considered (and why not)

- **Status quo (5 std predicates/char):** the baseline; 180–1580 ms full-Bible.
- **HashMap / sorted-vec keyed by char:** slower than an array/trie; the
  hashmap *regresses* ASCII (hashing costs more than std's inlined ASCII path).
- **Flat `[Class; 0x10000]` (128 KB):**
  - *`build.rs`-baked into the crate* → ~128 KB of mostly-zero data in the
    `.wasm` module: download weight for the web target. Rejected.
  - *lazy runtime static* → 128 KB resident for process life. Simplest and
    zero per-analyze cost, but the resident blob is exactly what the web/mobile
    target wants to avoid. Rejected as default; noted as the swap-in if a
    constrained build ever prefers RAM-for-zero-build.
- **Global static (any backing):** dissolves per-analyze build and parallelism,
  but a *global* table covers all chars so a global trie ≈ flat size — no
  memory win — and it means a resident blob. Rejected for the same web reason.
- **Per-corpus / stateful trie, cached across analyses:** ~2–43 KB, updated
  incrementally (only new codepoints allocate). Deferred — the per-analyze
  rebuild is cheap (µs incremental) and threading a *derived* lookup through
  the incremental `Stats` flow would serialize it across the wasm boundary or
  hold state in core, fighting ADR 0010. **Documented escalation** if a profile
  ever shows the full-corpus rebuild mattering; behind `get`, so it's a local
  change.
- **Whole Unicode range flat:** ~2.2 MB — pointless when the BMP holds every
  char our corpora use bar one emoji.
- **"Why doesn't std fuse this?"** Unicode has 100+ properties; std exposes
  each separately because no program wants all bundled and it can't know a
  caller's subset. Fusing a fixed subset is application-specific — what we do.

## Consequences

Measured in sous-chef (all rules, serial; finding counts unchanged), the casing
conversion on top of ADRs 0019's wins:

| script | before casing→trie | after | Δ |
|---|---|---|---|
| Thai | 900 ms | 396 ms | **−56%** |
| Amharic (Ethiopic) | 319 ms | 157 ms | **−51%** |
| Devanagari | 416 ms | 284 ms | **−32%** |
| Vietnamese | 222 ms | 210 ms | −5% |
| Latin (en/es) | ~150 ms | ~150 ms | ~flat (casing is a small % of ASCII cost) |

- Per-analyze build is µs on the incremental path, ~4–11 ms full-corpus — small
  vs a full analyze, and no resident/baked footprint.
- The trie is a few KB, transient, parallel-trivial (each analysis owns its
  own), so an ecosystem-parallel run over 1,000+ translations stays bounded.
- `clinging` is reserved as a bit, so the future spaced-punctuation work is a
  one-bit addition.

## Follow-up (not in this ADR)

- **Convert `scan_repeated_character_run`** to `class(c)` too. It's a
  `PerVerseRule`, so doing it well means building the `CharClass` once in
  `analyze_stateful` and sharing it with both rules (per-verse rebuild would
  churn 31k small allocations). That sharing is the natural next step and would
  also let casing drop its own per-analyze build.
- **Data hygiene (from the audits, not perf):** PUA chars in `am_ulb`/`gl_reg`
  (Amharic emits 14,478 findings — likely the PUA tripping rules), stray
  control chars in ~8 corpora, a mid-text BOM in `es-419_ulb`, U+FFFD in eBible
  `sim`.
