//! Rule traits and the registries `analyze` runs.
//!
//! Three shapes, one merged `Finding` stream (ADR 0010, ADR 0017):
//!
//! - [`PerVerseRule`] decides from a single verse's text alone — the hot,
//!   stateless majority (whitespace, hygiene). It returns bare `Span`s;
//!   the runner stamps `sid` + `code` + `severity`.
//! - [`ProjectRule`] needs the whole corpus (and optionally a parallel
//!   `source` corpus) and emits full `Finding`s itself. Knob-bearing
//!   project rules are *constructed from* the caller's `Config` in
//!   [`project_rules`], so `check` stays a pure function of the maps.
//! - [`StatefulRule`] *observes* the corpus into `RuleStats`, then *judges*
//!   from that cache — the shape that supports incremental re-analysis
//!   (ADR 0017). Constructed from `Config` in [`stateful_rules`].
//!
//! Whether a rule is per-verse or project is the *rule's* property;
//! execution cadence (every keystroke vs on save) is the orchestrator's.
//! There is deliberately no hot/cold tier in the type system.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::config::Config;
use crate::corpus::{BookGroup, Books, Corpus, KeyIdx, LocalKeyIdx, SiteAddr};
use crate::diagnostics::{Finding, RuleId, Severity};
use crate::signals;
use crate::span::Span;
use crate::stats::RuleStats;
use crate::tape::{Mask, TapeEntry};
use crate::token::Token;

/// Per-verse word tokenizations, keyed by the current call's global
/// `KeyIdx`, computed once per analyze and shared by every token-consuming
/// rule so the corpus is tokenized a single time instead of once per rule
/// (the UAX #29 word scan is a top cost on space-free and non-Latin
/// scripts). Built only when ≥2 token consumers are enabled — see
/// `analyze_stateful`. Global, not local: this cache is rebuilt fresh every
/// call (never serialized, never retained across calls), so there is no
/// cross-call stability requirement to preserve by staying book-local.
/// FxHashMap: internal-only (never serialized, never crosses the wasm
/// boundary), fast non-cryptographic hashing on the hot per-book walk (ADR
/// 0057 allocation-diet follow-up).
pub type TokenCache = FxHashMap<KeyIdx, Vec<Token>>;

/// The hot, stateless majority. `check` reads the verse's prebuilt scalar tape
/// (ADR 0045) — one shared decode+classify pass the runner does per verse —
/// instead of each rule re-walking `text.char_indices()`. `text` rides along
/// for the handful of scans that are byte-level (tab, `?`-run, USFM/HTML
/// markers) or need `text.len()`. `pub(crate)`: the tape type is internal, and
/// no consumer outside the crate names this trait.
pub(crate) trait PerVerseRule: Sync {
    fn id(&self) -> RuleId;
    fn severity(&self) -> Severity;
    fn check(&self, text: &str, tape: &[TapeEntry]) -> Vec<Span>;
    /// The per-verse dirty-bits gate (ADR 0046): the runner skips `check` on a
    /// verse whose [`Mask`] does not open this gate. The gate must be a **safe
    /// superset** of the rule's fire set — set on every verse `check` could
    /// return a finding for. Defaults to all-pass (always run), so a rule with
    /// no cheap prefilter simply never gets skipped.
    fn gate(&self) -> Mask {
        Mask::ALL_PASS
    }
}

