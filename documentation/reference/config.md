# Scripture Sous Chef: Core Concepts & Configuration

When translating the Bible into a new or low-resource language, translators don't have access to standard tools like spellcheckers, grammar checkers, or AI language models. Scripture Sous Chef is built for this exact scenario.

Instead of relying on pre-built dictionaries, the engine **learns the rules of the language by reading the translation itself**. It looks for statistical anomalies — things that break the patterns the translator has already established.

This document explains what the engine is actually observing, how it combines weak clues into strong warnings, and how you can configure it.

## 1. What Are We Actually Observing?

Because the engine doesn't "speak" the target language, it cannot check for meaning or theology. Instead, it observes **structural and statistical properties**.

We look at:
* **Frequencies:** Does this word appear 1,000 times everywhere else, but is spelled slightly differently here?
* **Associations:** When a period (`.`) appears, does it almost always predict that the next letter will be capitalized?
* **Proportions:** Is this verse in the target language wildly longer or shorter than the same verse in the English/Spanish source?
* **Context:** Does the word "and" ever appear right before a period?

The core philosophy is: **A translator's own dominant patterns are assumed to be correct.** If 99% of the text follows a pattern, the 1% that deviates is flagged as a likely typo or error.

## 2. How Rules Work Together (The "Aggregation Layer")

Sometimes a single statistical anomaly isn't enough to confidently call something an error. For example, a word appearing only once (a *hapax legomenon*) isn't necessarily a typo; it might just be a rare name like "Zechariah."

To solve this, the engine uses **Score Combination**. Rules emit "ticks" of suspicion. When multiple ticks happen in the same place, the engine combines them with a multiplier.

### Example: The "Double Signal" Multiplier
Imagine a translator accidentally types: `Jesus wept and. He prayed.`
1. **Rule A (Unexpected Sentence End)** notices that the word "and" is suddenly at the end of a sentence, which contradicts how "and" is used in the rest of the text. *(Suspicion score: Medium)*
2. **Rule B (Sentence Start Case)** notices that the word "He" is capitalized after a period. This is perfectly normal, so it doesn't flag anything.
3. However, if they typed `Jesus wept and. he prayed.` — Rule B would *also* fire because `he` is lowercase after a period.
4. **The Combination:** The engine sees Rule A and Rule B firing right next to each other. It applies a **Multiplier**. Two medium-suspicion warnings combine into one high-confidence error: "You likely have an accidental period here."

We never throw away a weak finding, but we only surface it to the user if it crosses a confidence threshold, which usually requires corroborating evidence.

## 3. How a Score is Built

Every cluster's `score` is fully traceable. The formula:

```
score = sum(rule_weight × finding.evidence) × product(matching pair multipliers)
```

Three independent levers:
* **`rule_weight`** (policy): how much one rule is worth on its own. Hygiene rules get high weights; sparse statistical rules get low weights.
* **`evidence`** (per-finding): how strong *this particular* hit is. A Dunning-graded rule firing on a g²=6677 word emits ~1.0; one at the threshold emits ~0.5. Hygiene rules emit 1.0.
* **multiplier** (policy): known co-occurrence patterns amplify the cluster's whole evidence sum.

The full breakdown for every cluster is written to `debug/<corpus>.clusters.json` under `score_breakdown` — you can verify the math without re-running the analysis.

## 4. Statistical Significance vs. Effect Strength

A common subtlety: the engine uses **Dunning's −2 log λ** to decide whether an association is statistically real. With a 30,000-verse corpus, even small deviations from baseline produce very high g² scores.

**g² answers "is this association real?", not "is this association overwhelming?"**

Example: a punctuation cluster might show `p_upper = 0.898` (89.8% of time followed by uppercase) with `g² = 155`. The g² is huge — the association is statistically rock-solid — but 10.2% of cases still go the other way. Whether to trust this as a hard rule is a *separate decision* gated by `trigger_upper_rate_min` (default `0.85`).

If you find the engine learning triggers you don't trust, **bump `trigger_upper_rate_min` toward 0.95** rather than the g² threshold. The g² threshold is about avoiding noise; the rate threshold is about effect strength.

## 5. Configuration & Defaults

The engine is designed to be cautious. We would rather miss a minor typo than overwhelm a translator with thousands of false alarms. This philosophy drives the defaults.

### Why the Defaults Exist as They Do

