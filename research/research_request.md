> **ARCHIVED** — Original research prompt sent to external agents. See `latest-agent-reports/synthesis.md` for the resulting synthesis.

# Research Request: Soundness, Feasibility, and Architectural Review

## What this is

This is a research-mode prompt for an AI agent with web access and
deep-research capability. The agent is being asked to take an
existing in-progress project — a small statistical engine for
detecting probable errors in Bible translations — and stress-test
its design against the best of modern statistics, NLP, and ML, with
particular attention to corpus-size constraints and to building
something that is useful from day zero on a brand-new translation
project.

The author is a software engineer at a Bible translation
organization (Wycliffe Associates). They are *not* trained in NLP,
classical ML, or computational statistics. The repo is the result
of working from first principles, reading research, and reasoning
about what should work at the data scale they have. They want
external expert grounding before committing to the architectural
direction described in `research/evidence_layer_design.md`.

The agent should treat this as a peer review, not as a teaching
exercise. Critique freely. Recommend course corrections. Cite real
literature, methods, and reference implementations.

## What you have access to

In addition to this prompt, the agent will be given:

1. **A complete bundled snapshot of the repository** (via repomix or
   equivalent). This includes all Rust source under `crates/`, all
   research notes under `research/`, default rule configurations,
   and integration tests. **Read this end-to-end before forming
   conclusions.** The architecture is intentional in places that
   may not be obvious from any single file.

2. **Calibration profiles in `data/calibration/`**: per-corpus
   profiles (`*.profile.json`) generated from a sweep over ~1000
   eBible translations. Each profile records corpus-level statistics
   the engine uses to derive its weights and thresholds: token
   counts, type counts, type/token ratio, hapax fraction, casedness,
   script mix, character-level KN perplexity, etc. **This is real
   data from real New Testaments. Use it for your feasibility
   analysis — don't reason in the abstract about "small corpora"
   when you have the actual distributions.**

3. **`research/` folder** with the prior thinking:
   - `VISION.md` — the project's stated philosophy and goals.
   - `METHODS.md` — the methods that have been considered or adopted.
   - `sil_audit.md` — an audit of two SIL Global codebases (Machine,
     silnlp) for techniques worth borrowing, with verdicts.
   - `sil_audit_implemented.md` — what's been ported so far, with
     deviations from the audit's recommendations.
   - `evidence_layer_design.md` — the proposed (not yet implemented)
     architecture for Bayesian per-cluster calibration and label
     flow. **This is the current architectural direction the author
     is most uncertain about and most wants critiqued.**
   - `gpt-research-response.md` and `Dunning-anomaly-detection.pdf`
     — earlier external grounding.

## The core thesis we want stress-tested

The engine's central commitments, in priority order:

1. **No LLMs, no large models, no language-specific dictionaries.**
   The engine must run embeddable inside translator workflows on
   modest hardware, possibly offline. It must work on a brand-new
   under-resourced language with no pre-existing labeled data.

2. **Many independent weak signals over one strong signal.** Each
   rule produces an evidence score in `[0, 1]`. An aggregator
   combines per-Sid (verse) scores using a hand-tuned weighted sum
   for v1, with calibrated `pair_multipliers` for known correlated
   rule pairs.

3. **Conservative defaults + label-efficient online updates.** The
   proposed architecture uses Beta-Binomial conjugate updates per
   cluster, where a "cluster" is a rule-specific equivalence class
   over findings (e.g. all `punct.paired-balance` findings about the
   `"` character). Labels arrive from explicit user dismiss/accept,
   from inferred-accept via edit-tracking, from git-history mining,
   and from cross-project pooled priors for universal clusters.

4. **Corpus scale: 150,000–250,000 tokens per New Testament.** Type
   counts range from ~6,000 (analytic languages like Indonesian) to
   ~22,000–23,000 (agglutinative like Bemba, Rai). For the
   high-type-count cases, ~70% of types are hapax — every "rare
   token is suspicious" signal degrades severely.

5. **Useful from day zero of a fresh project.** A translator opening
   a brand-new project with one verse drafted should still benefit
   from the engine. This drives the cross-project prior pooling
   proposal in `evidence_layer_design.md`.

The author has come to suspect that "many independent weak signals
combined with weights" is essentially a parameter-fitting problem
in disguise — i.e. *this is ML*, and the question is how many free
parameters can be responsibly fit against this much data with this
much label scarcity. They want a sober external read on whether the
chosen path is sound or whether they're walking into a class of
problem the field has already named and characterized.

## Research questions, by area

The agent should treat these as starting points, not a checklist.
Surface things the author hasn't thought to ask.

### A. Statistical / ML soundness

