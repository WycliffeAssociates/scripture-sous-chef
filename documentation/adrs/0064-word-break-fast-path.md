# ADR 0064: Word-break fast path over the fused `Class` table, plus a per-book adaptive ASCII gate

- **Date:** 2026-07-17
- **Status:** Accepted
- **Builds on:** [ADR 0021](0021-grapheme-segmenter-fast-path-fused-static-table.md)
  (the grapheme segmenter's fast-path/fallback shape, which this decision
  mirrors closely for word boundaries), [ADR 0022](0022-fused-table-category-and-script.md)
  (the fused `Class(u32)` table this reuses), [ADR 0045](0045-scalar-tape.md)
  (the scalar tape the fused walk already builds once per verse),
  [ADR 0057](0057-event-stream-engine.md) (the fused book walk in
  `stream.rs` this change's per-book gate lives inside), [ADR 0018](0018-parallelism-behind-a-feature.md)
  (the per-book parallel fan-out the per-book-local counter reasoning
  depends on).

## Context

`token.rs`'s `tokenize`/`tokenize_into` called `unicode-segmentation`'s
`unicode_word_indices()` directly for every verse. A calibration spike
(`documentation/calibration/2026-07-17-word-break-fast-path-survey.md` —
frequency/correlation survey, a hand-rolled prototype, a full-fleet
conformance + differential gate, and a throughput benchmark, all built as
throwaway examples before any production code changed) traced
`unicode-segmentation`'s own cost structure and found two exploitable
properties, mirroring exactly the reasoning that already justified
`grapheme.rs`'s hand-rolled fast path (ADR 0021):

1. **A whole-string ASCII gate dominates the crate's own cost profile.**
   `unicode-segmentation`'s word iterator (`word.rs` ~973-976) checks
   `s.is_ascii()` once per string: pure-ASCII text takes a cheap byte-level
   path; a single non-ASCII scalar anywhere routes the **entire** string
   onto a slower, general UAX #29 state machine. Measured directly (a
   microbenchmark pairing real `WA-en-ulb` verses against an ASCII-only
   control twin, materializing tokens both sides): the slow path costs
   ~10x the ASCII path per verse it's taken on. Fleet-wide (1,504 corpora,
   17.3M verses), ~68% of verses are either always-ASCII or trip the gate
   while being under 10% non-ASCII by scalar count — the gate is a bad
   match for the majority of real scripture text, not just English.
2. **Word-break's own per-scalar categories are, like grapheme-break's,
   almost entirely expressible from bits `Class` either already has or can
   cheaply gain.** `ALetter` ≈ `is_alphabetic()` (99.81% global
   correlation, residual explained by scriptio-continua scripts UAX #29
   routes to `Other` instead — Thai/Lao/Khmer/Myanmar/Han/Hiragana — already
   distinguishable via the existing script lane). `Numeric` ≈
   `is_decimal_digit()`. The `Extend`/`ZWJ` "ignore" set (WB4) is *almost*
   `is_extender()`, but not quite — see below. Every remaining category
   (`Hebrew_Letter`, `Katakana`, `WSegSpace`, `Format`, and the six
   `MidLetter`/`MidNum`/`MidNumLet`/`ExtendNumLet`/`Single_Quote`/
   `Double_Quote` separator categories) is small enough (14-331 UCD
   codepoints each) to enumerate directly, the same precedent
   `charclass_table.rs`'s engine-defined `QUOTE` bit already set.

### Why word-breaking needs its own bit, not just `grapheme.rs`'s `EXTENDER`

The calibration spike's prototype first tried reusing `Class::is_extender()`
(GCB `Extend`|`SpacingMark`|`ZWJ` — correct for grapheme clustering, where
all three glue to the base cluster) for the WB4 "ignore and absorb" set. The
full-fleet corpus differential caught this as wrong: some `GCB=SpacingMark`
scalars are genuinely `Word_Break=Other`, not `Extend` — concretely, U+0EB3
LAO VOWEL SIGN AM is `GCB=SpacingMark` (so `is_extender()==true`) but does
not appear in `WordBreakProperty.txt` at all (`Word_Break=Other`). Real UAX
#29 splits "ນ້ຳ" (Lao for "water") into two word-break segments even though
it is one grapheme cluster; reusing `is_extender()` for word-break absorption
wrongly fused it into one, and this single divergence was responsible for
essentially the entire pre-fix corpus mismatch rate (~2.07% of WA-subset
verses) during calibration. Grapheme clustering and word breaking need
genuinely different "glue to the previous unit" predicates, and no existing
bit expressed the narrower one.

## Decision

