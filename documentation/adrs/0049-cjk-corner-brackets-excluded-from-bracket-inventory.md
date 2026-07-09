# ADR 0049: CJK corner brackets are quotation marks — excluded from the bracket inventory

- **Date:** 2026-07-09
- **Status:** Accepted
- **Amends:** [ADR 0037](0037-bracket-balance-corpus-relative.md) (the
  `BRACKET_PAIRS` inventory it defines).
- **Relates to:** [ADR 0039](0039-quote-balance-deferred.md) (quote balance
  stays deferred; this keeps quote structure out of the bracket door).

## Context

The 2026-07-09 fleet survey (1,504 corpora) showed `punct.bracket-balance`
surfacing 4,578 findings, of which three Chinese editions carried 58%
(cmncbt 1,556; cmn-cu89s 543; cmn-cu89t 539). The audit
([calibration/2026-07-09](../calibration/2026-07-09-bracket-balance-cjk-audit.md))
found **100% of that volume comes from the CJK corner-bracket families**
「」 (U+300C/D) and 『』 (U+300E/F) — not from any glyph that is a bracket in
practice.

The corner brackets are `Ps`/`Pe` in the UCD, so they land in
`BidiBrackets.txt` and thence in `BRACKET_PAIRS`. But in Chinese/Japanese
typography **they are quotation marks**: 「」 is the primary quote, 『』 the
nested quote (and halfwidth ｢｣ U+FF62/63 the same). Running a LIFO bracket
matcher over them turns `punct.bracket-balance` into a de-facto quote-balance
rule.

Quote balance is deliberately **deferred** (ADR 0039) for exactly the reason
this storm exhibits: dialogue quoting nests deeply and *re-opens across
verse/paragraph boundaries without closing*, so a stack cannot tell the
continuation convention from a real unmatched opener. The sample inventories
are unmistakable — cmn-cu89s DEU 5:6–5:20 (Ten Commandments) is a run of
`「o! 『o! 「o! 『o! …`, each verse re-opening the speaker and divine-speech
quotes without closing. This is the ADR 0039 phenomenon, character-for-
character, having entered through the bracket door.

The prior bracket calibration (2026-07-06) surveyed 106 corpora and recorded
"CJK corners … none stormed"; that survey predated the Chinese and Japanese
editions in the fleet, so the collision was simply unobserved.

## Decision

**Exclude the corner-bracket family — 「」 (U+300C/D), 『』 (U+300E/F), and
halfwidth ｢｣ (U+FF62/63) — from `BRACKET_PAIRS`**, at generation time in
`xtask gen_charclass_table.rs`, mirroring the documented FD3E/FD3F supplement
pattern ADR 0037 established. The generator now applies a documented exclusion
list after parsing `BidiBrackets.txt` and regenerates `charclass_table.rs`.

Every other CJK bracket that is a genuine text delimiter stays in: 《》 title
marks, 〈〉 angle brackets, 【】 lenticular, （）［］ fullwidth parens/brackets.
The audit confirmed these pair at 99.7–100% and produce essentially no orphans
(the two surviving cmn-cu89s findings are legitimate one-sided （）).

The fix is at the **inventory boundary, not the matcher**. The matcher is
correct — it pairs a LIFO stack over declared pairs. The defect was the
inventory asserting that quotation marks are brackets. No script identity is
consulted at runtime (ADR 0037's principle holds); the exclusion is a fixed,
documented six-glyph list baked into the generated table.

The glyphs remain `Ps`/`Pe` punctuation in the fused class table — their
General_Category is unchanged. Only their membership in the *pairing
inventory* is removed, so `bracket_close_of` / `bracket_open_of` return `None`
for them and `match_book` skips them entirely.

## Consequences

- Fleet bracket-balance surfaced volume 4,578 → 1,920 (−58%); only four
  corpora change (cmncbt 1,556→0, cmn-cu89s 543→2, cmn-cu89t 539→1, jpn1965
  23→0), no corpus rises. Surgical.
- Chinese/Japanese editions lose bracket-balance coverage of their *text*
  brackets? No — they keep 《》（）【】. They lose the spurious quote coverage
  only.
- When a purpose-built quote engine ships (ADR 0039 revisit criteria), corner
  brackets become its inventory — a directional, continuation-aware pair set,
  not a LIFO bracket stack. This ADR does not build that; it stops the leak.
- Residual: the "85% of floor-0 candidates clear the floor" property drops to
  70% but persists — it is inherent to the corpus-relative score (an orphan in
  a family the corpus pairs at 90%+ scores ~0.9 by construction). The floor
  ranks rather than gates; that is ADR 0037's intended behavior.
- `charclass::bracket_open_of` / `bracket_close_of` are now `pub` (were
  `pub(crate)`) so the calibration harness (`--bracket` mode) can classify
  families exactly as the rule does. No production behavior change.
