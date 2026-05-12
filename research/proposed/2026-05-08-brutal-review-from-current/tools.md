The toolbox for asking questions about anomaly and consistency in an NT draft. Companion to `conrete-examples-by-cat.md` (the symptom list) and `concrete-examples.md` (the detector grouping). Where the other two ask "what could go wrong" and "what machinery groups the symptoms," this one asks "what are the tools we can reach for, and which one wins for which question."

Hygiene rules applied:
- Each question is tied to concrete symptoms from the prior list (by number).
- Where tools compete, one is **promoted** and the rest are listed with the reason they lose for our domain.
- Where tools stack (cheap → expensive backoff), they're shown as a ladder, not a menu.
- Tools that require infrastructure we don't have are listed honestly but separated, so they don't pretend to be options.
- Stats-textbook entries that don't earn a domain question are dropped.

Symptom-number references throughout point at `conrete-examples-by-cat.md`.

---

# Question 1: Is this token a real word in this language?

**Why this matters in our domain:** Gate for most word-level error detection. Symptoms #29–35 (typos producing nonwords), #98–99 (untranslated source leak), #100–103 (markup/marker leaks) all reduce to "this string isn't a word." If we can answer yes/no/maybe with calibrated confidence, half the noise on rare-word detectors disappears.

**What we'll actually have, today:** in practice the project rarely ships with a translator-supplied wordlist, and the related-language lexicon path is mostly aspirational. The *real* default is char-n-gram with KN smoothing — a weak signal, but the one we can always collect. Wordlist and bootstrap-lexicon are listed as future paths to plug into, not the v1 plan.

**Tools, ordered by leverage if available (cheap → infrastructure-heavy):**

1. **Translator-supplied wordlist (binary lookup)** — *future, if/when one exists.* 10-minute elicitation produces a list; lookup is O(1); the answer is "the translator said yes/no," which is as good as it gets. Direct domain authority. Treat the ingestion path as "ready to consume" rather than "expected input."
2. **Bootstrap lexicon from corpus.** A surface form appearing 20+ times is presumed real. Free, but circular — it presumes the corpus is mostly correct, which is exactly what we're trying to verify. Useful for words that are common enough to be unambiguous; less useful at the tail.
3. **Related-language lexicon transfer.** *Probably not available in practice.* Listed for completeness; if a gateway-language relative with a dictionary appears, treat its lexicon as a weak prior.
4. **Character n-gram with Kneser-Ney smoothing.** *The practical primary signal.* Implicit answer: "does this form's character sequence look like other forms in this language?" The current Laplace implementation under-flags rare contexts at NT scale — Kneser-Ney's continuation probability is specifically designed for the rare-context-backoff regime, which is exactly the rare-word case.

**Stacked, not competing:** the four answer the same question with different cost/confidence. In v1, char-n-gram does the work; bootstrap-lexicon downweights its noise where corpus repetition allows; the other two ingest if/when they materialize. Don't ensemble them at the same level — the lower paths give correlated signals when stacked, and a logistic-regression combiner only makes sense once labels exist.

**Distinct but related tool to call out:**
- **Anti-lexicon (known-wrong forms).** Useful, but answers a *different* question — "is this a confirmed error?" — and belongs to the feedback loop (Question 10), not to lexicon membership.

**What I'm not listing and why:**
- *Stem-lexicon vs surface-lexicon.* Same tool for analytic languages; only distinct when working segmentation exists for an agglutinative language (see Question 4 for stem clustering).
- *Frequency-tiered lexicon ("known-once vs known-many").* Reduces to bootstrap + raw frequency; not a separate tool.

---

# Question 2: Is this token / n-gram / position appearing where the corpus says it should?

**Why this matters in our domain:** The NT is highly formulaic and structured. Symptoms #120 (formulaic phrase variation: "Verily I say unto you"), #122 (genealogy pattern break), #44–48 (doublings and copy-paste duplications), #115 (hapax density spike) all reduce to "this thing is in the wrong place statistically."

**Sparse-data caveat up front:** at single-NT scale, per-book and per-genre conditioning splits ~150k tokens into thin slices, and "function word vs topical word" can't be distinguished without aligned data. The two conditioned variants below (book/genre, burstiness) risk manufacturing false positives in the very regime we're trying to be conservative about. They stay in the toolbox as *future* refinements once data volume or label volume justifies them; v1 uses raw frequency + n-gram frequency only.

