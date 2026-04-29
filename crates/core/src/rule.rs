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
//! Still leaning γ: each rule emits an optional `evidence_score` along
//! with its `Finding`s, a meta-pass fuses correlated findings, the UI
//! sees a single per-Sid sigmoid score. Touches the `Finding` shape
//! when it lands. Defer until ~5 statistical rules exist and we can
//! see what their evidence streams actually look like.
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

use crate::diagnostics::{Finding, RuleId};
use crate::project::Project;
use crate::signals;

/// A single signal. Implementations are typically zero-sized unit
/// structs (hygiene, simple statistical rules) or small structs
/// holding precomputed state (eventually).
pub trait Rule: Sync {
    fn id(&self) -> RuleId;
    fn check<'src>(&self, project: &'src Project<'src>) -> Vec<Finding<'src>>;
}

/// All rules wired in by default. The dogfood CLI's config can disable
/// individual rules; this list is the universe of what's available.
pub fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
    ]
}
