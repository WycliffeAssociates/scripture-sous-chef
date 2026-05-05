Statistical and Architectural Stress-Test of a Label-Efficient Error Detection Engine for Bible Translation
The architectural endeavor to construct a statistical engine for error detection in Bible translation within a low-resource environment presents a unique intersection of computational linguistics, Bayesian statistics, and information theory. The following report provides an exhaustive peer review of the proposed engine, evaluating its soundness against the specific constraints of New Testament-scale corpora, the absence of curated linguistic resources, and the requirement for embeddable, offline operation. This review focuses on the current architectural direction as outlined in the evidence layer design and the associated statistical methods, utilizing calibration data from real-world translations to ground the feasibility analysis.
Statistical Soundness in the Sparse-Data Regime
The foundational challenge of the engine is the pervasive nature of the sparse-data problem. Within a typical New Testament corpus of 150,000 to 250,000 tokens, Zipf’s Law dictates that a vast majority of word types appear only a few times. Specifically, in a sample of 500,000 sentences, over 60% of word types occur five or fewer times, and the sparsity is even more pronounced for word pairs, where over 93% exhibit an expected frequency of less than five.1 In the context of a single translation project, which is significantly smaller than common NLP datasets, the engine must operate almost entirely within the "long tail" of the distribution.
Comparative Analysis of Association Measures
The reliance on the Dunning Log-Likelihood Ratio (LLR, or ) for source-target co-occurrence and proportionality checks requires a rigorous critique. LLR was originally introduced to the NLP community as a superior alternative to the Pearson chi-square () test precisely because it handles rare events more gracefully by assuming a binomial or multinomial distribution rather than a normal one.2 However, its reliability in the extreme small-sample regime is controversial.
Empirical studies have demonstrated that significance values estimated with  for rare bigrams can differ substantially from the exact values computed by Fisher’s Exact Test.1 Specifically, LLR tends to underestimate the amount of noise in bilingual word associations when dealing with singleton-singleton pairs—cases where both the source and target words occur only once in the entire corpus.1 While LLR is a strong predictor of negative log p-values with high correlation, it often suffers from a standard deviation of approximately 0.55, leading to a ratio between predicted and actual p-values that ranges from 0.57 to 1.7.1
Feature
Log-Likelihood Ratio (G2)
Fisher's Exact Test
t-score
Statistical Basis
Asymptotic approximation
Exact combinatorial calculation
Normal distribution assumption
Small Sample Behavior
Tends to overestimate significance for rare pairs
Absolute reliability
Unreliable; assumes normality
Computational Cost
Very low ()
Historically high, now efficient ( with Gamma)
Very low ()
Handling of Rare Events
Good, but biased at 
Optimal
Poor; over-highlights common words
Feasibility for Engine
High
Recommended replacement
Not recommended

1
Given that the engine must be useful from "day zero" when the corpus is at its smallest, replacing LLR with Fisher’s Exact Test for significance arbitration is a justified architectural change. Modern implementations using numerical approximations of the gamma function and recurrence relations have eliminated the historical computational penalty for Fisher’s Test, making it feasible for sample sizes up to  on standard hardware.1
Bayesian Evidence Accumulation and Shrinkage
The proposed evidence layer utilizes Beta-Binomial conjugate updates to calibrate finding clusters. This is a theoretically sound choice for binary labels (accept/dismiss) arriving sequentially. The Beta distribution serves as the conjugate prior for the Binomial likelihood, allowing the posterior distribution of the error probability  to be updated analytically:

where  and  are prior parameters,  is the number of "true positive" labels, and  is the total number of labels for a cluster.5
However, a simple independent Beta-Binomial model per cluster will fail to reach calibration for rare clusters. The engine should instead move toward a Bayesian Hierarchical Model (BHM) that utilizes partial pooling.6 In this framework, the  parameters are shared across all clusters within a rule, allowing the engine to "borrow strength" from the global performance of the rule to stabilize estimates for sparse clusters.7
This mechanism is essentially a form of James-Stein shrinkage. In the small-sample regime, the James-Stein estimator proves that shrinking individual-level estimates toward a global mean results in a lower total mean squared error than using independent estimates.8 For the translation engine, this means that a finding for a rare character pair should be "shrunk" toward the average precision of that punctuation rule until enough local project labels accumulate to justify a deviation.
Model Approach
Treatment of Sparsity
Error Variance
Complexity
No Pooling (Independent)
High variability; unreliable for rare cases
High
Low
Complete Pooling (Global)
Ignores cluster-specific patterns
Low
Low
Partial Pooling (Hierarchical)
Optimal; uses group data to stabilize sparse points
Minimal (MSE optimal)
Moderate

