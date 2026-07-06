# Plan: corpus-relative repeated-character-run scoring

Status: implementation-ready. Testing tolerance: **alpha** — synthetic tests pin
the load-bearing scoring and incremental-state behavior; corpus repositories are
calibration inputs, never fixtures.

## Problem and solution

`lex.repeated-character-run` currently treats every run of three or more
identical letter graphemes as equally suspicious. Across 106 corpora, 7,960
sites are mostly ordinary orthography: vowel length, ideophones, and runs formed
at scriptio-continua word joins. The rule will retain that grapheme-aware
candidate detector but judge each site against corpus-wide recurrence of its
grapheme cluster and, where UAX #29 supplies a containing word, recurrence of
that run-containing word.

## Decisions

1. The rule becomes aggregate-only `StatefulRule` state, partitioned per book.
   `judge` re-scans only the supplied target verses, while scores use aggregates
   summed across retained books.
2. Per-book state contains a count of whitespace-delimited lexical units,
   raw verse-text run counts keyed by folded grapheme cluster, and frequencies
   of folded UAX #29 run-containing word types. It stores no sites and no
   general corpus word-frequency table. Calibration rejected UAX token count as
   the rate denominator because it inflates Thai/Lao into one token per grapheme.
3. Cluster recurrence counts raw verse-text run events, including sites outside
   UAX #29 tokens. This lets Thai/Lao joins self-suppress. A site outside a token
   receives a neutral word factor of `1.0`.
4. The cluster key is the complete first extended grapheme cluster, Unicode
   lowercased. Diacritics remain part of the key; case variants pool.
5. Evidence is
   `max(0, 1 - cluster_rate_per_10k / convention_rate_per_10k) *
   max(0, 1 - (word_frequency - 1) / word_recurrence_k)`.
   Scores and config inputs are clamped to finite domains.
6. Run length remains a candidate property only. A length-5 convention must not
   become suspicious merely for being long.
7. U+0640 ARABIC TATWEEL is excluded from the letter predicate. No broader
   modifier-letter or script allow-list is introduced.

## User stories and success criteria

1. As a reviewer, I want isolated `guerrras`/`joyfullly`-style slips near score
   1 so that likely typos rank first.
2. As a translator, I want recurring vowel length and ideophones suppressed by
   this corpus's own usage, without a language or script list.
3. As a scriptio-continua user, I want recurring run joins suppressed even when
   UAX #29 supplies no containing token.
4. As an editor integrator, I want typed knobs and opaque state to round-trip
   through wasm and incremental re-analysis.
5. As a maintainer, I want exact grapheme spans, deterministic output, no stored
   candidate sites, and no new shared abstraction.

Success means all known typo examples remain at `0.77..1.0`, all named
conventions fall below the shipped floor, the score histogram supports a clear
default-on/off decision, incremental scores equal whole-corpus scores, and the
106-corpus sweep produces a dated report.

## Scope by reviewable section

### A. Durable decision and detector

- Add ADR 0028 with the statistical contract, representation choices, known
  conflations, and provisional defaults.
- Keep the threshold-3 grapheme detector and add only the tatweel exclusion.

Gate: existing detector tests plus a tatweel regression test.

### B. Core state and score

- Add `RepeatedCharacterRunConfig` to `Config`.
- Replace the grapheme-rule registration with a stateful lexical rule.
- Add `RepeatedCharacterRunStats` to the closed `RuleStats` union, including
  merge and book removal.
- Test rare hapax, copied typo at frequency 2, recurring interjection,
  scriptio-continua no-token behavior, case-fold pooling, full-grapheme keys,
  invalid config, serde, incremental equality, and book removal.

Gate: focused core tests and the full core suite.

### C. Boundary and calibration

- Add wasm partial overrides and regenerate both package targets.
- Change `--repeat` from exploratory TSV-only output into a score report at
  floor zero while retaining per-site output needed for spot checks.
- Sweep candidate convention rate / recurrence K values, then run all 106
  corpora with the selected defaults. Inspect mixed-band corpora and named typo
  lists before freezing.

Gate: wasm tests/build, release sweep, finite score histogram, and recorded
freeze/defer decision.

### D. Documentation and review

- Finalize ADR 0028, the dated calibration report, ADR index, and `rules/lex.md`.
- Run format, core/wasm tests, workspace build, clippy, generated-contract
  inspection, then an adversarial standards/spec review against this plan and
  the handoff.

## Risks and rollback

- UAX #29 token boundaries are deliberately incomplete for scriptio continua;
  raw run recurrence is the primary protection there.
- Systematic typos are statistically indistinguishable from conventions and
  will suppress. The finding stays Info, not a correctness verdict.
- Morphological multi-grapheme reduplication remains outside this detector; a
  repeated single cluster inside it can still be conflated with orthographic
  lengthening.
- If the full histogram is not cleanly separable, keep the knobs exposed but
  default-disable the rule rather than freezing weak defaults.