* **Zipf gates (frequency minimums):** We only flag a word as being used in the "wrong position" if we've seen it at least 10 times in the "right position." We need enough data to be statistically sure.
* **Conservative thresholds:** We require overwhelming proof to learn a rule. For example, we only assume a character is a "sentence terminator" if it predicts a capital letter at least 85% of the time, and only if the math guarantees a less than 0.1% chance it's a coincidence.
* **Hygiene vs. statistics:** Hygiene rules (like using invisible formatting characters by mistake) have maximum weight because they are *always* wrong. Statistical rules have lower weights because they are educated guesses.

### The user-facing surface: Review Depth

The shipped rule catalog (`core::catalog`, wasm `rule_catalog()`) carries the
plain-language text a consumer renders: per rule a **title**, one sentence on
**what a finding is**, one on **why it may deserve an eyeball**, and — for
language-dependent toggles — the **enable question** a translator answers
("Does your language repeat words on purpose? If yes, leave this off"). The
wording holds two lines: the *translation* is the authority, never "the
language"; and findings are invitations to look, not verdicts.

Review Depth is one continuous project-wide control from `0` through `100`,
labelled **Strongest patterns first** → **Explore more patterns**. The shared
sentence is: “Review Depth controls how unusual a pattern must appear—and how
much corpus evidence must support that judgment—before it is shown.”

Mapped rules resolve their own native judging parameters from the effective
depth. A per-rule adjustment is relative, not an override:

```text
effective_depth(rule) = clamp(master_depth + adjustment(rule), 0, 100)
```

The default anchor is `50`, with no adjustments. It resolves to the existing
calibrated native defaults, so omitted review configuration is byte-identical
to current behavior. Explicit advanced native overrides apply afterward and
win field-by-field. Rules without an honest calibrated path remain fixed
on/off controls; the catalog's `review_control` field is the source of truth.

The three owner-adjudicated production anchors are `0 / 50 / 100`; deterministic
piecewise-linear interpolation derives interior depths, with half-up rounding
for integer fields. Their native values are rule-local and are derived from the
compact calibration TSVs, not fitted to the current project at runtime.
`convention_rate`, `confidence_z`, and native structure knobs remain advanced
calibration fields and are deliberately absent from the normal cards.

### Three Tiers of Configuration

1. **Engine learns from the corpus** (default). No config needed.
2. **Config supplements or overrides the learner.** Useful when you already know your conventions ("we use straight quotes," "our terminals are `.!?`").
3. **The engine reports when your config disagrees with what it observed.** Visible in `debug/<corpus>.stats.json` — a calibration aid.

You don't need to write code to change the engine's behavior. The CLI looks for a `sous.json` next to your corpus directory (or accepts `--config <path>`).

The loader accepts both **strict JSON and JSONC** — `// line comments` and `/* block comments */` are stripped before parsing. Comments inside string values are preserved. No other JSON5 extensions are enabled (no trailing commas, no unquoted keys) so configs stay portable to standard JSON tooling.

## 6. Full `sous.json` Reference

The config has **three peer top-level keys** — `aggregation`, `discourse`, and `rules`. They are siblings, not nested. The shape is:

```jsonc
{
  "aggregation": { ... },   // sibling 1: γ scoring tweaks
  "discourse":   { ... },   // sibling 2: corpus convention overrides
  "rules":       { ... }    // sibling 3: per-rule settings, keyed by rule id
}
```

Every available configuration option, set to its built-in default. Copy this and remove what you don't need to change — every field is optional.