/// Project-scoped rules receive the corpus as [`Books`] — the same shared
/// grouping the stateful phase walks (ADR 0042), so book-independent passes
/// fan out through [`map_books`] and nobody regroups the corpus.
pub trait ProjectRule: Sync {
    fn id(&self) -> RuleId;
    fn check(&self, books: &Books<'_>, source: Option<&Corpus>) -> Vec<Finding>;
}

/// A project-scoped rule that also consults per-verse tokens (e.g.
/// cross-verse duplicate-word). Receives the shared [`TokenCache`] when one
/// was built; when `None` it tokenizes the verses it needs itself.
pub trait ProjectTokenRule: Sync {
    fn id(&self) -> RuleId;
    fn check(
        &self,
        books: &Books<'_>,
        source: Option<&Corpus>,
        tokens: Option<&TokenCache>,
    ) -> Vec<Finding>;
}

/// Candidate **sites** a stateful rule visited while counting — forwarded
/// from `reduce` to `judge` *within one analyze call* so a book scanned this
/// call is never scanned twice (ADR 0044). Strictly ephemeral: sites never
/// enter [`RuleStats`], never serialize, never outlive the call — the
/// aggregates-only wire contract (ADR 0017) is untouched.
///
/// Per rule, per book, the candidates in scan order. **A book's presence in
/// the map means "reduce scanned it this call"** — an empty list is a scanned
/// book with zero candidates (judge emits nothing for it, scan-free), while
/// an *absent* book was carried from the prior and judge must re-scan it for
/// spans. Proportionality carries no sites: its judge emits from cached
/// ratios and never scans.
pub enum RuleSites {
    Casing(BTreeMap<Box<str>, signals::casing::CasingSites>),
    Proportionality,
    PunctuationAdjacency(BTreeMap<Box<str>, Vec<SiteAddr>>),
    PunctuationSpacing(BTreeMap<Box<str>, Vec<signals::punctuation::SpacingSite>>),
    RepeatedCharacterRun(BTreeMap<Box<str>, Vec<SiteAddr>>),
    PunctOnlyToken(BTreeMap<Box<str>, Vec<SiteAddr>>),
    MixedScript(BTreeMap<Box<str>, Vec<signals::script_mixing::MixedScriptSite>>),
    /// `uni.rare-glyph` carries no sites: surviving candidates are ultra-rare, so
    /// its judge re-scans the supplied books (the `sites`-free path) rather than
    /// forward every letter occurrence (ADR 0044, ADR 0053).
    RareGlyph,
    /// `case.mixed-case-word` carries no sites: surviving candidates are rare, so
    /// its judge re-scans the supplied books (the `sites`-free path) rather than
    /// forward every OtherMixed occurrence (ADR 0044, ADR 0055).
    MixedCase,
}

/// Pair each packed pure-location site (adjacency / repeated-run /
/// punct-only) with its verse's text by direct indexing into the owning
/// `BookGroup` — a site's `LocalKeyIdx` **is** its position in `group.texts`,
/// so unlike the old `Sid`-sorted merge-walk this never needs to search.
/// `f(local, text, span)`.
pub(crate) fn for_each_site_text<'a>(
    group: &BookGroup<'a>,
    sites: &[SiteAddr],
    mut f: impl FnMut(LocalKeyIdx, &'a str, Span),
) {
    for &addr in sites {
        let (local, span) = addr.unpack();
        f(local, group.text(local), span);
    }
}

/// A rule that **observes** the corpus into `RuleStats`, then **judges** from
/// that corpus context (ADR 0017). `reduce` summarises the verses it is given;
/// the caller `merge`s that into prior stats. Aggregate-only rules may re-scan
/// the supplied verses to recover spans without storing sites. Core stays pure
/// — the stats live in the caller, not the rule.
///
/// Both phases receive the corpus as [`Books`] — grouped once by
/// `analyze_stateful` and shared — because the **book is the unit of
/// everything** here (ADR 0042): stats supersede per book, casing's scan
/// carries sentence state across verse seams within a book, and the
/// `parallel` feature fans work out per book via [`map_books`]. `tokens` is
/// the shared per-analyze [`TokenCache`] when one was built; a rule that
/// tokenizes (repeated-character-run) reads it instead of re-tokenizing, and
/// every other rule ignores it.
pub trait StatefulRule: Sync {
    fn id(&self) -> RuleId;
    /// Count the supplied verses into this rule's stats, and hand back the
    /// candidate [`RuleSites`] visited along the way (ADR 0044) so a
    /// same-call `judge` can skip re-scanning those books.
    fn reduce(
        &self,
        books: &Books<'_>,
        source: Option<&Corpus>,
        tokens: Option<&TokenCache>,
    ) -> (RuleStats, RuleSites);
    /// Emit findings from the merged corpus `stats`. `books` holds the verses
    /// of the current call — a rule whose observations are *sparse* ignores it
    /// and emits from cached sites (proportionality); a rule with a *dense*
    /// candidate class caches only aggregates and recovers spans here: from
    /// the forwarded `sites` for books reduce scanned this call, by
    /// **re-scanning** any other supplied book (its counts came from the
    /// prior, so no sites exist). `sites = None` (or a mismatched variant)
    /// re-scans everything — always correct, never required in the orchestrated
    /// path. Emissions are for the supplied verses either way.
    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        tokens: Option<&TokenCache>,
        sites: Option<&RuleSites>,
    ) -> Vec<Finding>;
}

/// Run `f` over every book and collect the outputs **in `books`' presented
/// order** (index-aligned with `books`, which is caller order, not canonical
/// book order — see `Corpus`). Under the `parallel` feature the books fan out
/// over rayon (ADR 0042); the output is identical either way — an indexed
/// collect preserves input order, and books are disjoint — so the feature can
/// never change results, only wall-clock. This is the *one* place the
/// stateful phase's parallelism lives; rules call it and stay `cfg`-free.
pub(crate) fn map_books<T: Send>(
    books: &Books<'_>,
    f: impl Fn(&BookGroup<'_>) -> T + Sync,
) -> Vec<T> {
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        books.par_iter().map(&f).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        books.iter().map(&f).collect()
    }
}

/// Every per-verse rule wired in. The registry is complete — including
/// rules `Config::v1_defaults` disables by default — so an explicit
/// enable in config is all it takes to run one.
pub(crate) fn per_verse_rules() -> Vec<Box<dyn PerVerseRule>> {
    vec![
        Box::new(signals::whitespace::ExcessHWhitespace),
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
        Box::new(signals::hygiene::InvalidCodepoint),
        Box::new(signals::hygiene::ReplacementRun),
        Box::new(signals::hygiene::CombiningMarkWithoutBase),
        Box::new(signals::hygiene::MixedNumeralSystems),
        Box::new(signals::zero_width_space::RedundantZeroWidthSpace),
        Box::new(signals::structural::SourceMarkerLeftover),
        Box::new(signals::structural::MergeConflictMarker),
    ]
}