5
To implement this effectively without hand-curated priors, the engine should use Empirical Bayes.5 By utilizing the calibration profiles of the ~1000 eBible translations, the engine can fit the global  hyperparameters for each rule based on typical cross-project error distributions. This ensures the "day zero" experience is grounded in real-world data while allowing for local adaptation.
Aggregation Logic and Parameter Fitting
The current aggregator is a weighted sum of per-rule evidence. While simple to implement, the weighted sum lacks a formal probabilistic basis, making it difficult to integrate with the Bayesian evidence layer. A more sound approach is a Noisy-OR model or a Log-Odds aggregation scheme.
In the "many independent weak signals" theory, the Noisy-OR model assumes that an error finding can be triggered by any of the  independent rules. The probability of an error existing, given a set of signals , is:

where  is the evidence score and  is the calibrated precision of the rule. This is particularly resilient to the "signal saturation" problem where adding more rules might otherwise lead to an explosion of false positives.10
The Parameter-Count Problem and Identifiability
The engine's total parameter count—per-rule weights, per-cluster posteriors, and pair multipliers—must be weighed against the available data. With fewer than 500 explicit labels expected in the first year, the framework is at high risk of over-fitting or being non-identifiable.
Parameter Category
Estimated Count
Learning Source
Per-Rule Weights
~50
Global calibration sweep
Rule Hyperparameters
~100
Empirical Bayes (Global)
Cluster Posteriors
2 per active cluster
Online local labels
Pair Multipliers
~20-30
Correlation sweep (Global)

The responsible approach at this scale is to keep the number of parameters learned locally (per project) extremely small. By leaning on the global calibration profiles for weights and initial priors, the engine only needs to fit the cluster-specific offsets locally. This "low-parameter budget" is the only path that survives the reality of label scarcity in individual projects.
Corpus-Scale Feasibility and Agglutinative Morphology
The high hapax rate (70%) in agglutinative languages like Bemba and Rai represents the primary "feasibility wall" for the engine. Standard bigram and trigram counts become so sparse that "rarity" loses all meaning. The current proposal to use lemma-cluster induction is necessary, but the choice of algorithm is critical.
Unsupervised Morphological Segmentation (MIASEG)
The engine should prioritize Meaning-Informed Low-Resource Segmentation (MIASEG) over traditional unsupervised methods like Morfessor 2.0.12 Morfessor 2.0 relies on bare surface forms and often struggles with agglutinative structures, whereas MIASEG is inspired by language acquisition patterns, stripping suffixes and prefixes based on paradigms of words that share a core meaning.13
MIASEG’s effectiveness in low-resource settings is significant. Trained on as few as 500 to 1,000 words, it has achieved near-perfect (0.99-1.00) accuracy in Mongolian and high scores (0.96) in Turkish.13 By applying MIASEG as a pre-processing step, the engine can collapse surface forms into their constituent roots and affixes. This transformation dramatically reduces the effective type count and lowers the hapax fraction, allowing co-occurrence and n-gram rules to operate on morphemes rather than tokens.
Language
Training Size
MIASEG Performance
Morfessor Performance
Mongolian
500 words
0.99 Accuracy
Low
Turkish
10,000 words
0.96 Accuracy
0.54 Accuracy
Finnish
10,000 words
0.84 Accuracy
Moderate

