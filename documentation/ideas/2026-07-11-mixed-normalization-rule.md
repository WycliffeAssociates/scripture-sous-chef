# Idea — `uni.mixed-normalization` (composed vs decomposed inconsistency)

Date: 2026-07-11. Status: **proposal — approved direction, deliberately not
started** (user call: hold until scheduled; the detector data is ready-made,
the mapping infrastructure is not).

## Claim

A corpus that writes the same abstract glyph in **both** precomposed and
decomposed forms (`é` U+00E9 vs `e` + U+0301) is normalization-inconsistent —
a real text defect (search, matching, and rendering diverge) that today is
either invisible or **mislabeled**: ADR 0053 records the known residual where
a precomposed `é`, rare in a decomposed-convention corpus, surfaces as a
"rare letter" — the right signal wearing the wrong label. This rule converts
that residual into its honest finding.

## Evidence already in hand

The rare-glyph spike round 1 (calibration doc 2026-07-10, "Normalization
seam") shipped a dependency-free base+combining preflight: the fleet contains
heavy decomposed usage — e.g. `a` + U+0331 with **1,590,685 occurrences in 59
corpora**, plus many Indic/Malayalam/Myanmar/Telugu base+mark pairs. Affected
corpora are identifiable today; what's missing is the composed↔decomposed
equivalence to judge *mixing*.

## Design (the settled shape)

- **Mapping via house codegen, not a dependency.** The workspace deliberately
  has no Unicode-normalization crate. Vendor a **trimmed canonical
  decomposition table** (UnicodeData.txt canonical mappings, singletons and
  non-scripture blocks pruned) and generate a lookup with `cargo xtask
  gen-*`, exactly like the committed trimmed `BidiBrackets.txt` →
  `BRACKET_PAIRS` precedent. Auditable, no_std-clean, and the census can
  reuse the same table later.
- **Rule shape: the house two-factor, per equivalence class.** Per corpus,
  per abstract glyph (canonical equivalence class), tally composed vs
  decomposed occurrence counts; `score = dominance(majority form, N, z) ×
  rarity(minority recurrence, volume-scaled knee)`. Flag minority-form
  occurrences. A corpus that consistently uses either form is silent; a
  50/50 corpus has no convention and is silent (Wilson self-gates); a
  recurring minority is a second convention (knee) — same posture as
  spacing/casing.
- **One phenomenon, one finding:** when this rule fires on a glyph, the
  rare-glyph L-lane should skip that scalar's finding (the ADR 0034
  principle); the ADR settles the predicate.
- **Event-stream fit:** a small counting listener over scalar events with a
  short decomposition-window match (bounded lookahead like pooled spacing's
  seam read); census gains a `normalization` lane from the same accumulator
  per the feature-routing note in CLAUDE.md.

## Plan when scheduled

1. xtask codegen: vendored trimmed decomposition table → generated pair
   lookup (own commit; no behavior change).
2. Calibrate spike (`--normalization`): fleet per-pair volumes, per-corpus
   dominant form, minority shapes, knee sweep, samples for adjudication —
   expect the 59 preflight corpora to dominate; the open question is how
   many corpora genuinely *mix* vs consistently decompose.
3. ADR (next free number at write time) + production rule
   (`uni.mixed-normalization`, default-off like its siblings) + rare-glyph
   skip-predicate + synthetic tests + docs page under `uni.md`.
4. Lifts the ADR 0053 M-exclusion residual note and the rare-glyph
   composition-mix carve-out question at the same time.

## Explicitly deferred with it

Normalized-grapheme inventory keys for rare-glyph (ADR 0053 future work) —
the same vendored table unlocks it; decide there whether it's worth the key
churn once this rule owns the mixing signal.
