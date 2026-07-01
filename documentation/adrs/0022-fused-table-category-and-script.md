# ADR 0022: Extend the fused table to General_Category groups and script

- **Date:** 2026-07-01
- **Status:** Accepted
- **Extends:** [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md)
  (the fused static `Class` table) and [ADR 0020](0020-char-classification-fused-classbits-table.md)
  (the fused-byte idea). Also supersedes the per-char property lookups ADR 0015
  (`ScriptTag`) and `crate::unicode` added.

## Context

After the grapheme-segmenter work (ADR 0021), a full-corpus profile at all
rules (`hi_ulb`, Devanagari) showed the residual non-Latin tax had moved into
the rules still calling `std` / `unicode-properties` / `unicode-script`
**directly**, char by char:

| self-time cost | source |
|---|---|
| `unicode_properties::general_category` (~18.5k samples, the #1 center) | `is_combining_mark` / `is_punctuation` / `is_symbol` / `is_decimal_digit` in `crate::unicode`, called across the hygiene rules |
| `unicode_script::script` (~5.6k) | `script_of` (mixed-script, mixed-numeral, zero-width) |
| `alphabetic::lookup_slow` (~4k) | `is_alphabetic` reached outside the fused table |

These share the ASCII fast-path property casing/segmentation already exploit:
each is one branch for ASCII and a full range-table binary search for every
non-ASCII scalar. English pays ~0; Devanagari pays per character. Casing and
repeated-char already collapse this via the fused `Class` lookup — these rules
simply don't read from it yet.

This is the same work in spirit as ADR 0020/0021: a scalar property → a packed
field in our one table. It does **not** touch `unicode-segmentation` (the
segmentation *algorithms*); it partially subsumes the *property* crates
(`unicode-properties`, `unicode-script`), and even then only repacks their data
— the table is generated from them, not a reimplementation.

## Decision

1. **Widen `Class` from `u16` to `u32`** and add:
   - three General_Category-group booleans — `MARK` (Mn/Mc/Me), `PUNCT` (P*),
     `SYMBOL` (S*) — in the flag bits, and
   - an **8-bit script lane** (bits 16..=23) holding the engine's coarse
     [`ScriptTag`] (ADR 0015), encoded as `0 = None` and `1..=N` per variant.

   **`u32`, not a parallel `[u8; 0x10000]` script table.** The design
   principle is one lookup answering every per-char question; a parallel table
   reintroduces two array reads / two cache lines for the rules that need both
   flags and script (mixed-script reads every char's script; mixed-numeral
   reads a digit's script *and* its `DECIMAL` bit). Memory does not decide it —
   the flat BMP table goes 128 KB → 256 KB (`u32 × 65536`) vs 192 KB for a
   parallel byte table, and both are trivial next to the 22–34 MB corpus-span
   materialization ADR 0021 already declined. Locality and the single
   `class_of` API decide it: `u32`.

2. **Route the property predicates through the table.** `crate::unicode`'s
   `is_combining_mark` / `is_punctuation` / `is_symbol` / `is_decimal_digit`
   and `script::script_of` become `class_of(c)` reads. Their hand-curated ASCII
   fast-path arms are **deleted** (no shim): the generator computes the same
   General_Category answers for ASCII from the UCD, so the table's ASCII bits
   are identical to those arms by construction — one array read replaces the
   branch, and there is one classifier instead of several.

3. **The generator gains category + script**, computed from the same crates the
   old predicates used (`unicode-properties` group, `script::script_from_unicode`
   which wraps `unicode-script` + the MathAlphanumeric override), so **no new
   committed UCD files** are needed — those two crates are the provenance,
   version-pinned via `Cargo.lock`. `script_of` at runtime reads the table;
   `script_from_unicode` stays as the generator input and the oracle for tests.

## Alternatives considered

- **Parallel `u8` script table (keep `Class` at `u16`):** saves 64 KB (192 vs
  256 KB) but splits the model into two lookups. Rejected on locality — see (1).
- **Boolean category bits only, skip script:** the `general_category` center is
  the larger cost (18.5k vs 5.6k), so bits alone capture most of the win and
  fit in the spare `u16` bits. But `unicode_script::script` is still a top
  center, and folding it in is the same mechanism for a modest, bounded 64 KB —
  do it once rather than revisit. (If a build ever wants the 64 KB back, the
  script lane is the drop-first candidate.)
- **`icu_properties`:** heavier dependency; `crate::unicode` already declined it
  (its General Categories are too broad for the zero-width rules). Unchanged.

## Consequences

- The `general_category` / `unicode_script::script` / `alphabetic::lookup_slow`
  centers collapse to `class_of` reads — the largest remaining non-Latin win,
  measured after the change (see the ADR-0021 follow-up numbers).
- One classifier: rules stop calling `std` / `unicode-*` per char ad hoc.
- **Behaviour-preserving:** the table is generated from the same UCD/crate data,
  so predicate values are identical; a `matches_*` test asserts `class_of`
  agrees with the reference predicates over a script spread, and finding counts
  are unchanged across the corpora.
- Flat table 128 KB → 256 KB heap; committed ranges grow modestly (script runs
  are large contiguous blocks). `.wasm` may *shrink* if `unicode-script` /
  `unicode-properties` runtime calls are now dead-code-eliminated.
- `unicode-properties` / `unicode-script` remain deps (generator + oracle
  tests), but leave the hot path.

## Follow-up (not in this ADR)

- If `unicode-properties` / `unicode-script` end up called only by the generator
  and tests, consider gating them behind a `gen` feature / dev-dependency to
  keep the shipped `.wasm` lean.
