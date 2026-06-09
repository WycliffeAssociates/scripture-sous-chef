# Execution brief: deterministic rule expansion (v0.0.3)

- **Date:** 2026-06-09
- **Audience:** a fresh agent thread executing autonomously.
- **Status:** plan, not yet started. **Sequencing:** runs *after*
  `2026-06-09-proportionality.md` (proportionality is the preferred next
  rule). This batch is the follow-on.

You are expanding scripture-sous-chef from its single shipped content rule
(`lex.excess-h-whitespace`) plus the hygiene set to a **batch of
deterministic, trust-on-sight rules** graduated from `labs`, reshaped to
the v1 contract. You will also build the one piece of shared infra they
need (a word tokenizer). You will **not** ship any statistical / corpus-
model / learned rule. Read this whole brief, then the contracts it cites,
before writing code.

## Non-negotiable contract (read these first)

- `documentation/adrs/0010-...md` — pure analyzer: text in → addressable
  findings out. **No file IO, no onion calls, no verse segmentation in
  core.** Byte `Span` into the *given* verse text; `sid`-anchored; no
  rendered messages (the consumer localizes from the code).
- `documentation/adrs/0011-...md` — statefulness/incrementality. Anything
  needing a corpus model, cross-verse state, or per-edit refit is **out
  of scope here** (that's the Mode-B/labs future).
- `documentation/adrs/0012-...md` — `RuleId` is a **closed enum**; it is
  the typed config + localization surface. Adding a rule follows a fixed
  recipe (below). `Config` enable/disable already exists.
- `documentation/vision.md` §8 — the rule catalog and per-rule default
  severities. §10 — the calibration / finding-volume bar.

**Spirit guardrails (do not violate):**
1. No corpus statistics, KN/Dunning/BK-tree, learned weights, or scoring
   models. `score` stays `None` for every rule here.
2. No global offsets — byte `Span` into the verse `&str` you were handed.
3. Every rule is **deterministic** and either trust-on-sight (ship
   enabled) or, where there's real cross-language FP risk, ships at `Info`
   or **default-disabled** (documented). When unsure, calibrate (below)
   before deciding severity / default-enabled.
4. Word tokenization for lexical rules **is** in scope and is distinct
   from the verse/coordinate segmentation ADR 0010 reserves for onion. You
   may tokenize a verse's text into words; you may not derive verse text
   or coordinates.
5. Keep the closed-enum / `Config` / Tsify machinery intact — the `.d.ts`
   unions and the consumer's exhaustive maps must keep working.

## Foundational infra (build first — it's the centerpiece)

**`crates/core/src/token.rs` — a UAX #29 word tokenizer.** Reshaped from
the parked `labs` `analysis::tokenize` (`documentation/vision.md` §5.3,
§5.5). `unicode-segmentation` is already a core dep.

```rust
pub struct Token { pub span: Span }      // byte range into the verse text
pub fn tokenize(text: &str) -> Vec<Token> // UAX#29 word boundaries; words only
```

- Words only (skip whitespace/punctuation-only segments — those are their
  own rules). Grapheme-aware where length matters.
- Default plain UAX #29. A future `include_chars` knob (apostrophes,
  hyphens, ZWJ) is **deferred** — do not build the config for it now.
- Unit-test against multi-script samples. Deterministic, sub-ms.

This unlocks the token-aware rules (duplicate-word, punct-only-token,
repeated-character-run).

## The rules, in priority order

Add each as its own `RuleId` variant. Ship P0 first (each is
independently shippable); then P1; P2 only if quota remains. Group modules
sensibly: extend `signals/hygiene.rs` for the Unicode ones; add
`signals/punctuation.rs`, `signals/lexical.rs`, `signals/structural.rs`.

### P0 — deterministic, language-agnostic, ship enabled

