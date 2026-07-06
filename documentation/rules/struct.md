# `struct.*` — Structural markup leftovers

Markup that should never survive ingest into plain verse text. onion strips
USFM and the editor strips HTML *before* text reaches the analyzer, so any
remnant the analyzer sees is an **ingest bug upstream**, not a translation
issue. Both rules are pure byte scans with no language sensitivity and no
config beyond on/off.

Source: `crates/core/src/signals/structural.rs`. Both ship in the
deterministic default batch (ADR 0014).

---

## `struct.source-marker-leftover` — USFM / HTML markup leftovers

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags**
- `In the \v 2 beginning` → the `\v` marker
- `word \f + \ft note \f* more` → `\f`, `\ft`, `\f*` (footnote markers, including the `*` close)
- `a \+nd Lord\+nd* b` → `\+nd`, `\+nd*` (nested-marker `\+` form)
- `a <b>bold</b> word` → `<b>` and `</b>` (raw HTML/XML tags)
- `a \ b` → a lone backslash (no place in scripture body)

**Does *not* flag** — bare `|` or `^`. USFM's text grammar
(`([^\\]|\\[/~\\|])+`, "simple text up to the next marker") treats every
non-backslash byte, pipes and carets included, as legitimate content — only
the backslash is special. A surviving `|`/`^` would signal a buggy USFM
*parser* upstream, which is that parser's job to fix, not a property of the
translation (ADR 0026). A caret wedged mid-word (`b^bê`) is a real oddity,
but it's a punctuation-usage anomaly for a probabilistic rule to surface, not
a deterministic markup scan.

**Why it matters** — The text that reaches the analyzer is supposed to be
clean, projected verse content. A surviving `\v` or `<br/>` means the
USFM/HTML stripping pipeline upstream broke or was skipped. This is the
highest-value scan in the deterministic batch: it catches whole-pipeline
failures that would otherwise corrupt every downstream rule's view of the
text.

**Config** — On/off only. The pattern set is fixed; there are no knobs.

**Nuance & ADR ties**
- **Prose angle brackets are safe.** `5 < 7 and 7 > 5` does *not* flag. A
  `<…>` only counts as a tag when an ASCII letter immediately follows `<`
  (or `</`) **and** a closing `>` appears later in the same verse. An
  unclosed `<` (e.g. `a <unclosed forever`) never flags.
- **Backslash markers are matched structurally**, not against a known-marker
  list: backslash, optional `+`, ASCII alphanumerics, optional closing `*`.
  So an unknown or future USFM marker still flags.
- **The diff3 base marker (`|||||||`) belongs to
  `struct.merge-conflict-marker`,** not here. A bare `|` is legitimate USFM
  text; only a *run* of three or more is conflict evidence, so the pipe run
  lives with the other conflict-marker runs (see that rule's note).
- Language-blind by construction (`structural.rs` header) — no corpus stats,
  same behavior in every script.

**Open issues / future work** — None outstanding. The HTML matcher accepts
any `<letter…>` shape rather than validating against real tag names; that's
intentional (no scripture body legitimately contains `<…>`) but is the
obvious place to tighten if a corpus ever uses angle brackets in prose.

---

## `struct.merge-conflict-marker` — committed git conflict markers

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none

**Flags** — A run of **3 or more** identical `<`, `=`, `>`, or `|`:
- `<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature/x` → `<<<<<<<`, `=======`, `>>>>>>>`
- `||||||| merged common ancestors` → `|||||||` (the diff3 base marker)
- `ours=======theirs` → `=======` (fires even when a projection collapsed the marker's newlines)
- `<<<`, `>>>>` → fire (below git's default 7-char marker size)

**Why it matters** — A saved git merge conflict means two versions of a verse
got committed without anyone resolving them. The presence of these marker
heads is unambiguous evidence of that. No scripture text legitimately repeats
one of `< = >` three times in a row, so the bar is cheap and effectively
false-positive-free.

**Config** — On/off only. The minimum run length (`MIN_RUN = 3`) is a
built-in constant.

**Nuance & ADR ties**
- **We deliberately don't match git's exact form** (`<<<<<<<` seven chars,
  line-anchored). A non-default `conflict-marker-size`, a truncated paste, or
  a projection that dropped the marker's surrounding newlines would all slip
  past the strict form. Matching any 3+ run catches every variant at no
  false-positive cost.
- `<<` / `==` / `||` (runs of two) are ordinary text — quotes, a rule
  fragment, a bare USFM pipe — and do **not** flag.
- **The diff3 base marker (`|||||||`) is matched here** alongside the other
  conflict heads (ADR 0026). A bare `|` is legitimate USFM text, so
  `struct.source-marker-leftover` deliberately ignores it; only a run of
  three or more pipes is conflict evidence, which is this rule's job.
- Language-blind: the run is ASCII punctuation, never script.

**Open issues / future work** — None.