```jsonc
{
  // ─────────────────────────────────────────────────────────────────
  // Top-level sibling 1 of 3: γ aggregation layer.
  //
  // Controls how individual rule "ticks" are combined into per-Sid
  // clusters and ranked. Anything you don't override falls back to
  // the engine's compiled-in default.
  // ─────────────────────────────────────────────────────────────────
  "aggregation": {
    // Score at or above which a cluster is tagged `surfaced` and
    // shown in the CLI's default output. Below this, the cluster
    // still appears in the JSON (with `surfaced: false`) for audit,
    // but is hidden from the human-facing list.
    //
    // Raise this to be more selective ("show me only high-confidence
    // findings"); lower it to see weaker single-rule signals.
    "min_surface_score": 1.0,

    // Default per-finding weight for any rule not in
    // `rules.<id>.weight`. Almost always leave at 1.0; tune the
    // per-rule weight instead.
    "default_weight": 1.0
  },

  // ─────────────────────────────────────────────────────────────────
  // Top-level sibling 2 of 3: discourse-convention overrides.
  //
  // If your translation project already knows its conventions, you
  // can declare them here. The engine then skips the relevant
  // statistical learning step and uses your declarations as ground
  // truth. The learned values still appear in the stats JSON so you
  // can see whether the corpus's data agrees with you.
  // ─────────────────────────────────────────────────────────────────
  "discourse": {
    // Punctuation clusters (single chars or short strings like
    // ". ", "?", "!\" ") to treat as sentence terminators without
    // statistical learning. When set, both `pos.sentence-start-case`
    // and `pos.unexpected-sentence-end` use this set directly.
    //
    // Default: not set (engine learns from corpus).
    // Common English: [". ", "! ", "? ", ".\" ", "?\" ", "!\" "]
    "terminal_punctuation": null,

    // Punctuation clusters after which a lowercase follower is
    // *intentional* (typically dialogue tags like
    // `said, "X," she replied` where the lowercase "she" is normal).
    // Suppresses `pos.sentence-start-case` findings whose
    // predecessor cluster matches one of these strings.
    //
    // Default: not set.
    // Common English: [",\" ", ",' "]
    "dialogue_tag_punctuation": null
  },

  // ─────────────────────────────────────────────────────────────────
  // Top-level sibling 3 of 3: per-rule configuration.
  //
  // Each top-level key is a rule ID. Common fields (any rule):
  //   "enabled": true | false
  //   "severity": "info" | "warn" | "error"  (overrides rule default)
  //   "exceptions": ["GEN 1:1", "REV 22:21"]  (suppress for these Sids)
  //   "weight": 1.0   (per-rule aggregation weight override)
  //   "params": { ... }  (rule-specific numeric thresholds)
  // ─────────────────────────────────────────────────────────────────
  "rules": {

    // ─── Hygiene rules ─────────────────────────────────────────────
    // Always-wrong patterns. No corpus statistics, no params.
    // Maximum-weight findings; surface alone.

    "hyg.tab-in-body": {
      "enabled": true,
      "severity": "warn",
      "exceptions": []
    },
    "hyg.control-chars": {
      "enabled": true,
      "severity": "warn",
      "exceptions": []
    },
    "hyg.zero-width-misuse": {
      "enabled": true,
      "severity": "warn",
      "exceptions": []
    },
    "hyg.empty-verse": {
      "enabled": true,
      "severity": "warn",
      "exceptions": []
    },

    // ─── Source-relative rules ─────────────────────────────────────
    // Run only when --source <dir> is supplied to the CLI.

    "src.proportionality": {
      "enabled": true,
      "params": {
        // MAD-based robust z-score thresholds. A target verse whose
        // length-ratio (target / source) deviates from the per-book
        // OR per-project median by more than these many MAD-units is
        // flagged. Higher = stricter (fewer findings).
        //
        // Split by ADR 0069 into independent long-side/short-side
        // knobs (each measured against its own one-sided MAD — a
        // length ratio's short tail is bounded at zero, its long tail
        // is not, so one shared threshold mis-sizes one side). See
        // §6c below for the real, current field names
        // (`z_long`/`z_short`) and defaults.
        "z_upper": 3.0,
        "z_lower": 3.0
      }
    },

    // ─── Positional / discourse rules ──────────────────────────────

    "pos.sentence-start-case": {
      "enabled": true,
      "severity": "info",
      "weight": 1.0,
      "params": {
        // Of the word-starts preceded by a candidate trigger
        // cluster, this fraction must be uppercase for the cluster
        // to qualify as a learned trigger. **The single most
        // impactful knob for tuning false positives.** Bump toward
        // 0.95 to be stricter; lower toward 0.7 to surface weaker
        // signals.
        "trigger_upper_rate_min": 0.85,

        // Dunning −2 log λ minimum for a candidate trigger to be
        // considered statistically real. 10.83 corresponds to
        // p < 0.001 under χ²₁. Almost always leave alone — change
        // `trigger_upper_rate_min` instead.
        "g2_threshold": 10.83,

        // ── Lexicon thresholds (shared with USE) ──
        // The lexicon classifies each word as a proper-noun
        // candidate (IntrinsicUpper), case-neutral (IntrinsicLower),
        // ambiguous, or indeterminate. SSC and USE both filter their
        // Dunning input through this classification.

        // Minimum mid-flow observations before classifying a word.
        // Below this, the word is `Indeterminate` and excluded
        // from all downstream Dunning math.
        "intrinsic_min_obs": 5,

        // Mid-flow upper-initial rate at or above which a word is
        // `IntrinsicUpper` (a proper-noun candidate).
        "intrinsic_upper_rate_min": 0.95,

        // Mid-flow upper-initial rate at or below which a word is
        // `IntrinsicLower` (case-neutral).
        "intrinsic_lower_rate_max": 0.05,

        // ── Span-pairing corruption guard ──
        // Maximum number of Sid (verse) boundaries an unresolved
        // open punctuation may span before being silently pruned
        // as corruption. Surfaces as an `UnclosedOpen` anomaly
        // anchored to start_sid → end_sid in the message.
        // Real quotes spanning more than ~30 verses don't exist
        // in scripture; lowering this aggressively (e.g. 5) is
        // safer if your corpus has straight-quote ambiguity.
        "max_span_sids": 30,

        // ── Anti-trigger learning (for relaxed lexicon pass) ──
        // Maximum upper-rate after a punctuation cluster for the
        // cluster to be classified "non-terminal" and admitted
        // back into the lexicon's counted pool. Lower = stricter.
        "non_terminal_upper_rate_max": 0.15
      }
    },

    "pos.unexpected-sentence-end": {
      "enabled": true,
      "severity": "info",
      // Sub-1.0 weight: a USE finding alone sits below the
      // surface threshold; it surfaces only when corroborated
      // by another rule firing in the same Sid.
      "weight": 0.5,
      "params": {
        // Zipf gate. Only evaluate words with this many or more
        // total occurrences. Targets the head of the distribution —
        // function words — where "never terminal" is a reliable
        // claim. Below this, the rule skips the word entirely.
        "min_observations": 10,

        // Maximum `p_terminal` (fraction of occurrences immediately
        // before a learned terminal) for a word to qualify as
        // "never-terminal." Low = stricter. The learned word list
        // and the actual rates are visible under
        // `unexpected_sentence_end.never_terminal_words` in the
        // stats JSON.
        "never_terminal_rate_max": 0.05,

        // Same Dunning gate as SSC — see above.
        "g2_threshold": 10.83,

        // The trigger rate that defines "terminal" — same gate as
        // SSC. Both rules consume the same trigger set so their
        // notion of "what counts as a sentence terminator" stays
        // consistent.
        "trigger_upper_rate_min": 0.85
      }
    },

    // ─── Punctuation rules ─────────────────────────────────────────

    "punct.paired-balance": {
      // Surfaces orphaned/mismatched/unclosed open punctuation
      // (parens, quotes, brackets) discovered during the single-pass
      // SpanIndex build. Reads from the index — no separate scan.
      "enabled": true,
      "severity": "warn"
    }
  }
}
```