| code | family | what it flags | notes |
|---|---|---|---|
| `struct.source-marker-leftover` | STRUCT | USFM backslash markers (`\v \p \f \w …`), caret/pipe attribute remnants, raw `<…>` HTML/XML tags in verse text | **Highest value** — catches ingest bugs. Pure scan. Warning. |
| `punct.repeated-punct` | PUNCT | runs of 2+ identical punctuation (`,,`, `..`, `;;`) and disallowed mixed runs | built-in allow-list: `...` ellipsis, `?!`/`!?`, `--`/`—`. Warning. |
| `lex.duplicate-word` | LEX | two consecutive identical tokens (case-insensitive) | needs tokenizer. No built-in exceptions (Hebraic doubling → consumer suppression). Warning. |
| `lex.punct-only-token` | LEX | a token that is entirely punctuation/symbols (not a word, not a number) | needs tokenizer. **digit-only is deferred** (legit numerals). Warning. |
| `uni.combining-mark-without-base` | UNI | a combining mark at verse/token start or after whitespace/punctuation | extend `unicode.rs`. Warning. |
| `uni.mixed-script-in-token` | UNI | one token mixing scripts (Latin+Cyrillic homoglyphs etc.), ignoring Common/Inherited | uses `script.rs`. Catches homoglyph/encoding errors. Calibrate; likely Warning, maybe Info. |

### P1 — deterministic, mild FP risk (ship, lean `Info`, calibrate)

| code | family | what it flags | notes |
|---|---|---|---|
| `lex.repeated-character-run` | LEX | 3+ identical letters in a token | threshold built-in = 3. Info (language-variable). |
| `uni.mixed-numeral-systems` | UNI | a verse mixing two numeral systems (ASCII + local-script digits) | via Unicode numeric category / script. Info/Warning. |
| `punct.placeholder-leftover` | PUNCT | drafting placeholders: `[TODO]`, `[?]`, `???`, `***`, `<...>` | conservative built-in set. Warning. |
| `punct.bracket-balance` | PUNCT | unbalanced `()` `[]` `{}` within a verse | **quotes excluded** (cross-verse — deferred to book-scope per ADR 0011). Per-verse only. Info/Warning. |

### P2 — language-sensitive (ship **default-disabled**, opt-in, documented)

| code | family | what it flags | why default-off |
|---|---|---|---|
| `punct.space-before-punct` | PUNCT | space before `,.;:?!` | French/typographic conventions legitimately differ. |
| `case.sentence-initial-lowercase` | CASE | sentence-initial token lowercase (cased scripts) | needs a sentence-boundary heuristic; script-dependent. |

### OUT OF SCOPE — do NOT ship, do NOT stub dead code