13
This morphological layer provides the most plausible solution to the hapax-suspicion problem. Without it, the engine’s rules for "rare token" detection will be essentially unusable in over 50% of sub-Saharan African and Oceanic languages.
Character-Level Anomaly Detection: Kneser-Ney vs. NCD
The character-level Kneser-Ney (KN) perplexity signal is useful for identifying orthographic outliers. However, KN smoothing requires training a language model, even if the training is done online. An alternative that requires zero training and is highly robust in small samples is Normalized Compression Distance (NCD).
NCD uses a parameter-free approach based on Kolmogorov complexity to measure the similarity between two text sequences.14 It identifies anomalies by measuring the compressed size of a target sequence concatenated with a known "clean" sequence versus their individual compressed sizes. NCD has been successfully applied to HTTP anomaly identification, spam detection, and Domain Generation Algorithm (DGA) tracking.14
Metric
Mechanism
Training Required
Robustness
Kneser-Ney Perplexity
n-gram probability with smoothing
Yes (online or batch)
High, but sensitive to small context
NCD (Compression)
Information-theoretic similarity
No
Extreme; handles noise well
Substring Novelty
Checks for unseen character sequences
No
High false-positive rate

14
Given the "day zero" constraint, NCD provides a more resilient orthographic anomaly detector than KN perplexity, as it effectively captures structural patterns without needing to explicitly estimate probability mass for unseen n-grams.
Architecture and Evidence Flow
The evidence_layer_design.md proposal for per-rule cluster keys with a shared posterior store is a clean factoring. However, it requires a clear strategy for "universal" versus "project-specific" clusters.
Script-Universal Prior Pooling
The assumption that punctuation conventions are script-universal holds for many scripts (Latin, Cyrillic) but breaks down in others. Thai script, for example, does not use spaces to separate words, and Devanagari uses a danda (।) instead of a full stop.17 Universal clusters should therefore be keyed by both rule_id and script_id (using ISO 15924 codes). This prevents Western punctuation priors from polluting the analysis of scripts with different syntactic markers.
Persistence via JSONL Event Log
The use of a JSONL append-only event log is highly appropriate for this scale. With estimated event volumes of ~100 per day, a project would generate only ~36,500 events per year. Modern storage systems and even simple sequential scanners can process these volumes in milliseconds.19 This format provides a robust audit trail and simplifies the integration of git-history backfills, as each commit or user action can be modeled as a discrete event in the log.
Label Sourcing and Data Plumbing
The strategy to extract implicit labels from git history is a significant innovation, though it requires careful filtering to avoid noisy data.
Git History Mining and Edit Attribution
Mining git diffs for form-level corrections has precedent in software change prediction, where "bug fix" commits are identified by keyword matching in commit messages and the nature of the code changes.20 For translation, a "form-level correction" can be defined as an edit with a low Damerau-Levenshtein distance (1–2) that does not significantly change the word count of a verse.21
Pitfalls include "stylistic churn"—where a translator rephrases a sentence without correcting an error—and USFM marker refactors. The engine must utilize a parser to strip USFM markers before calculating diffs to ensure that only textual changes are analyzed.18
Edit Category
Git Signature
Label Interpretation
Correction
DL distance 1-2, low count of added/deleted chars
High-confidence "Accept" for finding
Refactor
High DL distance, similar meaning
Noise; discard as label
Marker Update
Changes limited to USFM backslash markers
Filtered out before processing
Reversion
Content restored to a version 2-3 commits prior
Strong "Dismiss" for whatever prompted the change