## 6b. Corpus-relative anomaly rules (typed config)

> The `sous.json` reference above predates the v1-reset core and does not match
> the shipped API — knob-bearing rules now grow a **typed sub-config** on the
> `Config` struct (one small struct per rule), not a `rules.<id>.params` map.
> This section documents the corpus-relative rule against that real surface; the
> older reference is retained for its conceptual material.

`lex.repeated-character-run`,
`case.sentence-initial-lowercase`, `case.inconsistent-word-casing`, and
`punct.bracket-balance` are corpus-relative rules with typed knobs. Each emits
a continuous `score ∈ [0, 1]` whose unit is **anomaly evidence**, not a
correctness verdict: 1 ≈ "unlike this corpus's own conventions", 0 ≈ "ordinary
here" (ADR 0032). For the dominance-verdict rules (casing,
bracket-balance) that evidence *is* the conservative dominance of the
convention the flagged site violates — same number, read from the
convention's side. All carry
`Severity::Info`. A finding is emitted only when its score reaches
`emit_score_min`, so established conventions emit nothing. Most stateful rules
are aggregate-only (tiny per-book counts, no per-occurrence sites); the two
casing rules are the exception (ADR 0051) — they cache a per-book **word case
table**, raw and mergeable, from which the lexicon and per-glyph habit are
derived at judge. (`punct.bracket-balance` is a whole-map project rule that
recomputes its family tallies per call.)

