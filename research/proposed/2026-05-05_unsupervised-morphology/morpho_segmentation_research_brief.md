# Research Brief: Unsupervised Morphological Segmentation for Translation Quality Signals

**Intended audience:** PhD-level researcher in computational morphology or NLP.
**Goal:** Literature audit + method recommendation for one narrow, well-defined problem.
**Context:** Pre-alpha research engine; researcher does not need to read our codebase.

---

## 1. System and use case

We are building a statistical translation quality checking engine for Bible translations.
The engine ingests a **target translation** (the work under review) and an **Gateway language
reference (such as English / Spanish / etc;)** (the source). It emits findings — anomalous verses likely containing
mistranslation, omission, addition, or character-level transcription errors — ranked by
severity.

The engine is entirely statistical and unsupervised. It does not use a neural model or
require labeled error examples. Its evidence comes from:

1. **Corpus-internal consistency** — does this verse look like it belongs to the same
   distribution as the rest of this translation?
2. **Cross-verse structural signals** — does punctuation, sentence-final pattern, and
   verse length align with what the source predicts?
3. **Lexical association tests** — do word pairs in this verse co-occur at rates
   consistent with the corpus as a whole?

Signal type 3 is the subject of this brief.

---

## 2. Corpus characteristics

### 2.1 The eBible corpus
The primary data source is the [eBible corpus](https://github.com/BibleNLP/ebible), which provides
plain-text New Testament translations in over 1,600 language editions. Each edition
is a single file; verse identifiers are standardised (USFM book/chapter/verse).

A **single corpus instance** presented to the engine is one NT translation in one
language/dialect. Representative statistics:

| Measure                      | English (analytic) | Greek NT (fusional) | Bemba / Rai (agglutinative) |
| ---------------------------- | ------------------ | ------------------- | --------------------------- |
| NT verses                    | ~7,900             | ~7,900              | ~7,900                      |
| Word tokens                  | ~175,000           | ~140,000            | ~130,000                    |
| Word types                   | ~8,000             | ~18,000             | ~60,000–90,000              |
| Type-token ratio (TTR)       | ~0.046             | ~0.13               | ~0.50–0.70                  |
| Unigram hapax ratio          | ~0.20              | ~0.45               | ~0.65–0.85                  |
| Word bigram hapax ratio      | ~0.45              | ~0.70               | ~0.85–0.95                  |
| Avg token length (graphemes) | ~4.5               | ~5.3                | ~7.5–11.0                   |

The engine's corpus profiler classifies each NT translation into three regimes based on
measured statistics:

```
classify_regime(p: Profile) -> Regime:
    if p.tokens_per_type >= 22 and p.bigram_hapax_ratio < 0.72:
        return Analytic          # English, Mandarin, Malay
    elif p.tokens_per_type < 9 or p.bigram_hapax_ratio > 0.80:
        return Agglutinative     # Turkish, Finnish, Bemba, Rai, most Bantu
    else:
        return Fusional          # Greek, Latin, Spanish, Russian
```

Across a sweep of 1,600+ eBible translations, roughly:
- ~25% are **Analytic** (word n-gram rules work well)
- ~35% are **Fusional** (word n-gram rules work with care)
- ~40% are **Agglutinative** (word n-gram rules are nearly useless)

### 2.2 What "nearly useless" means, precisely

The engine's lexical association path computes a 2×2 contingency table for every word
bigram (w₁, w₂) observed in the target NT:

```
Table2 {
    a: count(w₁ followed immediately by w₂),
    b: count(w₁ NOT followed by w₂),
    c: count(w₂ NOT preceded by w₁),
    d: count(neither w₁ nor w₂ in position),
}
```

When the table is well-populated (min expected cell ≥ 5), we use Dunning G²:

```
G²(T) = 2 * sum_cell [ observed * ln(observed / expected) ]
```

When the table is sparse (min expected cell < 5), we fall back to Fisher's exact
two-sided p-value, converted to a surprise score:

```
surprise(T) = -2 * ln( fisher_two_sided_p(T) )
```

In an **agglutinative NT** with bigram hapax ratio ~0.90:
- ~90% of all observed word bigrams appear exactly once in the entire NT.
- For any bigram (w₁, w₂) with count(w₁w₂) = 1, `a = 1`, and typically
  `b`, `c`, `d` are each in the range 1–5 as well.
- Fisher's exact p on a 2×2 table with all marginals ≤ 5 is essentially noise —
  no individual co-occurrence carries statistically separable signal from
  any other.

In practice: the association rule fires on nearly everything or nothing, producing
hundreds of false positives per translation, making the rule useless in the
agglutinative regime without downstream filtering that discards most of its output.

---

## 3. What we have tried

### 3.1 Compression texture (NCD proxy)

We train one zstd dictionary from all NT verses and score each verse as:

```
compression_texture_score(verse, dict) =
    compressed_len(verse | dict) / compressed_len(verse)
```

This is a conditional compression ratio, not classical NCD (no `C(xy)` term; dict
replaces the reference). Scores near 0.0 indicate familiar texture; scores near or
above 1.0 indicate the verse is alien to the corpus's character-level patterns.

**Result:** This works well across all morphological regimes. Character-level texture
is largely orthogonal to morphological complexity — a verse with a copy-paste error
from a different language still scores anomalously high even when the source language
is agglutinative. Park et al. (2020) confirm empirically that character-level models
show near-zero correlation with morphological complexity measures (TTR, MATTR):
Spearman ρ ≈ 0.15–0.19, vs ρ ≈ 0.76–0.80 for word-level BPE models.

**Limitation:** Compression texture is a verse-level signal. It cannot localise within
a verse which word or phrase is anomalous. The association path is supposed to do that.

### 3.2 Lemma clustering (prefix-based stem groups)

We group word forms that share a common prefix of length `k` (default k=4) and appear
at least `min_count` times each:

```
candidate_stem(form, k) = form[0..k]           # first k characters

families = {}
for (form, count) in eligible_word_forms:
    stem = candidate_stem(form, k)
    families[stem].append(LemmaForm(form, count))

# Keep only families with >= min_family_size distinct forms
```

Downstream rules can query `family_for_form(w)` and compare rarity against the whole
family count rather than the individual surface form.

**Result:** Works adequately for English (`walk`, `walked`, `walking` → stem `walk`).
Fails for agglutinative languages where:
- Stems are 2–3 characters long (Turkish: `git-`, `sev-`; Finnish: `ot-`, `vei-`).
- Prefixes are not stem markers — suffixes carry all the grammatical information.
- A 4-character prefix cut groups unrelated words (Turkish `gitmek` / `gitme` →
  `gitm` vs. `gözlük` / `gözlük` → `gözl`, but also `gözlem` / `göztaşı` → `gözl`,
  which is not a family).

### 3.3 Morphological complexity signals

We compute the following corpus-level diagnostics used to gate rule behaviour:

```
MorphologyStats {
    n_word_tokens: usize,
    n_word_types:  usize,
    n_hapax_types: usize,
    type_token_ratio:  f64,  # n_word_types / n_word_tokens
    hapax_ratio:       f64,  # n_hapax_types / n_word_types
    # Weight multipliers applied to char vs. word evidence:
    char_signal_weight: f64, # = 1.25 when morphologically_sparse
    word_signal_weight: f64, # = 0.65 when morphologically_sparse
}

morphologically_sparse =
    type_token_ratio > 0.10 AND hapax_ratio > 0.60
```

When `morphologically_sparse`, word-level signal weights are discounted. This is
a heuristic gate, not a fix.

---

## 4. The research question

**Can unsupervised morphological segmentation, trained on raw eBible text alone (no
annotations, no Unimorph, no lexicon), reduce the word bigram hapax rate on
agglutinative NT corpora enough that the G²/Fisher association tests recover usable
signal?**

Formally, we define "usable signal" as:

```
bigram_hapax_ratio_after_segmentation < 0.72
```

That is, the segmented-token bigram hapax ratio falls below the Analytic/Fusional
threshold in our classifier. Below 0.72, Fisher tests on individual bigrams begin to
produce distinguishable p-values, and the rule begins to emit meaningful findings.

A secondary threshold we'd accept:

```
hapax_reduction_relative >= 0.20
# (hapax_ratio_before - hapax_ratio_after) / hapax_ratio_before >= 0.20
```

If bigram hapax falls from 0.90 to 0.72, that is a 20% relative reduction and would
validate the approach. If it falls only to 0.88, the approach is not worth the
added pipeline complexity.

---

## 5. Constraints

The following are hard constraints. Any recommended method must satisfy all of them.

### 5.1 No per-language annotation

We have ~1,600 language editions and add new ones regularly. We cannot afford
per-language morpheme inventories, Unimorph entries, or hand-annotated word lists.
The segmenter must train from raw eBible text only.

**Methods that are therefore out of scope:**
- MIASEG (requires `<word, root_meaning, feature_set>` triples)
- Supervised neural segmentation (Peters & Martins 2022, etc.) unless a pretrained
  multilingual model with adequate zero-shot transfer exists

**Methods that are in scope:**
- Morfessor 2.0 (MDL + raw word frequencies)
- Goldsmith-style signature inference (suffix lattice from unigram/bigram co-occurrence)
- BPE / Unigram LM (raw text, no labels)
- Embedding-based Bayesian methods (Ustün & Can 2021) if embeddings can be trained
  in-corpus
- Pretrained character-level models with documented zero-shot transfer (BantuMorph,
  etc.) for specific language families

### 5.2 Corpus size floor

A single-book corpus (e.g. the Gospel of Mark only) contains ~680 verses and ~15,000
word tokens. The segmenter must not require more training material than this worst case.

Practically: the full NT (~7,900 verses, 130,000–175,000 tokens) is the typical case.
Some translations cover only the NT; a few cover OT+NT (~31,000 verses, ~650,000
tokens).

### 5.3 Concatenative morphology is the primary target

The languages causing the most signal loss are:
- **Bantu family** (Bemba, Lingala, Swahili, Zulu, ~200+ eBible editions): noun class
  prefixes + verb object markers + tense-aspect suffixes; almost entirely concatenative.
- **Rai languages** (Nepal/India): predominantly agglutinative, concatenative suffixes.
- **Austronesian** (Tagalog, Malay-family): moderate agglutination, largely concatenative.
- **Dravidian** (Tamil, Telugu, Kannada): heavily agglutinative, concatenative.
- **Turkic** (Turkish, Uyghur): concatenative agglutinative, well-studied.
- **Uralic** (Finnish, Hungarian, Estonian): concatenative agglutinative.

**Non-concatenative morphology** (Semitic root-and-pattern, Māori reduplication,
templatic morphology) is a secondary concern. We are not asking for a solution to
that; we are asking whether a concatenative segmenter recovers enough signal that
the non-concatenative remainder can be handled by the compression texture path.

### 5.4 Inference speed

The segmenter runs once per corpus at analysis time, before any per-verse rule
evaluation. The full NT is ~7,900 verses × average 25 tokens/verse ≈ 200,000 token
calls. The acceptable wall-clock budget is roughly:

```
segmentation_time(NT) < 10 seconds on a single CPU core
```

Morfessor 2.0 (Viterbi decode on trained model) trivially satisfies this.
Autoregressive neural models at token-by-token inference do not.

---

## 6. What we are asking for

**A literature audit**, not an implementation. Specifically:

### 6.1 Which unsupervised segmenters satisfy the constraints in §5?

For each method that qualifies:
- What is the training requirement (data volume, format)?
- Does it handle suffixing, prefixing, or both?
- What is the approximate segmentation quality (precision/recall against gold) on an
  agglutinative language at NT-scale (~150K tokens)?
- Has it been evaluated on Bible text or comparable low-resource religious text?
- What vocabulary size does it induce, and what is the resulting bigram hapax ratio on
  a known agglutinative corpus (Turkish BOUN or equivalent if available)?

### 6.2 Is there a known method that restores bigram association signal at NT scale?

Concretely: is there published evidence that any unsupervised segmenter reduces
word-bigram hapax ratio to below 0.72 (or equivalently, tokens-per-type above ~9)
on an agglutinative NT corpus or comparable text of similar size?

If yes: cite the paper, the method, and the measured hapax/TTR before and after.
If no: what is the best published reduction, and what additional material or supervision
would be needed to reach the threshold?

### 6.3 Is there a method in the 2012–2026 literature we have likely missed?

We have surveyed:
- Park et al. (2020): Morfessor > BPE on agglutinative Bible text; character-level
  models morphology-agnostic
- Chimalamarri et al. (2020): Morfessor improves cross-lingual embeddings on Bible data
- MorphAGram (2022): framework confirming Morfessor > BPE on 92 Bible languages by
  perplexity
- Ustün & Can (2021): Bayesian + word2vec boundary detection, outperforms Morfessor
  on Turkish
- Stephen & Libovický (2026): IBM Model 1 alignment as a gold-free segmentation evaluator
- Mutisya & Mugane (2026): BantuMorph — zero-shot ByT5-small for Bantu morphology
- Varatharaj & Todd (2024): Māori — Morfessor handles concatenative correctly;
  reduplication fails
- OSImUnr (2022 SIGMORPHON): char-trigram noise in agglutinative languages degrades
  association tests; morphological segmentation recovers ~60 percentage points on
  semantic similarity tasks
- Peters & Martins (SIGMORPHON 2022): supervised ULM segmentation (not applicable to us)
- Rubehn et al. (2025): numeral segmentation across 25 languages

**Specifically not yet surveyed with confidence:**
- Any work post-2023 on morpheme-aware language model adaptation for low-resource languages
- Transformer-based unsupervised segmentation (e.g. ByT5-style pretraining with
  morphological inductive bias) that runs without per-language data
- Any work that measures bigram association test quality (not just perplexity) as a
  function of segmentation method on agglutinative corpora

### 6.4 Honest negative result guidance

If the literature indicates that **no unsupervised segmenter recovers bigram association
signal at NT scale**, we want to know that clearly, with the evidence. The decision we
would then make is to retire the word bigram association rule for the Agglutinative
regime entirely and route those corpora through compression texture + character trigram
association only. That is a legitimate engineering outcome; we do not want a forced
positive recommendation.

---

## 7. What the researcher does not need

- Access to our codebase or any of our data. The eBible corpus is publicly available at
  https://ebible.org/Scriptures/ (plain text, UTF-8, verse-per-line format). Any paper
  that uses this corpus can be reproduced against the same data we use.
- Implementation. We are asking for a literature audit and method recommendation, not
  code. If the recommendation is "use Morfessor 2.0 trained on word unigrams from the
  raw NT text", that is complete guidance.
- Evaluation across all 1,600 languages. A representative sample of 3–5 well-chosen
  agglutinative languages (e.g. Turkish, Finnish, Swahili, Bemba, Tamil) against NT-scale
  text is sufficient to answer the research question.

---

## 8. Deliverable

A written document (5–15 pages) containing:

1. A table of unsupervised segmentation methods that satisfy the constraints in §5,
   with the data from §6.1 filled in for each.
2. A direct answer to the question in §6.2 (yes/no, with citation or absence of citation).
3. Any methods from §6.3 we appear to have missed, with a one-paragraph summary of
   each.
4. A prioritised recommendation: which single method, if we were to implement one,
   gives the best expected reduction in agglutinative bigram hapax ratio with the least
   annotation overhead?
5. An honest assessment of whether the threshold (bigram hapax ratio < 0.72 at NT scale
   without annotations) is achievable at all, and if not, what the ceiling appears to be.