20
The "edit-near-span" attribution logic should be viewed through the lens of weak supervision. If an analyzer flags a span and the user subsequently edits that span, the confidence in the "Accept" label should be proportional to the temporal and spatial proximity of the edit. This type of signal is common in search relevance (click-feedback) and can be used to incrementally update the Bayesian layer without explicit user interaction.
Interface and Workflow Integration
The decision to avoid a custom annotation UI early is pragmatic but places a high burden on the quality of labels sourced from the CLI and git. The diagnostic format (a list of findings with evidence scores) should be designed to encourage high-fidelity feedback.
Translator Workflow Tools (Paratext and Bloom)
Paratext is the primary environment for most Bible translators, featuring extensive collaboration tools and automated "basic checks".17 Integration with Paratext via the Plugin API 2.0 is the most likely path for the engine to reach active users.17 A Paratext plugin could consume the JSONL event log and present findings in the native "Translation Validation" window, allowing for explicit "Accept" or "Dismiss" actions that are then logged back to the engine.
Adjacent tools like "Bloom" (for literacy materials) and "Translation Studio" are also used in these contexts, but Paratext’s centralized wordlist and morphology tools make it the best candidate for early integration.17
Risks and Mitigation Strategies
Several risks have not been fully characterized in the current project notes.
Organizational Privacy: Even "anonymized" cross-project pooling can be re-identifying in a small organization. If a rare word is associated with a specific project, its presence in the global prior store could leak information about the translation's content. Mitigation: Hash all cluster keys using a salt that is discarded after the local project is finalized, or only share aggregate  parameters rather than the keys themselves.
Evaluative Deadlock: Without a "gold standard" held-out dataset, it is impossible to calculate a true F1 score. The engine risks optimizing for "translator agreement" rather than "translation accuracy." Mitigation: Periodically inject "synthetic" errors (e.g., character transpositions, bracket deletions) to measure the engine’s recall and the user’s diligence.
Agglutinative False Positives: In languages with high productivity, a single valid root can generate thousands of valid surface forms. If the morphological layer is not robust, the "rare token" rules will produce a deluge of false positives. Mitigation: Implement a strict "suppression threshold" where rules are disabled if their local precision falls below a baseline (e.g., 5%) after the first 100 verses.
Conclusion and Strategic Recommendations
The author’s central bet—that a label-efficient Bayesian aggregator can achieve useful precision-recall through many weak signals and cross-project priors—is sound and defensible. It aligns with modern research in Bayesian hierarchical modeling for small-area estimation and leverages the specific information-theoretic properties of text.7
Recommended Roadmap Adjustments
Implement Fisher's Exact Test immediately: The small expected frequencies for singleton word pairs make LLR an unreliable arbiter of significance.1
Prioritize MIASEG for Stem Induction: The "hapax wall" is the most significant technical risk. Morphological segmentation is the only credible way to restore the signal-to-noise ratio in agglutinative contexts.13
Transition to Noisy-OR Aggregation: This provides a formal probabilistic model for combining multiple findings per verse, which is more robust than a weighted sum.11
Deploy Empirical Bayes Global Priors: Use the existing 1000 calibration profiles to pre-calculate  hyperparameters. This ensures the engine is "smart" on day zero without requiring any project-specific labels.5
Standardize on Paratext Plugin API 2.0: To solve the "chicken-and-egg" label problem, the engine must move from a CLI tool to a background process that integrates with the translator’s existing workbench.17
The realistic ceiling for the engine in year 1 is the high-precision detection of form-level errors (punctuation, orthography, casing). By year 5, with the accumulation of thousands of implicit and explicit labels across diverse language families, the engine could mature into a tool capable of detecting semantic and consistency anomalies that are currently only caught by expert human consultants. The path chosen is a principled rejection of "big data" hubris in favor of robust, information-theoretic grounding.
Works cited
On Log-Likelihood-Ratios and the Significance of ... - ACL Anthology, accessed May 4, 2026, https://aclanthology.org/W04-3243.pdf
Extending the Cochran rule for the comparison of word frequencies between corpora - SciSpace, accessed May 4, 2026, https://scispace.com/pdf/extending-the-cochran-rule-for-the-comparison-of-word-6f785uqchy.pdf
On Log-Likelihood-Ratios and the Significance of Rare Events - Microsoft, accessed May 4, 2026, https://www.microsoft.com/en-us/research/wp-content/uploads/2004/07/rare-events-final-rev.pdf
Fishing for Exactness, accessed May 4, 2026, https://www.d.umn.edu/~tpederse/Pubs/scsug96.pdf
Bayesian Hierarchical Models - Data, Inference, and Decisions, accessed May 4, 2026, https://data102.org/ds-102-book/content/chapters/02/02_hierarchical_models.html
BAYESIAN HIERARCHICAL MODELS, accessed May 4, 2026, https://tesi.luiss.it/43547/1/781201_ROMANO_MARTINA.pdf
(PDF) Bayesian Hierarchical modeling for small-area estimation of disease Burden - ResearchGate, accessed May 4, 2026, https://www.researchgate.net/publication/392901661_Bayesian_Hierarchical_modeling_for_small-area_estimation_of_disease_Burden
TAU Statistics Seminar Home Page, accessed May 4, 2026, http://www.math.tau.ac.il/~yekutiel/TAU%20Statistics%20Seminar%20Home%20Page%2016-7.htm
arXiv:1705.04293v4 [stat.ML] 14 Jan 2021, accessed May 4, 2026, https://arxiv.org/pdf/1705.04293
Beyond Heuristic Prompting: A Concept-Guided Bayesian Framework for Zero-Shot Image Recognition - arXiv, accessed May 4, 2026, https://arxiv.org/html/2603.07911v1
Temporal Knowledge Graph Hyperedge Forecasting - Diva-portal.org, accessed May 4, 2026, https://www.diva-portal.org/smash/get/diva2:1986666/FULLTEXT01.pdf
Low-Resource Agglutinative Morphology Segmentation | PDF - Scribd, accessed May 4, 2026, https://www.scribd.com/document/881833012/SCiL-2024-Morphological-Segmentation
Belth | Meaning-Informed Low-Resource Segmentation of ..., accessed May 4, 2026, https://openpublishing.library.umass.edu/scil/article/id/2134/
A Compression-Based Method for Detecting Anomalies in Textual ..., accessed May 4, 2026, https://pmc.ncbi.nlm.nih.gov/articles/PMC8156803/
A Compression-Based Method for Detecting Anomalies in Textual Data - ResearchGate, accessed May 4, 2026, https://www.researchgate.net/publication/351628520_A_Compression-Based_Method_for_Detecting_Anomalies_in_Textual_Data
An Enhancement of Jiang, Z., et al.'s Compression-Based Classification Algorithm Applied to News Article Categorization - arXiv, accessed May 4, 2026, https://arxiv.org/html/2502.14444v1
Paratext History - Paratext, accessed May 4, 2026, https://paratext.org/about/history/
Balisage Paper: Scriptural Markup in the Bible Translation Community, accessed May 4, 2026, https://www.balisage.net/Proceedings/vol26/print/Robie01/BalisageVol26-Robie01.html
Git: Ensuring the integrity and authenticity of history - Blog - Darkwood, accessed May 4, 2026, https://blog.darkwood.com/index.php/article/git-ensuring-the-integrity-and-authenticity-of-history
Using Git Commit History for Change Prediction - Diva-portal.org, accessed May 4, 2026, http://www.diva-portal.org/smash/get/diva2:851524/FULLTEXT01.pdf
7.6 Git Tools - Rewriting History, accessed May 4, 2026, https://git-scm.com/book/en/v2/Git-Tools-Rewriting-History
How to Handle Git Tag Management - OneUptime, accessed May 4, 2026, https://oneuptime.com/blog/post/2026-01-24-git-tag-management/view
Advancing Church-Based Bible Translation: Technological Insights from Papua New Guinea Field Visit - ETEN Innovation Lab, accessed May 4, 2026, https://www.etenlab.org/post/advancing-church-based-bible-translation-technological-insights-from-papua-new-guinea-field-visit




