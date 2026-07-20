# Unicode Character Database extracts (UCD 17.0.0)

Provenance for the fused classification table and the grapheme-segmenter and
word-break conformance gates (ADR 0021, ADR 0064). These are **committed
reference data**, not generated — the single source of truth for both
`charclass_table.rs` (via `examples/gen_charclass_table.rs`) and the
in-crate conformance tests.

## Files

| File | Source (under `https://www.unicode.org/Public/17.0.0/ucd/`) | Used by | Note |
|---|---|---|---|
| `GraphemeBreakProperty.txt` | `auxiliary/GraphemeBreakProperty.txt` | table generator (`EXTENDER`/`COMPLEX` bits) | pristine |
| `emoji-data.txt` | `emoji/emoji-data.txt` | table generator (`Extended_Pictographic` → `COMPLEX`) | pristine |
| `DerivedCoreProperties-InCB.txt` | `DerivedCoreProperties.txt` | table generator (`INCB_*` bits) | **extract** — see below |
| `GraphemeBreakTest.txt` | `auxiliary/GraphemeBreakTest.txt` | `grapheme::tests::conforms_to_graphemebreaktest` | pristine |
| `WordBreakProperty.txt` | `auxiliary/WordBreakProperty.txt` | table generator (`WB_EXTEND`/`WB_SEP` bits, ADR 0064) | pristine |
| `WordBreakTest.txt` | `auxiliary/WordBreakTest.txt` | `token::tests::conforms_to_wordbreaktest` | pristine |

The full **`DerivedCoreProperties.txt`** (~1.1 MB, all derived properties) is at
<https://www.unicode.org/Public/17.0.0/ucd/DerivedCoreProperties.txt>. We consume
only its 506 `InCB` lines, so the committed copy is a trimmed extract: the
pristine `©`/version/license header followed by the `InCB` lines. Reproduce it
from the full file with:

```sh
{ sed -n '1,8p' DerivedCoreProperties.txt; grep 'InCB;' DerivedCoreProperties.txt; } \
  > DerivedCoreProperties-InCB.txt
```

## Version discipline

The Unicode version here **must match the `unicode-segmentation` version** in
`Cargo.lock` — that crate is both the runtime fallback and the conformance
oracle, so a version skew would make the two correctness gates disagree.
As of this writing: **Unicode 17.0.0**, `unicode-segmentation` 1.13.x.

## Refreshing to a new Unicode version

1. Bump `unicode-segmentation` and note the Unicode version it targets
   (`tables::UNICODE_VERSION` in that crate).
2. Re-download the files above from the matching
   `https://www.unicode.org/Public/<VERSION>/ucd/` path; re-trim the InCB
   extract with the command above.
3. `cargo xtask gen-charclass-table` to
   regenerate `src/charclass_table.rs`.
4. `cargo test -p ssc-core` — all gates (grapheme conformance, word-break
   conformance, `matches_std_predicates`) must stay green. Re-run the
   whole-corpus differentials (ADR 0021, ADR 0064) as the calibration check.

## Licence

Unicode data files are distributed under the Unicode Licence (see the header of
each file). They are included here as reference data for generation and testing.
