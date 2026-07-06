# ADR 0028: Repeated letter runs are judged against corpus recurrence

- **Date:** 2026-07-06
- **Status:** Accepted
- **Builds on:** [ADR 0017](0017-stateful-rules-stats-returning-analyze.md)
  (reduce/merge/judge) and
  [ADR 0024](0024-punctuation-adjacency-corpus-relative.md)
  (aggregate-only state with target re-scan).

## Context

`lex.repeated-character-run` detects three or more identical letter grapheme
clusters. The detector is useful, but the stateless verdict is not: 7,960 sites
across 106 corpora are about 85% legitimate orthography, including vowel
length, ideophones, and identical clusters meeting at word joins. Language and
script allow-lists would encode the wrong abstraction. The corpus already shows
whether a cluster or run-containing word is conventional.

## Decision

1. Keep the threshold-three, extended-grapheme candidate detector. Exclude
   U+0640 ARABIC TATWEEL narrowly because it is a stretching control, not a
   repeated letter. Do not exclude modifier letters generally.
2. Move the rule from `GraphemeRule` to aggregate-only `StatefulRule`. Per-book
   state stores a count of whitespace-delimited lexical units, raw-text
   run-event counts by cluster, and frequencies of UAX #29 word types whose
   folded form contains at least one candidate run. The folded-form gate lets a
   title-case `Eee` establish the same word convention as `eee` without storing
   general corpus word frequencies.
   It stores no sites. `judge` re-scans the target verses to recover spans.
3. Key recurrence by the complete first grapheme cluster, Unicode lowercased.
   This pools case variants while keeping combining marks and other cluster
   content significant.
4. Count cluster recurrence over raw verse text, not token contents. A run at a
   scriptio-continua join still contributes to corpus convention even when UAX
   #29 does not supply a containing token. Such a site's word factor is neutral.
5. Score each site as:

   ```text
   cluster_rate = cluster_run_events * 10,000 / whitespace_lexical_units
   cluster_factor = max(0, 1 - cluster_rate / convention_rate_per_10k)
   word_factor = max(0, 1 - (word_frequency - 1) / word_recurrence_k)
   evidence = cluster_factor * word_factor
   ```

   A recurring cluster convention suppresses every occurrence. In a corpus
   where runs are otherwise rare, recurrence of the containing word suppresses
   interjections and ideophones. Word frequency 2 remains positive so copied
   typos can still surface. `emit_score_min` avoids serializing suppressed sites.
6. Run length above three adds no evidence. Known legitimate length-five runs
   show that length is not a language-independent typo signal.
7. Findings remain `Severity::Info`. The full 106-corpus calibration leaves
   7,013/7,910 candidates below 0.1 and 762 above the 0.5 floor, with all named
   conventions suppressed and known typos at 0.770–0.994. The rule therefore
   stays default-on.

The frozen defaults are `convention_rate_per_10k = 2.0`,
`word_recurrence_k = 5.0`, and `emit_score_min = 0.5`; see the
[dated calibration report](../calibration/2026-07-06-repeated-character-run-corpus-relative.md).

## Consequences and limitations

- The scorer uses no language or script identity and adapts to each project.
- Incremental edits replace one book's aggregates while retaining corpus-wide
  scores; returned findings remain scoped to the supplied target verses.
- State is bounded by distinct run clusters and run-containing word types, not
  by finding count.
- A systematic widespread typo suppresses like a convention. Corpus recurrence
  cannot distinguish authorial intent.
- Morphological reduplication such as Gujarati `દાદાદાદી` repeats a
  multi-grapheme unit and is not solved here. If it happens to contain a
  single-cluster triple, this rule can still conflate it with lengthening.
- UAX #29 is not a dictionary segmenter. The raw-event factor is deliberately
  authoritative where it supplies no containing word token. It is also not the
  rate denominator: in calibration, Thai/Lao yielded about three million
  one-grapheme UAX tokens, which diluted 86/26 ordinary join-run events enough
  to surface them. Whitespace-delimited chunks are word-like in spaced text and
  one continuous unit in scriptio continua, making those recurring joins
  suppress without a script branch.