## FOLLOW UP CONVESATION I HAD

To implement this effectively without hand-curated priors, the engine should use Empirical Bayes. By utilizing the calibration profiles of the ~1000 eBible translations, the engine can fit the global $(a, b)$ hyperparameters for each rule based on typical cross-project error distributions. This ensures the "day zero" experience is grounded in real-world data while allowing for local adaptation.



This I don't understand what you mean using the calibration profile of the eBible translations because these are all different languages, so I don't understand how you're gonna be able to tune across different languages unless you're telling me to look for commonalities across the different languages.



The responsible approach at this scale is to keep the number of parameters learned locally (per project) extremely small. By leaning on the global calibration profiles for weights and initial priors, the engine only needs to fit the cluster-specific offsets locally. This "low-parameter budget" is the only path that survives the reality of label scarcity in individual projects. -> I don't quite understand this paragraph either and I would appreciate if you could elaborate on it a little more.



The character-level Kneser-Ney (KN) perplexity signal is useful for identifying orthographic outliers. -> I'm entirely open. Again, remember don't take anything I'm asking you as pushback. It's Um questions 'cause I really am out of my league with this stuff. But do you not think there's still some value in knowing the knees or nay distribution on the bigrams and trigrams, even if they are fairly rare? Or if there's a pretty long tail. I guess I don't understand why it requires training a model. I mean all training a model is is what we're talking about in terms of going over um probabilities and statistics. The real question, however, is how many parameters and how much data do you have? So like it's all training a model, it's just in this case our probabilities can be run in milliseconds for a single New Testament, for example.