**Tools, ordered by what we ship now vs later:**

1. **Raw frequency** — *v1 baseline.* Answers "is this rare or common." Trivial but necessary.
2. **N-gram frequency (bigram, trigram)** — *v1, the formulaic-phrase tool.* "Verily I say" as a trigram fires when it appears as "Truly I declare." Synoptic parallels and epistolary openings make this NT-specifically strong.
3. **Frequency conditioned on book/genre** — *deferred.* Gospels, Acts, epistles, Revelation have distinct vocabulary profiles in principle, but each book is only thousands of tokens. Add only when we have evidence the conditioning sharpens a specific rule's precision.
4. **Burstiness (variance of inter-occurrence distance)** — *deferred.* In principle distinguishes function words (uniform) from topical words (clustered). In practice, without alignment data we can't reliably tell which class a token belongs to, and the variance estimate is noisy at single-NT scale.

**v1 ships #1 and #2 only.** #3 and #4 stay documented so we don't re-derive them when the data warrants.

**Not listing and why:**
- *Skip-grams.* For our domain, bigrams catch the formulaic-phrase cases without the combinatorial blow-up. Skip-grams matter when arguments intervene unpredictably; in NT formulae, intervening tokens are themselves predictable.
- *Positional n-grams ("verse-initial trigram").* Subsumed by Question 11 (position-conditioned conformity) — same tool, sharper question.
- *Word/sentence embeddings.* Different question entirely (cross-lingual semantic match); see Question 5 infrastructure notes.

---

# Question 3: Does this string's character pattern look like this language?

