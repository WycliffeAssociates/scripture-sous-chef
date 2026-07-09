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
| `hyg.tab-in-body` | Det | Tab character in verse text | "A tab character in the text." | — |
| `hyg.control-chars` | Det | Invisible control characters | "A run of invisible control characters." | — |
| `hyg.zero-width-misuse` | Det | Stray invisible formatting character | "A stray invisible formatting character." | — |
| `hyg.empty-verse` | Det | Empty verse | "This verse has no text." | — |
| `hyg.invalid-codepoint` | Det | Broken character | "A broken character — the original was lost in conversion." | — |
| `hyg.replacement-run` | Det | Destroyed text (??? runs) | "A run of “?” — text likely destroyed by a bad conversion." | — |
| `struct.source-marker-leftover` | Det | Leftover markup | "Leftover file markup inside the text." | — |
| `struct.merge-conflict-marker` | Det | Merge-conflict leftovers | "A version-control conflict marker committed into the text." | — |
| `uni.combining-mark-without-base` | Det | Accent with nothing to attach to | "An accent mark with no letter in front of it." | — |
| `uni.redundant-zero-width-space` | Det | Doubled invisible word-break | "The invisible word-break character typed twice in a row." | — |
| `lex.duplicate-word` | Det | Doubled word | "The word “{word}” appears twice in a row." | `DuplicateWord { first_sid }` (cross-verse only) |
| `uni.mixed-numeral-systems` | Det | Mixed number systems | "A digit from a different number system than the rest of the verse." | *(proposed)* `target: char` |
| `prop.length-ratio` | Source | Verse length far from its source | "This verse is {ratio_pct}% the length of the source." | `LengthRatio { ratio_pct, scope }` |
| `case.sentence-initial-lowercase` | Corpus | Lowercase sentence start | "This translation capitalizes after ‘{glyph}’ in {upper} of {total} places; this word starts lowercase." | `CasingConvention { glyph, upper, total }` |
| `punct.spacing-anomaly` | Corpus | Inconsistent spacing around punctuation | "‘{mark}’ is {form} {majority} of {total} times ({pct}%); this one is written the other way." | `SpacingConvention { mark, spaced, attached }` |
| `punct.bracket-balance` | Corpus | Unmatched bracket | pairing: "This ‘{glyph}’ has no partner; the translation pairs it {majority} of {total} times." · short-span: "This pair stays open unusually long; {majority} of {total} pairs close within {window} verses." | `BracketWindow { window, measure, majority, total }` |
| `lex.punct-only-token` | Corpus | Stranded punctuation | "A lone punctuation mark, rare here — seen {count} times across {units} words of text." (`{units}` is the total token count — the score is per-token, not per-punctuation) | `PunctOnlyRate { count, units }` |
| `punct.adjacency-anomaly` | Corpus | Unusual punctuation combination | "The punctuation ‘{pattern}’ is unusual here — {k} of {lead_n} times, in {books} of {corpus} books." | `AdjacencyEvidence { pattern, k, lead_n, books, corpus }` |
| `uni.mixed-script-in-token` | Corpus | Mixed alphabets in one word | "This word mixes writing systems — a mix this translation uses in only {books} of {corpus} books." | `ScriptMixEvidence { k, n, books, corpus }` |
| `lex.repeated-character-run` | Corpus | Repeated letter | "‘{ch}’ repeats {run} times here — a repetition this translation doesn't otherwise use." | `RepeatEvidence { ch, run }` |

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
| `punct.spacing-anomaly` | ToDominantForm | drop/insert a space (direction from `spaced` vs `attached`) | ✅ |
| `uni.mixed-numeral-systems` | ToTarget | span digit → `target` (same value, dominant system) | ✅ (needs `target` arg) |
| `uni.mixed-script-in-token` | None *(deferred)* | homoglyph → majority script — needs a confusables table | ❌ |
| `lex.punct-only-token` | None | delete is *usually* right but not certain — review | ❌ |
| `punct.adjacency-anomaly` | None | collapse target ambiguous (`,,`→`,`? `?.`→`?`) | ❌ |
| `lex.repeated-character-run` | None | target repetition count unknown | ❌ |
| `uni.combining-mark-without-base` | None | a letter may be *missing*, not the mark spurious | ❌ |
| `punct.bracket-balance` | None | insertion point of the missing partner is unknown | ❌ |
| `hyg.invalid-codepoint` | None | original character is gone — needs re-import | ❌ |
| `hyg.replacement-run` | None | text destroyed — needs re-import | ❌ |
| `struct.merge-conflict-marker` | None | can't auto-pick a side | ❌ |
| `hyg.empty-verse` | None | nothing to fix *to* | ❌ |
| `prop.length-ratio` | None | semantic, not mechanical | ❌ |

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