Git History Mining and Edit Attribution -> Yeah, don't worry about the USFM bits. My I'm wondering if there's just some really lower case I don't know if there's any data that you might could find as an implicit label um based upon similarity when comparing a verse to a verse. If something got decapitalized, for example, in a language that used capitalization, or if you saw transposition of two letters, anything of that nature. But I don't know if that's gonna be doable or not.



Uh I'm not avoiding a custom annotation UI. It's simply that our users primary and goal is to edit their their new testament or old testament. They're not human labeling data for this purpose explicitly. It's probably gonna have to be a byproduct of the workflow of creating a new testament and revising it and reviewing it. I'm just saying I don't most of these languages don't have data where someone's explicitly gone in and says this biogram is not ever allowed or anything like that. That was part of the research proposal was to ask you for suggestions of how you could do this when the primary goal is simply to produce their text or review their text. So it's not that I'm avoiding it, it's that I don't have one and I wanted suggestions for how do you design a UI that very naturally feeds back into these statistical models.



Paratext and Bloom. Yeah, I worked for Wycliffe Associates. We have our own editors. It's not gonna be anything external. That's why I'm talking about UI and data structures that very naturally would feed backwards into this.

The confusion regarding Empirical Bayes and the "parameter budget" is entirely understandable, as these are technical terms for a simple goal: making sure the engine doesn't "hallucinate" an error just because it hasn't seen a word before. Since you are building this for internal Wycliffe editors, you have a unique opportunity to embed these statistical safety rails directly into the translator's workflow.

Here is a breakdown of those specific points and suggestions for the UI/data integration.

1. How Empirical Bayes "Tunes" Across Different Languages
The engine isn't looking for linguistic commonalities (e.g., "how do Swahili and Indonesian use verbs?"). Instead, it is looking for statistical commonalities in how the rules behave.

Think of each of your 50 or 60 rules as a "tool." Empirical Bayes uses the 1,000 eBible profiles to determine how reliable each tool is in various types of corpora before the translator even writes their first verse.  

The Global Prior: By looking at the 1,000 existing translations, we can see that the "Punctuation Balance" rule is highly reliable (99% precision) across almost all scripts, while the "Rare Token" rule is extremely noisy (10% precision) in languages with high hapax rates (like agglutinative ones).  

Borrowing Strength: On "Day Zero" of a new project, the engine doesn't know if the translator is careful or messy. It "borrows strength" from the global average. It starts with the assumption that a punctuation error is probably a real error, but a "rare word" finding is probably just a new word, because that is how those rules behaved in the 1,000 previous languages.

The Hyperparameters (a,b): These are just the "starting counts" for the Bayesian Beta distribution. Instead of starting at zero, you start with the average success/failure rate seen in the eBible sweep.  

2. The "Low-Parameter Budget"
In statistics, every time you let the engine "learn" something from a user’s click, you are adding a parameter. If you have 10,000 findings but only 500 "Accept/Dismiss" labels, the engine can easily find a way to fit those 500 labels perfectly while being completely wrong about the other 9,500.

Local vs. Global: To avoid this "overfitting," we keep the weights (how important Rule A is compared to Rule B) fixed based on the global 1,000-corpus data.

Specific Clusters: The only "parameters" we let the engine learn locally are the specific cluster-level offsets. For example, the engine might learn that in this specific project, the translator uses a unique character (like a glottal stop) that the "Rare Character" rule keeps flagging. After 5 or 10 "Dismiss" actions on that specific character, the engine learns to ignore it, but it doesn't change the weight of the "Rare Character" rule for everything else.

3. Kneser-Ney (KN) vs. Compression (NCD)
You are correct—"training" at this scale is just counting, and it happens in milliseconds. The term "training" in the previous report was used in the machine-learning sense of "estimating probability distributions."

The reason I suggested Normalized Compression Distance (NCD) as a possible replacement isn't about speed; it's about structural robustness.  

