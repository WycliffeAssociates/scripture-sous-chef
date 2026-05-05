# SIL Repositories — Audit & Synthesis

Companion to `VISION.md` and `METHODS.md`. This document captures everything
useful surfaced from auditing two SIL Global repositories against our design,
plus the back-and-forth that shaped which findings to pursue, defer, or
reject. The goal is a single reference so that future-us (or another agent)
does not have to re-do the audit.

## What this document is

A catalog of techniques, rules, and engineering patterns drawn from two SIL
codebases that we considered as inspiration, alternative, or supplement to
the methods in `METHODS.md`. Each entry includes:

- A short description of the technique
- Source files (with paths)
- The problem it solves and why we care
- A contrived example showing the rule firing
- Which existing or proposed rules / modules it interacts with
- A verdict: port now, port eventually, defer, reject — and the reasoning

## Source repositories

| Repo                       | Path                                            | Size     | Language | Stance                                                                                  |
| -------------------------- | ----------------------------------------------- | -------- | -------- | --------------------------------------------------------------------------------------- |
| SIL Machine                | `/Users/willkelly/Downloads/sil/machine-master` | ~91k LOC | C#       | Mature, production-grade NLP library; rule-based + classical ML; closest peer in spirit |
| SIL silnlp                 | `/Users/willkelly/Downloads/sil/silnlp-master`  | ~26k LOC | Python   | Research-leaning, NMT/QE pipelines; some reusable utilities                             |
| (Ours) scripture-sous-chef | this repo                                       | ~4k LOC  | Rust     | Targeting the same problem domain at a smaller, statistical, embeddable footprint       |

Direct file paths in this document are absolute and assume the SIL repos are
present at the paths above. Some line numbers are approximate; the audit
agents reported a few unreliable counts and we have noted those.

## Cross-cutting principles (recap)

Three commitments from `METHODS.md` apply to every entry below:

1. **Many independent weak signals over one strong signal.** Every technique
   here must produce a `score ∈ [0, 1]` that the aggregator can combine.
2. **No language-specific dictionaries, no LLMs, no big models.** ~150–250k
   tokens per NT is the design target. Anything that requires bigger data
   or labels gets rejected or deferred.
3. **Conservative defaults + label-efficient online updates.** Beta-Binomial
   conjugate updates absorb dismiss/accept feedback per cluster without
   batch retraining.

The cascade risk (many low-precision signals corroborating into many false
positives) is real but bounded by `aggregate.rs`'s pair-multiplier mechanism
— see §6 below.

---

## Index

| §   | Topic                                     |
| --- | ----------------------------------------- |
| 1   | Punctuation & quote handling              |
| 2   | Casing                                    |
| 3   | Morphology & lemma identification         |
| 4   | Variant detection & string clustering     |
| 5   | Statistics, smoothing, divergence         |
| 6   | Calibration, optimization, online updates |
| 7   | Unicode & script utilities                |
| 8   | USFM / scripture data plumbing            |
| 9   | Engineering & pipelining                  |
| 10  | Speculative / deferred                    |
| 11  | Resolved questions                        |
| 12  | Proposed rule IDs (consolidated)          |
| 13  | Proposed module additions (consolidated)  |
| 14  | Implementation batches & ordering         |

---

## §1. Punctuation & quote handling

### 1.1 Depth-stack quote pairing with whitespace classification

|                   |                                                                                                                                                                                                      |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source            | `SIL.Machine/PunctuationAnalysis/DepthBasedQuotationMarkResolver.cs`                                                                                                                                 |
| Their approach    | LIFO stack with depth tracking. Ambiguous (symmetric) marks classified by whitespace context: leading-WS = open, trailing-WS = close. Explicit handling of English vs Spanish quote-continuer styles |
| Our approach      | `SpanIndex` with toggle semantics + LIFO + Sid-distance corruption guard + book-boundary flush; emits `PairAnomaly` records                                                                          |
| Distinct elements | Whitespace-context classification; quote-continuer detection (English `"X." "Y."` vs Spanish `«X. »Y."`); language-aware `StandardQuoteConventions` registry                                         |

**Contrived example (continuer detection):**

```
"I am the way," he said. "And the truth," he added.
```

Our toggle semantics treats this as four separate quote events. With
continuer detection, the second `"` and third `"` are recognized as a
*resumption* of the same speaker turn, producing structurally correct
nesting depth.

**Interactions:**

- `signals/punctuation.rs` quote-balance rule. Their whitespace classification
  could feed evidence into our existing toggle decision, not replace it.
- `discourse.rs` terminal-punctuation learning. Continuer style affects which
  positions count as sentence-internal vs sentence-boundary.

**Verdict: defer.** Our toggle + corruption-guard approach already handles
the cases we have observed. Whitespace-context classification is a
worthwhile *additional* evidence channel for ambiguous marks, but not a
replacement. Revisit only if calibration on multi-language corpora shows
real failures the current resolver cannot explain.

### 1.2 Convention-narrowing pre-pass

|                   |                                                                                                                                                                                                |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source            | `SIL.Machine/PunctuationAnalysis/PreliminaryQuotationMarkAnalyzer.cs`                                                                                                                          |
| What it does      | Multi-pass corpus analysis: counts → word-position stats (initial / medial / final) → "earlier vs later in text" frequency tests → narrows to plausible conventions before detailed resolution |
| Distinct elements | Convention narrowing reduces search space before any depth resolution runs                                                                                                                     |

**Contrived example:**

A corpus uses both `"` and `«` but `«` appears 4 times and `"` appears 800
times — narrowing rules out `«` as the primary opening mark before any
balancing logic runs.

**Interactions:**

- `discourse.rs` config override. Could surface a "we believe this corpus uses
  X convention" suggestion, similar to how `CorpusProfile` recommends weights.

**Verdict: defer.** Useful pattern but not load-bearing for our model. If we
add a `recommend()` for discourse config (parallel to `CorpusProfile`), this
is the algorithm to crib from.

### 1.3 Convention scoring / tabulation

|                   |                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| Source            | `SIL.Machine/PunctuationAnalysis/QuotationMarkTabulator.cs`                                       |
| What it does      | Aggregates per (depth, direction) and scores corpus against a registry of known quote conventions |
| Distinct elements | Treats convention as a discrete classification problem, not a learning problem                    |

**Verdict: reject.** Their approach assumes a fixed registry of known
conventions per language. Our approach learns from the corpus directly. The
classification framing is incompatible with our convention-learning thesis.

### 1.4 Punctuation taxonomy (clinging classes)

