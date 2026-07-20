# Rule messaging & fix-capability reference

- **Created:** 2026-07-08
- **Scope:** every `RuleId` — its verdict kind, the default English
  user-facing text, the structured message args it carries (or should), and
  whether a front end can synthesize a `replace(haystack, needle)` from those
  args alone.
- **Related:** [ADR 0038](../adrs/0038-rule-catalog-two-tier-config.md) (the
  `RuleCard` catalog), [ADR 0048](../adrs/0048-descriptive-share-args-for-dominance-rules.md)
  (raw convention share alongside the Wilson score), [ADR 0010](../adrs/0010-pure-analyzer-contract-v1-reset.md)
  §6 (`FindingArgs`).

## How labels and localization work

Sous Chef ships its **own default English** for every rule and localizes
**by `code`** upstream — nobody has to accept our wording, but every rule is
accounted for:

- **Rule cards** (`crates/core/src/catalog.rs::card`) hold the static
  `title` / `what` / `why` / enable-question text. The `match id { … }` is
  **exhaustive over `RuleId`**, so adding a rule without a card fails to
  compile — the completeness guarantee the localization surface depends on.
- **Finding messages** are *structured, never rendered strings*
  (`FindingArgs`, `#[serde(tag = "kind")]` closed union). The engine emits
  named args; the consumer's ICU layer renders the sentence, keyed on `code`
  + `kind`. The same args fund the `replace()` — one payload, two uses.
- **Default English rendering** ships in `catalog::message(code, args) ->
  String` — an **exhaustive `match id`** that renders the fallback sentence
  from the args (no statistics vocabulary). An upstream consumer ignores this
  string and localizes off `code` + args; the playground displays it directly.
- **The engine ships data, not functions.** Nothing here is a serialized
  closure; it works identically in Rust and wasm, and no token array or patch
  set crosses the wire — only a handful of `Copy` fields per finding.

### Wire discipline (the constraints these args respect)

- Single glyphs/marks are `char` (4 B, `Copy`) — never a heap `String`.
  **Intentional exception:** `uni.mixed-normalization`'s `example` is a
  `String`. Its domain is an NFC-normalized grapheme cluster, not a single
  glyph — composition exclusions (Bengali `U+09DF` → `U+09AF U+09BC`) and
  multi-mark clusters can be more than one scalar, so `char` cannot
  represent it (ADR 0063).
- Counts are `u32` / rates are `f32` — `Copy`, set at `judge`, no extra alloc
  beyond the `Finding` itself.
- The only unavoidable per-finding string is `punct.adjacency-anomaly`'s
  2–4-char pattern run (a rare rule).
- Per-finding args are ≤ ~24 bytes. No whole-verse or token-array round-trips.
- Fix *capability* is a **static** `FixKind` on the `RuleCard` (zero
  per-finding cost); only genuine runtime values (dominant form, target
  digit) live in `FindingArgs`.

## Verdict kinds

| Kind | Meaning | Score? | Sensitivity dial? |
|---|---|---|---|
| Deterministic | fixed mechanical condition | no | no |
| CorpusRelative | judged against this translation's own patterns | yes `[0,1]` | yes (`emit_score_min`) |
| SourceRelative | judged against the paired source text | yes | yes |

## Messaging — default English + structured args

`{…}` are ICU placeholders. Where a share % or "form" word is shown, the
consumer derives it from the raw counts (e.g. an ICU `select` on which count
is larger); the engine ships the counts, not the derived value.

