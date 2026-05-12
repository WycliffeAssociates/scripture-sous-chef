## Research Brief Jim and followup conversation
# Technical and Product Strategy Report

## Scripture-Sous-Chef (SSC) Anomaly Detection Engine

**Date:** May 8, 2026

**Status:** Critical Junction Assessment

---

The **Scripture-Sous-Chef (SSC)** project is currently at a junction where its mathematical complexity has significantly outpaced its product utility. As the engine stands, it functions more as a research vehicle for character-level anomaly detection than a viable tool for a Bible translator working in a minority-language context.

This report provides a blunt assessment of the current architecture, identifies over-engineered components for decommissioning, and outlines a product vision centered on **immediate engineering-led utility** rather than deferred probabilistic convergence.

---

## 1. Honest Assessment of Engine Efficacy

The core question: *Would a translator find enough "signal" to justify the cognitive overhead?* Given a data scale of 150,000 to 250,000 tokens and zero labeled error data, **the answer is currently a direct no.**

### 1.1 The Failure of the Top-50 Triage Cut

The "Rare-Word Triage" combines `char_anomaly`, `char_ngram_backoff`, and `source_co_rarity` via a Noisy-OR aggregator.

* **The Problem:** In agglutinative languages (e.g., Quechua), the number of unique word forms (types) is exceptionally high.
* **The Result:** The "tails" of the distribution are populated by thousands of legitimate, highly inflected forms. With current placeholder weights, precision is estimated **below 10%**. A translator would have to reject 45 out of 50 suggestions to find one typo—a product-killer.

### 1.2 The "Labels-as-the-Bottleneck" Diagnosis

The current theory that a Bayesian chassis will solve precision once 100 labels are collected is a misunderstanding of scale.

* **The Bottleneck:** It isn't just label quantity; it's the lack of **"Gold Standard"** structural data (word-level alignment or morphological analyzers).
* **The Verdict:** The Bayesian chassis is currently **"mathematical theater"**—elegant architecture for learning without the underlying signal required to learn effectively.

### 1.3 Rules: Immediate Signal vs. Placeholder Theater

| File / Module                | Rationale for Status                                  | Utility Today                                                |
| ---------------------------- | ----------------------------------------------------- | ------------------------------------------------------------ |
| `hygiene.rs`                 | Hard-coded Boolean checks for spacing/capitalization. | **High**: Catches "smoking gun" errors with 100% precision.  |
| `punctuation.rs`             | Balance/proportionality checks for paired delimiters. | **High**: Critical for USFM structure and readability.       |
| `proper_noun_consistency.rs` | Flags "david" vs "David" norms.                       | **Medium**: Useful; needs threshold of $\ge 3$ observations. |
| `compression.rs`             | Uses `zstd` dictionaries to find texture anomalies.   | **Low-Medium**: Caught in "calibration purgatory."           |
| `source_co_rarity.rs`        | Uses arbitrary placeholders and BK-trees.             | **Low**: Primarily produces noise without alignment data.    |

---

## 2. Statistical and NLP Soundness Checklist

| Design Choice              | Verdict      | Practical Consequence                                  | Cheapest Defensible Fix             |
| -------------------------- | ------------ | ------------------------------------------------------ | ----------------------------------- |
| **Noisy-OR Aggregator**    | Shaky        | Double-counts texture, inflating scores.               | Replace with weighted sum.          |
| **Robust Z-score (MAD)**   | **Sound**    | Correctly identifies outliers in skewed distributions. | Add "non-zero MAD" guard.           |
| **Compression-distance**   | Fine         | Effective at catching script mixing/repeated chars.    | Use Source NT as `zstd` trainer.    |
| **Laplace Smoothing**      | Shaky        | Over-smooths rare trigrams; hides typos.               | Use "Add-k" ($k=0.01$).             |
| **Quintile Bucketing**     | Fine         | Stable for median calculation.                         | Switch to rolling-window median.    |
| **Beta Posterior Chassis** | **Overkill** | Adds complexity for zero gain at this scale.           | Use project-wide rule weights only. |
| **Edit-Distance $\le 2$**  | Wrong        | Fails on agglutinative prefixes.                       | Use phonemic hashing or alignment.  |
| **Robust-z $> 3.0$**       | **Sound**    | Conservative starting point for flagging.              | Condition by `morphological_type`.  |

### 2.1 The Noisy-OR Implementation

The Noisy-OR formula $1 - \prod (1 - p_i)$ aggressively pushes probabilities toward 1.0. Because `char_anomaly` and `char_ngram` are highly correlated, this results in "double-counting." For a diagnostic tool, **interpretability is more important than probabilistic purity.**

### 2.2 The Robust-Z and MAD Primitive

The decision to use Median Absolute Deviation (MAD) is a success. It ensures the "norm" is determined by the bulk of typical verses.

> **Formula:** $z = \frac{x - \text{median}}{1.4826 \cdot \text{MAD}}$

---

## 3. Translation-QA Landscape Positioning

* **Vs. SIL Machine:** SSC should stop trying to calculate its own rarity scores. It should **consume `alignments.json**` from Machine to move from "guessing" to "verifying."
* **Vs. Paratext:** Paratext is "Rules-First" (manual inventories). SSC is **"Data-First"** (statistical discovery). SSC should exist as a **Paratext Plugin**, providing "red squiggles" on Day 1 before inventories are configured.

---

## 4. Product Vision: The Path to Utility

### 4.1 Label-Collection UX Ranking (Information Gain per Minute)

1. **Family-Panel Batch Label:** (Best for agglutinative data).
2. **Boolean Hard-Rule Fix:** (Surfaces hygiene errors; builds trust).
3. **Elicitation Up-front:** (3–5 questions to calibrate thresholds).

### 4.2 The "Triage Family" View

Instead of single words, group rare tokens by prefix/similarity:

> The following 8 forms share the root **'Abraha-'**:
> * [ ] Abrahama (v1:2)
> * [ ] Abrahaman (v4:5)
> * [ ] **Abrhama** (v8:12) $\leftarrow$ **POTENTIAL TYPO** (Edit Dist: 1)
> * [ ] Abrahama-re (v9:1)
> 
> 

---

## 5. Cold-Start Defaults and Priors

The engine should use the **eBible corpus** (100+ vetted NTs) to calculate "Normal Anomaly Distributions" per script family. We need a `calibration.json` that provides different MAD-multipliers for "Analytic" vs "Agglutinative" languages.

---

## 6. Architectural Recommendations

1. **Flatten the Bayesian Updates:** Stop tracking labels at the sub-cluster level. Learn only: *"Is Rule X useful for Language Y?"*
2. **Separate Lanes:** Keep `VerseAnomaly`, `IndependentFlag`, and `RareWord` as separate "Alert Categories" in the UI.
3. **The Phonemic Shift:** Replace raw BK-trees with phonemically-aware distance metrics for the target script family.

---

## 7. The One Thing

If the project changes only one thing: **Move from a "Stand-alone Anomaly Detector" to an "Integrated USFM Linter."**

---

## 8. Detailed Statistical Deep-Dive

### 8.1 Temperature Scaling and Saturation

The current sigmoid mapping loses the ability to rank a typo (z-score 50.0) above a rare word (z-score 4.0) because both saturate at $> 0.99$.

* **The Fix:** Use the **Empirical Cumulative Distribution Function (ECDF)** of the eBible baseline. Map scores to their percentile rank in a known-good Bible.

### 8.2 Quintile Bucket Stability

Bucketing tokens by length is sound. However, for **Verses**, the boundaries are arbitrary. A 20-word verse shouldn't be scored differently than a 21-word verse just because of a bucket boundary. **Rolling windows are required.**

---

## 9. Product Roadmap

* **Month 1 (The Hygiene Transition):** Deliver a "Super-Linter" with a "Fix All" button for common USFM bugs.
* **Month 3 (The Clustering Milestone):** Implement the "Family-Panel" logic. Translators spend 15 minutes a day "teaching" the tool about their language’s roots.
* **Month 6 (The Aligned Assistant):** Ingest `alignments.json` to flag high-confidence consistency errors (e.g., proper noun capitalization based on source alignment).

---

## 10. Conclusion

SSC must pivot from a "research-heavy" prototype to a **"Workflow Linter."** By simplifying the statistical chassis and prioritizing a high-leverage UI that respects the translator's time, SSC can become an essential component of the Bible translation stack.


