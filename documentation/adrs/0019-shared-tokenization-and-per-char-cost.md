# ADR 0019: Shared tokenization, token-rule traits, and the per-character cost of non-Latin scripts

- **Date:** 2026-06-30
- **Status:** Accepted

## Context

A cross-script `samply` pass (full-Bible corpora, ~31k verses each, all
rules, serial) showed analysis cost scaling sharply with script:

| script | ms/run (before) | vs en |
|---|---|---|
| en (Latin/ASCII) | 180 | 1.0× |
| Devanagari | 619 | 3.4× |
| Thai (no word spaces) | 1580 | 8.8× |

Two findings explained almost all of it:

1. **The bottleneck is per-character Unicode property lookups**, not
   grapheme segmentation. On ASCII the std `char` predicates resolve via an
   inlined fast path; on any non-ASCII char they fall to a table binary
   search (`core::unicode::unicode_data::*::lookup_slow`). On Thai/Devanagari
   text — ~100% non-ASCII — every `is_alphabetic` / case query is a slow
   search. `is_alphabetic` alone was **71% of samples on Thai**, split
   between `casing::reduce` (per-grapheme classification) and `tokenize`
   (`unicode_word_ok` per char).

2. **Two rules tokenized every verse independently** — `MixedScriptInToken`
   (per-verse) and `DuplicateWord` (project-scoped, book-ordered) — so the
   UAX #29 word scan ran twice over the whole corpus.

## Decision

Five changes (one was measured and reverted; recorded here because the
negative result is the useful part):

1. **Casing `reduce_book`: classify each scalar once.** Compute
   `is_lowercase`/`is_uppercase` once and reuse; a cased letter short-circuits
   the `is_alphabetic` lookup (`lower || upper || is_alphabetic`). Pure perf,
   no behaviour change — caseless letters still count toward `total`.

2. **ASCII fast-path on the casing predicates — REVERTED.** Adding explicit
   `c.is_ascii() { … }` branches ahead of the std predicates produced **no
   measurable gain** (≤ noise on every corpus), because std already
   ASCII-fast-paths `is_alphabetic`/`is_lowercase`/`is_uppercase`/`is_numeric`.
   The Step-1 win came from calling *fewer* predicates, not from byte checks.
   Kept the tree clean rather than carry dead complexity. **Lesson: the lever
   for non-Latin is doing fewer per-char lookups, not fast-pathing ASCII** —
   ASCII is already the cheap floor.

3. **Duplicate-word: allocation-free case-insensitive compare.**
   `a.to_lowercase() == b.to_lowercase()` heap-allocated two `String`s per
   adjacent word pair; replaced with `eq_ignore_case` (byte-identical short
   circuit → ASCII `eq_ignore_ascii_case` → lazy char-wise `to_lowercase`
   compare). Helps every space-separated script. (No-space scripts like Thai
   skip the compare entirely — their inter-token gap is empty.)

4. **One shared tokenization per analyze.** Two new traits keep the existing
   `ProjectRule` (e.g. `BracketBalance`) untouched while letting token
   consumers share work:
   - `TokenRule { check(text, &[Token]) }` — per-verse token rules
     (`MixedScriptInToken`). The runner supplies tokens.
   - `ProjectTokenRule { check(target, source, Option<&TokenCache>) }` —
     project-scoped token rules (`DuplicateWord`).

   `analyze_stateful` builds a `TokenCache = HashMap<Sid, Vec<Token>>` **once,
   only when ≥2 token consumers are enabled**, and hands it to both. With 0–1
   consumers the lone rule tokenizes inline, so `v1_defaults` (mixed-script
   only) pays no cache overhead. This was the largest win, and the largest on
   non-Latin: Thai −42%, es −28%, Devanagari −22%.

5. **Casing `LowerSite` stores `Sid`, not `String`** (the wasm-ergonomics
   question). The hot `reduce` loop no longer calls `sid.to_string()` per
   candidate, and `judge` no longer re-`parse`s it — the old code round-tripped
   `Sid → String → Sid`. `Sid` stays a 6-byte `Copy` value natively; it
   crosses the wasm boundary as the canonical `"GEN 1:1"` **string** via a
   field-level serde adapter (`#[serde(with = "sid_as_string")]`) plus a tsify
   `#[tsify(type = "string")]` override. The string is materialised only when
   serde actually serialises (the incremental-cache hand-off), never on the
   native analysis path. This keeps the wire identical (`sid: string`, matching
   `Finding`/`DelimObservation`) with no hand-rolled wrapper — the pattern is
   reusable for `DelimObservation.sid`/`first_sid` if we want them `Copy` too.

## Consequences

Measured before → after (ms/run, all rules, serial; findings identical
throughout, so every change is behaviour-preserving):

| script | before | after | Δ | vs en (before→after) |
|---|---|---|---|---|
| en (Latin/ASCII) | 180 | 142 | −21% | 1.0× → 1.0× |
| es-419 (Latin+diacritics) | 253 | 155 | −39% | 1.4× → 1.1× |
| vi (Latin+combining) | 343 | 222 | −35% | 1.9× → 1.6× |
| am (Ethiopic) | 420 | 319 | −24% | 2.3× → 2.2× |
| hi (Devanagari) | 619 | 416 | −33% | 3.4× → 2.9× |
| th (Thai, no spaces) | 1580 | 900 | −43% | 8.8× → 6.3× |

The non-Latin gap narrowed but did not close: casing still classifies every
grapheme per char, irreducible on caseless scripts without a token-based
redesign of the rule (deferred — higher risk, lower marginal value than #4).

Rule registries gained `token_rules()` and `project_token_rules()` alongside
`per_verse_rules()`/`project_rules()`/`stateful_rules()`. Adding a
token-consuming rule means picking the right one of these four — the trait a
rule implements still encodes its data needs (ADR 0010/0017).

Unrelated correctness flag surfaced while measuring: **Amharic emitted 14,478
findings** (vs en's 29), so some rule over-fires on Ethiopic. Not a perf issue;
tracked separately.