| Code | Kind | Default title | Default finding message (EN) | Structured args (`FindingArgs`) |
|---|---|---|---|---|
| `lex.excess-h-whitespace` | Det | Doubled spaces | "Two or more spaces in a row." | — |
| `hyg.tab-in-body` | Det | Tab character in verse text | "A tab character in the verse text." | — |
| `hyg.control-chars` | Det | Invisible control characters | "A run of invisible control characters." | — |
| `hyg.zero-width-misuse` | Det | Stray invisible formatting character | "A stray invisible formatting character." | — |
| `hyg.empty-verse` | Det | Empty verse | "This verse has no text." | — |
| `hyg.invalid-codepoint` | Det | Broken character | "A broken character — the original was lost in a conversion." | — |
| `hyg.replacement-run` | Det | Destroyed text (??? runs) | "A run of “?” marks — text likely destroyed by a failed encoding conversion." | — |
| `struct.source-marker-leftover` | Det | Leftover markup | "Leftover file markup inside the verse text." | — |
| `struct.merge-conflict-marker` | Det | Merge-conflict leftovers | "A version-control merge-conflict marker committed into the text." | — |
| `uni.combining-mark-without-base` | Det | Accent with nothing to attach to | "An accent mark with no letter in front of it to attach to." | — |
| `uni.redundant-zero-width-space` | Det | Doubled invisible word-break | "The invisible word-break character typed twice in a row." | — |
| `lex.duplicate-word` | Det | Doubled word | "The same word appears twice in a row." (cross-verse variant, when `DuplicateWord` args are present: "This repeats the last word of the previous verse.") | `DuplicateWord { first_sid }` (cross-verse only; within-verse carries no args) |
| `uni.mixed-numeral-systems` | Det | Mixed number systems | "A digit from a different number system than the rest of the verse." | *(proposed)* `target: char` |
| `prop.length-ratio` | Source | Verse length far from its source | "This verse is {ratio_pct}% the length of the same verse in the source." (default, no args: "This verse is a very different length from its source.") | `LengthRatio { ratio_pct, scope }` |
| `case.sentence-initial-lowercase` | Corpus | Lowercase sentence start | "This translation capitalizes after ‘{glyph}’ in {upper} of {total} places; this word starts lowercase." (variants: "…after ‘{glyph}’ closing a quote…" when `quoted`; "…the first word after a sentence break…" when `glyph` is `None`, the book-initial word) | `CasingConvention { glyph: Option<char>, quoted, upper, total }` (`quoted` = the close-quote boundary class, ADR 0052) |
| `case.inconsistent-word-casing` | Corpus | Inconsistent word capitalization | "This translation writes ‘{word}’ capitalized in {upper} of {total} places; here it is lowercase." | `WordCasing { word, upper, total }` |
| `case.mixed-case-word` | Corpus | Odd capital inside a word | "‘{word}’ has a capital in the middle here — this translation writes it that way {other} of {total} times." | `MixedCaseWord { word, other, total }` |
| `punct.spacing-anomaly` | Corpus | Inconsistent spacing around punctuation | "‘{mark}’ is {form} on the {side} to {a word/a number/a mark} in only {count} of {total} places ({pct}%)." — one clause per violated side, joined by "; and " | `SpacingConvention { mark, left: Option<SpacingSide>, right: Option<SpacingSide> }` (each `SpacingSide { form, class, count, total }`, ADR 0054) |
| `punct.bracket-balance` | Corpus | Unmatched bracket | pairing: "This bracket has no partner — the translation pairs it in {majority} of {total} places." · short-span: "This bracket pair stays open unusually long — {majority} of {total} pairs close within a few verses." | `BracketWindow { window, measure, majority, total }` |
| `lex.punct-only-token` | Corpus | Stranded punctuation | "A lone punctuation mark, rare here — seen {count} times across {units} words of text." (`{units}` is the total token count — the score is per-token, not per-punctuation) | `PunctOnlyRate { count, units }` |
| `punct.adjacency-anomaly` | Corpus | Unusual punctuation combination | "The punctuation ‘{pattern}’ is unusual here — it appears {k} of {lead_n} times, in {books} of {corpus} books." | `AdjacencyEvidence { pattern, k, lead_n, books, corpus }` |
| `uni.mixed-script-in-token` | Corpus | Mixed alphabets in one word | "This word mixes writing systems — a mix this translation uses in only {books} of {corpus} books." | `ScriptMixEvidence { k, n, books, corpus }` |
| `lex.repeated-character-run` | Corpus | Repeated letter | "‘{ch}’ repeats {run} times here — a repetition this translation doesn't otherwise use." | `RepeatEvidence { ch, run }` |
| `uni.rare-glyph` | Corpus | Barely-used letter | "The letter ‘{glyph}’ appears only {count} times in this whole translation." | `RareGlyph { glyph, count }` |
| `uni.mixed-normalization` | Det | Mixed character encoding | "This text writes ‘{example}’ in two different encodings in {affected} places." | `Normalization { affected, example }` |

## Fix capability — what a front end can `replace()`