/// Every project-scoped rule wired in by default. Knob-bearing rules are
/// constructed from `config`'s typed sub-configs here, once per analyze
/// call — `ProjectRule::check` itself never sees the `Config`.
pub fn project_rules(config: &Config) -> Vec<Box<dyn ProjectRule>> {
    vec![Box::new(signals::bracket_balance::BracketBalance {
        cfg: config.bracket_balance,
    })]
}

/// Project-scoped rules that also consult per-verse tokens.
pub fn project_token_rules() -> Vec<Box<dyn ProjectTokenRule>> {
    vec![Box::new(signals::lexical::DuplicateWord)]
}

/// Every stateful (observe-then-judge) rule wired in, constructed from
/// `config`'s typed sub-configs (ADR 0017). Like the project registry, this
/// is complete — including rules `v1_defaults` disables.
pub fn stateful_rules(config: &Config) -> Vec<Box<dyn StatefulRule>> {
    vec![
        Box::new(signals::casing::SentenceInitialLowercase { cfg: config.casing }),
        Box::new(signals::casing::InconsistentWordCasing { cfg: config.casing }),
        Box::new(signals::proportionality::ProjectLengthRatio {
            cfg: config.proportionality,
        }),
        // Corpus-relative punctuation rules, both aggregate-only stateful.
        Box::new(signals::punctuation::PunctuationAdjacencyAnomaly {
            cfg: config.punctuation_adjacency,
        }),
        Box::new(signals::punctuation::PunctuationSpacingAnomaly {
            cfg: config.punctuation_spacing,
        }),
        Box::new(signals::lexical::RepeatedCharacterRun {
            cfg: config.repeated_character_run,
        }),
        Box::new(signals::lexical::PunctOnlyToken {
            cfg: config.punct_only_token,
        }),
        Box::new(signals::script_mixing::MixedScriptInToken {
            cfg: config.mixed_script,
        }),
        Box::new(signals::rare_glyph::RareGlyph {
            cfg: config.rare_glyph,
        }),
        Box::new(signals::mixed_case::MixedCaseWord {
            cfg: config.mixed_case,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The safety property the whole prefilter rests on (ADR 0046, ported from
    /// the spike's corpus-wide assertion): every per-verse rule's gate is a
    /// **safe superset** of its fire set — on any verse where `check` returns a
    /// finding, the verse's dirty-bits mask opens that rule's gate. If this
    /// held only on clean corpora the prefilter could silently drop findings;
    /// these synthetic verses fire *every* rule at least once (asserted below).
    #[test]
    fn every_gate_is_a_safe_superset_of_its_fire_set() {
        // A battery that fires all twelve rules plus clean / adjacent cases.
        let verses = [
            "",
            "   ",
            "In the beginning God created the heavens.",
            "मन ने कहा। हाँ भई हाँ।",
            "a  b",                                  // excess whitespace
            "End.  Next",                            // protected (no fire) but EXCESS_WS set
            "a\u{00A0}\u{00A0}b",                    // NBSP run
            "foo\tbar",                              // tab
            "foo\u{0007}bar\u{0085}baz",             // C0 + C1 controls
            "a\u{FEFF}b\u{2060}c\u{202E}d",          // zero-width / format
            "god\u{FFFD}\u{FDD0}\u{FFFE}x",          // invalid codepoints
            "a\u{1FFFE}b",                           // astral noncharacter
            "word ????? end",                        // ?×5
            "\u{0301}abc word.\u{0301} x",           // baseless marks
            "12 men and ४५ women",                   // mixed numerals
            "a\u{200B}\u{200B}b c\u{200B}\u{200B}d", // doubled ZWSP runs
            r"In the \v 2 \add beginning\add*",
            "a <b>bold</b> <br/> word",
            "<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> z",
            "||||||| base",
            "5 < 7 and 7 > 5", // lone comparisons (no conflict fire)
            "what?? really",   // ?×2 (no replacement fire)
        ];
        let rules = per_verse_rules();
        let mut tape = Vec::new();
        // Which rules actually fired somewhere — to prove the battery is real.
        let mut fired_any = vec![false; rules.len()];
        for text in verses {
            let mask = crate::tape::build_masked(text, &mut tape);
            for (i, r) in rules.iter().enumerate() {
                let fires = !r.check(text, &tape).is_empty();
                if fires {
                    fired_any[i] = true;
                    assert!(
                        mask.opens(r.gate()),
                        "{:?} fired on {text:?} but its gate stayed closed",
                        r.id()
                    );
                }
            }
        }
        for (i, r) in rules.iter().enumerate() {
            assert!(
                fired_any[i],
                "battery never fired {:?} — test is vacuous for it",
                r.id()
            );
        }
    }
}
