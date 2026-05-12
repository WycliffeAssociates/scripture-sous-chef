# ADR 0009: Delegate per-character script identity to the `unicode-script` crate

- **Date:** 2026-05-12
- **Status:** Accepted

## Context

`crates/core/src/script.rs::script_of` previously returned a script
name from a hand-rolled `match` over codepoint ranges. The table had
three problems:

1. **Bug: ASCII digits and punctuation attributed to Latin.** The
   `0x0000..=0x024F => "Latin"` row covered the entire Basic Latin
   block, which includes `0..9`, `.`, `,`, etc. The UCD assigns those
   to script `Common`, not `Latin`. Any rule that branched on
   `script_of(c) == Some("Latin")` was already being silently wrong;
   the new `orth.script-mixing` rule needed a workaround
   (`classify_script` special-casing `is_ascii_digit`) to keep the
   `Mark2` test case meaningful.
2. **Bug: Greek Extended (U+1F00..=U+1FFF) returned `None`.** Polytonic
   Greek — every NT manuscript — fell out of the table. The rule
   silently classified accented Greek chars as scriptless.
3. **Maintenance trap.** New Unicode versions add scripts and
   supplementary blocks (Latin Extended-E, Arabic Extended-C, …).
   Every addition is a row we'd have to remember to add.

## Decision

Delegate to the `unicode-script` crate (v0.5, from `unicode-rs`):

- Same maintainer as `unicode-segmentation` and `unicode-normalization`
  which are already workspace deps.
- O(1) lookup table generated from UAX #24.
- Lets us drop the codepoint-range `match` entirely.

`script_of` becomes a two-step lookup:

1. **Math homoglyph override.** `U+1D400..=U+1D7FF` (Mathematical
   Alphanumeric Symbols) is `Common` in the UCD by spec. For
   homoglyph detection that's the wrong answer — a math-bold `M`
   inside an otherwise Latin token is the attack we want to flag.
   Keep an explicit override for that block, returning the pseudo-
   script `"MathAlphanumeric"`.
2. **Crate lookup.** Map `Script` enum variants to the same short
   strings the engine has been emitting (`"Latin"`, `"Greek"`,
   `"Cyrillic"`, `"CJK"`, …). `Common`, `Inherited`, and `Unknown`
   collapse to `None`. The string table is the migration's
   single-point-of-stability: historical calibration dumps that key
   off script names stay comparable.

## Consequences

- `classify_script` in `orth.script-mixing` no longer fights
  `script_of` over ASCII digits. The `is_ascii_digit` branch
  remains, but as policy ("digits are their own mixing identity")
  rather than as a bug workaround.
- Polytonic Greek now resolves to `"Greek"` automatically.
- Latin Extended, Cyrillic Supplement, Arabic Extended, and every
  other supplementary block resolve correctly without table edits.
- The `"CJK"` identity stays collapsed from `Hiragana | Katakana |
  Han`. The prior single-block-per-CJK behaviour is preserved so
  histograms in `profile.rs` don't shift.

## Out of scope

We considered `unicode-security` (UTS #39 — restriction levels,
confusable detection, identifier profiles). It's the right framework
for a future homoglyph-cluster rule, but premature for a single
script-mixing signal whose ergonomics (per-corpus `allowed_scripts`,
per-rule ignore patches) are intentionally non-UTS-#39. Revisit when
the homoglyph cluster rule lands.

## References

- UAX #24: <https://www.unicode.org/reports/tr24/>
- `unicode-script`: <https://crates.io/crates/unicode-script>
- Commit 1 work-package: `research/proposed/2026-05-08-brutal-review-from-current/`