They share one scoring library, `crates/core/src/evidence.rs` (ADR 0032):
`strength(k, n, rate, z)` — Wilson-shrunk convention strength;
`dominance(k_major, n, z)` — Wilson lower bound of a majority form; and
`from_strengths(&[s])` — the noisy-OR residual `∏(1 − sᵢ)` for independent
convention axes. Every `confidence_z` knob below feeds the same Wilson
arithmetic. (`odds_amplify` went with `punct.adjacency-anomaly`, its only
consumer.)

> **Zero-width space is no longer scored corpus-relative.** The
> `uni.zero-width-space-anomaly` scorer (and its `Config.zero_width_space` knobs)
> was retired for lack of a demonstrated error class. U+200B's redundant
> placements are now flagged deterministically by `uni.redundant-zero-width-space`
> — Info, **default-on, no knobs** — so it has no entry here. See the rules
> catalog and ADR 0027.

> **`uni.mixed-normalization`** (a supplied corpus writes canonically
> equivalent grapheme clusters in more than one raw Unicode encoding) is
> Warning, deterministic and corpus-scoped like the redundant-ZWSP rule
> above, so it has no typed sub-config or entry here either — but unlike
> that rule, it ships **default-off, no knobs**: recording every grapheme
> cluster in the corpus measured a real warm-path cost even after a
> `Class`-bit prefilter closed most of an initial regression. Toggle it
> through the same `rules` map every rule uses. See the rules catalog and
> ADR 0063.

### `lex.repeated-character-run` (`Config.repeated_character_run`) — **default ON**

| knob | meaning |
| --- | --- |
| `convention_rate_per_10k` | raw runs of one folded grapheme cluster per 10,000 whitespace lexical units at which that cluster's convention strength saturates; default 2.0. The per-10k value is converted to a fraction internally (`/ 10⁴`) and fed to Wilson `strength` |
| `word_recurrence_k` | repeats beyond the first that drive a run-containing word's convention strength to one; default 5.0 |
| `confidence_z` | Wilson confidence for the cluster axis (ADR 0032); shrinks small-corpus rates toward 0 so early drafts still emit; default 1.96 |
| `emit_score_min` | surfacing floor; default 0.5 |

`evidence = (1 − cluster_strength) · (1 − word_strength)` — the noisy-OR
residual of two independent convention axes (ADR 0028, 0032), with
`cluster_strength = strength(count, lexical_units, rate/10⁴, z)`. The cluster
count scans raw verse text, including runs at scriptio-continua joins. The
word strength is zero when UAX #29 supplies no containing token. The rate
denominator uses whitespace-delimited lexical units because Thai/Lao UAX
tokenization inflated one grapheme into one token and hid established joins.

**Stricter (fewer findings):** lower `convention_rate_per_10k`, lower
`word_recurrence_k`, or raise `emit_score_min`. **Looser:** reverse those.

### `case.sentence-initial-lowercase` + `case.inconsistent-word-casing` (`Config.casing` consumers) — **both default OFF**

The two rules share one per-word case lexicon and two-factor score
`dominance × rarity`, but their judging configs are independent (ADR 0051/0052,
amended by ADR 0070). `Config.casing.sentence_initial` owns the positional
consumer and `Config.casing.inconsistent_word` owns the intrinsic consumer.
Review Depth maps both consumers separately; these native fields remain
advanced overrides.

| knob | meaning |
| --- | --- |
| `*.evidence.emit_score_min` | the two-factor emission floor for that consumer; default **0.95**, the frozen midpoint that clears the homograph/adjective/plural false-positive band while keeping genuine proper-noun and forced-position slips |
| `*.evidence.recurrence_k` | the absolute recurrence knee `k`: `rarity = 1 − min(minority − 1, k)/k` over the minority-form count. Default **32** at Review Depth 50; each consumer may resolve its own value. Sanitised through `clamp_count` |
| `*.evidence.confidence_z` | Wilson confidence for that consumer's dominance; default **1.96** at Review Depth 50 |
| `sentence_initial.trust_gate` | the learned-`terminal_strength` gate for the positional rule (ADR 0052). Default **0.90** at Review Depth 50; intrinsic casing has no trust gate. Sanitised through `clamp_unit` |