1. **Hand-roll a domain-tailored word-break walker** in `crates/core/src/token.rs`:
   a subset of UAX #29 (WB3d, WB5-WB13b) over per-scalar "atoms" built from
   the fused `Class` bits, deferring `Class::is_complex()` scalars (Hangul
   jamo pieces, Regional Indicator, emoji, Prepend, Control, CR, LF) to
   `unicode-segmentation` per verse — the exact same `COMPLEX`-bucket
   contract `grapheme.rs` already uses, for the same reason (scripture has
   ~zero emoji/flags, so this fallback is measured rare, not load-bearing;
   confirmed directly against `GraphemeBreakProperty.txt` that
   `is_complex()` covers every `WordBreakTest.txt` case touching those
   categories). `unicode-segmentation` stays a dependency as both the
   fallback and the correctness oracle — worst case, everything routes to
   it and behavior is exactly as before, only slower.

2. **Two new `Class` bits — the last 2 genuinely free (bits 30, 31; bit 6
   stays reserved for a future `clinging` flag)** — added to
   `crates/core/src/charclass.rs` and derived in
   `xtask/src/gen_charclass_table.rs` from the already-committed
   `testdata/ucd/WordBreakProperty.txt` (the same UCD source `grapheme.rs`'s
   bits are derived from, now also feeding these two):
   - **`WB_EXTEND`** = `Word_Break` ∈ {`Extend`, `ZWJ`} — narrower than
     `EXTENDER`, specifically because `SpacingMark` must NOT join here (see
     Context).
   - **`WB_SEP`** = `Word_Break` ∈ {`MidLetter`, `MidNum`, `MidNumLet`,
     `ExtendNumLet`, `Single_Quote`, `Double_Quote`} (42 UCD codepoints total)
     — a hot-loop candidate-separator prefilter, mirroring why `QUOTE` (14
     chars) already gets its own bit rather than a per-char match: a single
     OR'd bit test fast-rejects the common case, and a literal char match
     (`token.rs`'s `wb_sep_category`) disambiguates which of the six on the
     rare hit.

   Every other category the walker needs (`Hebrew_Letter` 75 codepoints,
   `Katakana` 331, `WSegSpace` 14, `Format` 58) is small enough to hardcode
   as direct range matches in `token.rs` itself, costing zero additional
   `Class` bits and zero runtime UCD-file parsing (unlike the calibration
   prototype, which parsed `WordBreakProperty.txt` at startup for
   convenience — production code hardcodes the same sets instead, the same
   way `charclass_table.rs`'s `QUOTE_CHARS` is a literal).

   The `ALetter`/`Numeric` residuals (codepoints where the real `Word_Break`
   value diverges from `is_alphabetic()`/`is_decimal_digit()`) are hardcoded
   too, but **exhaustively computed from the committed UCD file**, not
   hand-picked from corpus exposure — a real bug this port found in its own
   first draft, worth recording exactly: the first pass hardcoded only the
   two residual codepoints the calibration prototype's corpus differential
   had happened to surface (U+00B8 CEDILLA for `ALetter`, U+066B ARABIC
   DECIMAL SEPARATOR for `Numeric`). Re-running the full-fleet differential
   against the *ported* code (not the prototype, which still did a runtime
   UCD parse for this fallthrough and so never had the gap) surfaced 64,150
   mismatches (0.37% of verses) — `WA-wud-reg` and others use U+02C2-02C5
   (arrowhead "modifier letter" glyphs, GC=Sk) as quotation-mark substitutes,
   and those are ALSO `Word_Break=ALetter`-but-not-alphabetic, just never
   exercised by the two originally-known examples. A one-off scan
   (`examples/compute_residuals.rs`) cross-referencing every
   `WordBreakProperty.txt` `ALetter`/`Numeric` range against
   `char::is_alphabetic()`/`GeneralCategory::DecimalNumber` found the
   **complete** sets — 65 and 14 codepoints respectively, both now hardcoded
   in full (`is_aletter_residual`/`is_numeric_residual`) — and the
   differential dropped to exactly zero. This is the exact discipline the
   oracle-gating process exists to enforce: the bug was caught before Step 3
   even ran, by re-checking Step 4's fresh differential against the newly
   ported code rather than trusting that "the prototype was proven correct"
   automatically transferred to a rewritten fallback path.

3. **A whole-string ASCII gate, exposed as three functions instead of one**
   so a caller with per-book context can bypass the per-verse check:
   - `tokenize_into` (unchanged public/`pub(crate)` signature): checks
     `text.is_ascii()` and dispatches to one of the two below. This is what
     any caller without book-level context gets (tests, and any future
     standalone caller of `tokenize`).
   - `tokenize_oracle_into`: always calls `unicode-segmentation` directly.
     Nothing to beat on pure-ASCII input — `unicode-segmentation`'s own
     ASCII path is already at its floor cost — only to match, and
     delegating means zero hand-rolled ASCII boundary logic to get subtly
     wrong.
   - `tokenize_hand_rolled_into`: always runs the walker from (1), with its
     own per-verse `is_complex` fallback.

4. **A per-book adaptive gate, in `stream.rs`'s `walk_book` and
   `drive_book`, not per-verse in `token.rs`.** Sample the non-ASCII
   codepoint density of a book's first `ADAPTIVE_SAMPLE_N` (5) verses (or
   fewer, for a short book), decide ONCE whether the whole book delegates
   (`tokenize_oracle_into` for every verse) or hand-rolls
   (`tokenize_hand_rolled_into` for every verse), and never re-check
   `is_ascii()` per verse again for that book. This subsumes the per-verse
   ASCII gate's benefit for Latin-with-diacritics languages (Spanish,
   Portuguese, and similar): their verses are rarely 100% ASCII (so the
   per-verse gate rarely fires) but are overwhelmingly low-density (so the
   per-book decision correctly commits to delegating for the whole book
   anyway). A plain per-book-local `bool` — no atomic, no mutex: a book's
   own walk is already strictly sequential even under the parallel per-book
   fan-out (ADR 0018), so there is nothing to synchronize, and a
   counter/decision shared *across* books would need real synchronization
   paid on every verse of the hot loop this whole change exists to speed up,
   for no corresponding benefit (each book's own density is what determines
   whether *its* walk should delegate — there's no reason to want
   cross-book sharing). Applies uniformly regardless of whether a book is in
   the `counted` or anchor (uncounted) set — a pure tokenization-performance
   detail, not a counting concern, so it isn't gated on that distinction.
   The threshold (`ADAPTIVE_THRESHOLD = 0.30`) is a **placeholder**, not a
   measured crossover — see Consequences.

