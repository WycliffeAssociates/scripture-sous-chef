# scripture-sous-chef

A pure, addressable **content analyzer** for scripture text. It receives
the plain text of each verse and returns **ranges** — spellcheck-style
findings ("there are two spaces here," control characters, …) — that an
editor can resolve to highlights. Aimed at field Bible translators working
in low-resource majority-world languages.

sous is a *separate concern* from USFM structure. Structural lint (unknown
markers, unclosed notes) belongs to [`usfm_onion`](https://github.com/WycliffeAssociates/usfm-onion);
sous targets the *rendered glyphs* of a verse at sub-token character
ranges. onion is the **single segmenter of record** — it projects each
verse to lossless plain text plus a segment map; sous consumes that text
and never derives its own.

## Contract

```rust
fn analyze(target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding>
// VerseMap = { Sid -> text }   (onion's vref map; source is optional)
```

- **Input**: onion's lossless verse text. sous reads no files, calls no
  onion, runs no segmentation of its own.
- **Output**: `Finding { sid, code, severity, range, score }`, where
  `range` is byte offsets into that text. Project to UTF-16
  (`range.to_utf16(text)`) or graphemes at the consumer boundary; the wasm
  binding does this so the editor gets UTF-16 with zero conversion.
- Localisation lives upstream (the editor's i18n catalog keyed by `code`);
  sous ships no rendered message.

See [ADR 0010](documentation/adrs/0010-pure-analyzer-contract-v1-reset.md)
and [the v1 reset design narrative](documentation/v1-reset-design.md).

## Status

v1 ships a small set of deterministic, zero-knob rules:

- `lex.excess-h-whitespace` — runs of 2+ horizontal whitespace
  (sentence-boundary aware)
- `hyg.tab-in-body`, `hyg.control-chars`, `hyg.zero-width-misuse`,
  `hyg.empty-verse`

The statistical / corpus-calibrated engine (anomaly detection, clustering,
source-relative proportionality, ingest pipeline) lives on the **`labs`**
branch and graduates back one rule at a time behind the contract above.

## Layout

- `crates/core` — the pure analyzer: `analyze`, addressing (`Span` +
  UTF-16/grapheme adapters), `Finding`, and the rules
- `crates/wasm` — wasm-bindgen bindings (`analyze_vref`) for the editor
  (web + Tauri), returning UTF-16 ranges

## Usage

```sh
cargo test -p ssc-core      # the pure heart
cargo check --workspace     # includes the wasm crate
cargo build --workspace
```