- **`case.sentence-initial-lowercase`** (positional): a forced-position
  lowercase site (after an attached terminal — bare or quote-context — or
  book-initial; never verse-initial). `score = habit(class) × rarity(word's
  forced-lowercase count)`, where `habit` is the **lexicon-restricted**
  capitalize-after-class dominance — measured only over words the lexicon calls
  intrinsically lowercase, so proper nouns starting sentences don't inflate it
  (the decontaminated ADR 0035 number). A site whose class the learned witnesses
  distrust (`trust < trust_gate` — an untrusted list-comma or an unpoliceable
  quote context) is not scored positionally; a *trusted* quote-context class
  (`."`, `:"`) that ADR 0051 could not see at all is newly policeable.
- **`case.inconsistent-word-casing`** (intrinsic): a lowercase site of a word
  the corpus writes capitalized. `score = dominance(word's soft-censored
  capitalized share) × rarity(word's lowercase count)`. The first casing
  coverage of mid-flow text. Soft censoring re-enters forced-position uppercase
  at weight `1 − habit(glyph)`: in a no-habit corpus a word capitalized only at
  sentence starts still earns a profile; in a strong-habit corpus the position
  explains the capital.

A both-quadrant site (forced-position lowercase of a capitalized word) may fire
both rules — corroboration. Caseless scripts stay silent by construction (no
cased word-starts, no convention). Both default-off because ~24% of cased
languages don't reliably capitalise after a period, and noun-capitalizing
orthographies (German, Danish) storm the intrinsic channel — enabling is a
per-project language question.

**Stricter (fewer findings):** raise `emit_score_min`, or **lower**
`recurrence_k` (treat a smaller recurring minority as an established second
convention → silent). **Looser:** lower `emit_score_min`, or raise
`recurrence_k`. Raising `trust_gate` (positional only) demands more boundary
trust before a site is scored — but note the surfaced total is flat across
`[0.50, 0.95]`, so it is not a routine sensitivity dial.

### `punct.bracket-balance` (`Config.bracket_balance`) — **default ON**

| knob | meaning |
| --- | --- |
| `window_verses` | the long-span bar and reported-inventory radius (u16, default 16). No longer a matching circuit-breaker (ADR 0037): pairing reads the whole book stream with no distance cutoff |
| `confidence_z` | Wilson confidence for both dominance verdicts; default 1.96 |
| `emit_score_min` | surfacing floor; default 0.5 |