**Why this matters in our domain:** Catches keyboard garbage (e.g. `;lkjasdf`), merge-conflict markers (#101), and orthographic textures that don't reduce to "real word vs nonword." Symptoms #6 (surface-pattern normality detector cluster) — typos producing tokens that lexicon lookup *can't* answer because the form is novel.

**Tools, with explicit competition:**

1. **Character n-gram surprisal (Kneser-Ney).** Local — catches "one weird trigram in an otherwise normal word."
2. **Compression ratio against project corpus.** Holistic — catches "an entire word that doesn't fit the patterns."
3. **Compression ratio against gateway-language prior (eBible).** Same as #2 but with an external prior for cold-start cases when the project corpus is small.

**These overlap and compete.** Ensembling all three is what creates the correlated-factor problem in the current Noisy-OR pipeline. Honest take for our domain:

- **Promote at verse scale: compression ratio.** Whole-verse pattern check; complements n-grams without duplicating them.
- **Promote at word scale: char-n-gram surprisal (Kneser-Ney).** Conservative default for the v1 demotion away from Noisy-OR theater.
- **Open empirical question: word-scale compression vs char-n-gram.** The user's intuition is that word-scale compression naturally discovers multi-gram patterns (bigram, trigram, 4-gram, 5-gram) without committing to an n; that's a real point. The counter-claim ("n-grams are better-conditioned at NT-corpus scale") is *currently untested*. Disposition: leave compression-at-word-scale runnable but not in the default ensemble until a head-to-head on the same corpus says which wins. <!-- @ai? word-scale compression vs char-ngram-KN: needs a head-to-head before either is the default. -->
- **Gateway-language prior:** keep as a cold-start option, not a default. Adds value only when project corpus is below some threshold (worth deriving empirically).

**Tied to detector 6 (surface-pattern normality).** Catches: transposition-to-nonword (#29), deletion-to-nonword (#34), insertion (#35), repeated-letter typos (#33), keyboard garbage, merge markers (subset of #101).

**Not listing and why:**
- *Script-of-character analysis.* Different question — "are all characters from the expected script?" That's detector 2 in the structural file; near-100% precision once expected scripts are set. Listed under Question 11 because it's a position/conformity check, not a pattern-modeling check.
- *Character-class transition probabilities (digit-after-letter, etc.).* Subsumed by char-n-gram; doesn't earn its own line.

---

# Question 4: Are these two surface forms the same underlying word?

**Why this matters in our domain:** Symptoms #60 (proper noun spelling inconsistency: Peter/Petros/Petro), #62 (diacritic-presence variant), #61 (transliteration variants), #118 (proper noun spelled differently across the NT). Also unlocks the canonical-distance trick for cheap proper-noun source alignment (see Question 14).

**v1 scope decision: proper nouns only.** The variant-identity machinery should ship constrained to tokens we have a good reason to believe are proper nouns — uppercased mid-sentence in case-aware scripts; corpus-attested capitalized lemma; or appearing in a slot where the source-side has a proper noun. This is deliberately conservative. The general-vocabulary version (Spanish `hablo`/`habló`, English `find`/`fine`) re-opens the probability-theater problem from the last round; we don't take that on yet. Scope expansion is a future decision once the proper-noun version is shipping clean findings.

**Tools, as a backoff ladder (cheap → expensive, accept first confident match) — applied within proper-noun scope:**

1. **Lowercase + diacritic-fold + exact match.** Catches "Jerusalem" / "jerusalem" / "Jérusalem." Free, high-precision, run it first.
2. **Damerau-Levenshtein at small distance, gated on proper-noun context.** Catches "Petros" / "Petrs" / "Pteros." **Promoted over plain Levenshtein** because transposition is a single operation in Damerau and a single common typo class; plain Levenshtein scores transpositions as two edits and underweights them. *Gate:* only fire when at least one of the two forms is uppercased mid-sentence (i.e. intrinsically marked as a proper noun) or matches a corpus-attested capitalized lemma. This gate is what keeps the rule conservative.
3. **Phonetic encoding (Double Metaphone).** Catches "David" / "Dawid" / "Dafyd." **Promoted over Soundex/NYSIIS/Caverphone** — they answer the same question; Double Metaphone is the only one that handles multiple language families adequately.
4. **Repeatable-stem detection** *(replaces consonant-skeleton)*. The question to ask is "is there a repeated stem across the places where the source text has this proper noun?" rather than "do these two strings share consonants?" Consonants alone fail on cases like Arabic `isa` (almost no consonants to skeletonize) and over-fire on accidental consonant matches. A repeatable-stem check looks across all target tokens aligned (by Sid + capitalization) to the same source proper noun, extracts the longest common subsequence or shared prefix, and flags forms that don't contain it. Conservative; needs same-Sid source-target pairing (Q5/Q14) to be available.
5. **Stem clustering** (existing `lemma_cluster.rs`, `candidate_families.rs`). Catches inflectional variation. Most expensive; requires working segmentation. For v1, only the proper-noun-restricted slice runs.

**Use as a ladder, not an ensemble.** Each rung catches a class the previous one misses, but stacking five scores per pair is overkill.

**Worked examples motivating the proper-noun scope:**

- *Mary/Mark:* source says "Mary ran to the tomb. Mary then said…" Both Marys aligned to the same source proper noun. If target says "Mary ran to the tomb. Mark then said…" the second is a stem mismatch *given the source-aligned slot* — high-precision finding. Pure target-side variant-identity (Damerau-1 between "Mary" and "Mark") would flag this too, but with high false-positive rate elsewhere; the source-aligned version is much sharper.
- *tomb/tome:* both are real English words, so single-signal lexicon / variant-identity can't decide. But "tome" as a hapax in Easter-narrative context plus "tomb" being the expected proper-noun-context word combines multiple weak signals — see Question 9 on the open additive-combination question.

**What I'm not listing and why:**
- *Weighted edit distance.* Worth the complexity only if we have data to learn weights; we don't yet. Damerau-Levenshtein with cost 1 is the right default.
- *Sound-class-based edit distance.* Overlaps phonetic encoding; we don't need both. Pick the phonetic-encoding path.
- *Bilingual word embeddings.* Different question — needs aligned bilingual training data we don't have.

**Tied to:** detector 8 (variant identity stack). Symptom #25 (general-vocabulary diacritic-presence checks): explicitly out of v1 scope per the proper-noun-only decision above.

---

# Question 5: How much does the target verse diverge from its source counterpart?

**Why this matters in our domain:** Symptoms #52 (whole-verse omission), #53 (whole-verse duplication), #92 (merged verses), #93 (split verses), and *gates* the 🔴 → 🟡 promotion of symptoms #55–59 (wrong proper noun, wrong number, wrong pronoun, wrong tense, wrong same-field substitution). Without source alignment, all of those are buried. With it, they become tractable.

**Tools, at progressively finer granularity:**

1. **Length-ratio z-score, length-bucketed** (currently in `source_relative.rs`). Coarsest. Catches gross omissions, additions, merges, splits. Works without alignment infrastructure.
2. **Per-book length-ratio z-score.** Same question, conditioned on book. Hebrews has a different verbosity profile than 1 John; without per-book conditioning, book-specific anomalies hide.
3. **Conditional NCD (normalized compression distance) between source-target pairs.** Catches "different information content at same length" — the wrong-passage-in-right-length-slot case. More sensitive than length alone; harder to interpret per-instance.
4. **Word-level alignment (IBM Models or similar).** **The unlock.** Once available, symptoms #55–59 and #116–117 (inconsistent translation of the same source term) become tractable. High infrastructure cost; treat as one build-decision, not as multiple rules.
5. **Repeatable-stem + same-Sid proper-noun match.** The poor-man's proper-noun alignment described in Question 14. Free if you accept "proper-noun-only" scope. *Note:* this replaces what was previously framed as a consonant-skeleton check — see Q4 #4 for why repeatable-stem-across-source-occurrences is the right framing rather than vowel-stripping.

**Promoted ordering:** ship 1 + 2 now (already there); add 5 as a cheap proper-noun-specific shim; treat 3 and 4 as separate roadmap decisions, not as overlapping rules.

**What I'm not listing and why:**
- *Cross-lingual sentence embeddings (SBERT etc).* Real tool, requires neural infrastructure outside scope, and the precision targets discussed don't need it.
- *Neural attention as alignment.* Same — requires an MT model.
- *Round-trip translation.* Same — also confounds errors in the round-tripper.

---

# Question 6: Is this co-occurrence real, or could it be chance?

**Why this matters in our domain:** This is the one question where stats-textbook entries are legitimate, but I owe you the proper "which one and why."

The question arises whenever we want to *learn a corpus convention* from observed counts: "punctuation P clings left" (#76), "after period, next character is uppercase" (#37), "word W is never sentence-terminal," "source term X aligns with target term Y" (when alignment exists).

The shape of every one of these problems is a 2×2 contingency table:

|              | next char uppercase | next char lowercase |
| ------------ | ------------------- | ------------------- |
| after period | 487                 | 12                  |
| after comma  | 31                  | 234                 |

We want to know: is the row × column association real?

**Three tools, same question, different regimes:**

- **Chi-square.** Oldest. Works when all expected cell counts ≥ 5. For well-populated tables, gives nearly identical answers to G². No special property that recommends it for our domain.
- **Dunning's G² (log-likelihood ratio).** **Promoted as the default.** Same null, same input, different statistic. Chi-square's approximation degrades at small expected counts (which is exactly where rare-predecessor analyses live in NLP); G² is better-behaved at the boundary. For abundant tables it agrees with chi-square; for sparse ones it doesn't lie. This is the narrow but real sense in which "Dunning is an upgrade of chi-square."
- **Fisher's exact test.** Computes the exact probability rather than approximating. Always correct. Cost: summing hypergeometric probabilities — fine for one table, real overhead across thousands.

**The decision rule already in `association.rs` is correct:** use G² when minimum expected cell count ≥ 5, fall back to Fisher when sparse. This is the standard recipe.

**One question, three implementations.** Not three different questions. Don't ensemble — pick by regime.

**What I'm not listing and why:**
- *Permutation tests.* For arbitrary statistics over complex null hypotheses. We don't have those. Our nulls are independence of two binary factors, which has closed-form tests. Adding permutation here is reaching for a hammer we don't need.
- *Bonferroni / FDR correction.* Real concern when running thousands of tests, but the per-rule precision tracking in Question 8 handles the practical decision ("should I trust this flag?") better than family-wise correction. Worth revisiting if we ever ship a "scan all 150k tokens for X" report mode.

---

# Question 7: Is this value an outlier compared to its peers?

**Why this matters in our domain:** Verse-length z-scores (#110, #111), per-verse compression ratios, token-length distribution outliers (#112, #113), hapax density spike (#115). General "is this in the tail" for continuous values.

**Tools:**

1. **MAD-based z-score (robust z).** **Promoted as the default.** Uses median and median absolute deviation; resistant to outliers in the baseline data, which matters because the baseline *contains* the very outliers we're looking for. Currently in `mad.rs`.
2. **Tukey fences (1.5× / 3× IQR).** Same family; mostly used for visualization. Pick MAD or IQR but not both — they answer the same question with different cutpoints.
3. **Per-cohort robust z.** Refinement: outlier-ness is meaningless without specifying "outlier compared to what." Length-bucket cohorts (already in `length_buckets.rs`) for proportionality; per-book cohorts for book-specific anomalies.

**Promoted path:** MAD with per-cohort conditioning. Drop IQR from active consideration unless a specific need arises.

**What I'm not listing and why:**
- *Bootstrap confidence intervals.* Useful for *uncertainty around* an outlier judgment, not for the *judgment itself*. We rank by point estimate; bootstrap doesn't change the ranking.
- *Standard z-score (mean + SD).* Inferior to MAD here because the baseline is contaminated by exactly the values we want to flag.

---

# Question 8: Given a flag fired, how confident should I be that it's a real error?

**Why this matters in our domain:** The per-rule calibration question. Every rule needs to be answerable: "of the times this rule fired in the past, what fraction were real?" — and that fraction has to be tracked, updated, and used to threshold.

**Tools, as a progression (each step adds capability when data warrants):**

1. **Per-rule precision tracking (simple count).** **The starting point.** `tp / (tp + fp)`. Cheap, interpretable, works at any sample size including N=1.
2. **Beta-binomial posterior over rule precision.** Formal version, adds principled uncertainty when sample size is small. Already in `posterior.rs`. **Honest take:** the math is right but the binary "show/don't-show this flag" decision rarely benefits from the uncertainty band — you threshold on the point estimate either way. Premature given current label counts.
3. **Per-rule × cluster posterior.** Sub-routing: a rule may have different precision for high-frequency proper nouns vs low-frequency ones. Currently supported in `posterior.rs`. **Premature** until per-rule sample sizes are large enough to slice.
4. **Logistic regression.** Once ≥ 50 labels exist with multiple weakly-correlated signals, learns weights properly and handles correlation. The right replacement for Noisy-OR when labels exist.

**Promoted sequencing (confirmed for next refactor):** Use #1 today. Move to #2/#3 only when sample sizes warrant; move to #4 when label count crosses threshold and Noisy-OR's correlated-factor problem becomes the bottleneck. Don't build infrastructure for steps you're not ready for.

**Distinct but related:** the *anti-lexicon* mentioned under Question 1 is a specialization of this loop — confirmed-error labels feed both per-rule precision and a direct-suppression filter.

---

# Question 9: How do I combine multiple flags on the same verse into one judgment?

**Why this matters in our domain:** Currently solved with Noisy-OR. It is silently inflating scores when its independence assumption fails — which is constantly, because `char_anomaly` and `char_ngram_backoff` are measuring overlapping things at the same scale.

**Tools, with explicit honesty:**

1. **Maximum-of-evidence.** Show the strongest signal. Conservative; doesn't combine evidence but doesn't double-count.
2. **Noisy-OR.** Correct when factors are independent. Wrong when they're correlated. Our current situation has correlated factors.
3. **Weighted log-odds sum** (= logistic regression at inference). Handles correlation correctly *if weights are learned* from labels. Same data prerequisite as Question 8 #4.

**Promoted path** (mirrors what I said in the prior audit, but anchored here): **drop Noisy-OR for now; use max-of-evidence as the interim.** Max-of-evidence under-combines but doesn't lie. Move to weighted log-odds when label counts support learning the weights.

**Domain-specific recovery for correlated factors:** the simpler fix before the labels arrive is to *not* ensemble overlapping detectors at the same scale — see Question 3. Char-n-gram at word scale, compression at verse scale, no parallel duplicates. This removes most of the correlation without changing the combiner.

**Open question — narrow additive combination for specific rule co-firings.** <!-- @ai? when (if ever) should two signals co-firing on the same token boost confidence beyond max, and how to define those combinations without re-inventing Noisy-OR? -->

Max-of-evidence under-combines on purpose, but there are real cases where two signals firing together carry more weight than either alone. Worked examples:

- *tomb/tome.* Both are valid English words, so single-signal variant-identity won't decide. "tome" appearing as a hapax in Easter-narrative slots, *and* "tomb" being the expected word in the canonical neighborhood, *and* the source-aligned position favoring "tomb" — three weak signals that collectively suggest a typo. Max-of-evidence shows the strongest of the three; the *combination* is what makes it a finding.
- *Mary/Mark.* Source has "Mary" twice in adjacent verses (canonical-distance close, proper-noun-aligned). Target has "Mary" then "Mark." Damerau-1 on the second + canonical-distance-close + source-aligned proper-noun-stem mismatch is the kind of co-firing that should beat max.

The user's stated principle: "to say that nothing combines and creates multiple signals is throwing out the baby with bathwater." Agreed.

Why this stays open in v1: we don't have a principled way yet to specify *which* signal pairs are legitimately additive vs which are correlated and would just inflate scores again. Re-inventing Noisy-OR by hand for specific pairs is the same trap. The two candidate framings worth thinking about:

1. **Whitelist additive combinations explicitly.** Maintain a small set of "these two signals are independent in domain meaning and additive when they co-fire" rules, written as code. Forces the engineer to argue for each addition. Conservative, low risk.
2. **Wait for labels.** Logistic regression learns the weights including any interaction terms we encode. The principled answer, but blocked on data.

Tentative path: lean on #1 for the proper-noun-aligned cases (Mary/Mark style) because the signal independence is clear (canonical-distance + source-stem + Damerau are about different things). Don't generalize to other rule pairs without a domain argument for each one.

**What I'm not listing and why:**
- *Bayesian model averaging.* Real tool, requires multiple competing models with posteriors; we don't have that. The "average over many model variants" use case doesn't apply.
- *Calibration via Platt / isotonic.* Belongs to Question 8 (per-rule precision → probability mapping), not to combination.

---

# Question 10: Which rules and items should learn from translator feedback?

**Why this matters in our domain:** Feedback is expensive (one translator's time). We want it where it pays off, and we want to ask diversely rather than re-asking the same kind of question.

**Tools — and honesty about how thin this section actually is:**

1. **Priority-ordered (highest-suspicion-first).** Default. Reasonable for single-translator workflow at 100–500 items.
2. **Active learning (label-uncertainty-first).** Show the example whose label most reduces uncertainty — for binary classification, the one closest to the decision boundary. Moderate complexity; not yet implemented; high leverage *once* per-rule precision is calibrated enough that "near the boundary" is meaningful.
3. **Diversity-aware sampling (MMR-style).** Avoid asking 10 instances of the same error before asking 1 of a different type. Particularly relevant for the elicitation UI: the translator's first 50 labels should span error types, not concentrate.

**Promoted path:** priority-ordered today; diversity-aware sampling next (it pays off at small N, before active learning is meaningful); active learning when per-rule sample sizes warrant.

**What I'm explicitly not pretending exists here:** cross-project label transfer, crowd disagreement modeling, hierarchical models over projects. Real techniques, but they assume infrastructure (multiple projects, multiple labelers) we don't have. Listing them as "options" would pretend otherwise.

---

# Question 11: At position P, what character class is dominant — and does this instance match?

**Why this matters in our domain:** Conformity-by-position is a distinct question from frequency (Q2) or outlier-ness (Q7). Symptoms #37 (sentence-initial lowercase), #76 (intermedial clinging punctuation), #80 (verse-start orphan punctuation), #81 (missing terminal punctuation), #38 (proper-noun lowercased mid-sentence), #40 (ALL CAPS outlier), and others all reduce to "given I'm in position P, the corpus says class C is dominant — and this token is not in C."

**Tools:**

1. **Per-position character-class distribution table.** For each (position, class) pair, store the observed conditional probability. Sentence-initial × uppercase: 0.97. After-comma × lowercase: 0.91. Cheap, transparent, corpus-derivable.
2. **Significance test on the conditional** (Question 6). To decide whether `P(class | position)` is a real convention or noise, run G² on the position-vs-class table. This is where the seemingly-pure-stats Question 6 plugs into the actually-useful position-conditioned rule.
3. **Script-of-character analysis.** Specialization: "position is anywhere; expected class is `script == corpus_majority_script`." Currently in `script.rs`. Listed here rather than Q3 because it's a *conformity* check, not a *pattern-model* check.
4. **`ClingingClass` (already implemented).** The instantiation of #1 for punctuation directionality — learned per-codepoint.

**Promoted path:** ship per-position conditional tables as the core machinery; let `ClingingClass`, sentence-start-case, and casing-distribution conformity all be specializations of the same machinery rather than parallel implementations. The corpus introspection step (see cross-cutting notes in `concrete-examples.md`) lives here — before enabling each per-position rule, confirm the conditional is sharp enough (e.g., ≥95%) to justify flagging the minority.

**Tied to detectors 10 and 11** in `concrete-examples.md`.

---

# Question 12: For every opener, is there a matching closer in scope?

**Why this matters in our domain:** Span/pair integrity is a *discrete structural check*, not a probabilistic one — distinct from every other question above. Symptoms #77 (unmatched paired punctuation), #87 (quote opened never closed), #88 (nested quotes with wrong levels), #84 (bracket-family mismatch), #90 (quote close before quote open), #78 (curly-open paired with straight-close).

**Tools:**

1. **Span tracker with pair-family table** (already in `discourse.rs`). For each opener, follow forward and confirm a matching closer within the allowed span. Pair family table encodes which closers match which openers (curly-with-curly, bracket-with-bracket).
2. **`max_span_sids` per-corpus configuration.** Quotes legitimately span multiple verses in NT translations; the rule cannot fire on every cross-verse open. Already configurable.
3. **Corpus-learned nesting convention.** For nested quotes, the corpus picks an outer-inner pattern; flag the deviation. Learned, not hard-coded — French and English NT translations differ on guillemets vs curly-doubles.

**This is a one-tool question.** The "competition" is between strictness configurations (how much span tolerance, how strict the family check), not between rival statistical methods.

**Not listing and why:** parser-based bracket-matching, constituency parses, etc. — overkill for a token-stream check that already works.

---

# Question 13: For codepoints with stylistic variants, is this the corpus-dominant variant?

**Why this matters in our domain:** Symptoms #20 (smart vs straight quotes), #21 (apostrophe variants), #22 (hyphen/en-dash/em-dash/minus), #23 (triple-dot vs ellipsis), #7 (NBSP vs regular space), #26 (NFC vs NFD mix). The user's hard-rule decision on quotes lives here.

**Tools:**

1. **Per-function variant-family table.** Hand-curated list of codepoints that "do the same job" — quote-openers, quote-closers, apostrophes, dashes, ellipses, spaces. Small, stable table.
2. **Per-corpus majority within each family.** Count occurrences per codepoint in the family, pick the dominant one (≥95% threshold). Flag minorities.
3. **Position conditioning** (overlap with Question 11). Some families have position-dependent majorities — opening vs closing quote codepoints are different. Conditioning on position-in-pair matters; otherwise paired codepoints look like majority-vs-minority of themselves.

**Promoted path:** ship variant-family tables + majority detection; position-condition only where it matters (quotes specifically); user decision to *not* normalize NFC/NFD globally still leaves room to detect within-file mixing here.

**Note the cross-cut with Question 4:** "Are two surface forms the same word" is the *word*-level version of this question. Here we're at codepoint level for stylistic-variant detection. Same logical shape, different units.

---

# Question 14 (meta): Given canonical position, how should I weight a similarity signal?

**Why this matters in our domain:** Your contribution. Most of the questions above ask "are these things similar?" without conditioning on *where* they are. Canonical distance is a multiplier, not a tool of its own — it modifies how much evidence "similar things" carry.

**v1 scope decision: proper nouns only.** The user's intent for canonical distance was specifically the Mary/Mark case — proper-noun-aligned slots where source-side repetition makes a target-side variant suspicious. General-vocabulary use (hapax-clustering, distributional anomalies in common words) is *maybe* worth exploring later but is not what canonical distance is being introduced for. Long-tail hapaxes in general vocabulary risk too much noise without a clean way to gate.

**The shape of the modifier (proper-noun version):**

Two proper-noun forms with Damerau-distance 1 occurring 2 verses apart, aligned to the same source proper noun, are very likely a typo of one another. The same pair 800 verses apart are more likely legitimate transliteration variants (different gospel writers, different epistolary contexts). The signal is `edit_similarity / log(canonical_distance + 1)` or similar — the exact function matters less than the principle that proximity + source-stem consistency together strengthen the "same intended form" hypothesis.

**Where it threads into other questions (proper-noun scope unless noted):**

- **Q4 (variant identity, proper nouns):** down-weight long-range matches; up-weight short-range matches when source-stem-aligned. Damerau-1 within 5 verses with same source proper noun is a typo; Damerau-1 across the canon is a spelling-convention split.
- **Q9 (multi-signal additive combination):** canonical distance is one of the "different things" that combine additively with Damerau + source-stem in the proper-noun-aligned co-firing case (see Q9 open question).
- **The proper-noun alignment trick.** Same canonical Sid in source and target = canonical distance zero. Combined with the **repeatable-stem detection** in Q4 tool #4 (not consonant-skeleton — see that section), this gives cheap proper-noun source alignment without IBM Models or embeddings: "source verse X has a capitalized-or-script-isolated token whose lemma is L; target verse X has a token sharing the stem of L's repeatable target-side rendering; declare aligned." Unlocks proper-noun consistency checking across the corpus without any of Question 5's heavier infrastructure.
- *Out-of-scope-for-v1:* using canonical distance to weight Q2 distributional anomalies on general vocabulary, or Q5 anomaly clustering. Possible later; punted now.

**Why it's a meta-tool, not a tool:** there's no separate "canonical distance detector." It's a weight applied inside other detectors' scoring functions. The right place to build it is as a `Sid::canonical_distance(other) -> u32` utility plus a documented convention for how each rule consumes it.

**Tied to:** detector 8 (variant identity stack, proper-noun slice) gets it as a weighting; detector 13 (source-aligned) gets it for free since same-Sid is canonical-distance-zero.

---

# What got cut from the prior 120-item list, and why

Quick honest inventory of what I had to remove during the audit:

**Stats tools without a domain question:**
- Permutation tests — closed-form tests work for our 2×2 cases.
- Bonferroni / FDR — per-rule precision tracking handles the practical version.
- Bootstrap CIs — they answer uncertainty-around-the-answer, not the answer.
- Standard z-score — MAD dominates for our use case.

**Tools requiring infrastructure we don't have (kept as roadmap, not as options):**
- Neural / contextual / sentence / bilingual embeddings.
- IBM Models / EM alignment / grow-diag-final symmetrization.
- Round-trip translation.
- Hierarchical Bayesian project models.
- Crowd-disagreement modeling.

**Tools that overlap and shouldn't be listed separately:**
- Chi-square / Dunning / Fisher — one question, three regime-implementations (Question 6).
- Plain Levenshtein / Damerau — pick Damerau.
- MAD / IQR — pick MAD.
- Soundex / NYSIIS / Caverphone / Double Metaphone — pick Double Metaphone.
- Char-n-gram-at-word-scale + compression-at-word-scale — pick char-n-gram at word scale (Question 3).

**Tools that answer the wrong question:**
- Anti-lexicon as a Question 1 entry — it's Question 8/feedback.
- Skip-grams and positional n-grams — folded into Question 11 where the question is sharper.
- Calibration (Platt/isotonic) listed as combination — belongs to per-rule precision (Question 8).

---

# Cross-reference: question → symptoms (from `conrete-examples-by-cat.md`)

| Question                           | Symptoms primarily addressed                                             |
| ---------------------------------- | ------------------------------------------------------------------------ |
| 1 Lexicon membership               | #29, #31, #33, #34, #35, #98, #99, #100, #101, #102, #103                |
| 2 Distributional / formulaic       | #44–48, #115, #120, #121, #122, #114                                     |
| 3 Character-pattern normality      | #29, #33, #34, #35, #101 (subset), keyboard garbage                      |
| 4 Variant identity                 | #60, #61, #62, #63, #118                                                 |
| 5 Source-target divergence         | #52, #92, #93, #110, #111, gates #55–59, #116, #117                      |
| 6 Significance (G²/Fisher/χ²)      | Underpins learned conventions in Q11, Q13, and detector calibration      |
| 7 Outlier-ness                     | #110, #111, #112, #113, #115                                             |
| 8 Per-flag confidence              | Cross-cutting: every detector's precision threshold                      |
| 9 Multi-flag combination           | Currently Noisy-OR for all detectors; mostly affects detector-6 ensemble |
| 10 Feedback prioritization         | Cross-cutting: elicitation UI workflow                                   |
| 11 Position-conditioned conformity | #37, #38, #40, #41, #42, #76, #80, #81, #66, #68, #69                    |
| 12 Span / pair integrity           | #77, #78, #84, #87, #88, #90                                             |
| 13 Codepoint variant conformity    | #7, #20, #21, #22, #23, #26                                              |
| 14 (meta) Canonical distance       | v1: weighting on Q4 (proper-noun variant identity) + proper-noun alignment trick. Q2/Q5 weighting deferred. |