hapax-suspicion, ngram-rarity, char-KN orthographic, mixed-casing-vs-
corpus, proper-noun-case-consistency, similar-token-cluster (BK-tree),
the proportionality family (length/token/punct-density ratio — graduation
#2, needs config thresholds + the statefulness ladder), repeated-phrase-
proximity, cross-verse-token-boundary, quote-direction / quote-balance
(cross-verse), missing/extra-verse (FP-heavy on in-progress drafts),
verse-order (needs original order, absent from the sorted `VerseMap`).

These need a corpus model, source+threshold config, cross-verse state, or
carry high draft-time FP. **Leave them parked. Do not add `AnalysisContext`
scaffolding or empty stats modules** — that's dead code and violates the
"build it when forced" discipline (ADR 0011). The deterministic batch *is*
the wiring: it establishes the tokenizer + the per-rule pattern these will
later follow.

## Per-rule recipe (the pattern — follow it for every rule)

1. **`diagnostics.rs`** — add the `RuleId` variant, `#[serde(rename =
   "family.kebab")]` with the exact code, a `code()` match arm, and an
   `ALL` entry. The compiler enforces `code()`/`ALL`; the existing
   `rule_id_wire_strings_are_stable` test enforces the rename.
2. **`signals/<family>.rs`** — a `PerVerseRule` impl returning byte
   `Span`s; the runner stamps `sid`/`code`/`severity`. Keep a
   `pub const NAME: RuleId = RuleId::Variant` alias for readable refs.
3. **`rule.rs`** — register in `per_verse_rules()`. (P2 rules: register
   them but ship them default-disabled — see below.)
4. **Severity** per the tables / vision §8.
5. **Tests** matching the style in `signals/whitespace.rs` &
   `hygiene.rs`: a positive case that fires *and asserts the span slices
   the offending substring*, a clean-text negative, and edge cases.
6. The wasm `.d.ts` `RuleId` union auto-updates on build; the consumer's
   localization map will flag the new code as a TS error until localized
   (intended — that's the closed-set payoff).

**Default-disabled P2 rules:** the cleanest in-spirit way is for
`per_verse_rules()` to still return them, but have `analyze`'s default
`Config` disable them — i.e., introduce `Config::v1_defaults()` (all P0/P1
on, P2 off) and make `analyze` use it instead of `Config::all()`. Keep
`Config::all()` for "literally everything." Document this in ADR 0013.

## Config knobs — keep them deferred

Ship every P0/P1 rule with **built-in constant defaults** (allow-lists,
the repeated-char threshold). Do **not** redesign `Config`'s value type
from `bool` to a per-rule struct in this run — that's a separate additive
change for when a consumer actually needs to customize an allow-list. The
only `Config` change here is the optional `v1_defaults()` for P2 default-
off. This keeps the run focused on rules + tokenizer, not config schema.

## Calibration gate (do this before shipping — it's the trust-on-sight check)

Reference corpora live in `corpora/` (e.g. `en_ulb`, `es-419_ulb`, and the
`*_reg/` minority-language sets). After the batch compiles + unit tests
pass:

1. Write a small throwaway binary or test that runs the full batch (P0+P1
   enabled, P2 off) over a couple of reference corpora and **counts
   findings per rule per book**.
2. Apply vision §10's bar: a clean reference Bible should produce
   *bounded* findings. If a rule floods (e.g. `source-marker-leftover`
   firing thousands of times ⇒ likely a rule bug or real ingest issue;
   `duplicate-word` firing on legitimate doublings), **do not ship it
   enabled** — fix it, downgrade to `Info`, or default-disable it, and
   record the call.
3. Write the result as `documentation/calibration/2026-06-09-deterministic-
   batch.md` (counts + the keep/downgrade/disable decision per rule).

Do not blindly ship a flooding rule. Calibration is a real decision point.

## Done criteria & release

- `cargo test -p ssc-core` green (every new rule unit-tested).
- `cargo build -p ssc-core --no-default-features` clean (native, no wasm
  deps).
- `npm run build:wasm` clean; `pkg-bundler/sous_chef_web.d.ts` shows the
  grown `RuleId` union.
- Calibration report committed; no enabled rule floods a clean reference.
- **ADR 0013** written + indexed: the deterministic-batch decision, the
  word-tokenization-is-in-scope distinction from onion's segmentation, the
  P0/P1/P2 buckets + why each borderline rule landed where it did, and the
  `v1_defaults()` default-off mechanism. List which labs rules graduated
  and which stayed parked, and why.
- Update `vision.md` §8 tier tables to mark the shipped rules.
- Commit, **tag `v0.0.3`**, push (workspace `Cargo.toml` stays `0.1.0`;
  the tag is the release ref, mirroring onion). Consumer pins
  `github:WycliffeAssociates/scripture-sous-chef#v0.0.3`.

## Suggested ordering (so you can stop gracefully on quota)

1. Tokenizer + its tests.
2. P0 rules (each: variant → module → register → tests), in table order.
3. Calibration pass over P0; fix/downgrade/disable as needed.
4. P1 rules + extend calibration.
5. P2 rules (default-disabled) — only if quota remains.
6. ADR 0013 + vision §8 update + calibration report.
7. Build wasm, commit, tag `v0.0.3`, push.

If quota runs tight, a clean stop after step 3 (tokenizer + P0 + their
calibration) is a perfectly good `v0.0.3`. Prefer fewer, well-calibrated,
shipped-enabled rules over a large batch of uncalibrated ones.

## Downstream follow-up (NOT this run — note for the human)

The consumer (`scripture-editor-proto-2`) must add localization entries
for each new `RuleId` (its `sousLocalization.ts`, keyed off the exported
union — new codes surface as TS errors until handled) and decide which to
surface. That's consumer work, tracked there, after the tag lands.