Two dominance verdicts per open-glyph family (ADR 0037): an orphan scores
the family's corpus-wide pairing dominance
(`dominance(matched_events, events, z)`); a matched pair spanning more than
`window_verses` scores the family's short-span dominance
(`dominance(short_pairs, pairs, z)`), anchored at the opener. The inventory
is the UCD BidiBrackets pairs plus the U+FD3E/FD3F supplement; quotes stay
excluded. A never-paired glyph (gux's letter-`]`) self-suppresses at ~0.

**Stricter (fewer findings):** raise `emit_score_min`. **Looser:** lower it
(weak-convention families' orphans surface near the floor).

## 6c. Cross-map (source-paired) rules

These two rules need a **declared source/reference corpus** (`--source
<dir>` on the CLI) and emit nothing when it's absent — unlike §6b's
rules, which are corpus-relative against the target alone.

### `prop.length-ratio` (`Config.proportionality`) — **default ON**

| knob | meaning |
| --- | --- |
| `z_long` | robust z-score magnitude, against the LONG-side one-sided MAD, past which a verse longer than typical for its book (or project) fires; default **3.5** |
| `z_short` | robust z-score magnitude, against the SHORT-side one-sided MAD, past which a verse shorter than typical fires; default **3.5** |
| `min_verses` | minimum target∩reference verse count a book needs before its OWN distribution is judged; smaller books are still covered via the whole-project channel. Default **50** |

Replaces the earlier single `z_threshold` (ADR 0069, 2026-07-30): a length
ratio's short tail is bounded at zero but its long tail is open-ended, so
one shared threshold mis-measures whichever side is actually wider —
`z_long`/`z_short` are independent knobs, each scored against its own
one-sided MAD (with a pooled-MAD fallback when a side has fewer than 3
strict deviations — a thin-sample self-gate, the same shape as every other
corpus-relative rule's recurrence knee). Judged twice per verse — its own
book, and the whole project — and flagged if either channel fires; a book
under `min_verses` is still covered through the project channel.

**Stricter (fewer findings):** raise `z_long`/`z_short` independently, or
raise `min_verses` (fewer small books get their own book-channel judgment —
they're still covered via the project channel). **Looser:** lower either z.
See `documentation/rules/prop.md` for the percent-terms reading of the
shipped default (≈65% deviation from a book's own typical ratio, tier-1
median) and what this rule deliberately cannot see (whole-verse deletion,
source-language paste) by design.

### `lex.untranslated-word` (`Config.untranslated_words`) — **default OFF**

| knob | meaning |
| --- | --- |
| `corpus_gate_share` | corpus-wide copied-token-share ceiling; at or above this the WHOLE corpus is silenced (the creole/closely-related-language case — a high baseline copy rate is expected, not evidence). Default **0.5** |
| `word_recurrence_k` | a word recurring at or above this rate per 10,000 target tokens, corpus-wide, is excused from every verse's copied-count numerator (proper nouns, loanwords, conventions). Default **40.0** |
| `run_bonus` | per-extra-token multiplier on the excusal-adjusted verse fraction for the longest ADJACENT run of copied tokens — a paste is characteristically contiguous, so an adjacent run counts for more than the same number of scattered singles. Default **0.5**, evidence-backed by a partial-paste seed-fault sweep: recall collapses below ≈0.25, and false-positive rate accelerates above 0.5 — 0.5 sits at the knee |
| `emit_score_min` | the sensitivity floor: a verse's `(fraction × (1 + run_bonus×(max_run−1))).min(1.0)` score must clear this before it materializes as a finding. Default **0.7** |

All four knobs are judging-only — map/reduce never read them, so a
knob-only config change re-judges without re-mapping or re-reducing
anything. A copied token whose ORIGINAL (pre-fold) case shape is `Title`-
or `AllCaps`-shaped is excused from scoring unconditionally (no knob — a
structural gate, not a tunable one; see `documentation/rules/lex.md`).

**Stricter (fewer findings):** raise `emit_score_min`, lower `run_bonus`
(less credit for contiguity), or lower `word_recurrence_k` (more words
excused as conventions). **Looser:** reverse those. Raising
`corpus_gate_share` risks letting a genuinely-related-language pair's
baseline copy rate through as false positives — see
`documentation/rules/lex.md`'s standing stop clause before enabling this
rule at all.

## 7. Common Tuning Recipes

### "Show me only high-confidence findings"

```jsonc
{ "aggregation": { "min_surface_score": 1.5 } }
```

Hides single-rule clusters; only multi-rule (corroborated) findings surface by default.

### "Stop flagging dialogue tags"

For English projects with `said, "X," she replied`:

```jsonc
{ "discourse": { "dialogue_tag_punctuation": [",\" ", ",' "] } }
```

### "I know my terminals are .!?"

```jsonc
{ "discourse": { "terminal_punctuation": [". ", "! ", "? ", ".\" ", "?\" ", "!\" "] } }
```

Skips Dunning learning for SSC/USE; treats your declared set as ground truth. The engine still records what it *would* have learned in stats so you can audit.

### "Make the engine stricter about what counts as a trigger"

```jsonc
{ "rules": { "pos.sentence-start-case": { "params": { "trigger_upper_rate_min": 0.95 } } } }
```

### "Aggressively prune stale open quotes"

For corpora with straight-quote inconsistency:

```jsonc
{ "rules": { "pos.sentence-start-case": { "params": { "max_span_sids": 5 } } } }
```

## 8. What's Surfaced Where

| File | Contents |
|---|---|
| `debug/<corpus>.json` | All findings with verse text — the raw output |
| `debug/<corpus>.stats.json` | Per-rule stats: lexicon classifications, learned triggers, span-model rejections, bootstrap config, intrinsic-upper word lists |
| `debug/<corpus>.clusters.json` | Aggregated clusters with `score_breakdown` showing every weight, evidence, and multiplier that built each score |

The CLI's stdout shows surfaced clusters by default; pass `--all` to print everything (including unsurfaced).


## The census is config-independent

`census(map) → Inventory` (ADR 0058) has no entry in this document by
design: nothing in `Config` — rule enablement or any knob — can change a
census count, a sort, or a row. Its one option, `example_cap`, is a
presentation capacity (how many example sites ride along per row), not a
judgment knob.