The KN Problem: KN relies on n-gram counts. In a small New Testament, most trigrams (n=3) only appear once. If the translator makes a typo, it also appears once. The statistics can't easily tell the difference between a rare valid word and a typo.  

The NCD Solution: NCD doesn't count n-grams. It simply asks: "If I add this verse to a file containing the rest of the New Testament, does the file size grow more than expected?". It's an information-theoretic way to find "surprising" text without needing to smooth out a long tail of zeros.  

4. UI Design for "Natural" Feedback
Since your users are editors, not "labelers," the UI must treat their editing actions as implicit signals. You don't need a "Yes/No" box for every finding.

A. The "Spellcheck" Paradigm (Smart Underlining)
Integrate findings as non-intrusive markers (like the wavy red/blue lines in Word).

Inferred Accept: If the engine flags a word and the user edits that specific span (changing "Gof" to "God"), that is a high-confidence "Accept" label for the finding.

Inferred Dismiss: If the engine flags a word, and the user edits other parts of the verse but ignores the flagged word, that is a medium-confidence "Dismiss" label.

Persistent Neglect: If a finding remains on-screen for 3 separate editing sessions and the user never clicks it, the engine should automatically "shrink" its evidence score toward zero for that cluster.

B. Git-History as a Label Source
You can mine the git history for "repair patterns".  

