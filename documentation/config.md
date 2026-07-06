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
        // OR per-corpus median by more than these many MAD-units is
        // flagged. Higher = stricter (fewer findings).
        //
        // Use `z_threshold` to set both at once, or split them when
        // you want different sensitivity for "too long" vs "too short."
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

`punct.adjacency-anomaly` is the one corpus-relative rule with tunable knobs. It
emits `Severity::Info` with a continuous `score ∈ [0, 1]` — a **conformance
surprise**, not a correctness verdict (1 ≈ "unlike anything this corpus does",
0 ≈ "ordinary here"). The score is the Wilson lower bound of an observed rate
`k/n`, divided by a convention rate and clamped (see `methods.md`
§"Corpus-relative rate shrinkage"); a finding is emitted only when its score
reaches `emit_score_min`, so an established convention emits nothing. It is
aggregate-only stateful — it caches tiny per-book counts, not per-occurrence
sites.

> **Zero-width space is no longer scored corpus-relative.** The
> `uni.zero-width-space-anomaly` scorer (and its `Config.zero_width_space` knobs)
> was retired for lack of a demonstrated error class. U+200B's redundant
> placements are now flagged deterministically by `uni.redundant-zero-width-space`
> — Info, **default-on, no knobs** — so it has no entry here. See the rules
> catalog and ADR 0027.

### `punct.adjacency-anomaly` (`Config.punctuation_adjacency`) — **default ON**

| knob | meaning |
| --- | --- |
| `convention_rate` | share of a lead glyph's run-start opportunities above which a pattern is "established" (coarse); default 0.5 |
| `confidence_z` | Wilson confidence — load-bearing when a lead glyph is exclusive to one pattern; default 1.96 |
| `emit_score_min` | surfacing floor; **default 0.5**. Kept high so moderate-frequency conventions (e.g. Arabic `۔۔` ≈ 0.48) stay suppressed; lower it to also surface low-evidence novelties (a doubled novel mark ≈ 0.32) — ADR 0024 |

`evidence = 1 - strength(k, N_start(lead))` per exact pattern (ADR 0024).

**Stricter (fewer findings):** raise `emit_score_min` toward 1.0 (surface only
near-certain anomalies) and/or lower the `*_convention_rate` (more patterns count
as established → silent). **Looser:** lower `emit_score_min`.

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
