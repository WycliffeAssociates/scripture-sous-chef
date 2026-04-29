//! Design notes for the `Rule` trait. *No trait yet* — the shape is open
//! until we have two or three signals partially implemented and can see
//! what they actually need. Listing options here so the decision is in
//! one place.
//!
//! ## What every rule needs to do
//!
//! 1. Identify itself (`RuleId`).
//! 2. Read its config (enabled flag, thresholds — config is *override*,
//!    not the primary configuration surface; see `signals::hygiene` for
//!    the statistical-first / config-as-override policy).
//! 3. Examine the project (target verses; sometimes source verses;
//!    sometimes corpus-wide aggregates computed in a prior pass;
//!    sometimes the discourse-level view from `crate::discourse`).
//! 4. Emit zero or more `Finding`s with spans into either `Verse.nfc`
//!    or (mapped back through) the discourse stream.
//! 5. Skip findings whose `(rule_id, sid)` is in the `ExceptionSet`.
//!
//! ## Candidate trait shapes
//!
//! ### A. Stateless function pointers
//! ```ignore
//! pub type RuleFn = for<'a> fn(&'a Project<'a>, &RuleConfig) -> Vec<Finding<'a>>;
//! ```
//! Pros: dead simple, easy to register in a static table.
//! Cons: a `fn` pointer has no `self`, so any per-rule precomputed
//! state (a trained char-LM, a glossary index) has to live somewhere
//! outside the function and be threaded in through arguments. That's
//! doable, but you end up reinventing `&self` by hand: a registry of
//! per-rule state keyed by `RuleId`, looked up at call time. Workable
//! but ugly.
//!
//! ### B. Trait with a `prepare` step
//! ```ignore
//! pub trait Rule {
//!     fn id(&self) -> RuleId;
//!     fn prepare(&mut self, project: &Project) {} // optional precompute
//!     fn check<'a>(&self, project: &'a Project<'a>) -> Vec<Finding<'a>>;
//! }
//! ```
//! Pros: per-rule state lives in the impl struct. Clean.
//! Cons: dyn-dispatch + boxing; lifetimes get hairy with `&mut self`.
//!
//! ### C. Two-phase: corpus stats first, then rules
//! Engine builds `CorpusStats` (token freqs, char-LM, hapax sets) once,
//! every rule receives `&Project, &CorpusStats, &RuleConfig`. Rules stay
//! stateless; expensive shared work is hoisted into one obvious place.
//!
//! Currently leaning **C** — most rules read from the same handful of
//! aggregates, and "compute every aggregate once, pass it to every
//! rule" matches how the calibration profiler already works. Decide
//! for real once `analysis::dunning` is implemented and we can see
//! what one full signal looks like.
//!
//! TODO: pick A/B/C after the first signal lands. Then write the
//! trait. Then refactor the signal to fit.
//!
//! ## Hygiene vs. statistical signals
//!
//! Two architecturally distinct rule populations:
//!
//! - **Hygiene** (`signals::hygiene`): truly invariant patterns —
//!   never legitimate regardless of corpus or language. Tab in body,
//!   C0/C1 control chars, ZWSP misuse in scripts that don't use
//!   joiners. Fixed severity, no statistics. Bar for inclusion is
//!   high: if there's any plausible language where the pattern is
//!   fine, it doesn't belong here.
//! - **Statistical** (everything else): convention is *observed* from
//!   the corpus, with config as override. Threshold derived from
//!   corpus shape (sigmoid-weighted by morphology / orthographic
//!   complexity per METHODS.md §5.9.2). Need `analysis::*` primitives
//!   and a precomputed corpus-stats aggregate.
//!
//! The motivation for keeping config as *override* and not the primary
//! surface is the user population: field translators are not
//! tech-literate and shouldn't have to set 10K levers to get good
//! defaults. The engine should observe "this corpus uses single
//! spaces" and act accordingly; config exists for the ambiguous /
//! corrupted-corpus cases.
//!
//! ## Score combination — leaning γ
//!
//! METHODS.md calibrates each statistical signal *independently*. It
//! does NOT specify how findings combine into a per-verse confidence
//! score. Three options:
//!
//! ### α. Independent findings, no combination
//! Each finding stands alone with its own severity. UI groups by Sid
//! and stacks them. Simplest; loses cross-rule reinforcement.
//!
//! ### β. Per-verse aggregate score
//! Sum or max-pool findings' severities into a single `verse_score`.
//! Risks double-counting correlated rules (proportionality +
//! length-anomaly often co-fire).
//!
//! ### γ. Two-stage with cross-rule escalation *(leaning this)*
//! Pass 1: every rule emits `Finding` with its own severity, *plus* an
//! optional `evidence_score: f32` in `[0, 1]`. Pass 2: a small set of
//! "meta rules" reads the evidence stream and escalates Info → Warn →
//! Error when independent rules co-fire on the same Sid (e.g.
//! proportionality + char-LM-surprisal both firing upgrades both to
//! Warn). The source-relative "upgrade/downgrade only" policy is
//! exactly this shape.
//!
//! Why γ for v1: data sparsity. NT-sized corpora produce noisy
//! single-rule signals; multiple co-firing rules push noise down and
//! catch genuinely-suspicious "never a word" type cases that any one
//! rule would miss.
//!
//! γ also opens the door to a **single combined sigmoid score** per
//! Sid for UI. Once every rule emits `evidence_score`, the meta-pass
//! can fuse them (logistic regression with hand-set or
//! corpus-fit weights) into one [0, 1] number per Sid. The UI then
//! becomes "drag a threshold slider, see all verses above the line"
//! instead of "configure 20 rules' severities individually." That's
//! the right ergonomic shape for the user population.
//!
//! TODO: revisit after `pos.sentence-start-case`, `src.proportionality`,
//! and one hygiene rule are implemented. γ is leaning hard but defer
//! the wire-up until we have real findings to fuse.
