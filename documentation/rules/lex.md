# `lex.*` — Lexical & content-whitespace

Token-aware rules over the UAX #29 word stream (`crate::token::tokenize`),
plus the content-whitespace scan. The `lex.` namespace spans two source
files: `lexical.rs` (the token rules) and `whitespace.rs`
(`lex.excess-h-whitespace`). The id is the stable surface — don't read the
prefix as a promise about which file the code lives in.

---

## `lex.excess-h-whitespace` — doubled spaces mid-clause

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `whitespace.rs`

**Flags** — A run of 2+ horizontal whitespace (space or tab) inside verse
content:
- `a  b` → the double space
- `mid  clause` → the double space

**Clean** — `a b` (single space); `End.  Next` (a double space *after*
sentence-ending punctuation is a legitimate spacing convention); `   a` (a
leading run is not content whitespace).

**Why it matters** — A double space mid-clause is almost always a stray
keystroke. But the classic "two spaces after a period" is a real typographic
convention, so a run that immediately follows sentence-ending punctuation
(`. ! ? : ;`) is protected and not flagged.

**Config** — On/off only.

**Nuance & ADR ties** — Leading runs (before any real content in the verse)
are skipped. Embedded newlines are *not* detected: newlines are absent from
the slice-1 vref projection, and a line break isn't cleanly highlightable
(ADR 0010). The scan is byte-level (every predicate — horizontal whitespace,
sentence-ending punctuation — is ASCII). Ports the semantics of onion's
`scan_excess_content_whitespace`.

**Open issues / future work** — Newline-in-body detection is deferred
(ADR 0010). The sentence-end protection set (`. ! ? : ;`) is fixed; no knob.

---

## `lex.duplicate-word` — the same word twice in a row

> **Severity** Warning · **Default** OFF · **Scope** per-verse · **Knobs** none · **Source** `lexical.rs`

**Flags** — Two consecutive identical tokens (case-insensitive) separated by
**whitespace only**:
- `in the the beginning` → `the the`
- `And And he said` → `And And`

**Clean** — `yes, yes` / `truly, truly I say`: the gap holds a comma, not just
whitespace, so it reads as rhetorical repetition, not a typo.

**Why it matters** — In non-reduplicative languages, `the the` is a
near-perfect typo signal (every en/es ULB hit is a real doubling error).
**But** reduplication is core grammar across much of this tool's audience —
Vietnamese `đời đời` ("forever"), Bantu doubling, and many more — producing
600+ legitimate hits per NT in a reduplicative language (deterministic-batch
calibration). So it **ships disabled** and is enabled per-project where
doubling is genuinely unusual.

**Config** — On/off only, and **off by default**: `Config::v1_defaults`
disables it; `Config::all()` includes it. Turn it on in a project's
`.sous/rules.json` where reduplication isn't a feature of the language.

**Nuance & ADR ties** — The whitespace-only-gap requirement is exactly what
separates a typo (`the the`) from rhetoric (`yes, yes`). Matching is
case-insensitive (`The the` flags). See ADR 0014 and the
`2026-06-09-deterministic-batch` calibration report.

**Open issues / future work** — A corpus-observed reduplication-rate gate
(auto-enable only where doubling is statistically rare in *this* corpus) is
the obvious graduation path into the `labs`/stateful tier, but isn't built —
today it's a manual per-project toggle.

---

## `lex.punct-only-token` — *(write-up pending discussion)*

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `lexical.rs`

In the "needs discussion / understanding" set — the interesting part is the
allow-list reasoning (why a *lone* detached danda or `?` is a legitimate
spacing convention in Nepali/etc. and is **not** flagged, while multi-mark
chunks and stray symbols are). Full write-up to follow.

---

## `lex.repeated-character-run` — *(write-up pending discussion)*

> **Severity** Info · **Default** on · **Scope** per-verse · **Knobs** none (threshold 3 is built in) · **Source** `lexical.rs`

In the "suggestion" set: today it flags 3+ identical letter graphemes
(`heeello`) at a hardcoded threshold. Open question is whether to make it
*observe-and-flag-above-threshold* against the corpus norm (vowel length /
ideophones make long runs legitimate in some languages). Full write-up — and
the threshold/observation design — to follow.