Pattern Recognition: If you see a commit where ["] was changed to [”] in 50 different places, the engine can extract this as a "correction pattern".

Damerau-Levenshtein (DL) Distance: If a change has a DL-distance of 1 (a single character swap, addition, or deletion), it is almost certainly an orthographic correction rather than a stylistic rephrasing. These are "gold-standard" labels for your Bayesian layer.

5. Agglutinative Languages and MIASEG
For the agglutinative languages (Bemba, Rai, etc.), the "verse-to-verse similarity" you mentioned is exactly what tools like MIASEG (Meaning-Informed Segmentation) use.  

In your internal Wycliffe editor, you likely have access to the verse-level alignment (knowing which target verse corresponds to which source verse). MIASEG can look at two target words that share the same source-root meaning and "strip" the suffixes to find the stem. This allows the engine to recognize that a word appearing for the "first time" is actually just a known root with a new suffix, preventing a false "Rare Word" alarm.  

Summary of Recommendations for your Architecture:

Use Fisher's Exact Test for those rare singleton pairs—it is the "gold standard" for small-count significance and is fast enough for your millisecond goals.  

Embed "Implicit Labeling" into the editor. Don't ask the user for feedback; watch their git commit and their "ignore" behavior on markers.

Key your "Global Priors" by Script. Don't let Latin punctuation rules influence a Devanagari or Ge'ez project.

Bayesian Hierarchical Models - Data, Inference, and Decisions
Source icon
data102.org/ds-102-book/content/chapters/02/02_hierarchical_models.html
Belth | Meaning-Informed Low-Resource Segmentation of ...
Source icon
openpublishing.library.umass.edu/scil/article/id/2134
Bayesian Hierarchical Models - Data, Inference, and Decisions
Source icon
data102.org/ds-102-book/content/chapters/02/02_hierarchical_models.html
A Compression-Based Method for Detecting Anomalies in Textual ...
Source icon
pmc.ncbi.nlm.nih.gov/articles/PMC8156803
An Enhancement of Jiang, Z., et al.'s Compression-Based Classification Algorithm Applied to News Article Categorization - arXiv
Source icon
arxiv.org/html/2502.14444v1

So I don't understand how still I'm supposed to do these statistical commonalities. Yes, there's a thousand e-bobble profiles, but how do I know how reliable each tool is in the various types of corpora? These aren't languages that I speak, nor do I have access to the speakers. So how are you supposed to tell other than saying here's my suspicion, how often are they firing? But well it's probably better to run it against the corpora. Part of me is like, well Well why should I expect that to be any better than my suspicions to a degree?

Mostly 'cause I don't I don't know whether the noise is true noise or what's an actual issue with all these languages that we don't have speakers for.



I'm not discounting NCD, but that seems strange. If I add this verse to a file containing the rest of the New Testament, does the file size grow more than expected? Wouldn't that just be looking for length?

The skepticism regarding speaker-less reliability and the mechanics of NCD is well-placed. In the absence of native speakers, the engine must rely on mathematical proxies for human behavior. Below is a breakdown of how the engine derives reliability from cross-language data and why NCD is more than just a length check.

1. Evaluating Rule Reliability Without Speakers
You don’t need to speak the language to measure the informativeness of a rule. Instead, the engine looks for statistical patterns in the "findings" across the 1,000 eBible profiles to determine which tools are likely "broken" or "noisy."

The Specificity Proxy: If a rule (e.g., "Rare Character") fires on 0.05% of verses in 950 out of 1,000 languages, but on 40% of verses in the remaining 50 languages, the engine can infer that the rule is a high-precision tool that has "broken" on those 50 languages (likely due to a script or character set it doesn't understand).

The "Surprise" Distribution: A good anomaly detection rule should produce a "heavy-tailed" distribution—meaning it stays silent most of the time but screams loudly when it finds something. By sweeping the 1,000 corpora, you can calculate the Entropy of each rule. Rules with low entropy (they mostly fire on very specific, rare things) are assigned higher initial weights than rules with high entropy (they fire randomly everywhere).

Parallel-Corpus Consistency: Since these 1,000 eBibles are translations of the same source text (the New Testament), the engine can use cross-project consistency as a proxy for truth. If a rule flags a verse in Language A, and similar rules fire on that exact same verse (e.g., John 3:16) across 200 other independent languages, the probability that the finding is a legitimate translation issue (or a difficult verse) is statistically higher, even if you don't speak a word of those languages.

2. Why NCD Isn't Just a Length Check
The concern that Normalized Compression Distance (NCD) only measures length is addressed by the "Normalized" part of the algorithm. The formula is:

NCD(x,y)= 
max(C(x),C(y))
C(xy)−min(C(x),C(y))
​
 
Where C(x) is the compressed size of string x, and C(xy) is the compressed size of the two strings joined together.

Normalization: By subtracting the compressed size of the individual strings, the formula removes the "length" factor. It only measures the overlap in patterns.

How it detects "Typos": Imagine the compressor has already scanned 7,000 "clean" verses of a New Testament. It has built a internal dictionary of the language's "normal" character sequences (syllables, common roots).

If you give it a new, correct verse, it finds many matches in its dictionary, and the file size C(xy) barely grows beyond C(x). The NCD is near 0.

If you give it a verse with a typo (e.g., "Gid" instead of "God"), that specific character sequence isn't in the dictionary. The compressor has to describe that sequence "from scratch," causing C(xy) to jump. The NCD is near 1.

Universality: Unlike n-grams, which only look at 2 or 3 characters at a time, NCD captures any repeating pattern of any length that the compressor can find.

3. Implicit Ground Truth via "Repaired Patterns"
The most powerful way to know if a tool is reliable without speakers is to watch what happens to the text over time via Git History Mining.  

The "Correction" Signature: If you see a Git commit where a word was changed from A to B, and the Damerau-Levenshtein distance is 1 (a single character swap), this is a "Natural Typo Label."

Closing the Loop: You can now go back and "test" your rules. Did Rule X (the orthographic checker) flag that word before the translator fixed it?

If yes: Rule X gets a "Reliability Point."

If no: Rule X gets a "Missed Point."

By running this retroactively across the history of the 1,000 eBibles (if history is available) or your internal Wycliffe projects, you can generate a precision/recall score for every single rule without ever asking a human for feedback.

Recommendations for your UI/Data Layer:
Watch the "Ghost" Labels: In your Wycliffe editor, track every time a user manually fixes a word that one of your rules would have flagged. Use these as "Accept" labels to update your Bayesian priors locally.

Filter "Stylistic Churn": Use the "1-2 character change" rule to differentiate between a typo fix (which is a valid label for your engine) and a re-translation of a sentence (which is noise and should be ignored by the Bayesian layer).

Cluster by Script: When using the 1,000 eBible profiles for priors, only pool data from languages that share the same Script ID (e.g., Latin, Arabic, Devanagari). Punctuation and character distributions are script-dependent, not language-dependent.