5. **Two permanent correctness gates**, mirroring ADR 0021's exactly:
   - A committed `#[test]` (`token::tests::conforms_to_wordbreaktest`) runs
     the UCD `WordBreakTest.txt` conformance suite (1,944 cases, Unicode
     17.0.0) — now committed at `crates/core/src/testdata/ucd/WordBreakTest.txt`
     alongside `WordBreakProperty.txt` (the generator's source of record for
     `WB_EXTEND`/`WB_SEP`, also now committed), the same convention
     `GraphemeBreakProperty.txt`/`GraphemeBreakTest.txt` already established.
   - **The whole-fleet corpus differential against `unicode-segmentation`**
     stays a calibration run (corpora are gitignored), not a committed test
     — same convention `grapheme.rs` follows. Re-run for this port: **zero
     mismatches on the handled path across all 17.3M verses of the full
     1,504-corpus fleet.**
   - **The oracle-gated engine-rework discipline** (repo `CLAUDE.md`): the
     full-fleet `calibrate --dump-findings` (both `default` and `all`
     configs) is byte-identical before and after this port — see the
     calibration doc's final section for the exact before/after diff.

## Alternatives considered (and why not)

- **Keep `unicode-segmentation` everywhere (status quo):** the baseline and
  the fallback. Measured ~1.3-5.6x slower on every non-Latin script tested
  (Ethiopic, Devanagari, Tamil, Malayalam, Thai, Myanmar), and no faster than
  parity on Latin scripts even with the per-verse ASCII gate alone (Spanish/
  Portuguese specifically stayed ~0.85x — *slower* than the reference —
  until the per-book adaptive gate subsumed their case). Kept as the safety
  net for `is_complex` verses and as the correctness oracle, not the hot
  path.
- **Reuse `Class::is_extender()` for WB4 absorption instead of a new bit:**
  tried first in the calibration spike; caught by the full-fleet
  differential as wrong (the Lao `SpacingMark` divergence in Context). Not
  viable without either a precise small-set correction on top of
  `is_extender()` or a dedicated bit — the dedicated bit (`WB_EXTEND`) was
  chosen since 2 bits were free and a correction list adds a branch to the
  same hot per-scalar path a bit avoids.
- **Per-verse-only ASCII gate (no per-book adaptive layer):** measured
  directly — closes almost the entire gap for English (95.68% pure-ASCII
  verses; per-verse gate essentially always fires) but leaves Spanish/
  Portuguese at ~0.85x, because routine diacritic use means the per-verse
  gate almost never fires for them (~7.5% of verses) even though their
  overall density is well under any plausible crossover. Rejected as
  incomplete once this was measured; the per-book layer was added
  specifically to close that gap, and did (both corpora reach ~0.99-1.02x
  once ported — see Consequences).
