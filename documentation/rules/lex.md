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

## `lex.repeated-character-run` — corpus-unusual repeated letter graphemes

> **Severity** Info · **Default** on · **Scope** stateful corpus · **Knobs** `convention_rate_per_10k`, `word_recurrence_k`, `emit_score_min` · **Source** `lexical.rs`

**Flags** — Three or more identical extended grapheme clusters where both the
cluster and its containing word are unusual for this corpus:

- `joyfullly` in English → `lll`, score 0.994 in calibration
- `guerrras` in Spanish → `rrr`, score 0.974
- a copied `destruccción` occurring twice still surfaces at 0.790
- Thai `ภรรรยา` (a tripled ro han in `ภรรยา`, "wife") → `รรร` — rare
  corpus-wide, so it surfaces even though no UAX #29 token contains it

**Clean** — Double letters (`bookkeeper`); digits/punctuation; U+0640 tatweel
kashida stretching; established vowel length/ideophones; and recurring
scriptio-continua joins such as Thai `ขอออก` where the `อออ` spans two words.

**Why it matters** — A third repeated letter is a strong typo clue in many
languages, but a universal verdict creates thousands of false positives in
languages that use long vowels, expressive repetition, or unspaced word joins.
The rule learns those conventions from the project itself. No language or
script identity is consulted.

**Scoring** — The fixed threshold-three grapheme scan supplies candidates. The
score multiplies two corpus-relative factors:

```text
cluster_rate = raw cluster-run events * 10,000 / whitespace lexical units
cluster_factor = max(0, 1 - cluster_rate / convention_rate_per_10k)
word_factor = max(0, 1 - (containing_word_frequency - 1) / word_recurrence_k)
score = cluster_factor * word_factor
```

When UAX #29 supplies no containing token, `word_factor = 1.0`; raw run
recurrence still suppresses scriptio-continua conventions. The denominator is
whitespace-delimited lexical units, not UAX token count: Thai/Lao UAX word
segmentation produced one token per grapheme and diluted real recurrence.

**Config** — Defaults are `2.0` runs per 10k lexical units, word recurrence
`K = 5`, and emission floor `0.5`. Lower the convention rate or raise the floor
for fewer findings. The rule is default-on; map its `RuleId` to `false` to skip
both reduction and judgment.

**Nuance & ADR ties** — Tatweel (U+0640) is excluded in the scan itself, not
by scoring: kashida is a stretching control whose repetition is inherently
typographic (`الإيمــــــان` is one word, elongated), so runs of it can never
be the doubled-letter error this rule hunts. That is a one-character
Unicode-semantics carve-out, not a script allow-list — the no-script-identity
principle (ADR 0023/0025) stands. Stats are aggregate-only and partitioned per book:
cluster counts, run-containing word counts, and lexical-unit count, with no
stored sites. Incremental analysis replaces one book's aggregates, scores
against the retained corpus, and re-scans only the supplied target verses for
spans. The cluster key is the full first grapheme lowercased, so case variants
pool while combining marks remain significant. Run length above three adds no
weight. See ADR 0028 and the 2026-07-06 calibration report.

**Open issues / future work** — Systematic typos suppress like conventions;
corpus counts cannot infer intent. Multi-grapheme morphological reduplication
such as Gujarati `દાદાદાદી` is outside this detector and remains a known
conflation if it happens to contain a single-cluster triple.
