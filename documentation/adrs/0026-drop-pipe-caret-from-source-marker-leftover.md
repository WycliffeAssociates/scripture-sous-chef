# ADR 0026: Drop `|`/`^` from source-marker-leftover — a bare pipe is valid USFM text

- **Date:** 2026-07-06
- **Status:** Accepted
- **Builds on:** [ADR 0014](0014-deterministic-rule-batch.md) (the deterministic
  batch that introduced `struct.source-marker-leftover` and
  `struct.merge-conflict-marker`).
- **Amends:** `struct.source-marker-leftover` — removes its `|` and `^` arms.
  Moves the diff3 base marker (`|||||||`) into `struct.merge-conflict-marker`,
  superseding the "pipe overlap is deliberate" coordination note both rules
  carried.

## Context

`struct.source-marker-leftover` flagged four byte classes as markup that should
never survive ingest into plain verse text: backslash markers (`\v`, `\f`, …),
raw `<…>` HTML/XML tags, and — the subject of this ADR — bare `|` pipes and `^`
carets, on the theory that they were USFM attribute/special-text remnants
(`\w grace|strong="G5485"\w*` losing its markers but keeping the pipe).

That theory doesn't survive the USFM grammar. The "text" production — simple
text up to the next marker — is:

```
([^\\]|\\[/~\\|])+
```

The only special character in USFM running text is the **backslash**; it must
be followed by a marker letter or one of the four escape chars `/ ~ \ |`. Every
other byte — pipes and carets included — is the `[^\\]` branch: legitimate
content. A raw `|` or `^` between markers is spec-valid text.

So a surviving `|`/`^` in projected verse text tells us nothing about the
*translation*. At most it signals a buggy USFM **parser** upstream — and that is
the parser's bug to fix, not a property this analyzer should assert about the
text. Two concrete symptoms of the old behaviour:

- **False positives on legitimate content.** Any verse using `|` or `^` as text
  fired. Observed in the wild in e.g. `sô turu ané bêbê whã b^bê supitu é mé.` —
  a real caret-in-word oddity, but not markup.
- **Double-flagging the one blessed escape.** For `\|` (the spec's *correct* way
  to write a literal pipe), the backslash arm flagged the `\` and the pipe arm
  flagged the `|` — two findings on valid input.

The caret-in-word case (`b^bê`) is genuinely interesting, but it is a
**punctuation-usage anomaly** — a punctuation character wedged where script
letters belong — and belongs to a probabilistic, corpus-relative rule
(`crate::signals::punctuation`, cf. ADR 0024), not a deterministic markup scan
that can only say "present / absent."

## Decision

1. **`struct.source-marker-leftover` no longer flags bare `|` or `^`.** The two
   arms are deleted (no shim — pre-alpha). The rule keeps its two well-founded
   arms: backslash markers and `<…>` tags, both of which are genuine
   ingest/strip bugs when they survive.
2. **The diff3 base marker moves to `struct.merge-conflict-marker`.** That rule
   now matches runs of three or more `<`, `=`, `>`, **or `|`**. A bare `|` is
   legitimate text, but no scripture body repeats a pipe three times, so a
   *run* is unambiguous conflict evidence — the same low-bar, false-positive-free
   reasoning the other three conflict heads already use. This preserves diff3
   coverage that would otherwise have been lost when the pipe arm left
   source-marker-leftover.

## Rationale

- **Scope discipline.** The analyzer judges translations, not parser
  correctness. A leftover attribute pipe is an upstream-tooling defect with a
  clear owner (the USFM parser); catching it here conflates two very different
  failure classes and generates noise the translator can do nothing about.
- **Spec-first, not heuristic.** The grammar is explicit that `|`/`^` are text.
  Flagging them was a heuristic guess about what *might* have produced them; the
  guess is wrong often enough (any legitimate use) to fail calibration.
- **The interesting signal isn't lost — it's rehomed.** The mid-word caret is
  real, but its right treatment is statistical punctuation-usage analysis, where
  context ("inside a word") is what makes it anomalous. A deterministic
  presence scan can't express that and shouldn't try.
- **Delete, don't shim.** Consistent with the project's pre-alpha stance
  (no back-compat layer): the arms are removed outright.

## Consequences

- Bare `|`/`^` in verse text no longer produce `struct.source-marker-leftover`
  findings. Corpora that legitimately use these characters stop generating false
  positives, and the `\|` double-flag is gone.
- `struct.merge-conflict-marker` gains the diff3 base marker (`|||||||`); its
  match set is now `< = > |` at run-length ≥ 3. Runs of one or two pipes remain
  clean.
- **Capability deferred:** a caret/pipe wedged mid-word is currently unflagged
  until the punctuation-usage rule covers it. Accepted, documented tradeoff —
  the deterministic scan was the wrong home for it regardless.
- `struct.source-marker-leftover` is now strictly about backslash markers and
  HTML/XML tags — markup that has *no* legitimate place in projected verse text,
  a cleaner boundary than the mixed "markup + maybe-legitimate-punctuation" set
  it carried before.