`FixKind` is a **static** property of the rule (proposed for `RuleCard`). The
front end reads it off the card it already fetches and synthesizes the edit
from the op + the finding's `range` + verse text; only `ToDominantForm` /
`ToTarget` need a value from `FindingArgs`.

| Code | `FixKind` | Replace the front end builds | Bulk-safe (`updateSafe`) |
|---|---|---|---|
| `lex.excess-h-whitespace` | CollapseWhitespace | span → `" "` | ✅ |
| `hyg.control-chars` | Delete | span → `""` | ✅ |
| `hyg.zero-width-misuse` | Delete | span → `""` | ✅ |
| `struct.source-marker-leftover` | Delete | span → `""` | ✅ |
| `uni.redundant-zero-width-space` | CollapseRun | span → one ZWSP | ✅ |
| `hyg.tab-in-body` | CollapseWhitespace | span → `" "` | ✅ |
| `lex.duplicate-word` | DropDuplicate | keep the first word | ✅ within-verse |
| `case.sentence-initial-lowercase` | Uppercase | `range` letter → `char::to_uppercase` | ✅ |
| `case.inconsistent-word-casing` | Uppercase | `range` letter → `char::to_uppercase` (capitalize the flagged lowercase occurrence) | ✅ |
| `punct.spacing-anomaly` | ToDominantForm | drop/insert a space (direction from `spaced` vs `attached`) | ✅ |
| `uni.mixed-numeral-systems` | ToTarget | span digit → `target` (same value, dominant system) | ✅ (needs `target` arg) |
| `uni.mixed-script-in-token` | None *(deferred)* | homoglyph → majority script — needs a confusables table | ❌ |
| `lex.punct-only-token` | None | delete is *usually* right but not certain — review | ❌ |
| `punct.adjacency-anomaly` | None | collapse target ambiguous (`,,`→`,`? `?.`→`?`) | ❌ |
| `lex.repeated-character-run` | None | target repetition count unknown | ❌ |
| `case.mixed-case-word` | None | correct clean shape unknown (could be all-lower or titlecase) — review | ❌ |
| `uni.rare-glyph` | None | the intended letter is unknown — a stray that needs review or re-import | ❌ |
| `uni.combining-mark-without-base` | None | a letter may be *missing*, not the mark spurious | ❌ |
| `punct.bracket-balance` | None | insertion point of the missing partner is unknown | ❌ |
| `hyg.invalid-codepoint` | None | original character is gone — needs re-import | ❌ |
| `hyg.replacement-run` | None | text destroyed — needs re-import | ❌ |
| `struct.merge-conflict-marker` | None | can't auto-pick a side | ❌ |
| `hyg.empty-verse` | None | nothing to fix *to* | ❌ |
| `prop.length-ratio` | None | semantic, not mechanical | ❌ |
| `uni.mixed-normalization` | None *(project-wide action, not per-finding)* | bulk `text.normalize("NFC")` over every verse in the project — no dominant form, example, or count needed; gated on the editor adopting a whole-project resident `Galley` (ADR 0062/0063 §11) | ❌ |

### `updateSafe()` — bulk apply

A front end can offer a one-switch "apply all safe fixes" over a verse or a
selection: iterate findings, and for each whose card `FixKind` is in the safe
set (everything marked ✅ above), apply the implied op to its `range`. All
front-end-side, right-to-left by offset so spans don't shift. No engine wire
cost — the ops are inferred from `code` + `FixKind`, the values (where needed)
already ride in `FindingArgs`.

## Status (2026-07-08)

- **Shipping:** every scored rule's structured args —
  `SpacingConvention`, `CasingConvention`, `BracketWindow` (measure + share),
  `PunctOnlyRate`, `AdjacencyEvidence`, `ScriptMixEvidence`, `RepeatEvidence`
  (ADR 0048). Plus `catalog::message(code, args)` — the default English label
  for **every** rule, rendered from those args (deterministic rules render
  static text). The playground displays it in the preview and drill-down.
- **Proposed, not built:** `FixKind` on `RuleCard` and any `updateSafe()`
  affordance; the numeral `target` arg (deferred — deterministic rules go
  through `PerVerseRule::check -> Vec<Span>`, which carries no args; adding it
  means widening that trait, which we're not doing just for a deferred fix).
- **Deferred by decision:** `mixed-script` homoglyph fix (confusables table).