1. The engine relies on Dunning log-likelihood ratio (LLR) for
   source-target co-occurrence and for proportionality checks.
   Is this defensible at the sample sizes involved (per-Sid Dunning
   on ~8,000 verse pairs in an NT)? Are there alternatives — e.g.
   Fisher's exact test, mutual-information variants, t-score —
   that would behave better in the small-sample regime, especially
   for the long tail of low-frequency tokens?

2. The proposed Bayesian layer is Beta-Binomial conjugate per
   `(rule_id, cluster_key)`. Is this the right family? Should we
   consider hierarchical Bayes (clusters partially pool within a
   rule, rules partially pool within an engine), Empirical Bayes,
   or shrinkage estimators like James-Stein for the small-cluster
   case? What does the literature on label-efficient online
   learning at small scale recommend?

3. The aggregator is currently a weighted sum of per-rule evidence,
   with planned `pair_multipliers` to discount correlated rule
   pairs. The audit calls this "many independent weak signals"
   theory — N signals each with FP rate p, K corroborations needed
   → aggregate FP rate ~p^K. Is this assumption reasonable in
   practice for our rule set? What better-grounded aggregation
   schemes (log-odds, noisy-OR, calibrated stacking, Bayesian model
   averaging) should we evaluate? The audit defers Gaussian-mixture
   calibration to v2; is GMM appropriate, or are isotonic / Platt
   scaling better at our label volumes?

4. **The parameter-count question.** Across the proposed full
   engine, count the free parameters: per-rule weights, per-cluster
   posteriors, pair multipliers, surface thresholds, etc. With
   150-250k tokens per project and likely <500 explicit labels in
   the first year, are we in a regime where the framework can
   actually be calibrated, or are we kidding ourselves about
   identifiability? What's the responsible parameter budget at this
   data scale?

### B. Corpus-scale feasibility

5. Hapax-suspicion at 70% hapax types (Bemba, Rai). The current
   thinking is that lemma-cluster induction (audit §3.1) collapses
   surface variation enough to bring the effective hapax rate
   down. Is this a reasonable expectation? What does the
   morphology literature say about NT-scale unsupervised
   morphology on agglutinative low-resource languages?

6. The character-level Kneser-Ney perplexity signal (used for
   orthographic anomaly detection): defensible at this scale? What
   alternatives exist for character-level anomaly detection that
   don't require training (substring-novelty against the corpus,
   compression-based methods, etc.)?

7. **n-gram independence.** The author flagged that for
   agglutinative languages, even bigram and trigram counts are
   long-tailed enough that "rarity" loses meaning. Is this borne
   out empirically? What signal types actually survive at high
   morphological productivity, and which the engine currently
   relies on should be downweighted or dropped for those corpora?

8. Cross-project prior pooling for "universal" clusters. Which
   clusters are *actually* language-universal vs. script-universal
   vs. project-specific? The proposal posits punctuation conventions
   are script-universal — is that actually true across the
   languages translators handle (Latin, Cyrillic, Devanagari, Ge'ez,
   Arabic, Thai)? Where does it break?

### C. Architecture: aggregation & evidence flow

9. `evidence_layer_design.md` proposes per-rule cluster keys with a
   shared posterior store keyed by `(rule_id, cluster_key)`. Is
   there a cleaner factoring — e.g. shared latent-cluster models
   across rules, hierarchical mixture models, or graph-structured
   evidence propagation — that would be worth the additional
   complexity?

10. The proposal is to absorb the existing `ExceptionSet` (per-Sid
    rule-suppression config) as the dismiss channel for the
    Bayesian layer. Are there design pitfalls in collapsing an
    explicit suppression mechanism into evidence accumulation?
    What do other domains (search relevance, anti-spam, recommender
    systems) do here?