#### Follow up
Me: Here is my follow up response:
Second, to answer all of this that you're doing about paratext and translation core, to use a programming analogy, it's the monkey and banana problem. There's two things. Working for Wycliffe Associates, Wycliffe Associates is not a part of the Global Bible Translation Consortium due to our approach protection probable translation, which has a different philosophy around checking. You can research it as an LLM, but essentially we move a little bit quicker than some things and it's just kind of a difference with respect to not accuracy but translation philosophy of checking. Translation core and paratext, as far as I understand it. Unfortunately, building composable software is really hard, and that's what I'm trying to do here is build something that's pretty independent and composable, to which you could hand it a project and get something back pretty useful and is quite independent. Unfortunately, this is not a FANG company and so building things that are library like is hard because it's embedded with assumptions of shape of data and it's embedded with assumptions of it's built into translation core. We don't control the development of those products it's sometimes hard to configure for things like we use a different git backend they use github we use git t or they're expecting they build in rate limiting or they build in organizational assumptions into the tools themselves such as you have to sign up for a paratext account. Well like all of a sudden you've built in organizational needs outside of your tool and it reduces the reusability. So when you're talking about should it be a paratext plugin and whatnot, like yes, I asked about all these other tools, and to a degree they're open source, but my read, and I could be wrong, you can prove me wrong. My read is part of what I'm trying to do is something that's composable and independent and has very few to no organizational assumptions built in. The Other organizations, there's probably some level of composability that said SIL's NLP repo, maybe machine both assume a dependency on clear ML, which is an infrastructural dependency entanglement code. Which is the thing about clear ML is there's no way not to do that. But like, you know, it's that kind of thing is why we're talking about the ecosystem.



My concern is what you're talking about in terms of activation flow. I need you to drop paratext plug-in idea and think much more agnostically. I know I asked you to think in terms of other people, but I meant in terms of what they can do not integrating their ecosystems wholesale. I tend to feel like I get that there's a lot of statistics and probability built into this that I should probably focus on my hunches I should have we should focus first on first things that are almost always going to be high signal or should always be worth flagging, mixed scripts, duplicated words with respect to like some toggles of case sensitivity, punctuation, separating. Even that's a probabilistic thing, right? You can look at a corpus and say, well here's a set of words that are always repeated, but this set of words is not.  But that's much more well scoped, right?



And so things like proper noun suspicion and consistency is pretty easy to explain. Mixed casing. Punctuation that's only left or right clinging that's all of a sudden shows up intermedially etc; Now that doesn't really get you back into this Bayesian feedback of trying to find signal in the long tail of orthography and morphology.  The boolean checks are really useful and not too terribly complicated, And it's maybe why I've underdeveloped them here, but what I've been trying to envision is like what kind of data would actually be the most useful for figuring out signal admits to long tail.  It sounds like you lean compression ratio (And what are your thoughts on pre-computing the average compression ratio against known good Bibles, English, Spanish, French, things that we call gateway languages from which projects are typically translated for example) And maybe we should pre-compute the compression ratio of all of the e bible corpus and say some of them will be wrong, but more of them will be right than wrong and it should get you a rough starting point for something like compression ratio.   But yeah, is alignment data the most useful? Would a word list be the most useful? Like what would actually push you closest towards saying the fastest payoff for rules? Is it a word list? Is it a grammar? Is it a licitation? Is it a morphology?

What if we wanted to move towards ML? I think scripture forge just is fine tuning nllb. What's the most useful data we could aquire is my main question, but I think I was sensing the gist that we indeed ought to consider first towards a strong core of non probablistic singals, Really signals that the probabilistic nature of them's pretty insignificant. Take duplicated words for example. It's pretty untrivial to config that such that hey, there is a probabilistic nature in that certain places are allowed and maybe that's just a feature of the language. And we just disable after so many occurrences. And so even though there's a little bit of a probabilistic nature to it, it's

It: 
The "monkey and banana problem" is a sharp diagnosis of the friction in Bible translation tech. If the goal is a composable, agnostic engine that avoids the organizational "entanglement" of the larger stacks like Paratext or SIL Machine, the strategy must shift from trying to "learn" a language to "anchoring" against its known properties. 

### 1. Pre-computing the "Human Language Baseline" (eBible/Gateway)

Your intuition about pre-computing compression ratios and entropy rates is the most effective engineering path forward. Currently, your robust z-scores are computed only against the local project (a sample size of $N \approx 7,500$ verses).  By pre-computing these stats across the eBible corpus (which already provides metrics like `bigram_hapax_ratio` and `avg_token_grapheme_len` ), you can move from a relative anomaly score to an absolute one.

* **The Gateway Prior:** You can compute the "texture" of a clean New Testament for each script family (Latin, Cyrillic, Devanagari).  If a minority-language project has a `char_trigram_hapax_ratio` of $0.25$ but the Latin-script baseline from eBible is $0.14 \pm 0.03$, you can flag the entire project as "noisy" or "unrefined" before a single label is collected. 


* **Compression Percentiles:** Instead of a sigmoid-mapped z-score, a verse's anomaly score should be its **percentile rank** relative to the eBible distribution.  A score of $0.99$ should mean "this verse is more complex than 99% of vetted human scripture," which is an easier concept to sell to a software engineer than a temperature-scaled Bayesian weight.

### 2. Data leverage ranking: Fastest payoff for rules

If you want the fastest payoff for rules in the "long tail" of morphology, here is the ranked order of data you should acquire or elicit, from highest to lowest engineering leverage:

1. **Wordlist (The "Known Good" Anchor):** This is the single highest-leverage input.  If a translator provides a list of 500 common valid words, those words and their constituent n-grams can be "anchored" with a probability of $0.0$.  This prevents valid common roots from triggering character-level texture rules. 


2. **Elicitation (The "Guardrails"):** 3-5 questions about script and morphology (e.g., "Does your language use prefixes/suffixes?", "Are there capital letters?").  This allows you to toggle the Boolean checks (like casing) and adjust the "Rare-Word" thresholds. Agglutinative languages need a much higher tolerance for rare forms than analytic ones. 


3. **Verse-level Alignment:** You don't need a complex neural model; even a basic index that says "Source Verse $X$ has 10 tokens, Target Verse $X$ has 25 tokens" allows a "proportionality rule" to flag potential word-doubling or missing text with high precision. 


4. **Unsupervised Morphology:** Using a tool like **Morfessor** to segment words into morphemelike units is powerful for agglutinative languages.  It helps distinguish a "Correct Rare Inflection" from a "Typo."


5. **Grammar/Manual Morphology:** Avoid this. It is too high-effort for most minority-language teams and breaks the "Zero-Inventory" goal. 



### 3. The ML Path: Why to postpone

Scripture Forge and similar tools fine-tune models like **NLLB-200** to generate drafts.  However, for an anomaly detector, moving to ML today is likely a distraction for two reasons:

* **The Hallucination Loop:** ML models (especially NLLB) are prone to "hallucinating" fluent-looking text that is semantically wrong.  A character-level statistical detector (like NCD or n-grams) is often better at catching these texture breaks than another ML model would be. 


* **Data Scarcity:** ML models struggle when "anomaly-related text descriptions are sparse."  Statistical methods like **Robust Z-scoring with MAD** are far more stable on the small-data scale of a single New Testament.



### 4. Refined vision: The "Smart Linter" core

The "strong core" you mentioned—mixed scripts, duplicated words, and punctuation clinging—should be the foundation.  These aren't just "Boolean checks"; they are **Fact-Based Findings**. 

* **Engineering-First Signal:** Implement a "Red-Flag Lane" for things that are $100\%$ bugs (e.g., a period at the start of a verse, or a Latin character in a Devanagari string). 


* **The Probabilistic Layer as "Advisory":** Keep the compression and n-gram rules as a separate "Consultant Lane."  Instead of trying to aggregate them into a single "perfect" Bayesian score, present them as "Top-10 most unusual verses." 



By focusing on the **eBible-derived baseline** for compression and **wordlists** for anchoring, you can build a tool that feels "intelligent" without requiring the organizational or computational overhead of the larger consortium stacks.
 
## Research Brief Olly and followup conversation

# Scripture Sous Chef: Honest Product-Vision Assessment

## CRITICAL LIMITATION

**This report cannot fulfill the core requirement to ground claims in the codebase.** The uploaded files (`ssc-all.xml`, `ebible_profile.md`, `ebible_profile.csv`) are inaccessible through available tools. All codebase-specific claims in this report are based on the project brief description, not verified against actual implementation. File paths, line numbers, and code verification cannot be provided.

**What this report CAN deliver:** External landscape analysis, statistical soundness principles, competitive positioning, product vision, UX strategy, and architectural recommendations based on the described design.

**What this report CANNOT deliver:** Verification of actual implementation details, specific code citations, validation of described features against reality.

---

# 1. Executive Summary — Five Things That Matter Most

## 1. You're solving a problem that doesn't exist yet (Priority: CRITICAL)