|                      |                                                                                                                                                                                                                                                                                                                 |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source               | `silnlp/common/normalizer.py` (~600 lines)                                                                                                                                                                                                                                                                      |
| Their approach       | Each punctuation char is `LEFT_CLINGING` / `RIGHT_CLINGING` / `LEFT_RIGHT_CLINGING` / `UNCLINGING`. Warning codes: `CONSECUTIVE_PUNCTUATION`, `BORDERED`, `RIGHT_CLINGING_CHARACTER_STARTING_SENTENCE`, `LEFT_CLINGING_CHARACTER_ENDING_SENTENCE`, `LEFT_RIGHT_CLINGING_NOT_TOUCHING_EXACTLY_ONE_NONWHITESPACE` |
| Our approach         | Several ad-hoc rules in `signals/punctuation.rs` and `signals/hygiene.rs` each maintain their own per-character lists                                                                                                                                                                                           |
| Why theirs is better | Single shared classification table replaces several rules' open-coded character lists; new rules become one-line lookups                                                                                                                                                                                        |

**Concrete categories:**

| Class                 | Examples                                | Convention                           |
| --------------------- | --------------------------------------- | ------------------------------------ |
| `LEFT_CLINGING`       | `(` `[` `{` `«` `"` `'` (opening)       | Space before, no space after         |
| `RIGHT_CLINGING`      | `)` `]` `}` `»` `,` `.` `;` `:` `!` `?` | No space before, space after         |
| `LEFT_RIGHT_CLINGING` | `—` (em-dash) `–` (en-dash)             | Spaces both sides (script-dependent) |
| `UNCLINGING`          | `'` (apostrophe in some contexts)       | Flexible                             |

**Contrived example fires:**

```
"Jesus said , I am the way ."
```

- ` ,` violates `RIGHT_CLINGING` (space before flagged char). Warning: `space-before-right-clinging`.
- ` .` same.

**Interactions:**

- Replaces piecemeal logic in `SSC-PUNCT-002 space-before-punct`, `SSC-WS-002
  space-around-punct-consistency`, `SSC-PUNCT-003 repeated-punct`.
- Becomes a single Unicode-aware table consumed by all hygiene punctuation
  rules.

**Verdict: port now (Batch A).** High value, low cost. Place the table in
`crates/core/src/unicode.rs` or a new `punctuation_class.rs`. Refactor
existing punctuation rules to consume it.

---

## §2. Casing

### 2.1 Unigram truecaser pattern

|                |                                                                                                                                                                                            |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Source         | `SIL.Machine/Translation/UnigramTruecaser.cs` (~190 lines)                                                                                                                                 |
| Their approach | Per word, track all observed casing variants and their counts; **skip sentence-initial positions during training**. At test time, return the most-frequent observed casing                 |
| Our approach   | `analysis/lexicon.rs` co-inference: classify words as `IntrinsicUpper / IntrinsicLower / Ambiguous / Indeterminate`, restrict learning to mid-flow positions only, two-pass strict→relaxed |
| Verdict        | Ours is strictly more principled                                                                                                                                                           |

**Worth keeping in mind:** their sentence-initial-skipping logic is an
implementation reference for handling delayed sentence starts (quotes,
brackets) when we expand `lexicon.rs` casing inference. The structural idea
is the same as ours; their code is shorter and might be a useful sanity
check during testing.

**Verdict: reject as feature, retain as reference.** Skim the file when
implementing edge cases in `lexicon.rs`.

---

## §3. Morphology & lemma identification

This was the most-discussed area in our exchanges. The decision tree:

```
Need to handle morphological fragmentation of frequent entities?
├── Yes → Lemma-cluster induction (§3.1) — DO FIRST
│         └── Still fragmented after that on Bemba/Rai?
│             ├── Yes → Add PoorMansStemming as low-weight independent signal (§3.2)
│             └── No → Stop here.
└── No → Reject all morphology work.
```

Morfessor / FlatCat (§3.3) was considered and rejected.

### 3.1 Lemma-cluster induction (SSC-LEMMA-001)

|              |                                                                                                                                                                                                                                          |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source       | Synthesized from this conversation; not a direct port                                                                                                                                                                                    |
| What it does | Group surface variants of the same entity (`Ἰησοῦς`, `Ἰησοῦν`, `Ἰησοῦ`) into a single lemma cluster using two existing signals: edit-distance proximity (`bktree.rs`) + source-anchored Dunning LLR co-occurrence (`source_relative.rs`) |
| Why now      | Lemma fragmentation poisons frequency-derived signals across the board: hapax detection, IntrinsicUpper voting, source-relative co-occurrence, length statistics                                                                         |

**Algorithm sketch:**

```
Source-anchored branch:
  For each source token s with LLR-significant target correlates {t_1..t_k}:
    For pairs (t_i, t_j) where:
      edit_distance(t_i, t_j) is low AND
      lcs_fraction(t_i, t_j) ≥ 0.6:
        Group into cluster anchored at s.

Target-only branch (IntrinsicUpper tokens not source-anchored):
  Sort by frequency descending.
  Greedy: for each high-frequency token t, find BK-tree neighbors n where:
    edit_distance(t, n) ≤ k_dyn AND
    lcs_fraction(t, n) ≥ 0.6 AND
    n.frequency < t.frequency / 5:
      Absorb n into t's cluster.
```

