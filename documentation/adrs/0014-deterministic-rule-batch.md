# ADR 0014: The deterministic rule batch — tokenizer, eleven rules, and shipped defaults

- **Date:** 2026-06-09
- **Status:** Accepted (`punct.space-before-punct` amended by
  [ADR 0029](0029-punctuation-spacing-corpus-relative.md) — replaced by the
  corpus-relative, bidirectional `punct.spacing-anomaly`)

## Context

After [ADR 0013](0013-proportionality-first-cross-map-rule.md) shipped
proportionality (v0.0.3), the engine still had only one content rule
plus the hygiene set. A batch of deterministic, trust-on-sight rules was
parked on `labs` in statistical clothing; the execution brief
(`documentation/plans/2026-06-09-deterministic-rule-expansion.md`)
selected the subset that needs **no corpus model, no learned weights, no
cross-verse state** — reshaped to the ADR 0010 contract (byte `Span`
into the given verse text, `sid`-anchored, no rendered messages,
`score = None`).

## Decision

1. **Word tokenization is in core** (`crates/core/src/token.rs`): plain
   UAX #29 word boundaries via `unicode-segmentation`,
   `tokenize(text) -> Vec<Token { span }>`, words only. This is
   *word* tokenization of text sous was handed — distinct from the
   verse/coordinate *segmentation* ADR 0010 reserves for onion, which
   remains out. The `include_chars` per-project knob (vision §12.15)
   stays deferred; so does any tokenizer config surface.

2. **Eleven new `RuleId`s ship in one batch**, each one line in
   `define_rule_ids!`, a `PerVerseRule` impl in a family module
   (`signals/{structural,punctuation,lexical,casing}.rs`, Unicode rules
   extending `signals/hygiene.rs`), registered in `per_verse_rules()`:

   | code | severity | default |
   |---|---|---|
   | `struct.source-marker-leftover` | Warning | on |
   | `punct.repeated-punct` | Warning | on |
   | `lex.punct-only-token` | Warning | on |
   | `uni.combining-mark-without-base` | Warning | on |
   | `uni.mixed-script-in-token` | Warning | on |
   | `uni.mixed-numeral-systems` | Warning | on |
   | `lex.repeated-character-run` | Info | on |
   | `punct.placeholder-leftover` | Warning | on |
   | `punct.bracket-balance` | **Info** | on |
   | `lex.duplicate-word` | Warning | **off** |
   | `punct.space-before-punct` | Warning | **off** | *(amended by ADR 0029 → `punct.spacing-anomaly`, Info, corpus-relative)* |
   | `case.sentence-initial-lowercase` | Info | **off** |

   General-category predicates (mark / punctuation / symbol / decimal
   digit) come from the `unicode-properties` crate, added to `core` —
   we do not hand-roll category tables (consistent with ADR 0009's
   delegate-to-crate stance, and the named-codepoint approach in
   `unicode.rs` stays for the *curated* zero-width list where GC is
   deliberately too broad).

3. **`Config::v1_defaults()` is the shipped default**, used by
   `analyze` and as the wasm boundary's base config (caller overrides
   merge on top). It returns the full registry with the
   convention-dependent rules mapped off. `Config::all()` remains for
   "literally everything." Rules are still *registered*
   unconditionally — default-off rules are one config entry away, not a
   different build.

4. **Calibration is a gate, and it rewrote three rules** (full data:
   `documentation/calibration/2026-06-09-deterministic-batch.md`):
   - `lex.duplicate-word` ships **default-disabled**. It is
     near-perfectly precise in non-reduplicative languages (every en/es
     hit is a real typo) and floods reduplicative ones (vi `đời đời`
     731, anl `boi boi` 753, acz 648) — and minority-language
     reduplication is this tool's core audience, so the wrong default
     punishes exactly the users the product exists for.
   - `punct.repeated-punct` exempts quote characters from
     identical-run detection (es-419 writes `''` / `""` corpus-wide as
     quote conventions).
   - `lex.punct-only-token` flags only unambiguous wreckage
     (multi-mark cores, stranded opening brackets, symbols) after
     stripping quotes/closers; a single detached ordinary mark (GC Po)
     or dash is a *spacing convention* (Nepali pre-danda/`?` spacing:
     ~47k legitimate hits in `ne_ulb`) and is the opt-in
     space-before-punct family's business, not this rule's.
   - `punct.bracket-balance` is Info: per-verse scope structurally
     cannot see legitimate cross-verse parenthetical asides (24 in the
     English ULB), so it informs rather than warns until book-scope
     re-scan exists (ADR 0011).

5. **What stayed parked** (per the brief, unchanged): everything
   needing a corpus model or cross-verse state — hapax/ngram families,
   BK-tree clustering, quote-direction/balance, missing/extra/ordered
   verse, repeated-phrase proximity, the other proportionality ratios.
   No `AnalysisContext`, no stats scaffolding, no dead code was added
   for them.

## Rationale

- **Tokenizer before rules:** three of the batch's rules are
  token-scoped; UAX #29 via the already-present dependency is
  deterministic, sub-ms, and language-blind — the only tokenization
  worth having before per-project knobs are demanded by a consumer.
- **Default-disabled beats Info for language-mismatched rules.** An
  Info finding still renders; 700 of them per NT is chrome that erodes
  trust-on-sight. Disabling preserves the rule for the languages where
  it is excellent (a per-project config entry), instead of splitting
  the difference badly for everyone.
- **Calibration decisions are data, not vibes:** each
  keep/downgrade/disable call above cites corpus counts from eleven
  corpora across six scripts; the report is committed alongside.
- **`v1_defaults` as a constructor, not a changed `Config` semantic:**
  "absent ⇒ enabled" stays the type's rule (ADR 0012); the *entry
  points* opt into curated defaults. Rust consumers calling
  `analyze_with_config` choose their own base explicitly.

## Consequences

- The wasm `RuleId` union grows twelve members; the consumer's
  exhaustive config/localization maps fail to typecheck until each is
  handled — the intended ADR 0012 behavior. None of the batch rules
  carries `score` or `args`.
- The wasm boundary's semantics shift subtly: an omitted config now
  means `v1_defaults()`, not all-rules-on, and explicit `rules` entries
  merge over that base (previously they replaced the whole map).
  Pre-alpha, no compat layer kept.
- Real issues found in the reference corpora during calibration
  (en `joyfullly`, ne `|` ingest leftovers, es `guerrras`) are upstream
  corpus bugs, not engine concerns — left in place as honest test data.
- Released as tag **v0.0.4** (v0.0.3 was proportionality, which the
  briefs had sequenced first).

## References

- Execution brief:
  `documentation/plans/2026-06-09-deterministic-rule-expansion.md`
- Calibration:
  `documentation/calibration/2026-06-09-deterministic-batch.md`
- [ADR 0010](0010-pure-analyzer-contract-v1-reset.md) (contract),
  [ADR 0011](0011-statefulness-incrementality-strategy.md) (what stays
  parked), [ADR 0012](0012-ruleid-closed-enum-config-surface.md)
  (per-rule recipe), [ADR 0013](0013-proportionality-first-cross-map-rule.md)
- `documentation/vision.md` §8 (catalog), §10 (volume bar), §12.15
  (tokenizer knob, deferred)
