# ADR 0036: Excess-whitespace reads Unicode classes — `Zs`+tab runs, STerm protection

- **Date:** 2026-07-06
- **Status:** Accepted
- **Amends:** [ADR 0014](0014-deterministic-rule-batch.md)
  (`lex.excess-h-whitespace`'s byte-scan design).

## Context

The rule's byte scan encoded two ASCII lists: horizontal whitespace was only
`0x20`/`\t` (a doubled NBSP — a common paste artifact — was invisible), and
the double-space-after-sentence protection recognised only `. ! ? : ;` — the
identical convention after danda `।`, Ethiopic `።`, Arabic `۔`, or Burmese
`။` was flagged as an error while English got the courtesy. The clearest
remaining Latin-centrism in the deterministic batch (2026-07-06 audit).

## Decision

Move the scan to `char_indices` with both predicates read from the fused
table: horizontal whitespace = `White_Space` minus the line-break scalars
(plus tab), and the protection = UCD **`Sentence_Terminal`** — a new fused
bit (bit 7) generated from a committed `PropList-SentenceTerminal.txt`
extract, following the existing trimmed-UCD-file pattern. `STerm`
specifically, not `Terminal_Punctuation`: the latter includes commas and
list separators, which the protection must not excuse. Mixed runs
(space+NBSP) count as one run.

Note `:` and `;` were in the old ASCII protection set but are **not**
`Sentence_Terminal`; a double space after them now flags. That is the
correct reading — they don't end sentences, and the two-space convention is
a sentence-boundary convention — and it applies equally to every script's
non-terminal separators.

## Consequences

- The protection follows the property: 106-corpus survey shows the
  non-Latin corpora gaining the courtesy while NBSP-run damage becomes
  visible for the first time.
- One new fused-table bit consumed (bit 7); bits 24–31 remain free, bit 6
  still reserved for the clinging class.
- The scan is chars, not bytes — negligible cost (the rule walks each verse
  once either way, and the fused lookup is one indexed read).
