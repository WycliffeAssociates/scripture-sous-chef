# ADR 0015: Script identity is a `Copy` tag enum, not a `&'static str`

- **Date:** 2026-06-29
- **Status:** Accepted

## Context

A `samply` profile of a full-Bible `analyze` pass (44k samples,
`sousChefPlayground`) showed the per-character Unicode work dominating
the run, with three findings that were structural, not micro:

1. **`scan_zero_width_misuse` (21%)** computed `majority_script(text)`
   on *every* verse. That helper walks every character — `script_of`
   (a `unicode-script` bsearch + a large match) counted into a
   `HashMap<&'static str, usize>`. String **hashing alone** was ~8% of
   the whole run. All of it existed only to decide whether ZWNJ/ZWJ are
   allowed for the verse's script — which only matters *if the verse
   actually contains a ZWNJ or ZWJ*. The overwhelming majority of verses
   contain no zero-width characters at all, so the entire pass was
   computed and discarded.

2. **`general_category` bsearch (25%)** — `unicode-properties` resolves
   every General Category query through a binary search over range
   tables, even for the ASCII codepoints that dominate the corpora.

3. Script identity was modelled as a `&'static str` short name
   (`"Latin"`, `"Greek"`, …). Every count keyed on it hashed a string;
   every mixed-script comparison compared strings; mapping
   `Script -> &str` was its own hot match (`script_name`, ~7% self).
   The string was never actually consumed *as text* anywhere in the
   engine — the doc comment's "stable for calibration dumps" rationale
   had no code behind it.

ADR 0009 established that we delegate script lookup to `unicode-script`
rather than hand-rolling ranges; this ADR keeps that and only changes
the *representation* the engine carries downstream.

## Decision

1. **`script_of(char) -> Option<ScriptTag>`**, where `ScriptTag` is a
   `#[derive(Clone, Copy, PartialEq, Eq, Hash)]` enum
   (`crates/core/src/script.rs`). The `Script -> ScriptTag` table is the
   same set of variants the `&str` table named (Hiragana/Katakana/Han
   still collapse to `Cjk`; the U+1D400..=U+1D7FF override still yields
   `MathAlphanumeric`). No backward-compat string accessor is kept —
   this is pre-alpha and `script_of`'s only callers are in `hygiene.rs`.
   Rules now count, compare, and `matches!` on the tag directly, so the
   hot paths never hash or compare script *names*.

2. **`scan_zero_width_misuse` computes the joiner allow-list lazily** —
   at most once, on the first ZWNJ/ZWJ encountered
   (`Option<bool>` + `get_or_insert_with`). A verse with no zero-width
   chars never calls `majority_script` at all.

3. **ASCII fast paths in `crates/core/src/unicode.rs`** for
   `is_punctuation`, `is_symbol`, `is_combining_mark`, and
   `is_decimal_digit`: branch the `c < 0x80` case to a compile-time
   `matches!` (or `false` / `is_ascii_digit`) ahead of the
   `unicode-properties` bsearch. The ASCII General Category split is
   spelled out explicitly — note `is_punctuation` is narrower than
   `char::is_ascii_punctuation`, which also counts `$ + < = > ^ ` | ~`
   (those are Symbol, not Punctuation).

4. **`scan_punct_only_token` checks its cheap all-punct/symbol gate
   first** (`signals/lexical.rs`), short-circuiting on the first letter
   of any ordinary word, so the per-chunk `Vec<char>` / `String`
   allocation only happens for the rare punctuation-only chunk instead
   of once per word.

Behaviour is unchanged: the full core suite (107 tests) passes, and the
reorder/laziness are semantics-preserving rewrites.

## Consequences

- Measured against the committed baseline (`criterion`, this machine):

  | bench | before | after | change |
  |---|---|---|---|
  | `analyze/full_bible` (en_ulb, 31k verses) | ~840 ms | ~304 ms | **−64%** |
  | `analyze/nt` (en_ulb NT) | ~182 ms | ~69 ms | **−62%** |
  | `analyze/nt_rayon` | ~38 ms | ~15 ms | **−60%** |
  | `analyze/nt_devanagari` (bap-x-rai_reg) | ~309 ms | ~259 ms | **−16%** |

  Latin-script corpora gain most (ASCII fast path + lazy majority).
  Devanagari gains less from the ASCII path but still benefits from the
  lazy joiner computation. `proportionality/nt_vs_bible` is unchanged
  (it touches none of this code; the ±4% movement is laptop noise).

- The `perf-baseline.md` calibration table is now stale and should be
  re-recorded against this commit.

- Adding a tracked script means one row in `ScriptTag` + one row in the
  `Script -> ScriptTag` table, same as before — no string to keep in
  sync across call sites.

- The ASCII fast paths are a maintenance note, not a correctness risk:
  the non-ASCII arm is the original `unicode-properties` query
  unchanged, so the only thing that could drift is the ASCII set, which
  is fixed by the standard.