- **A cross-book-shared adaptive decision (one counter for the whole
  fleet/corpus):** considered and explicitly not built — see Decision
  point 4's synchronization argument. No calibration evidence suggested
  book-to-book sharing would even help (each book's own prefix already
  predicts its own density well, per the calibration doc's sampling
  survey), so there was nothing to weigh against the real cost of adding
  synchronization to a hot loop.

## Consequences

- **Real, measured wins on non-Latin scripts, ~1.3-5.6x depending on
  script** (Ethiopic ~1.3-1.4x, Devanagari/Tamil ~1.7-1.9x, Malayalam
  ~1.9-2.2x, Myanmar ~4.5-4.8x, Thai ~5.4-5.6x — see the calibration doc's
  Part 3 for the full range and how it was measured, and Step 5 of this
  ADR's own port for the production-code re-measurement). Latin scripts
  (English, Spanish, Portuguese) land at parity with the reference
  (~0.85-1.02x) rather than losing to it, via the per-book adaptive gate.
- **Zero new resident memory.** Both new bits pack into the existing
  `charclass_table.rs`'s already-resident `u32` per scalar (adding bits to
  an already-nonzero cell costs nothing further at runtime); the small
  hardcoded range/literal tables in `token.rs` are a handful of `matches!`
  arms compiled into the binary, negligible next to the existing table.
- **The hardcoded sets are exhaustive, not corpus-sampled — deliberately, after
  the port's first draft got this wrong.** `Hebrew_Letter`/`Katakana`/
  `WSegSpace`/`Format` and the `ALetter`/`Numeric` residuals are each the
  *complete* set of codepoints `WordBreakProperty.txt` assigns to that
  category (or that diverges from the reused bit), computed directly against
  the committed UCD file rather than hand-picked from what the calibration
  corpora happen to exercise. This distinction is not academic: see Decision
  point 2 for the real bug this caught mid-port. Because the sets are
  exhaustive against the full UCD 17.0.0 `Word_Break` property (not merely
  against the current 1,504-corpus fleet's observed content), a future
  corpus in any of these categories is expected to keep matching the true
  UAX #29 answer without further discovery work — the same standing
  `grapheme.rs`'s own hardcoded grapheme-break bits already provide.
- **`ADAPTIVE_THRESHOLD = 0.30` is a placeholder pending a real crossover
  measurement.** The calibration doc's book-level sampling survey pins down
  N=5 as a reliable sample size across several candidate thresholds
  (15%/25%/40%/50%), but the actual density at which the hand-rolled walker
  starts winning over delegating is not yet measured directly — only ~0%
  (delegating wins big), ~10% (delegating still wins), and ~50%+ (the
  walker wins big) are pinned down. 30% is simply the midpoint of that
  still-unmeasured 10-50% gap. Revisiting this threshold with real
  measurement is the most impactful open follow-up from this change.
- **The two throwaway calibration examples this ADR's decision was built
  from** (`crates/core/examples/word_break_survey.rs`,
  `word_break_ascii_gate_bench.rs`, `word_break_prototype.rs`,
  `ascii_gate_book_sampling.rs`) are left in place for now — cleanup is a
  separate, later step, not part of landing this ADR.

## Follow-up (not in this ADR)

- **Measure the real ALetter/Extend/Numeric crossover density directly**
  (a hand-rolled-walker-vs-delegate throughput comparison swept across a
  range of synthetic or real densities) to replace `ADAPTIVE_THRESHOLD`'s
  30% placeholder with a measured value.
- **Harden the small hardcoded residual sets** (`ALetter`/`Numeric`
  residuals, `Hebrew_Letter`/`Katakana`/`WSegSpace`/`Format`) against future
  corpora outside the current fleet — either by widening them from a fuller
  UCD scan, or by re-deriving them at `xtask` build time the same way
  `WB_EXTEND`/`WB_SEP` themselves are, rather than hand-transcribing ranges.
- **A word-break-specific complexity gate**, narrower than reusing
  `grapheme.rs`'s `is_complex()` wholesale: the calibration doc's Part 3
  found Khmer's hand-rolled-walker win partly capped by U+200B ZERO WIDTH
  SPACE (Khmer's conventional inter-word separator) being
  `GraphemeBreakProperty=Control` and therefore `is_complex()==true`, even
  though its real `Word_Break` value (`Other`) needs no Hangul/RI/emoji-style
  special handling for word-breaking specifically. "Complex enough to need
  care when clustering graphemes" and "complex enough to need care when
  breaking words" are different questions this port did not try to
  separate.
- **Batched fallback / single persistent cursor** for `is_complex`-heavy
  input, mirroring the equivalent open follow-up already recorded in
  ADR 0021 — not needed for scripture today (the fallback rate stays low
  across the fleet except Khmer, whose ceiling is capped by the item above,
  not by fallback-call overhead itself).