11. JSONL append-only event log as the persistence format. Is this
    the right choice given expected event volumes (estimate:
    100s/day during active translation, scaling to 100k+ over a
    project's life including git-history backfill)? Are there
    better-supported event-store patterns for this kind of small
    structured-event workload?

### D. Label sourcing & data plumbing

12. **Git history mining as label source.** The proposal is to
    diff a project's git history and extract form-level changes
    (punct adds/moves, casing fixes, Damerau-Levenshtein 1–2
    substitutions, transpositions) as implicit labels. Has this
    been done before in a translation-QA or text-correction
    context? What's the prior art on form-vs-content edit
    classification? What pitfalls should we expect (false-positive
    edits from stylistic preferences, cleanup churn, formatting
    refactors, etc.)?

13. **Edit-near-span attribution.** When an analyzer finding has
    a span and a subsequent edit overlaps that span, that's an
    implicit accept. Survey the active-learning and weak-supervision
    literature on attributing user actions to model predictions.
    What confidence calibration is reasonable here?

14. **Cross-project anonymized prior pooling.** Anything available
    on federated priors for small structured-event data, where the
    sharing model is opt-in and privacy-preserving? Bible
    translation organizations are a small enough community that
    even "anonymized" labels can be re-identifying.

### E. Interface / data collection

15. **The chicken-and-egg of label collection without UI.** The
    project's stance is that we will not build annotation UI early
    — instead, the CLI plus the user's existing git workflow plus
    edit-tracking will produce labels. Survey precedents: tools
    that successfully collected labels from non-ML-literate users
    via CLI / git-driven flows. Active-learning systems that
    avoided custom UI. Translation-memory and CAT tools that did
    or didn't crack this problem.

16. The user-facing diagnostic format — currently a list of findings
    with a one-sentence message and an evidence score per Sid. Is
    there interface-design work (HCI / explainable-AI / linguistic
    annotation tools) that informs how to present these findings
    to a translator non-expert in a way that encourages
    high-fidelity feedback?

17. **Translator workflow integration.** What translation tools
    are actually in use in low-resource Bible translation contexts
    today (Paratext, Bloom, Translation Studio, etc.)? Where would
    a tool like this fit, and what integration patterns have
    succeeded in adjacent space?

### F. Adjacent prior art the author may be missing

18. **Cross-domain connections.** What other software domains have
    solved analogous problems — small-corpus structured-text QA
    with online label feedback? Anti-spam systems pre-deep-learning
    are the obvious one (Bayesian classification with online
    updates). What others? OCR post-processing? Code-style linting
    with confidence scoring? Search-relevance with click-feedback?

19. **Specific tools and papers worth reading.** The audit covered
    SIL Machine and silnlp. What else should the author have read?
    Particularly:
    - Modern unsupervised morphological segmentation at low
      resource (post-Morfessor — Adaptor Grammars, MorphAGram, BPE
      variants, byte-level approaches).
    - Recent low-resource translation QA / quality estimation work
      that doesn't require an MT model.
    - Probabilistic programming approaches to multi-signal
      anomaly detection at small scale.
    - Anything from the digital-humanities corpus-error-detection
      literature.

## Constraints / non-negotiables

The agent should propose alternatives, but should be aware these are
hard constraints:

- **No reliance on LLMs or pre-trained large models** for the core
  engine. Suggesting "and then call GPT-4" is not a viable
  recommendation.
- **No reliance on hand-curated language-specific resources** (no
  per-language stopword lists, no dictionaries, no morphological
  paradigms). The engine targets a long tail of underserved
  languages where these resources do not exist.
- **Embeddable / runnable on modest hardware.** A laptop should be
  fine; a translator's workstation might be Chromebook-class.
  Ruling out anything that needs GPU.
- **Pre-alpha software, no users yet.** The architecture can change
  freely; backward compatibility is not a constraint. *Clean
  redesigns are preferred over compatibility shims.*

## What we want back

A research report (Markdown, ideally a few thousand words, sectioned
to mirror the questions above) that:

1. **Audits the existing direction.** What's sound, what's
   precarious, what's outright wrong. Be specific. Cite code paths
   and design-doc sections by reference.

2. **Recommends concrete adjustments**, ordered by leverage and
   cost. We will translate this into an implementation roadmap.

3. **Identifies the highest-impact pieces of prior art** the author
   should read before proceeding. Aim for a focused reading list
   (10–20 papers / tools, not 200).

4. **Calibrates expectations.** Given the parameter budget, label
   scarcity, and corpus scale, what should we realistically expect
   the engine to be able to do at year 1, year 2, year 5 of
   maturity? Where will the ceiling be, and is the ceiling worth
   the investment?

5. **Flags risks the author hasn't named.** Especially anything in
   the agglutinative-language space, anything around evaluation
   methodology (we have no held-out gold-standard data and may
   never), and anything around the social/organizational dynamics
   of label collection in a translation org.

6. **Names alternatives we may be undervaluing.** If a different
   class of approach (e.g., contrastive scoring against a
   neighboring translation, latent-variable models, conformal
   prediction) would dominate the current path at this scale, say
   so plainly.

## A specific hypothesis we'd like validated or refuted

**The author's current bet, in one sentence:** "A label-efficient
Bayesian aggregator over many small independent signals, bootstrapped
from cross-project priors and git-history mining, can achieve
useful precision-recall on form-level translation errors at New
Testament scale without any per-language model training or curated
linguistic resources."

We'd like the agent to either argue this is a defensible bet,
identify the specific points at which it's most likely to fail, and
quantify (where possible) the realistic ceiling — or argue the
opposite and propose a credibly better path.

## Note on tone

The author is asking for a real critique, not a politeness pass.
Bluntly identify weak reasoning. Push back on aesthetic decisions
that don't survive scrutiny. Recommend things the author won't want
to hear if you have evidence to support the recommendation. The
project will be better for it.