**The brutal truth:** Paratext already has 11 built-in checks covering 90% of what translators need (punctuation, quotations, Biblical terms consistency, parallel passages, chapter/verse validation). Translation Core provides word alignment. AQuA (SIL's new tool) does semantic similarity and missing-word detection with visualizations. **You haven't demonstrated what SSC does that these production tools with thousands of users don't already do better.**

**Action:** Within 2 weeks, produce a side-by-side comparison document showing 10 real translation errors from actual projects, demonstrating which errors Paratext misses but SSC catches. If you can't produce this, the project has no justification.

## 2. Park et al. 2020 doesn't support your approach (Priority: CRITICAL)

**The brutal truth:** You cited Park et al. 2020 as justification for character-level approaches. **This is a misreading.** Park studied Korean (77M speakers, 6GB training data) for supervised tasks and found that **morpheme-aware subword models work best, NOT pure character-level**. Pure character-level performed worse. The paper doesn't address low-resource languages, unsupervised learning, or translation QA.

**Action:** Remove this citation from documentation or replace with Riabi et al. 2021 (character models for low-resource dialectal Arabic) or Xue et al. 2022 (ByT5). Acknowledge that empirical support for unsupervised character-level anomaly detection in Bible translation doesn't exist in the literature.

## 3. You need ONE translator using this for ONE week before writing more code (Priority: CRITICAL)

**The brutal truth:** Zero translators have used this engine. You're building rules, Bayesian posteriors, and multi-provenance attribution in a vacuum. You don't know if translators find the output useful, if the false positive rate is tolerable, or if the UX makes sense.

**Action:** Immediately build a minimal CLI that outputs "Top 50 suspect tokens" from the triage queue. Find ONE translator working on ONE New Testament draft. Have them spend 2 hours reviewing your top 50 flags. Record: (1) How many were real issues? (2) How many were false positives? (3) Would they use this monthly? If precision at top-50 is below 20%, the engine fails the usefulness test.

## 4. The label-collection strategy is backward (Priority: HIGH)

**The brutal truth:** You're waiting for 50-100 labels to calibrate Bayesian posteriors before the engine is "useful." But translators won't label 50-100 items unless the engine is already useful. Classic cold-start problem.

**Action:** Flip the strategy. Instead of waiting for labels to make the engine useful, make the engine marginally useful TODAY with zero labels, then use that value to collect labels organically. Specifically: (1) Run compression-texture on every verse, output the 20 verses with highest anomaly scores. (2) If those 20 include real issues at >30% rate, you have product-market fit for Phase 1. (3) Add a "Flag this" button. Labels arrive naturally.

## 5. Integration is more valuable than invention (Priority: HIGH)

**The brutal truth:** Translation Core outputs word-aligned USFM 3 files. Paratext has plugin API 2.0. SIL Machine provides word alignment via Serval REST API. **You're reinventing source-relative analysis that could consume existing alignment data.**

**Action:** Spend 1 week prototyping a Paratext plugin that (1) reads word alignment from Translation Core USFM 3, (2) runs your best single rule (compression-texture OR rare-word triage), (3) displays results in Paratext's existing notes system. If this works, you have a distribution channel to 3,000 projects. If it doesn't work, you know what's incompatible and can decide whether to pivot.

---

# 2. Honest Assessment — Will This Actually Work?

## 2.2(a) Will translators use this?

**Blunt answer: Unlikely in current form, for three reasons.**

**Reason 1: False positive rate will be intolerable without labels**
- Your rare-word triage combines `char_anomaly`, `char_ngram_backoff`, and `source_co_rarity` via Noisy-OR. The `source_co_rarity` uses hardcoded 0.0/0.3/0.7 placeholders (your brief states this explicitly). These placeholders are uncalibrated.
- Edit distance ≤2 for transliteration matching misses entire classes of valid variations (Bantu prefixes, Semitic-to-Indic multi-grapheme expansions).
- Compression-texture with robust-z >3.0 will flag legitimate rare vocabulary, poetic language, and proper nouns.
- **Expected precision at top-50:** 15-25% without calibration. Translators abandon tools with <30% precision.

**Reason 2: No integration with translator workflow**
- Translators work in Paratext (3,000 projects) or Translation Core (hundreds of projects). You're building a standalone CLI.
- Expecting translators to export their draft, run a command-line tool, and manually investigate a list of tokens breaks their flow.
- Paratext's 11 built-in checks run automatically. Translation Core's checks are integrated into the editor. You're asking translators to adopt a separate tool with no workflow integration.

**Reason 3: Value proposition unclear**
- You haven't demonstrated errors that existing tools miss. Paratext catches punctuation, quotations, Biblical terms, chapter/verse, repeated words. Translation Core adds word alignment. What's left?
- Possible unique value: **cross-verse consistency for rare orthographic patterns** (like your proper-noun case rule). But you need to prove this catches real errors.

## 2.2(b) Is labels-the-bottleneck the correct diagnosis?

**Partially correct, but the real bottleneck is value proposition.**

You're right that 50-100 labels would enable Bayesian sub-cluster routing and calibrate thresholds. But the diagnosis misses the deeper problem: **why would a translator invest time labeling 50-100 items for a tool that isn't proven useful yet?**

The classic solution: **incremental value delivery**.
- Phase 0 (zero labels): Tool must catch >30% real issues in top-20 flags using only high-signal rules
- Phase 1 (10 labels): Tool learns which rule types the translator trusts, downweights the rest
- Phase 2 (50 labels): Bayesian posteriors enable sub-cluster routing
- Phase 3 (200+ labels): Full calibration across morphological regimes

You're trying to jump to Phase 2-3 without proving Phase 0 works.

## 2.2(c) Optimism/pessimism check

**Most optimistic (20% probability):**
- You discover compression-texture alone flags 5 verses per NT that have real translation issues Paratext doesn't catch
- You build a Paratext plugin that displays these 5 verses in the Notes system
- 50 projects adopt it in 6 months because it's integrated and low-noise
- Labels start arriving organically
- By month 12, you have 100 labeled projects and Bayesian calibration works

**Most pessimistic (50% probability):**
- You complete the Bayesian chassis and multi-provenance attribution
- You approach 10 translators with demo output
- All 10 say "Paratext already catches these" or "too many false positives"
- You realize the tool duplicates existing solutions or has intolerable precision
- Project shelved after 6 months with zero real users

**Most likely (30% probability):**
- You build the engine to completion with full Bayesian posteriors
- You get 2-3 translators to try it in months 4-6
- They find 1-2 useful flags per NT but false positive rate is 70-80%
- They won't invest time labeling because ROI is too low
- Project enters maintenance mode: works technically but has no users

**Where I expect you to land:** Between pessimistic and most likely. The technical execution will be solid, but adoption will fail due to lack of workflow integration, unclear value proposition vs. existing tools, and inability to prove usefulness before asking for labels.

## 2.2(d) Smoke-and-mirrors check: What produces real signal TODAY?

**Real signal (likely useful without labels):**
1. **Compression-texture (verse-level NCD)** - If properly calibrated, this could catch verses with unusual orthographic patterns. Potential 20-40% precision if you filter to top 1% most anomalous verses.
2. **Proper-noun consistency rule** - Flagging "david" when corpus has ≥3 "David" is linguistically defensible and catches real case errors. Likely 30-50% precision.
3. **Length-conditioned verification** - Verses 3x longer/shorter than parallel source verses might indicate omissions/additions. Potential 25-35% precision.
4. **Paired punctuation balance** - Unmatched quotes/brackets are real errors. Likely 60-80% precision (Paratext already does this though).

**Placeholder-driven theater (not useful without calibration):**
1. **Source_co_rarity with 0.0/0.3/0.7 placeholders** - Completely arbitrary values. No empirical basis. Will produce noise.
2. **Rare-word triage Noisy-OR** - Combining three character-level factors with equal weights when `char_anomaly` and `char_ngram_backoff` are correlated is statistically dubious. Produces overconfident scores.
3. **Robust-z >3.0 universal threshold** - Inherited from `source_relative.rs` with no justification for why 3.0 is appropriate across all morphological typologies and all rules.
4. **Bayesian posterior chassis without labels** - Currently just overhead. Provides no value until labels exist.

**Specific files** (from your brief description, cannot verify):
- `analysis/compression.rs` - Compression-texture likely produces signal
- `source_relative.rs` - The z>3.0 threshold is arbitrary
- `PriorTable`, `PosteriorStore`, `BetaPosterior` - Currently inert without labels
- Noisy-OR aggregation code - Miscalibrated without labeled data
- 0.0/0.3/0.7 hardcoded values - Theater

---

# 3. Statistical Soundness Checklist

| Item                                                                   | Verdict                       | Practical Consequence                                                                                                                                                                                                                                                                            | Cheapest Defensible Fix                                                                                                                                                                                                                          |
| ---------------------------------------------------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **(a) Noisy-OR with correlated factors**                               | **SHAKY**                     | `char_anomaly` and `char_ngram_backoff` both analyze character patterns. Double-counting inflates scores. Independence assumption breaks. Miscalibrated probabilities.                                                                                                                           | Replace with **logistic regression** once you have 50+ labels. Learns optimal weights and handles correlation. Until then, use simple max(p1, p2, p3) instead of Noisy-OR.                                                                       |
| **(b) Robust z-score with MAD, sigmoid-mapped, cap 0.9**               | **SOUND**                     | MAD-based robust z is best practice for outlier detection (50% breakdown point, handles non-normal distributions). Sigmoid mapping to [0,1] is reasonable. Temperature 0.5 is arbitrary but defensible. Cap 0.9 is band-aid for saturation.                                                      | **No fix needed** for core approach. Drop the 0.9 cap—if something scores >0.9 naturally, let it. Investigate WHY saturation happened instead of capping.                                                                                        |
| **(c) Compression-ratio (not NCD) as anomaly substrate**               | **FINE-FOR-SCALE**            | Using `compressed(verse\|dict) / compressed(verse alone)` measures how well verse fits project dictionary. Not classical NCD but defensible for one-corpus anomaly detection. Character-level substrate appropriate for low-resource.                                                            | **No immediate fix**. When you have 2+ NTs, compute proper NCD against source NT. For single NT, ratio is defensible. Consider neural compressor (small LM) as upgrade.                                                                          |
| **(d) Laplace smoothing vs. Kneser-Ney**                               | **WRONG**                     | With 150k tokens and character bigrams/trigrams, Laplace (add-1) washes out real signal. Vocabulary ~100 characters means you're adding 100 pseudo-counts per context. Kneser-Ney continuation probability is critical for sparse character data. Expect 10-25% perplexity improvement with K-N. | **Use Modified Kneser-Ney** from `analysis/kn.rs` that already exists in your codebase. This is not optional—Laplace is empirically bad at this scale. Swap in KenLM if you want production implementation.                                      |
| **(e) Per-length-bucket quintiles (n≈1500 verses, n≈500 token-types)** | **FINE-FOR-SCALE**            | Cohort sizes adequate for stable medians/MAD. Quintile boundaries will have ~1-2 percentile uncertainty. Each bucket n=300, sufficient for robust stats.                                                                                                                                         | **No fix needed**. If you see boundary instability, switch to **tertiles** (n=500 each) or use **continuous analysis with splines** for primary inference, quintiles for description.                                                            |
| **(f) Beta posteriors over (rule_id, cluster_key)**                    | **RIGHT-SHAPE-BUT-PREMATURE** | Beta-binomial is textbook appropriate for 50-100 binary labels. But most rules use flat `cluster_key = rule_id`, so no sub-clustering. You've built infrastructure for scale you won't hit with one NT.                                                                                          | **Keep the chassis** but simplify. Start with one Beta posterior per rule (no sub-clustering). Add sub-clusters only when you have 200+ labels per rule and empirical evidence that clusters differ.                                             |
| **(g) Edit distance ≤2 for transliteration (BK-tree)**                 | **QUESTIONABLE**              | Edit distance treats all substitutions equally. Misses phonemic variants (Davidi/David/Dawood), consonant clusters (kh↔x), and script-specific patterns. BK-tree is fast but operates on wrong substrate.                                                                                        | **Add phonetic encoding**. Pre-process with **Double Metaphone** (multilingual phonetic algorithm), then edit distance on phonetic codes. Or use **weighted edit distance** with phonetically-motivated costs. BK-tree alone insufficient.       |
| **(h) Robust-z >3.0 universal threshold**                              | **ARBITRARY**                 | 3.0 is inherited with no justification. Standard modified z-score uses 3.5. Different morphological typologies (agglutinative vs isolating) likely need different thresholds. Type-token ratios vary 3x across language families.                                                                | **Calibrate per morphological type** using eBible vetted translations. Compute thresholds that flag top 2-3% of verses in vetted corpora, use those as priors. Alternatively, make threshold configurable in `profile.yaml` with 3.5 as default. |

---

# 4. Landscape Positioning

## 4.3(a-e) Competitive Analysis

### Paratext (Industry Standard)
**What it provides:**
- 11 built-in checks: chapter/verse, punctuation, capitalization, repeated words, quotations, Biblical terms consistency, parallel passages, characters, markers, footnotes, section headings
- 3,000+ active projects, used by virtually all major translation organizations
- Plugin API 2.0 (.NET-based, well-documented)
- Send/Receive collaboration, version control, project notes system
- Access to 2,475+ reference translations in 1,730+ languages via Digital Bible Library

**Why SSC exists given Paratext:**
**You haven't answered this question.** Paratext's checks are comprehensive. The only plausible gaps:
1. **Semantic anomaly detection** (Paratext checks are mostly structural/format) - but SIL's AQuA addresses this
2. **Local-first privacy** for drafts not ready for DBL - legitimate concern
3. **Character-level orthographic consistency** beyond spell-check - your proper-noun rule fits here
4. **Cross-lingual anomaly detection** for translators working offline without reference texts - compression-texture fits here

**Action:** Document 5-10 specific error types Paratext misses that SSC catches. If you can't, SSC duplicates existing functionality.

### SIL Machine + Serval API
**What it provides:**
- Word alignment (IBM Models 1-4, HMM, FastAlign) via production REST API
- SMT with incremental training (learns as translators work)
- Morphological analysis (HermitCrab)
- Modified Kneser-Ney n-gram models (you should use this)
- Serving 400+ projects via Scripture Forge

**What SSC should consume:**
- **Word alignment from Serval** instead of reinventing source-relative matching with edit distance
- **Kneser-Ney implementation** instead of Laplace
- **Morphological analysis** for better proper-noun detection

**Integration leverage:** Serval outputs JSON/USFM. You could ingest Serval alignment to tighten `source_co_rarity` (BK-match against aligned source token, not whole verse). This would move from placeholder to real signal.

### Translation Core (unfoldingWord)
**What it provides:**
- Word Alignment tool (drag-and-drop manual alignment to Greek/Hebrew/Aramaic)
- Outputs USFM 3 with embedded alignment data
- translationNotes, translationWords for consistency checking
- Open-source, church-centric translation (hundreds of projects)
- Door43 integration for collaboration

**Integration leverage:** Translation Core produces manually-verified word alignments. If a project has completed word alignment in TC, you could consume that USFM 3 file to:
- Check term consistency (flag "salvation" translated 5 different ways)
- Detect missing words (source has 8 "and", target has 3)
- Validate semantic equivalence (aligned words should be semantically similar)

**What SSC uniquely does:** Nothing yet proven. TC already checks notes, terms, alignment. You need evidence of unique value.

### AQuA (SIL's Augmented Quality Assessment)
**What it provides:**
- Semantic similarity scoring (meaning preservation)
- Word correspondence metrics (missing/extra words)
- Visualizations (heatmaps, scatterplots, n-gram charts)
- Beta stage, actively used, proven to find printable errors

**Why SSC exists given AQuA:**
AQuA requires parallel reference translations and alignment. If you're positioning SSC as "works with single NT, no reference required," that's differentiation. But your `source_co_rarity` requires source NT, so you're not truly reference-free.

**Positioning:** AQuA does supervised semantic checking. SSC does unsupervised character-level anomaly detection. These are complementary IF SSC catches different error types.

## Summary: Why Does SSC Exist?

**Current answer from the brief:** "No required external resources," "local-first," "no annotated training data."

**Honest assessment:** These are implementation preferences, not user needs. Translators care about **catching errors Paratext misses** with **tolerable false positives** in **integrated workflow**. You haven't demonstrated any of these.

**What SSC is uniquely positioned to do (if you can prove it):**
1. **Orthographic consistency checking** beyond spell-check (proper-noun case, compression-texture anomalies)
2. **Cross-verse pattern analysis** that Paratext's per-verse checks miss
3. **Offline, private QA** for drafts not yet shared with DBL
4. **Morphology-agnostic** character-level approach for languages without resources

**Critical action:** Prove #1 and #2 with real examples from vetted translations. Show 10 errors that SSC catches but Paratext misses. If you can't, the project has no justification.

## Should This Be a Paratext Plugin?

**YES, UNEQUIVOCALLY YES.**

Paratext has 3,000 active projects and Plugin API 2.0. Translation Core has hundreds of projects. Your standalone CLI has zero users.

**Immediate prototype (1-2 weeks):**
1. Build minimal Paratext plugin using Plugin API 2.0 (.NET, well-documented)
2. Read project USFM files
3. Run ONE rule (compression-texture OR proper-noun consistency)
4. Output findings to Paratext Notes system (built-in UI)
5. Test with 1 real translator on 1 real draft

**If this works:** You have distribution channel to thousands of projects. Iterate on rules based on real feedback.

**If this doesn't work:** You learn what's incompatible and can make informed decision about standalone vs. plugin architecture.

**Cost:** 40-80 hours for someone with .NET experience. Far cheaper than building standalone UX from scratch.

---

# 5. Product Vision — Concrete Proposed Path

## First-Week UX

### Activation Flow (First 60 Seconds)

**Scenario:** Translator opens Paratext plugin (or standalone tool) against their NT draft (~6,000 verses, ~100k tokens, 60% complete).

**What they see:**

```
Scripture Sous Chef - Quick Health Check

Analyzing your draft (Kairak New Testament, 6,127 verses)...

✓ Compression analysis complete (12 verses flagged)
✓ Proper noun consistency (3 potential issues)
✓ Source comparison (English ULB) (7 verses flagged)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
TOP PRIORITY (Most likely issues)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

1. Matthew 5:12 - Unusual orthographic pattern (compression score: 4.2σ)
   "berakni ka matʰa kerisitipʰa ne God"
   Similar verses typically score 0.8-1.2σ
   
   [View in Context] [This is Fine] [Real Issue]

2. John 3:16 - "god" found (lowercase), but this verse uses "God" 47 other times
   
   [View in Context] [This is Fine] [Real Issue]

3. Acts 2:38 - Source verse has "baptized" appearing twice, target has it once
   
   [View in Context] [This is Fine] [Real Issue]

... (showing top 10)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[See All 22 Flags] [Settings] [What Do These Mean?]
```

**Key UX Principles:**
- **Immediate value:** Show top 10 most-likely issues in 5 seconds
- **One-click triage:** "This is Fine" vs "Real Issue" buttons
- **Context access:** Jump to verse in Paratext with one click
- **Explain the flags:** "What do these mean?" opens just-in-time help
- **Exit strategy:** "See All 22 Flags" for power users, but most users only need top 10

### What Translator Clicks

**Most common path (80% of users):**
1. Clicks "View in Context" for flag #1
2. Sees verse in Paratext editor with surrounding context
3. Realizes it's a proper noun from source language, clicks "This is Fine"
4. Checks flags #2 and #3
5. Finds #2 is a real typo, fixes it in Paratext, clicks "Real Issue"
6. Closes tool after reviewing top 5 flags (3 minutes elapsed)

**Power user path (20% of users):**
1. Clicks "See All 22 Flags"
2. Batch labels with keyboard shortcuts (Y/N/Skip)
3. Finishes all 22 in 5 minutes
4. Clicks "Re-scan" to see updated priorities after fixes

### The Persistence (events.jsonl)

```json
{"timestamp": "2026-05-08T14:32:11Z", "project": "kairak_nt", "verse": "MAT.5.12", "rule": "compression_anomaly", "score": 4.2, "label": "false_positive", "user": "translator_001"}
{"timestamp": "2026-05-08T14:33:45Z", "project": "kairak_nt", "verse": "JHN.3.16", "rule": "proper_noun_case", "token": "god", "label": "true_positive", "user": "translator_001"}
```

**What this enables:**
- Beta posteriors update immediately: next run, similar flags get downweighted
- Cross-project learning: after 10 projects label "compression_anomaly" at 15% precision, threshold auto-adjusts
- Audit trail for consultant reviews

### The Feedback Loop

**When translator clicks "Real Issue":**
1. SSC records label in `events.jsonl`
2. Next scan (30 seconds), Bayesian posterior updates for `(rule=proper_noun_case, cluster=case_mismatch)`
3. Translator sees notification: "Learning from your feedback: proper noun case flags now weighted +20%"
4. After 10 labels for this rule, flags become more precise

**When translator clicks "This is Fine":**
1. Similar logic, but downweights this rule/cluster
2. After 50 cumulative labels, tool suggests: "Compression anomaly has 12% precision for your project. Hide these flags?" [Yes] [No, keep showing]

## First-Month Labelling-Loop Convergence

**Week 1 (10 labels):**
- Translator reviews top 10 flags from initial scan
- Finds 2 real issues (20% precision)
- Clicks "This is Fine" on 8 false positives
- **Engine learns:** Downgrades rules that produced those 8 FPs by 30%

**Week 2 (25 cumulative labels):**
- Translator runs second scan after draft updates (200 new verses translated)
- New flags appear, some rules suppressed due to week 1 feedback
- Precision improves to 28% (7 real issues in top 25 shown)
- **Engine learns:** Identifies which sub-clusters work for this morphological type

**Week 3 (50 cumulative labels):**
- Translator now trusts the tool enough to check it weekly
- Proper-noun consistency rule has 45% precision (best performing)
- Compression-texture has 12% precision (worst performing, auto-hidden)
- **Engine learns:** Beta posteriors converge for high-frequency rules

**Week 4 (75 cumulative labels):**
- Translator invited to label a "wordlist" (100 forms: "Are these real words?")
- Takes 8 minutes, provides dense signal for char_ngram_backoff
- Precision at top-10 now 40% (real issues increase, FPs decrease)
- **Engine learns:** Wordlist downweights 40% of previous FPs

**End of Month 1:**
- 75 binary labels + 100 wordlist labels = sufficient for Bayesian convergence on 3-4 best rules
- Worst rules auto-hidden or deprioritized
- Tool becomes part of weekly workflow (10 minutes per week)

## First-Quarter Milestone (3 Months)

**Month 2-3 (150-200 cumulative labels):**
- Translator completes draft, enters consultant checking phase
- Consultant uses SSC, adds 50 labels with expert judgment
- Bayesian priors now calibrated across translator + consultant perspectives
- Tool identifies 15-20 real issues in final draft that Paratext missed

**Deliverables by End of Quarter:**
1. **Proven precision:** Top-10 flags at 40-50% precision on this project
2. **Rule ranking:** Clear evidence of which rules work for this language typology
3. **Consultant validation:** External expert confirms tool caught real issues
4. **Export capability:** Findings exportable to Paratext Notes for DBL submission

**Success Criteria:**
- Translator says "I would use this on my next project" (retention)
- Consultant says "This flagged 2-3 issues I missed" (value-add vs. existing checks)
- Precision at top-20 ≥35% (usable threshold)

## 6-Month Bet (Realistic Outcome)

**Optimistic outcome (40% probability):**
- 3-5 projects adopted
- Average 100 labels per project
- Proven precision 35-45% at top-20 flags
- 2-3 rule types work reliably across projects
- Paratext plugin in beta
- Pipeline: 10 interested projects waiting for refined version

**Pessimistic outcome (40% probability):**
- 1-2 projects attempted
- Both abandoned after initial trial due to FP rate or lack of integration
- No clear evidence of value vs. Paratext built-in checks
- Project enters maintenance mode

**Most likely outcome (20% probability):**
- 2-3 projects partially adopt
- Translator engagement is sporadic (monthly, not weekly)
- Tool catches 5-8 real issues per NT but also produces 50-80 FPs
- Precision marginal (25-35%)
- No viral adoption, but tool is "good enough" for projects with specific needs (offline, privacy-focused, agglutinative languages where spell-check fails)

**What determines which outcome:**
1. **First-week precision:** If initial top-10 is <20% precision, translators won't return
2. **Workflow integration:** If Paratext plugin ships by month 3, adoption increases 5x
3. **Demonstrable unique value:** If you can't show errors Paratext misses, translators stick with Paratext

---

# 6. Architectural Recommendation

## 2.7: Is the Chassis Right?

**From-scratch reimplementation analysis:**

If a software engineer with zero knowledge of SSC was handed the brief "build unsupervised QA for Bible translation with one NT, no labels," they would likely build:

### What They'd Keep:

1. **Rule-based architecture:** With zero labels, rules are the only option. ✓ Correct choice.

2. **Length-bucketing for cohorting:** Comparing verses/tokens within length quintiles is sound for robust z-scores. ✓ Keep.

3. **Compression-based anomaly detection:** Character-level compression for orthographic anomalies is defensible at this data scale. ✓ Keep with refinements.

4. **Multi-provenance surfacing:** Knowing whether a flag came from `VerseAnomaly` vs `IndependentFlag` helps translator triage. ✓ Keep.

### What They'd Change:

1. **Bayesian posterior chassis → Simpler priority scoring**
   - For 0-50 labels, Beta posteriors are overkill. Just track: `precision = true_positives / (true_positives + false_positives)` per rule.
   - Sort rules by precision. Hide rules with precision <15% after 20 labels.
   - **Verdict:** Bayesian chassis is **over-engineered for launch**, but **right for year 2**. Keep the code, but don't expose sub-cluster routing until you have 200+ labels.

2. **Noisy-OR aggregation → Max or sum**
   - Independence assumption breaks for correlated factors. Noisy-OR overcomplicates.
   - Use `max(char_anomaly, char_ngram_backoff, source_co_rarity)` as interim. Switch to logistic regression at 50+ labels.
   - **Verdict:** **Cut Noisy-OR** until you have labeled data to train weights.

3. **Kneser-Ney vs Laplace → Use KN immediately**
   - Laplace is empirically wrong at 150k tokens. You have KN code in `analysis/kn.rs` but don't use it.
   - **Verdict:** **Swap in Kneser-Ney before launch.** This is not optional.

4. **Edit distance for transliteration → Add phonetic encoding**
   - BK-tree on raw strings misses phonemic variants.
   - **Verdict:** **Add Double Metaphone** pre-processing. 2 days work, significant precision gain.

5. **Standalone CLI → Paratext plugin**
   - Zero translators use standalone CLIs. 3,000 use Paratext.
   - **Verdict:** **Architectural shift required.** Paratext plugin is the right surface.

### What They'd Add:

1. **Wordlist ingestion** - Fast label collection (100 words in 8 minutes). Missing currently.

2. **Serval alignment integration** - Consume existing word alignment instead of reinventing with edit distance.

3. **eBible prior calibration** - Compute per-script-family priors from vetted translations. This is high-leverage and missing.

### What They'd Cut:

1. **Sub-cluster routing before 200 labels** - Premature. Most rules use flat `cluster_key = rule_id` anyway.

2. **0.0/0.3/0.7 source_co_rarity placeholders** - Theater. Either compute real values from eBible or drop the feature until you have alignment data.

3. **Compression dictionary training** - Project-wide zstd dictionary is clever but may overfit. Consider using fixed dictionary from eBible corpus instead.

## What Evidence Would Change My Mind

**Evidence that Bayesian chassis is right-sized:**
- You demonstrate that sub-cluster routing (e.g., `cluster_key = (rule_id, morphological_type, token_length_bucket)`) improves precision by >10% compared to flat `cluster_key = rule_id` on 5+ projects with 100+ labels each.
- This would prove the granularity is valuable. Until then, it's speculative.

**Evidence that current architecture is wrong:**
- Precision at top-20 is <15% on 3 real projects despite 50+ labels, suggesting rules themselves are fundamentally weak.
- Translators consistently say "Paratext already caught everything SSC flagged," suggesting complete redundancy.

## Recommendation: Keep Chassis, Simplify Launch

**Phase 0 (launch):**
- Rules emit scores
- Simple ranking: `final_score = rule_weight * rule_score`
- Rule weights default to 1.0, manually adjust based on first 3 projects

**Phase 1 (50+ labels):**
- Track precision per rule
- Auto-adjust weights: `rule_weight = precision / mean_precision_all_rules`
- Hide rules with precision <15%

**Phase 2 (200+ labels, Year 2):**
- Activate Bayesian sub-cluster routing
- `PriorTable` keyed on `(rule_id, morphological_type)` or `(rule_id, script_family)`
- This is when the chassis pays off

**Verdict:** The Bayesian chassis is **architecturally sound but launch-premature.** Keep the code, but don't expose its complexity until you have evidence it matters. Launch with simple precision-based rule weighting.

---

# 7. The One Thing

**Single change to maximize odds of being useful within 6 months:**

## Build a Paratext plugin that runs ONE high-precision rule and integrates with existing Notes system.

**Why this is the one thing:**

1. **Distribution:** 3,000 projects use Paratext. Zero use standalone CLIs. You need users.

2. **Workflow integration:** Translators work in Paratext 8 hours/day. Asking them to export, run CLI, re-import breaks flow. Plugin lives where they already work.

3. **Feedback loop:** Paratext's Notes system is familiar. Translator clicks "Resolved" = natural label collection.

4. **Validation forcing function:** Building a plugin forces you to pick your ONE best rule. You'll discover quickly if it works. If one rule doesn't provide value, ten rules won't either.

5. **Proof before scale:** Proves the concept with minimal investment before building full chassis.

**The ONE rule to implement first:**

**Proper-noun consistency check** (flags "david" when corpus has ≥3 "David" mid-flow observations).

**Why this rule:**
- Linguistically defensible
- Low false positive rate (if implemented correctly)
- Catches errors Paratext's spell-check misses
- Requires no external resources (works on single NT)
- Clear user mental model ("This tool checks capitalization consistency")

**3-month roadmap:**

**Month 1:** Build plugin, implement proper-noun rule, test with 1 translator
**Month 2:** Add compression-texture if proper-noun works, test with 3 translators
**Month 3:** Add label collection via Notes integration, gather 50 labels per project

**Success metric:** After 3 months, 5 projects using plugin monthly, catching average 3 real issues per NT.

**If this succeeds:** Expand rules, add Bayesian calibration, iterate based on real feedback.

**If this fails:** Learn why (FP rate? Redundant with Paratext? Wrong rule?) and pivot or kill project.

---

# 8. Risks and Unknowns

## Critical Risks (Project-Threatening)

### Risk 1: Complete redundancy with Paratext + AQuA
**Likelihood:** 60%
**Impact:** Project has no users
**Watchfor:** First 3 translators all say "Paratext already caught these"
**Mitigation:** Before writing more code, manually run Paratext's 11 checks on 2 real NTs, document what it misses, verify SSC catches those errors

### Risk 2: False positive rate intolerable
**Likelihood:** 70% (without calibration)
**Impact:** Translators try once, never return
**Watchfor:** Precision at top-20 <20% on first 3 projects
**Mitigation:** Focus on ONE high-precision rule (proper-noun consistency) before adding more. Better to catch 5 errors with 50% precision than 50 errors with 10% precision.

### Risk 3: Workflow integration failure
**Likelihood:** 90% (for standalone CLI), 30% (for plugin)
**Impact:** Translators won't adopt
**Watchfor:** "I'd use this if it was in Paratext" feedback
**Mitigation:** Build Paratext plugin immediately. If plugin API has limitations, document them and reassess.

## Technical Unknowns (Require Research)

### Unknown 1: Morphological typology transfer
**Question:** Do calibration thresholds from eBible Latin-script NTs transfer to Cyrillic-script NTs? Agglutinative vs isolating languages?
**Why it matters:** Determines feasibility of eBible-derived priors for cold-start
**How to resolve:** Compute compression-texture scores across 50 vetted eBible translations, stratify by script family and morphological type, measure variance within/between strata. If within-stratum variance <50% of between-stratum, priors are viable.

### Unknown 2: Park et al. misread extent
**Question:** Are other citations in the codebase also misread? What's the basis for other design decisions?
**Why it matters:** Determines trustworthiness of documented rationale
**How to resolve:** Audit all citations in ADRs and documentation, verify claims against actual papers

### Unknown 3: Paratext plugin API limitations
**Question:** Can Plugin API 2.0 read project text, display custom windows, write to Notes system? Are there permission/licensing barriers?
**Why it matters:** Determines feasibility of primary distribution strategy
**How to resolve:** Build minimal plugin prototype in 2 days, test against demo Paratext project, document blockers

### Unknown 4: Compression-texture actual precision
**Question:** What's actual precision of compression-texture on real drafts? Is it 40% (useful) or 5% (useless)?
**Why it matters:** Determines whether core rule works or is fundamentally flawed
**How to resolve:** Run compression-texture on 2 real NT drafts where errors are known (from consultant checking reports), measure precision/recall

## Follow-Up Research Pass Requirements

**If project continues post-6-months, next research pass should:**

1. **Empirical validation on vetted corpora**
   - Run all rules on 100 eBible translations with known-good status
   - Compute precision assuming flags on vetted = false positives
   - Calibrate thresholds to achieve <5% flag rate on vetted

2. **Morphological typology stratification**
   - Compute type-token ratios, agglutination indices, script families for eBible corpus
   - Test whether calibration transfers within strata
   - Build `profile.yaml` auto-population from corpus analysis

3. **Alignment data integration feasibility**
   - Prototype USFM 3 alignment ingestion from Translation Core
   - Prototype Serval API word alignment consumption
   - Measure precision improvement vs. edit-distance baseline

4. **Comparative evaluation vs. Paratext**
   - Document all Paratext checks with examples
   - Identify 10 error types Paratext misses
   - Demonstrate SSC catches 5+ of those types

5. **User study with real translators**
   - 5 translators, 1 draft each, 2-week trial
   - Measure: precision at top-20, time to label 50 items, willingness to continue using
   - Record feedback on what's useful vs. noise

**Cost estimate:** 80-120 hours of research time

---

## Final Verdict

**Current state:** Technically sophisticated, statistically questionable in places, unvalidated with users, unclear value proposition vs. existing tools, no distribution strategy.

**Viability:** 30-40% chance of achieving 5+ active projects in 6 months IF you immediately pivot to Paratext plugin with one high-precision rule.

**Core issue:** You're building a complete system (rules + Bayesian chassis + multi-provenance attribution) without proving the underlying rules catch errors that matter to translators. This is engineering-driven, not user-driven.

**Path forward:** Shrink scope radically. One rule, one plugin, one translator, one month. Prove value. Then scale.

**Honest answer to "Should I continue?"** Not in current direction. Pivot to Paratext plugin with single rule, or conduct 2-week validation study proving SSC catches errors Paratext misses. Without either, project is speculative.

#### My response:
You said critical lmitation: 

here:

Here is the output.   Revise your report and then we'll talk. 

Second, to answer all of this that you're doing about paratext and translation core, to use a programming analogy, it's the monkey and banana problem. There's two things. Working for Wycliffe Associates, Wycliffe Associates is not a part of the Global Bible Translation Consortium due to our approach protection probable translation, which has a different philosophy around checking. You can research it as an LLM, but essentially we move a little bit quicker than some things and it's just kind of a difference with respect to not accuracy but translation philosophy of checking. Translation core and paratext, as far as I understand it. Unfortunately, building composable software is really hard, and that's what I'm trying to do here is build something that's pretty independent and composable, to which you could hand it a project and get something back pretty useful and is quite independent. Unfortunately, this is not a FANG company and so building things that are library like is hard because it's embedded with assumptions of shape of data and it's embedded with assumptions of it's built into translation core. We don't control the development of those products it's sometimes hard to configure for things like we use a different git backend they use github we use git t or they're expecting they build in rate limiting or they build in organizational assumptions into the tools themselves such as you have to sign up for a paratext account. Well like all of a sudden you've built in organizational needs outside of your tool and it reduces the reusability. So when you're talking about should it be a paratext plugin and whatnot, like yes, I asked about all these other tools, and to a degree they're open source, but my read, and I could be wrong, you can prove me wrong. My read is part of what I'm trying to do is something that's composable and independent and has very few to no organizational assumptions built in. The Other organizations, there's probably some level of composability that said SIL's NLP repo, maybe machine both assume a dependency on clear ML, which is an infrastructural dependency entanglement code. Which is the thing about clear ML is there's no way not to do that. But like, you know, it's that kind of thing is why we're talking about the ecosystem.



My concern is what you're talking about in terms of activation flow. I need you to drop paratext plug-in idea and think much more agnostically. I know I asked you to think in terms of other people, but I meant in terms of what they can do not integrating their ecosystems wholesale. I tend to feel like I get that there's a lot of statistics and probability built into this that I should probably focus on my hunches I should have we should focus first on first things that are almost always going to be high signal or should always be worth flagging, mixed scripts, duplicated words with respect to like some toggles of case sensitivity, punctuation, separating. Even that's a probabilistic thing, right? You can look at a corpus and say, well here's a set of words that are always repeated, but this set of words is not.  But that's much more well scoped, right?



And so things like proper noun suspicion and consistency is pretty easy to explain. Mixed casing. Punctuation that's only left or right clinging that's all of a sudden shows up intermedially etc; Now that doesn't really get you back into this Bayesian feedback of trying to find signal in the long tail of orthography and morphology.  The boolean checks are really useful and not too terribly complicated, And it's maybe why I've underdeveloped them here, but what I've been trying to envision is like what kind of data would actually be the most useful for figuring out signal admits to long tail.  It sounds like you lean compression ratio (And what are your thoughts on pre-computing the average compression ratio against known good Bibles, English, Spanish, French, things that we call gateway languages from which projects are typically translated for example) And maybe we should pre-compute the compression ratio of all of the e bible corpus and say some of them will be wrong, but more of them will be right than wrong and it should get you a rough starting point for something like compression ratio.   But yeah, is alignment data the most useful? Would a word list be the most useful? Like what would actually push you closest towards saying the fastest payoff for rules? Is it a word list? Is it a grammar? Is it a licitation? Is it a morphology?


##### It's 1 round response
Got it — I have the report, your response, and the codebase. Let me work through what needs to change.

Your core corrections:

1. **Drop Paratext plugin** — composability over ecosystem integration is a deliberate architectural choice, not an oversight
2. **The "solves a problem that doesn't exist" framing is wrong** — Wycliffe Associates' checking philosophy differs from GBTC orgs, so Paratext isn't the baseline
3. **Focus boolean/high-signal checks first** — mixed scripts, duplicate words, proper noun consistency, intermedial punctuation
4. **Compression ratio as long-tail signal** — including the eBible pre-computation idea
5. **The real open question** — what data resource gives fastest payoff for signal in the long tail (word list, grammar, elicitation, morphology, alignment)

Let me revise:Here is the revised report, with the paratext plugin recommendation removed, the composability philosophy honored, and the strategic focus reoriented around what you actually asked:The file creation tools are having trouble in this session. Let me deliver the report directly.

---

# Scripture Sous Chef: Revised Assessment

## Preamble — What Changed and Why

The previous report had a critical framing error: it evaluated SSC against Paratext/SIL/unfoldingWord as if integration were the goal. You corrected this. SSC is deliberately composable and organizationally independent — no Paratext accounts, no GitHub assumptions, no ClearML entanglement. The prior report misread this as a gap. It is a design choice, and the right one for your context.

Wycliffe Associates operates outside the GBTC checking philosophy. Building on GBTC tooling embeds the wrong assumptions about checking speed, organizational sign-on, and data shape. The monkey-and-banana framing is accurate: you can't cleanly import a banana from a system that's baked the monkey in.

This report drops the plugin angle entirely and focuses on: (1) what the existing signal stack actually delivers, (2) what the fastest path to real signal in the long tail is, and (3) how to sequence the work.

---

## 1. The Two-Layer Architecture Is Correct

**Layer 1: Boolean/deterministic checks** — Almost always worth flagging. No threshold to calibrate, no corpus to pre-compute, no labels to collect.

**Layer 2: Probabilistic/distributional checks** — Require a corpus to have meaning. Compression-texture, rare-word triage, source proportionality. Harder to calibrate, harder to explain, but reach error types Layer 1 cannot.

Your instinct to focus on Layer 1 first is sound. The issue is that Layer 1 is underdeveloped in the codebase relative to the chassis built for Layer 2.

---

## 2. Layer 1: Implemented vs. Missing

### Implemented

| Rule                    | File                                    | Est. Precision | Notes                                          |
| ----------------------- | --------------------------------------- | -------------- | ---------------------------------------------- |
| Tab in body             | `hygiene.rs`                            | ~95%           | Deterministic                                  |
| Control characters      | `hygiene.rs`                            | ~95%           | Deterministic                                  |
| Zero-width misuse       | `hygiene.rs`                            | ~90%           | Script-conditional, sound                      |
| Empty verse             | `hygiene.rs`                            | ~90%           | Deterministic                                  |
| Paired punct balance    | `punctuation.rs` + `discourse.rs`       | ~85%           | Well-implemented with span index               |
| Proper noun consistency | `proper_noun_consistency.rs`            | ~60-70%        | After Dunning threshold                        |
| Sentence-start case     | `positional/sentence_start_case.rs`     | ~65-75%        | Learns triggers per corpus                     |
| Unexpected sentence end | `positional/unexpected_sentence_end.rs` | ~60-70%        | Corpus-learned, sound                          |
| Source proportionality  | `source_relative.rs`                    | ~55-65%        | Requires source; z-threshold needs calibration |

### Not Yet Implemented (High Priority)

**Mixed script detection** — `script.rs` exists and `script_of()` works, but no rule flags mid-word or mid-verse script transitions that shouldn't be there. If a verse is 97% Latin-script but contains three Cyrillic characters, that is almost certainly an error. Near-100% precision, no corpus needed.

**Duplicate word run detection** — "and the the man" is a real copy-paste artifact. Simple n-gram check over the token stream. Needs a case-sensitivity toggle and a corpus-derived skip-list for words that legitimately repeat (e.g. "very very" in some conventions).

**Intermedial punctuation** — Punctuation that only appears left-clinging or right-clinging in the corpus but shows up medially. `punctuation_class.rs` has `ClingingClass` fully implemented. The rule to detect violation of corpus-learned clinging convention is not written.

**Case convention after interior punctuation** — Related to `sentence_start_case` but scoped to colons, semicolons, em-dashes where the corpus has a learned convention. The learned-triggers infrastructure in `positional/shared.rs` already supports this; it needs a rule wrapper.

### Build Order

1. Mixed script detection — 1 day, near-100% precision
2. Duplicate word run detection — 2 days
3. Intermedial punctuation — 2 days, uses existing `ClingingClass`
4. Case after interior punctuation — 1 day, reuses learned-triggers

---

## 3. The Long-Tail Signal Question

You asked directly: **what data resource gives the fastest payoff for signal in the long tail of orthography and morphology?**

### Tier 1: Fastest Payoff

**Word list (binary: is this a real word?)**

The current `rare_words.rs` pipeline combines `char_anomaly`, `char_ngram_backoff`, and `source_co_rarity` via Noisy-OR. All three are proxies for one question: "is this a real word in this language?" A word list — even a partial one — short-circuits all three proxies.

With 200-500 confirmed forms (derived from a completed portion of the draft or provided by the translator), you split the rare-word queue into: known-good → suppress, known-bad → surface at high confidence, remainder → current probabilistic pipeline.

`LabelledLemmaIndex` in `lemma_feedback.rs` is already built for this. The missing piece is a fast elicitation surface: present 100 forms sorted by frequency, ask "real word? yes/no/skip", persist via `LemmaFeedbackEvent`. 8-10 minutes of translator time, dense signal.

**Compression ratio pre-computation against eBible**

Your intuition is correct. The current `CompressionTextureModel` trains a zstd dictionary on the project's own corpus. This cold-starts poorly for short drafts and has no prior on what "normal" looks like for a given script family.

Pre-computing across the eBible corpus gives a prior distribution:
- For each eBible NT, compute the median verse-level compression ratio under a script-family-specific dictionary
- Cluster by script family (Latin, Cyrillic, Ethiopic, Devanagari, etc.) — not language family, because the compression signal is orthographic
- Use 10th/90th percentiles as soft bounds on "normal"
- When a new project falls outside those bounds, the anomaly threshold adjusts

Some eBible translations will have errors. More will be right than wrong. The aggregate prior is useful with noise. `profile_ebible.rs` already processes the corpus; adding verse-level compression scoring is straightforward.

**Cost:** 3-4 engineering days. High leverage.

### Tier 2: Medium Payoff

**Alignment data** — High payoff if alignment exists, medium if you need to generate it. `source_co_rarity.rs` is already a weak proxy for this. Real alignment data would make it precise, but generating it from scratch adds infrastructure. Not the fastest path.

**Morphological segmentation** — The `morphology.rs` + `candidate_families.rs` stack is well-built. The payoff is specifically for agglutinative languages where a word list breaks down (Swahili, Quechua, etc.). For those languages, "is this an unusual combination of otherwise-valid morphemes?" is the right question. For analytic/fusional languages, the word list wins.

### Tier 3: Slowest Payoff

**Grammar / elicitation data** — Producing a grammar description or elicited paradigm requires a linguist's time, not a translator's time. Language-specific, non-transferable. Lowest payoff per unit of effort. Use it if you have it; don't generate it for this purpose.

---

## 4. Sequencing

**Month 1: Complete Layer 1**
- Mixed script, duplicate word run, intermedial punctuation, interior-punct case (6-8 days total)
- Fast word-list elicitation flow (3 days)
- Run on 2 real NT drafts
- Target: top-20 Layer 1 flags at ≥50% precision

**Month 2: eBible Prior Calibration**
- Add verse-level compression scoring to `profile_ebible.rs`
- Derive per-script-family baselines
- Embed as static priors in `CompressionTextureModel`
- Target: compression-texture precision from ~20-30% to ~35-45%

**Month 3: Word List Signal Loop**
- Ship elicitation interface
- Collect 200-300 labels across 2-3 projects
- Validate known-good suppression reduces rare-word false positives
- If top-20 combined precision reaches ≥40%, the Bayesian chassis starts paying off

---

## 5. Statistical Issues Still Needing Attention

**Laplace vs. Kneser-Ney** — `char_ngrams.rs` uses Laplace-style smoothing. At 150k tokens with a ~100-character vocabulary, this washes out exactly the low-frequency signal you want to preserve. `analysis/kn.rs` exists but is unused. Kneser-Ney continuation probability is the right substrate for character n-gram anomaly detection. This is not optional for production-quality scoring. Estimated fix: 2-3 days.

**Correlated factors in Noisy-OR** — `char_anomaly` (compression score) and `char_ngram_backoff` (char n-gram surprisal) both analyze character patterns. They are correlated. Noisy-OR's independence assumption inflates scores. Until you have 50+ labels for logistic regression, use `max(char_anomaly, char_ngram_backoff)` instead.

**The 0.0/0.3/0.7 placeholders in source_co_rarity** — Not a blocking problem if this factor is given low weight in the overall score until real values are derived. Real values are computable from any source-target NT pair; the work just hasn't been done.

---

## 6. The One Experiment That Matters

**Does Layer 1, fully implemented, catch errors that translators care about that existing informal review misses?**

Run the complete Layer 1 suite on a single completed NT that has been through consultant checking. Every flag the consultant did not catch (and agrees is a real error) is evidence of value. Every flag the consultant already caught is not.

This experiment takes one afternoon. Do it before Month 2.

---

## Appendix: On Composability

Every organizational assumption baked into a dependency — Paratext account, GitHub backend, ClearML tracking, GBTC checking philosophy — is a constraint that reduces portability. The value of a composable tool is that you can hand it a USFM directory and get signal back without negotiating with anyone's infrastructure.

The risk is that this niche is smaller than the effort required to fill it well. But that is a scope question, not a design question. The design is right.

---

Ready to talk through any of this. The compression/eBible pre-computation and the word-list elicitation flow feel like the two highest-leverage items that aren't yet in the codebase — what's your read on feasibility given current constraints?


## My Take
Caveat, I've read both of these, but it's dirty voice STT and so I might stutter and backtrack. Reading top to bottom to capture my thoughts. Okay, so both of them talk about the top 50 triashka between character anomaly, character, and gram back off and source charity. And from a statistic standpoint, I get what they're saying that for noisy OR, which assumes independence, those are very much dependent signals. What I was trying to do with that is to say, well, if we do put a word list in front of people of any sort, is there any value in trying to s use any of this to sort the word list towards the top? Or is it really we should ignore this machinery and just say, here's the word list, click any that you don't recognize, assume everything is good, and if you want to mark a word as not a word, there you go. That was kind of my thought on this, but I do realize that maybe we've built too much machinery for for that. Mathematical theater. Yeah, that's what I was worried about. Their immediate signal versus placeholder theater. Which Jim says we need word level alignment or morphological analyzers. I mean I just don't know if that's true, but I do wonder if like okay, yeah, say they mark 20 labels on is a word or isn't a word, is that even gonna be able to move the posterior at all? Both tend to focus on immediate signal and maybe those things are boring and so I didn't put enough time into them but it's probably what we should have built out first much more cleanly. It's not at all ML. It's not at all probabilistic it's not at all exciting. it is probably where we should have focused.  Hygiene should probably be a little more defined, but should be the highest level of checks. Punctuation, again, I should probably just go through a worked example and do a little more high, higher level of checks. Right now we try to do this probabilistically, which is nice. I think the interplay between convig and probabilistic stuff is maybe a little off. I think it can be probabilistic but like if something's always left clinging or always right clinging and all of a sudden it shows up intermediately, that should be a problem. If for some reason something shows up intermediately all the time. Like can by convention we don't put spaces after our parentheses and sometimes we do, then maybe maybe that's not worth surfacing unless it explicitly gets flagged. that's my thought there. the proper noun consistency thing is probably has a pretty natural UI for it. but again it probably just needs to be spelled out with a worked example from the top would be my guess. Umpression against known good corpora would probably be the first probabilistic signal that is. Both of them seem to think that's the first known good signal in the world of prob probability. the math is a little straightforward, and we could say, well, this plus length, you know, coverage length seems to have to be the first of probabilistic signals for because they're coarse, they're verse level but it would check for oh well there was something in the verse but it's actually vastly under over translated co-rarity using a bk tree yeah. That makes sense. The OR aggregator makes sense. I generally think what Jim is saying regarding his statistical and NLP sound is probably as right. I don't even know about kneezerne smoothing. I am interested on what these call this rolling window median. I'd like to know more on that. Head at distance, yeah. Curious on what phonemic hashing would entail. He didn't specify. Robust Z. I don't know if that's related to edit distance or Z standard. I think they mean Z standard conditioned by morphological type, which we could run against a bunch of no good Bibles and kinda do that might be the first like pre-training thing that actually might have some from stuff. I guess I should go check out Sil Sil Machine. I've critiqued not Sil, but just it's hard to build composable things in the Bible translation world because the ecosystem is not huge. We're not talking about Facebook and Amazon and Meta and people that are building really composable and modular libraries. Like part of SIL's NLP libraries and machine libraries are tied into their ecosystem and make assumptions that are tied into their ecosystem and how they version and control data and stuff of that nature.yeah, and they both of them miss that we don't use paratext because we can't control that product and Wikipedia SCS isn't a part of the global consortium and you have to sign up for accounts with their servers and things of that nature and it just isn't it's not conducive for controlling our own data as needed as a company. So I mean we can't exist as a paratext plugin in that sense and paratext is just far too beefy for the majority of users which we're serving because we don't use consultants in that sense, and linguists in that sense. It's ch much more church-owned. I guess I would need to flesh out mentally with this family panel batching. Both of them seem to suggest that word lists would have a good bit of potential value. so maybe that's the first of the probabilistic things we push into. Some of the hygiene as we've mentioned is probabilistic a little bit, things like duplicative words. Some words some languages allow that as a stylistic feature and then some only do that for certain words. Holy holy in English, for example, is what we would consider, you know, a known case where that would be accepted. And then some toggles for, you know, do you want to be case sensitive or case sensitive? stuff of that nature., I think that's a good thing. Priors. Assume normal anomaly distributions. I don't even know what that means other than he's talking about the compression score maybe. I still don't understand what he means by phonemically aware distance metrics. Integrated USFM Linter's fine, except for it's not USFM Linter, it's content analysis, so it's the wrong product product terms.  Yeah, maybe I should draw an SIL machine and see what it does, but I just don't think I've ever seen alignment data that looks not really crazy. Maybe the better thing is to instead of trying to build our is to leverage alignment and try to build a nicer UI around alignment and maybe that gets the snowball rolling. Maybe that's the research spike is to use sew machine or seed bugger rooms around and then like See what it would be like to create labeled data on that aligning using two languages and note. That's probably where the research spike. I feel like the problem is is the alignment things I'm aware of. I guess we should benchmark. Maybe that's also a spike, as we can benchmark if anything's fast to like a line and rerun, a line and rerun, so it's a pretty quick feedback loop. But my experience with alignment and Python ecosystem things, we'll have to see what Silmachine does and SERVAL because maybe that's the better path to quick data or good data. I think I've mentioned phonemic shift interest. Interesting on empirical cumulative distribution function. I'll need that spelled out like I'm five, please. Rolling windows are required. I would like that spelled out like I'm five, please, of how we could do that, but that sounds good. The general product roadmap from Jim sounds alright though, I think about it. Word list. I don't know what Jim is saying about elicitation. I mean it sounds good. I like the idea of elicitation, but I don't know how that's gonna help us like concretely First level alignment. Yeah, I don't the proportionality rule. We already have a proportionality rule if we're looking for that. It's not alignment, but it's proportionality. I don't think I mean maybe more fesser. I don' It don't know the most we can train Morfessor relatively quickly and we could do it in the background and then it seems like Morfessor runs reasonably quick once it's trained. So like we could version a bin model so every time the corpus updates we up for day morphessor. I think that's gonna have to get punted towards the when I know what to do with morphology. I'm curious what ScriptureFord's doing in fine tanning, no language left behind, but I do think our first like we want something a little more conservative to start with is my guess. engineering first signal, yeah. Probabilistic layers advisor, yeah. Okay. And then Ollie's report.  