//! The `Rule` trait. Picked after 4 hygiene rules landed as free
//! functions and the shape they wanted became obvious. See git history
//! for the candidate-comparison design notes.
//!
//! ## The trait
//!
//! ```ignore
//! pub trait Rule: Sync {
//!     fn id(&self) -> RuleId;
//!     fn check<'src>(&self, project: &'src Project<'src>) -> Vec<Finding<'src>>;
//! }
//! ```
//!
//! The trait takes the whole `Project`, not a single `Verse`. Three
//! reasons:
//!
//! 1. **Discourse-level rules need cross-verse access.** Sentence-start
//!    capitalisation, paired-punctuation balance, etc. cannot be
//!    expressed by a per-verse function.
//! 2. **Source-relative rules need both corpora.** Per-verse signature
//!    would force every source-relative rule to thread the source
//!    corpus through some side channel.
//! 3. **Iteration shape varies.** Hygiene rules iterate every verse;
//!    proportionality iterates `source ∩ target`; sentence-start
//!    iterates the discourse stream. Putting iteration in the rule
//!    rather than the engine means each rule expresses what it actually
//!    needs.
//!
//! Per-verse hygiene rules pay for this with a one-line `flat_map`
//! over `project.target.verses.values()` — cheap.
//!
//! ## Stateful rules (later)
//!
//! Char-LM and glossary rules need precomputed state. Three options:
//!
//! - Lazy state inside the rule struct (e.g. `OnceCell<CharLm>` populated
//!   on first `check`).
//! - Add an optional `prepare(&mut self, project: &Project)` method.
//! - Two-phase: engine builds a shared `CorpusStats` aggregate, passes
//!   it to every `check`.
//!
//! Defer until a stateful rule actually exists. The current trait
//! accommodates all three: option 1 needs no change; options 2/3 are
//! additive.
//!
//! ## Score combination (γ from the prior design notes)
//!
//! **Rules emit independent, equal-weight ticks.** No rule consults
//! another rule's signal in its own logic. If two rules should be
//! "always considered together," that policy lives in the aggregator,
//! not in either rule's body or signature — otherwise rule-to-rule
//! coupling spreads through the engine and there's no single place to
//! reason about it.
//!
//! **The aggregation layer (deferred).** Runs after all rules
//! complete. Groups findings by `Sid` *and* by byte-range proximity
//! within a Sid — many rule pairs naturally fire at adjacent
//! positions on the same boundary (e.g. a never-terminal word at
//! offset N and a missing-capitalisation finding at offset N+k), and
//! that adjacency is the signal. Per cluster: tick count, set of
//! rule IDs that fired, any policy-derived confidence boost.
//!
//! **Policy as data, not code.** Correlated-rule pairs / triples are
//! a declarative table — start as a const in `core`, lift to config
//! when worth tuning. Rules don't know they're in the table.
//!
//! **Two tiers of evidence weight.** Some rules fire on findings
//! that are *intrinsically* high-confidence regardless of
//! corroboration: hygiene-class anomalies (merge-conflict markers
//! `>>>>>>> HEAD`, keyboard-vomit runs, abnormally long tokens, all-
//! hapax sequences, gross duplications). Those should rise to the
//! top of the ranking even when they're a single tick. Statistical
//! rules (sparse-data convention learners like
//! `pos.unexpected-sentence-end`) need fusion to be confident — a
//! single tick is weak signal. The aggregator distinguishes these
//! tiers; the simplest first cut is per-rule intrinsic-weight as
//! data alongside the correlation policy.
//!
//! Defer the implementation until ~5 statistical rules exist and we
//! can calibrate against real findings. The current shape is
//! forward-compatible: `Finding` stays the same, `AnalyzeStats`
//! grows a sibling field, no rule signatures change.
//!
//! ## Parallelism (forward-compatibility note)
//!
//! Three layers that may eventually want parallelism, plus what we've
//! already committed to so they remain possible:
//!
//! 1. **File-level ingest** (read USFM file → parse → NFC →
//!    segmentation). Lives in `scc-ingest` and `scc-core::verse`. Every
//!    file is independent; every verse is independent for NFC and
//!    segmentation. Trivially `par_iter`-able. No API impact — the
//!    output is still `BTreeMap<Sid, Verse>`. Will be a `parallel`
//!    Cargo feature in `scc-core` (default off, so WASM and minimal
//!    builds stay clean) when we measure that sequential is the
//!    bottleneck.
//!
//! 2. **Corpus-stats build** (n-gram counts, hapax sets, char-LM
//!    training). When these land they'll live in a `CorpusStats` type
//!    in core, computed via map-reduce: each verse contributes counts
//!    in parallel, a reduce step merges. **Design note:** keep
//!    `CorpusStats` aggregable — `HashMap<K, u64>` with a `merge`
//!    method — so the build path can be sequential *or* parallel
//!    without changing the rest of the engine. The rules don't care
//!    which.
//!
//! 3. **Rule-level parallelism**. The engine can dispatch
//!    `Rule::check` calls across rayon threads because of the `Sync`
//!    supertrait below. `Vec<Finding>` per rule, merge at the end. A
//!    rule that internally wants to parallelise its verse iteration
//!    can do so within its `check` body — that's its call, not the
//!    engine's.
//!
//!    **Caveat:** the `stats: &mut AnalyzeStats` argument is
//!    sequential-only. When parallel dispatch lands, the trait shape
//!    flips so each rule returns its stats contribution and the
//!    engine merges after fork-join. The `AnalyzeStats` *struct*
//!    survives that change; only `Rule::check`'s signature flips.
//!
//! ### What `Rule: Sync` commits us to
//!
//! Don't put non-`Sync` state in a rule struct. No `Rc`, no `Cell`,
//! no `RefCell`, no `ThreadRng`. If a rule needs interior mutability
//! (lazy state), use `OnceLock` or `Mutex<…>`, both of which are
//! `Sync`. Same for any future `CorpusStats`: must be `Send + Sync`.
//!
//! `Project`, `NamedCorpus`, `Verse`, `Sid`, `Finding`, `Diagnostics`
//! are already `Send + Sync` (only owned `String`s, `Vec`s, `BTreeMap`s,
//! `Copy` types). Don't introduce non-thread-safe types into them.
//!
//! The same shape applies to corpus-derived caches like `Lexicon` —
//! build once in the engine when a second rule needs it, threaded
//! through `Project` or a future `AnalysisContext` (METHODS.md §5.6).

use crate::context::AnalysisContext;
use crate::diagnostics::{AnalyzeStats, Finding, RuleId};
use crate::project::Project;
use crate::signals;

/// A single signal. Implementations are typically zero-sized unit
/// structs (hygiene, simple statistical rules) or small structs
/// holding precomputed state (eventually).
///
/// `stats` is a shared sink. Stat-bearing rules write into their own
/// named slot on `AnalyzeStats`; hygiene rules ignore it. By
/// convention a rule writes ONLY its own slot — nothing in the type
/// system stops a misbehaving rule from stomping someone else's.
pub trait Rule: Sync {
    fn id(&self) -> RuleId;
    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>>;
}

/// All rules wired in by default. The dogfood CLI's config can disable
/// individual rules; this list is the universe of what's available.
pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // Hygiene
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
        // Source-relative
        Box::new(signals::source_relative::Proportionality),
        // Positional / discourse
        Box::new(signals::positional::SentenceStartCase),
        Box::new(signals::positional::UnexpectedSentenceEnd),
        Box::new(signals::punctuation::PairedPunctBalance),
    ]
}