**The `lcs_fraction` guard is load-bearing.** A naïve `length/4` edit-distance
threshold fails on Bantu prefix paradigms (a single locative prefix can be
3–4 chars on a 6-char stem; that's edit distance 4 but a valid variant).
LCS fraction handles prefixes, suffixes, and infixes uniformly because it
asks "how much of the shared root survives?" rather than "how many edits?"

**Contrived example (Greek NT):**

```
Token frequencies:
  Ἰησοῦς : 400 (nominative)
  Ἰησοῦ  : 350 (genitive/dative)
  Ἰησοῦν : 150 (accusative)
  Ἰησοῖ  :   2 (vocative)

Source anchor: "Jesus" (English)

Dunning LLR for ("Jesus", token) per target token:
  ("Jesus", Ἰησοῦς) : g² = 6,400  ← high
  ("Jesus", Ἰησοῦ ) : g² = 5,200  ← high
  ("Jesus", Ἰησοῦν) : g² = 1,900  ← significant
  ("Jesus", Ἰησοῖ ) : g² = 28     ← significant
  ("Jesus", Πέτρος) : g² = 0.4    ← noise

Edit distances among the four high-LLR tokens:
  All within distance 2; LCS fraction > 0.7.
  → Cluster {Ἰησοῦς, Ἰησοῦ, Ἰησοῦν, Ἰησοῖ}, total count 902.
```

**Contrived counter-example we want to *not* group:**

```
"John" → Ἰωάννης    (count 60)
"Joan" → ...         (does not appear in NT)

Edit distance between "John" and "Joan": 1.
Length/distance ratio: 4/4 = 1. Naïve heuristic merges them.

Defense:
  - "John" and "Joan" have *different source anchors*. The source-anchored
    branch never compares them.
  - In the target-only branch, "Joan" is absent so the question never arises.
  - For "Mary" vs "Mark": LCS fraction is 0.5, below threshold. Stays
    separate even on edit distance alone.
```

**Interactions:**

| Module                                 | What changes                                                                                                                                                         |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `analysis/lexicon.rs`                  | Vote IntrinsicUpper at lemma level, not surface level. Greatly improves classification accuracy for frequent proper nouns                                            |
| `analysis/dunning.rs` (positional)     | Aggregate position-conditional counts at lemma level. Removes the "Ἰησοῦν appears never at sentence-start" false anomaly because it's now part of the Ἰησοῦς cluster |
| `signals/lexical.rs` (hapax-suspicion) | A hapax inside a known lemma cluster gets its evidence demoted near zero. Catches the "Mahalalel appears once but is a clear variant of a frequent name" case        |
| `signals/source_relative.rs`           | Co-occurrence stats aggregate at lemma level for proper nouns, sharpening LLR for the entity itself                                                                  |

**Verdict: port now (high priority).** This is foundational. Place in
`crates/core/src/analysis/lemma.rs`. Should land before any further work
on hapax-suspicion calibration.

### 3.2 PoorMansStemming as supplementary signal

|                                              |                                                                                                                                                                                                                                    |
| -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source                                       | `SIL.Machine/Morphology/PoorMansStemmingAlgorithmBase.cs` (~305 lines), `PoorMansAffixIdentifier.cs`, `PoorMansStemmer.cs`                                                                                                         |
| Their approach                               | Unsupervised affix discovery scored by `curveDrop` (probability-mass drop when extending the candidate affix), `randomAdj` (vs. random-ngram baseline), `VIScore` ("Variety Index" — paradigm coherence), optional `syllableScore` |
| What it gives that lemma-clustering does not | Token-level morphological-plausibility score: "this token decomposes into a known stem plus an *unknown* affix"                                                                                                                    |

**Contrived example PoorMans catches that lemma-clustering misses:**

```
Bemba corpus contains:
  Davidi  (count 80, attested with prefixes: ku-, kwa-, na-, pa-)
  kuDavidi, kwaDavidi, naDavidi, paDavidi : all attested

Token in target verse: kxaDavidi (typo for kwaDavidi)

Lemma-clustering: edit distance 1 to kwaDavidi, LCS fraction high → absorbed.
PoorMans: stem 'Davidi' attested; prefix 'kxa-' unattested in the affix
inventory. Independent evidence that this is a typo, not a variant.
```

**The key property:** PoorMans's evidence is *independent* from
lemma-clustering's evidence. Even if lemma-clustering absorbs the token
incorrectly, PoorMans flags it. The two together form a corroborating pair.

**The honest noise concern:**

PoorMans on a single NT will overfit on agglutinative targets (Bemba, Rai)
where data is sparsest and affix inventories are largest. It will invent
affixes that are coincidence rather than morphology. **This is acceptable**
in the multi-signal framework provided that:

1. The signal weight in `AggregationPolicy` is low (sub-1.0).
2. A PoorMans flag firing alone never surfaces — corroboration is required.
3. The aggregator's pair multipliers are calibrated to recognize correlated
   errors (PoorMans tends to be wrong on the same tokens hapax-suspicion is
   wrong on; their co-firing should not be treated as fully independent).

**Interactions:**

| Module                                        | Interaction                                                                                                  |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `SSC-LEX-HAPAX-001`                           | A hapax whose decomposition uses unattested affixes gets its evidence *upgraded* (corroborating typo signal) |
| `signals/orthographic.rs` (char-KN surprisal) | Empirical co-firing rate is high because both fire on rare strings; pair multiplier should be < 1.0          |
| `analysis/lemma.rs`                           | Sequential dependency: lemma-clustering must run first so PoorMans operates on lemmas, not surface forms     |

**Verdict: port eventually, after lemma-clustering ships and we have
calibration data showing the pair-correlation matrix.** Slot in
`crates/core/src/analysis/morphology.rs`. Multi-day port; significant math.

### 3.3 Morfessor / FlatCat — rejected

|         |                                    |
| ------- | ---------------------------------- |
| Source  | `silnlp/common/flatcat_stemmer.py` |
| Verdict | Reject                             |

**Reasoning:**

- Morfessor was published evaluating on ≥1M-token corpora.
- Our scale is 150–250k tokens per NT, with the highest type-counts
  (Bemba, Rai: 22–23k types) precisely where morfessor most needs data.
- FlatCat is semi-supervised and improves with seed labels — we don't have
  any.
- PoorMansStemming has the same shape but is in-house, embeddable, and
  inspectable. If we want morphology, that's the right starting point.

This judgment may be wrong if the user later cares enough to provide seed
morpheme lists. Until then, neither tool earns its keep.

---

## §4. Variant detection & string clustering

### 4.1 Pairwise alignment with extended ops

|                |                                                                                                                                                                                         |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source         | `SIL.Machine/SequenceAlignment/PairwiseAlignmentAlgorithm.cs`                                                                                                                           |
| Their approach | Needleman-Wunsch / Smith-Waterman with **transposition** (`teh` ↔ `the`), **expansion** (1→2, e.g. `ʃ` ↔ `sh`), **compression** (2→1). Four modes: Global, SemiGlobal, HalfLocal, Local |
| Our approach   | `analysis/bktree.rs` over Damerau-Levenshtein, which has transposition but not expansion/compression                                                                                    |

**The expansion/compression operations are the differentiator.** They
handle script-romanization variation and digraph differences:

```
Possible target spellings of the same word:
  ʃalom    (using IPA ʃ for /sh/)
  shalom   (using digraph)
  sjalom   (using sj)

Damerau-Levenshtein distances:
  d("ʃalom", "shalom") = 2 (delete ʃ, insert sh)
  d("ʃalom", "sjalom") = 2

With expansion:
  d("ʃalom", "shalom") = 1 (single expansion ʃ → sh)
  d("ʃalom", "sjalom") = 1 (single expansion ʃ → sj)

The corpus would treat all three as one cluster instead of three.
```

**Interactions:**

- `analysis/bktree.rs` neighborhood query gets richer.
- `SSC-CONS-001 similar-token-cluster` becomes more accurate for
  multi-orthography corpora.
- `analysis/lemma.rs` uses these distances as input.

**Verdict: port soon (Batch B).** Extend the existing edit-distance metric
in `bktree.rs` rather than replacing it.

### 4.2 Hierarchical clustering (UPGMA / Neighbor-Joining)

|                |                                                                                                                                                               |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source         | `SIL.Machine/Clusterers/UpgmaClusterer.cs`, `NeighborJoiningClusterer.cs`, `ClusterExtensions.cs`                                                             |
| Their approach | Build a dendrogram over a set of items with pairwise distances; root the tree at the midpoint of its longest path → that node is the natural "canonical" form |

**Why this matters for us:** `SSC-CONS-001 similar-token-cluster` currently
produces unstructured neighborhoods. With UPGMA, the cluster has structure
— a root token (canonical form) and a hierarchy of variants. The diagnostic
becomes "this rare token is a 2-edit variant of [canonical], which appears
N times" instead of "this rare token is similar to a bunch of other tokens."

**Contrived example:**

```
Cluster of 5 surface forms (in some Latin-script target corpus):
  yesterday  (count 47)
  yesturday  (count 1)
  yesterdaY  (count 1)
  yestrday   (count 1)
  yeasterday (count 1)

UPGMA dendrogram, rooted at midpoint of longest path:
  → yesterday is the root.
  → all four singletons are leaves at distance 1 or 2 from yesterday.

Rule output:
  "yesterday" is the canonical form for {yesturday, yesterdaY,
   yestrday, yeasterday}; all four are likely typos.
```

**Interactions:**

- `SSC-CONS-001` upgrades from "this token is similar to that token" to
  "this token is a typo of [canonical]".
- `SSC-LEMMA-001` (§3.1) could optionally use UPGMA structure when a cluster
  has more than 2 members, to identify the lemma's canonical surface form.

**Verdict: port soon (Batch B).** ~150 lines per algorithm. Standalone
module `analysis/clustering.rs`.

### 4.3 DBSCAN — density-based outlier detection

|                  |                                                                                         |
| ---------------- | --------------------------------------------------------------------------------------- |
| Source           | `SIL.Machine/Clusterers/DbscanClusterer.cs`                                             |
| What it gives us | The "noise cluster": items that have no dense neighborhood. Naturally surfaces outliers |

**Why it's complementary to BK-tree + UPGMA:**

- BK-tree gives neighborhoods.
- UPGMA structures known clusters.
- DBSCAN identifies tokens that are *not* in any cluster — which is exactly
  what we want for "this rare token has no near-neighbors and is
  morphologically odd; flag it."

**Contrived example:**

```
Low-frequency token set in a Bemba corpus:
  Most rare tokens cluster around frequent stems (variants of common verbs).
  One token — kxaDavidi — has no neighbors within edit distance 2 that are
  also low-frequency (or any neighbors at all within the relevant cluster
  density threshold).

DBSCAN classifies kxaDavidi as noise.
The engine surfaces it: "rare token with no morphological neighbors;
likely typo or transcription error."
```

**Interactions:**

- `SSC-LEX-HAPAX-001` evidence: a hapax in DBSCAN's noise cluster gets
  evidence upgraded.
- `signals/orthographic.rs`: pair multiplier with DBSCAN-noise cluster
  membership should be moderate (correlated but not identical).

**Verdict: port eventually.** Useful but secondary. Land after UPGMA in
`analysis/clustering.rs`.

### 4.4 Confusable-character detection (gap)

Neither repo has this explicitly. Listed because it surfaces a genuine
hole in our spec.

**The problem:**

```
Cyrillic А (U+0410) vs Latin A (U+0041)
Greek χ  (U+03C7) vs Latin x (U+0078)
Greek ο  (U+03BF) vs Latin o (U+006F)
Cyrillic о (U+043E) vs Latin o (U+006F)
```

Mojibake or accidental keyboard-layout switches produce tokens that *look
right* but contain mixed-script characters. The Unicode Consortium
publishes a `confusables.txt` table specifically for this.

**Proposed rule: `SSC-UNI-CONFUSABLE-001`** — a single token contains
characters that are visually confusable but from different scripts. High
confidence that this is an encoding accident.

**Interactions:**

- `SSC-UNI-002 mixed-script-in-token` (§7.2) is the more general check;
  confusable detection is the high-precision subset.

**Verdict: port eventually, low priority.** Catches a real failure mode
but requires the Unicode confusables data table.

---

## §5. Statistics, smoothing, divergence

### 5.1 Simple Good-Turing as profiler input

|                 |                                                                                                                                                               |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source          | `SIL.Machine/Statistics/SimpleGoodTuringProbabilityDistribution.cs` (~140 lines)                                                                              |
| Their approach  | Linear regression on log-log count-of-counts with a 1.96σ smooth/unsmooth boundary test                                                                       |
| What it answers | "How much probability mass should I reserve for things I haven't seen?"                                                                                       |
| Why we want it  | A direct quality signal about *project maturity*: it tells us not just "this corpus is small" but "this corpus is still introducing new vocabulary at rate X" |

**Why we previously rejected it for LM smoothing:** modified Kneser-Ney
absorbs the same job inside its smoothing recurrence. That rejection still
stands for §3.2 of `METHODS.md`. **What we missed:** Good-Turing as a
*standalone* "novelty mass" estimator, fed into `CorpusProfile` as a project-
maturity axis, is a different use-case and is genuinely useful.

**Contrived signal:**

```
Project A:  150k tokens, GT-novelty-mass = 0.03  → mature draft
Project B:  150k tokens, GT-novelty-mass = 0.18  → still drafting; high
                                                    rate of new vocabulary
                                                    each ~5k tokens

Engine response:
  - For Project B, de-rate hapax-suspicion (high false-positive risk while
    drafting).
  - For Project A, hapax findings carry stronger weight.
```

**Interactions:**

- `profile.rs` `CorpusProfile` gets a new field, `gt_novelty_mass: f32`.
- `data_volume_score` in `recommend_weights()` extends to use it.
- `SSC-LEX-HAPAX-001` reads it from `AnalysisContext` to scale evidence.

**Verdict: port eventually (Batch C territory).** Small, ~80 Rust lines.
Modest impact but easy and inspectable.

### 5.2 Witten-Bell, Lidstone — rejected

|         |                                                                                                     |
| ------- | --------------------------------------------------------------------------------------------------- |
| Source  | `SIL.Machine/Statistics/WittenBellProbabilityDistribution.cs`, `LidstoneProbabilityDistribution.cs` |
| Verdict | Reject. Modified KN dominates these for our use case                                                |

### 5.3 Jensen-Shannon divergence

|                       |                                                                                                                               |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Source                | `SIL.Machine/Statistics/StatisticalMethods.cs` (~30 lines for the function)                                                   |
| What it answers       | "How different are these two full distributions?" — bounded in [0,1], symmetric, well-defined when one distribution has zeros |
| Distinct from Dunning | Dunning answers "is this 2×2 contingency table non-independent?"; JSD answers "are these full distributions different?"       |

**Proposed rule: `SSC-PROP-004 per-verse-vocab-drift`**

Compute the empirical token distribution per verse. Compare each verse's
distribution to the per-book baseline distribution via JSD. Flag verses
with high JSD relative to the book's median JSD.

**Contrived example:**

```
Per-verse JSD against book baseline (Mark, sample):
  MRK 1:1   JSD = 0.04   (typical)
  MRK 1:2   JSD = 0.05   (typical)
  ...
  MRK 4:32  JSD = 0.08   (modestly different — has rare botanical vocab)
  ...
  MRK 9:14  JSD = 0.41   (extreme outlier)

Investigation of MRK 9:14: the translator accidentally pasted half of
MRK 9:14 followed by half of LUK 9:14 — vocabulary distribution is mixed
across two source pericopes.

The length-ratio rule (SSC-PROP-001) would NOT catch this — total verse
length is plausible. JSD-against-baseline catches it because the
*vocabulary mix* is wrong even when the length is right.
```

**Interactions:**

- `SSC-PROP-001 length-ratio-outlier` — these are independent signals.
  Verses that trip both should be high-priority surfacing candidates.
  Pair multiplier > 1.0 in `AggregationPolicy`.
- `SSC-CONS-002 repeated-phrase-proximity` — the paste-from-elsewhere case
  often trips both.

**Verdict: port now (Batch A).** ~30 lines for JSD primitive plus rule.
Cheap, novel, addresses a class of errors no current rule catches.

### 5.4 LogSpace arithmetic

|              |                                                                            |
| ------------ | -------------------------------------------------------------------------- |
| Source       | `SIL.Machine/Statistics/LogSpace.cs` (~45 lines)                           |
| What it does | Numerically stable `log(x+y)` via `log(x) + log(1 + exp(log(y) - log(x)))` |

**Verdict: keep in mind.** Useful when we eventually combine many small
probabilities multiplicatively (e.g. Bayesian posteriors over multi-rule
findings). Not needed for v1.

---

## §6. Calibration, optimization, online updates

This section was the most-discussed conceptually. Three layers, each at a
different scale of label availability.

| Layer                        | Mechanism                                                            | Labels needed        | When                 |
| ---------------------------- | -------------------------------------------------------------------- | -------------------- | -------------------- |
| Beta-Binomial conjugate      | Per-cluster posterior; corpus-derived prior + dismiss/accept updates | 0+                   | v1                   |
| Hand-tuned weighted sum      | Existing `AggregationPolicy`                                         | 0                    | v1 (already specced) |
| Gaussian-mixture calibration | Score → probability mapping on aggregated rule output                | ~200+                | v2+                  |
| Logistic regression          | Few features, regularized, on pooled findings                        | ~200+                | v2+                  |
| Nelder-Mead threshold tuning | Outer optimizer on any meta-objective                                | depends on objective | optional             |

### 6.1 Beta-Binomial conjugate updates

**The framing (Q&A from this conversation, captured for reference):**

> Prior: the corpus-derived `p_upper(cluster)` from Dunning. Encoded as a
> Beta distribution with pseudo-counts equal to the observed upper/lower
> counts. Likelihood: each user dismissal of a finding from cluster c =
> "user says this should have been lowercase." Each user acceptance =
> "user says this should have been uppercase." Posterior: Beta updates
> trivially — Beta(α + accepts, β + dismisses). Trigger decision: use the
> posterior mean instead of the raw observed rate.

**Properties:**

- Works at zero labels. Posterior = prior. Engine behaves as today.
- Each label is one observation. No batch retraining.
- Confidence narrows with data. After 50 labels on a cluster, the posterior
  is tight; before, it's diffuse and the prior dominates.
- No overfitting. Two parameters per cluster.
- Auditable. Every cluster's stats can show "prior was 89% upper from
  corpus; after 12 dismisses + 1 accept, posterior is 71% upper; cluster
  auto-demoted."

**Asymmetric labels — critical:**

If the UI only collects dismisses, "no dismiss" could mean any of: user
agreed, user hasn't reviewed, user is confused. The mitigation is two-click
UI:

- **Accept** (positive label — "this is a real error")
- **Dismiss** (negative label — "this is not an error here")
- **No click** — unlabeled, ignored statistically

Or natural-workflow inference: when the translator makes the edit the rule
suggested, that's an implicit accept. When they keep the text and click
dismiss, that's a negative.

**Interactions:**

- `discourse.rs` terminal-punctuation triggers: per-cluster posterior on
  `P(upper | cluster)`.
- `SSC-LEMMA-001`: per-cluster posterior on "this lemma cluster is correctly
  identified."
- Every per-cluster signal in the engine eventually grows a Beta-Binomial
  layer.

**Verdict: port early.** Module: `analysis/bayesian.rs` or inline in
`aggregate.rs`. Two parameters per cluster, online updates trivial. This is
the closest thing to "free training data" we'll get.

### 6.2 Gaussian-mixture calibration

|                |                                                                                                                                                      |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Sources        | `SIL.Machine/QualityEstimation/UsabilityParameters.cs` and `silnlp/nmt/quality_estimation.py:266-388`                                                |
| Their approach | Two-component Gaussian mixture (usable / unusable), per-class mean, variance, count. Posterior probability via Bayes' rule with Gaussian likelihoods |
| When to use    | Map raw aggregated score → `P(real_anomaly                                                                                                           | score)`. Replaces hard threshold once labels exist |

**This is NOT the same as Beta-Binomial.** They operate at different layers:

- Beta-Binomial: posterior over per-cluster *properties* (does this cluster
  trigger uppercase?).
- GMM: posterior over per-finding *quality* (is this aggregated finding a
  real anomaly given its score?).

**Verdict: defer to v2.** Wait for ~200 hand-labeled findings. Slot into
the surfacing layer in `aggregate.rs`, replacing the fixed threshold.

### 6.3 Nelder-Mead simplex

|                 |                                                                                                  |
| --------------- | ------------------------------------------------------------------------------------------------ |
| Source          | `SIL.Machine/Optimization/NelderMeadSimplex.cs` (~250 lines)                                     |
| What it does    | Gradient-free optimization for noisy / non-smooth objectives                                     |
| Why we noted it | Could tune meta-parameters: surface threshold, per-rule weights, MAD multipliers, prior strength |

**Not opposed to Bayesian methods.** Nelder-Mead is just an optimizer; it
doesn't compete with Beta-Binomial. Where it would earn its keep is on
*outer* knobs the conjugate framework leaves untouched (e.g., the prior
strength itself).

**Verdict: defer.** Useful eventually, but grid search is fine for v1
calibration. Revisit only if a real bottleneck appears.

### 6.4 Big-model approaches — rejected at our scale

For the record, since this came up as a question:

| Approach                                      | 8k verses (~150–250k tokens) | 31k verses (~600k–1M tokens) | Verdict       |
| --------------------------------------------- | ---------------------------- | ---------------------------- | ------------- |
| RNN / Transformer fine-tune                   | No                           | No                           | Reject        |
| Logistic regression, 50+ features             | Overfits                     | Risky                        | Skip          |
| Logistic regression, ~5 features, regularized | OK with ≥200 labels          | Fine                         | OK eventually |
| Random forest / GBT, shallow                  | OK with ≥500 labels          | Fine                         | OK eventually |
| Beta-Binomial conjugate per cluster           | Works at zero labels         | Works                        | **v1**        |
| Hand-tuned weighted sum                       | Works at zero labels         | Works                        | **v1**        |

The dividing line is *number of parameters*, not corpus size.

---

## §7. Unicode & script utilities

### 7.1 Unicode-script lookup table

|                  |                                                                                                                                            |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Source           | `silnlp/common/script_utils.py` (~2000 lines, mostly data)                                                                                 |
| What it provides | Codepoint → script-name lookup for 112+ scripts (Latin, Greek, Devanagari, Tamil, Khmer, Ethiopic, ...) plus 25 Unicode general categories |

**Rust ecosystem note:** the `unicode-script` crate already provides this.
We do not need to embed the data manually.

**Verdict: use `unicode-script` crate.** Enables the next two rules.

### 7.2 Mixed-script-in-token (SSC-UNI-002)

A single token contains characters from > 1 script.

**Contrived example:**

```
Token: Cаlvary
       │└ Cyrillic а (U+0430)
       └  Latin C (U+0043)

Looks like "Calvary" but contains a Cyrillic character. Likely
copy-paste accident or keyboard-layout switch.

Engine output: "Token contains characters from Latin and Cyrillic scripts."
```

**Interactions:**

- Currently swept under our Tier 1 `SSC-UNI-001 unicode-anomaly`. Splitting
  it out gives finer diagnostics.
- `SSC-UNI-CONFUSABLE-001` (§4.4) is the high-precision subset.

**Verdict: port now (Batch A).** ~50 lines.

### 7.3 Charset-divergence-per-verse (SSC-UNI-003)

|                |                                                                                   |
| -------------- | --------------------------------------------------------------------------------- |
| Source         | `silnlp/nmt/alphabet_similarity.py` (~80 lines)                                   |
| Their approach | Jaccard-style set similarity over character inventories of two projects. Heatmaps |
| Our adaptation | Per-verse character distribution Jaccard distance from per-book baseline          |

**Contrived example:**

```
Per-verse character-set Jaccard distance from book baseline (Mark):
  MRK 1:1   d = 0.02
  ...
  MRK 9:14  d = 0.31  ← outlier

Investigation: verse contains characters from a different writing system
that don't appear elsewhere in the book. Either encoding error, copy-paste
from wrong project, or genuine code-switching the engine should not
silently absorb.
```

**Interactions:**

- Independent of `SSC-PROP-004 per-verse-vocab-drift` (JSD on tokens).
  These are different distributions of different things.
- Pair multiplier with `SSC-UNI-002 mixed-script-in-token` should be < 1.0
  (correlated — both fire on encoding accidents).

**Verdict: port soon (Batch A or B).** Cheap, catches a real error class.

---

## §8. USFM / scripture data plumbing

### 8.1 ScriptureRef path notation

|                |                                                                                                                                                                        |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Source         | `SIL.Machine/Corpora/ScriptureRef.cs` (~200 lines)                                                                                                                     |
| Their approach | `(BookNum, ChapterNum, VerseNum, Versification)` plus a `Path` of `(position, name)` pairs for non-verse content (e.g. `MAT 1:1/1:s` = section heading after Matt 1:1) |
| Our `Sid`      | Opaque-but-orderable string                                                                                                                                            |

**Why path notation matters:**

When section-aware analysis becomes a thing (Tier 3 in `VISION.md` §8.4
"pericope-aware analyses"), we'll need a way to address non-verse content
without polluting verse text. Path-suffixes are how Machine handles this.

Also: their `ChangeVersification()` method explicitly handles KJV / NRSV /
LXX / MT mappings.

**Verdict: bookmark, do not port.** The pattern is the value, not the
code. When we need it, the data model is here for reference.

### 8.2 Marker taxonomy

|                |                                                                                                                                                                                                                                |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Source         | `silnlp/common/usfm_utils.py` (~70 lines)                                                                                                                                                                                      |
| Their approach | Three classifications: `CHARACTER_TYPE_EMBEDS` (`\fig`, `\fm`, `\jmp`, `\rq`, `\va`, `\vp`, `\xt`), `PARAGRAPH_TYPE_EMBEDS` (`\lit`, `\r`, `\rem`), `NON_NOTE_TYPE_EMBEDS`                                                     |
| Why we care    | Verse-content computation should treat semantic-content embeds (`\xt` cross-references with content) differently from structural embeds (`\rem` translator remarks). A footnote's text shouldn't count toward verse word count |

**Contrived example:**

```
Raw verse: \v 1 In the beginning\f + \fr 1.1 \ft Alternate reading: ...\f*
                 God created the heavens and the earth.\rem TODO: review.

Without marker classification:
  - "Alternate reading" gets counted as verse content. Length-ratio
    inflated. Tokenizer gets footnote prose. Hapax stats polluted.
  - "TODO: review." stays in the text.

With marker classification:
  - \f...\f* contents are NON_NOTE_TYPE_EMBED (footnote): excluded.
  - \rem contents are PARAGRAPH_TYPE_EMBED with rem-class: excluded.
  - Verse text is "In the beginning God created the heavens and the earth."
```

**Interactions:**

- `crates/ingest/src/usfm.rs`: extends marker handling beyond what
  `usfm-onion` currently strips.
- `signals/lexical.rs`: cleaner inputs improve hapax detection.
- `SSC-STRUCT-005 source-marker-leftover`: detection of marker leakage
  becomes more precise.

**Verdict: port to `crates/ingest/`** when adding a new format adapter
or auditing the existing one. Not a core-engine concern.

---

## §9. Engineering & pipelining

### 9.1 Phased progress reporter

|         |                                               |
| ------- | --------------------------------------------- |
| Source  | `SIL.Machine/Utils/PhasedProgressReporter.cs` |
| Pattern | Multi-phase progress with per-phase counts    |

**Verdict: port when the CLI grows multiple phases.** Not urgent.

### 9.2 Per-script tokenizers

|              |                                                                                                                              |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| Source       | `SIL.Machine/Tokenization/{LatinWordTokenizer,WhitespaceTokenizer,ZwspWordTokenizer,LineSegmentTokenizer,RegexTokenizer}.cs` |
| Pattern      | `ITokenizer` interface; pluggable per script                                                                                 |
| Our position | We use ICU4X `WordSegmenter` (locked decision in `VISION.md` §11). ICU4X subsumes their ZWSP and Latin tokenizers            |

**Verdict: keep current ICU4X choice.** Their per-tokenizer plugin pattern
is an antipattern at our scale; ICU4X handles it uniformly.

### 9.3 Object pooling, skip lists, etc.

**Verdict: reject.** Rust's RAII handles allocation differently from C#.
These are C#-specific GC mitigations.

---

## §10. Speculative / deferred

### 10.1 Rank-Biased Overlap (RBO)

|              |                                                                                                                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Source       | `silnlp/alignment/rbo.py`                                                                                                                                                                                                            |
| What it is   | Webber et al. 2010 ragged-list similarity metric with a persistence parameter                                                                                                                                                        |
| Possible use | Proper-noun consistency: rank target tokens by per-verse co-occurrence Dunning g² with a source proper noun. If those rankings are stable across books, the noun is rendered consistently; if they shift radically, that's a finding |

**Verdict: speculative.** Not confident it earns its keep at NT scale.
Park as Tier 3.

### 10.2 AER, P@k, R@k for monolingual consistency

|        |                               |
| ------ | ----------------------------- |
| Source | `silnlp/alignment/metrics.py` |

**Verdict: low value.** Designed for evaluating alignments. Adapting them
to monolingual signals would be more work than it's worth.

### 10.3 RNN / NMT model perplexity

`silnlp/smt/train.py:103` exposes per-verse perplexity from a trained LM.

**Verdict: reject.** Requires training. We do not train.

---

## §11. Resolved questions (Q&A from this exchange)

For future-us, the resolved questions in this conversation:

### Q: Does Morfessor work at NT scale?

**A:** Mediocre on a single NT, especially for agglutinative targets. Works
well at ≥1M tokens. At 150–250k tokens with 22–23k types (Bemba, Rai), it
over-segments. Verdict: skip Morfessor; PoorMansStemming has the same shape
in-house and is a better candidate if we want morphology at all.

### Q: Is "Bayes" in this audit the same as Beta-Binomial conjugate updates?

**A:** Yes for the per-cluster online label absorption. The Gaussian-mixture
"Bayesian" calibration mentioned in the SIL audit is different — it's a
batch-trained two-component mixture mapping aggregated score → quality
probability. Beta-Binomial is the v1 move because it works at zero labels.
GMM is v2+, after ~200 labels exist.

### Q: Is Nelder-Mead opposed to Bayesian methods?

**A:** No. Nelder-Mead is an optimizer. It can optimize any objective,
including a Bayesian posterior's MAP point. Where it would earn its keep is
tuning *outer* meta-parameters (prior strength, surface threshold, weights).
Not urgent for v1.

### Q: Can RNN / true logistic regression work at 8k verses?

**A:** RNN: no, won't work at this scale. Logistic regression with ~5
features and ≥200 labels: works fine. Logistic with 50+ features: overfits.
The dividing line is *number of parameters*, not corpus size.

### Q: How does PoorMansStemming compare to the "Semantic Anchor" lemma-clustering approach?

**A:** They solve different problems. Lemma-clustering answers "are these
surface forms variants of the same entity?"; PoorMans answers "is this
token's morphological structure consistent with the corpus?". Lemma-
clustering is foundational and ships first. PoorMans is an additional
independent signal that can supplement it later, if needed.

### Q: Can PoorMans coexist with lemma-clustering?

**A:** Yes. They produce independent evidence and corroborate each other in
the aggregator. The cascade risk (correlated errors looking like
independent evidence) is handled by `AggregationPolicy`'s pair multipliers
once the empirical co-firing matrix is measured.

### Q: Is "many independent signals corroborating" actually safe?

**A:** Mathematically benign in the truly-independent case (N signals each
with FP rate p, K corroborations needed → aggregate FP rate ~p^K). The risk
is correlated errors (multiple signals wrong on the same tokens). The
mitigation is `AggregationPolicy.pair_multipliers` calibrated empirically
from co-firing data.

---

## §12. Proposed rule IDs (consolidated)

New rules surfaced by this audit. None are committed yet; they update
`VISION.md` §8 when accepted.

| ID                       | Name                           | Tier             | Source ref | Status                                  |
| ------------------------ | ------------------------------ | ---------------- | ---------- | --------------------------------------- |
| `SSC-LEMMA-001`          | lemma-cluster-induction        | 1 (foundational) | §3.1       | Proposed                                |
| `SSC-PROP-004`           | per-verse-vocab-drift (JSD)    | 2                | §5.3       | Proposed                                |
| `SSC-UNI-002`            | mixed-script-in-token          | 2                | §7.2       | Proposed (split from existing umbrella) |
| `SSC-UNI-003`            | charset-divergence-per-verse   | 2                | §7.3       | Proposed                                |
| `SSC-UNI-CONFUSABLE-001` | confusable-character-mix       | 2                | §4.4       | Proposed                                |
| `SSC-CONS-004`           | variant-cluster-canonical-form | 2                | §4.2       | Proposed (extends `SSC-CONS-001`)       |
| `SSC-MORPH-001`          | morphological-affix-anomaly    | 3                | §3.2       | Deferred until lemma-clustering ships   |

---

## §13. Proposed module additions (consolidated)

| Module                                                       | Path                                                   | Source ref | Priority                    |
| ------------------------------------------------------------ | ------------------------------------------------------ | ---------- | --------------------------- |
| `analysis/lemma.rs`                                          | new                                                    | §3.1       | High                        |
| `analysis/jsd.rs`                                            | new                                                    | §5.3       | High                        |
| Punctuation clinging-class table                             | extend `unicode.rs` or new `punctuation_class.rs`      | §1.4       | High                        |
| Extended edit metric (transpose / expand / compress)         | extend `analysis/bktree.rs`                            | §4.1       | Medium                      |
| `analysis/clustering.rs` (UPGMA + DBSCAN)                    | new                                                    | §4.2, §4.3 | Medium                      |
| Beta-Binomial conjugate update layer                         | inline in `aggregate.rs` or new `analysis/bayesian.rs` | §6.1       | High                        |
| `analysis/good_turing.rs` (novelty mass for `CorpusProfile`) | new                                                    | §5.1       | Low-Medium                  |
| `analysis/morphology.rs` (PoorMans-style affix discovery)    | new                                                    | §3.2       | Deferred                    |
| GMM calibrator                                               | extend `aggregate.rs` surfacing layer                  | §6.2       | Deferred until labels exist |

---

## §14. Implementation batches & ordering

Three batches, each independently shippable.

### Batch A — Quick wins (low risk, high value, days each)

1. **JSD primitive + `SSC-PROP-004`** in `crates/core/src/analysis/jsd.rs`.
   Reference: `SIL.Machine/Statistics/StatisticalMethods.cs:31-50`.
2. **Punctuation clinging-class table** in `crates/core/src/unicode.rs`.
   Reference: `silnlp/common/normalizer.py`. Refactor several rules in
   `signals/punctuation.rs` and `signals/hygiene.rs` to consume it.
3. **Unicode-script lookup** via the `unicode-script` crate; add
   `SSC-UNI-002 mixed-script-in-token`.
4. **Charset-divergence-per-verse** rule (`SSC-UNI-003`).

### Batch B — Variant-clustering upgrade (week-scale)

5. **Extended edit metric** in `crates/core/src/analysis/bktree.rs`:
   transposition (already there), expansion (1→2), compression (2→1).
   Reference: `SIL.Machine/SequenceAlignment/PairwiseAlignmentAlgorithm.cs:98-119`.
6. **`crates/core/src/analysis/clustering.rs`** with UPGMA + DBSCAN.
   Reference: `SIL.Machine/Clusterers/{UpgmaClusterer,DbscanClusterer,ClusterExtensions}.cs`.
7. **Update `SSC-CONS-001 similar-token-cluster`** to surface canonical
   forms via cluster centers (`SSC-CONS-004`).

### Batch C — Foundational, then optional morphology (multi-week)

8. **`crates/core/src/analysis/lemma.rs`** (`SSC-LEMMA-001`) using existing
   `bktree.rs` and `source_relative.rs` + LCS-fraction guard.
9. **Beta-Binomial conjugate update layer** with two-click UI affordance
   (accept / dismiss).
10. **Calibration pass on existing rules** with the new lemma index. Measure
    co-firing correlation matrix. Tune `AggregationPolicy.pair_multipliers`
    from data.
11. **(Conditional)** `analysis/morphology.rs` PoorMans-style affix discovery
    and `SSC-MORPH-001`. Only if step 10 shows lemma-clustering insufficient
    on Bemba / Rai.

### Things explicitly deferred or rejected

| Item                                      | Reason                                              |
| ----------------------------------------- | --------------------------------------------------- |
| Morfessor / FlatCat                       | Requires more data than we have                     |
| Quote convention narrowing pre-pass       | Our toggle resolver handles our cases               |
| Quote convention scoring (registry-based) | Incompatible with our convention-learning thesis    |
| Witten-Bell, Lidstone smoothing           | Modified KN dominates                               |
| Nelder-Mead optimization                  | Grid search fine for v1                             |
| Gaussian-mixture calibration              | Defer until ~200 labels exist                       |
| Logistic regression / RNN ranker          | Defer until ≥300 labels and ~5 well-chosen features |
| RBO (rank-biased overlap)                 | Speculative; not confident it earns its keep        |
| AER / P@k / R@k for monolingual use       | Low value adapting from alignment evaluation        |
| LM perplexity from trained LM             | Requires training; we don't train                   |
| Object pooling, skip lists                | C#-specific GC mitigations; not relevant in Rust    |
| ScriptureRef path notation (now)          | Bookmark; not v1                                    |
| Per-script tokenizer plugins              | ICU4X subsumes                                      |

---

## Appendix A — Source-file index

Files cited in this document, grouped by topic. All paths absolute.

### SIL Machine — punctuation & quotes

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/PunctuationAnalysis/DepthBasedQuotationMarkResolver.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/PunctuationAnalysis/PreliminaryQuotationMarkAnalyzer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/PunctuationAnalysis/QuotationMarkTabulator.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Utils/StringExtensions.cs
```

### SIL Machine — casing

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Translation/UnigramTruecaser.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Translation/UnigramTruecaserTrainer.cs
```

### SIL Machine — morphology

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Morphology/PoorMansStemmingAlgorithmBase.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Morphology/PoorMansAffixIdentifier.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Morphology/PoorMansStemmer.cs
```

### SIL Machine — string clustering

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/SequenceAlignment/PairwiseAlignmentAlgorithm.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Clusterers/UpgmaClusterer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Clusterers/NeighborJoiningClusterer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Clusterers/DbscanClusterer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Clusterers/ClusterExtensions.cs
```

### SIL Machine — statistics

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Statistics/StatisticalMethods.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Statistics/SimpleGoodTuringProbabilityDistribution.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Statistics/WittenBellProbabilityDistribution.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Statistics/LidstoneProbabilityDistribution.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Statistics/LogSpace.cs
```

### SIL Machine — calibration & optimization

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Optimization/NelderMeadSimplex.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/QualityEstimation/UsabilityParameters.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/QualityEstimation/ChrF3QualityEstimator.cs
```

### SIL Machine — USFM & versification

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Corpora/UsfmParser.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Corpora/UsfmToken.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Corpora/ScriptureRef.cs
```

### SIL Machine — engineering

```
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Tokenization/LatinWordTokenizer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Tokenization/ZwspWordTokenizer.cs
/Users/willkelly/Downloads/sil/machine-master/src/SIL.Machine/Utils/PhasedProgressReporter.cs
```

### silnlp — utilities & analysis

```
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/normalizer.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/script_utils.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/usfm_utils.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/postprocesser.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/translation_data_structures.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/corpus.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/analyze.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/common/flatcat_stemmer.py
```

### silnlp — alignment & metrics (mostly out of scope)

```
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/alignment/metrics.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/alignment/rbo.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/alignment/verse_segmentation/break_scorers.py
```

### silnlp — quality estimation

```
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/nmt/quality_estimation.py
/Users/willkelly/Downloads/sil/silnlp-master/silnlp/nmt/alphabet_similarity.py
```

---

## Appendix B — How to use this document

When picking up work from this audit:

1. **For an implementation task**, jump to §14 and pick a batch. Each entry
   has a primary section reference (§3.1 etc.) for the algorithm, and an
   appendix entry for the source files.

2. **For a design question** ("should we add X?"), check §11 Resolved
   questions first; if not addressed there, the relevant § probably has the
   reasoning.

3. **For a hand-off briefing to a fresh agent**, the §14 batch-by-batch
   instructions are the canonical text. They are self-contained.

4. **When updating `VISION.md` §8 with a new rule**, copy the rule's row
   from §12 here, expand to the standard table format (with severity,
   summary, etc.) used in `VISION.md`.

5. **When this document gets stale**, archive it in `research/` with a date
   suffix and write a new one. Don't edit the rationale of resolved
   questions in place — they're a record of why we made the choice we did.


