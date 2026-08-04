//! `uni.nonletter-usage-anomaly` — **Unusual nonletter usage**.
//!
//! One convention-learned rule over every *visible nonalphabetic* extended
//! grapheme cluster: punctuation, quotes, symbols, digits, emoji. It replaces
//! `punct.spacing-anomaly`, `punct.adjacency-anomaly` and `lex.punct-only-token`,
//! whose three narrow candidate domains and incompatible scorers left `mov$ing`,
//! a lone `~`, `th3e` and `wo"rd` unobservable while disagreeing with one another
//! about the same convention.
//!
//! Calibrated over the 1,504-corpus VREF fleet; the packet, the falsified models,
//! the adjudicated knobs and the old/new overlap ledger are in
//! `documentation/calibration/2026-08-04-nonletter-usage-probe.md`.
//!
//! ## Claim
//!
//! **Observes:** a visible nonalphabetic grapheme cluster occurs with a corpus
//! count, a logical start/end attachment, a bounded outer attachment topology, and
//! directed adjacent-nonletter relationships.
//!
//! **May infer:** this occurrence is an unusual use of a visible nonletter
//! relative to *this translation's own observed conventions*, and is worth review.
//!
//! **Does not establish:** that the grapheme is invalid, misspelled, semantically
//! wrong, an unmatched quote/bracket, or universally misplaced.
//!
//! **Legitimate counterexamples:** medial `*` as an orthographic convention;
//! Ethiopic doubled punctuation; a quote serving both roles; numeric grouping;
//! superscript numerals; a deliberately detached sentence mark; emoji.
//!
//! ## Three independently sufficient channels, composed with `max`
//!
//! ```text
//! score = max(absolute_rarity, placement_anomaly, sequence_anomaly)
//!
//! placement_anomaly = max(start_anomaly, end_anomaly, topology_anomaly)
//! sequence_anomaly  = max(directed_pair_anomaly, same_glyph_continuation)
//! ```
//!
//! `max`, never noisy-OR: the channels describe one correlated occurrence, so any
//! one is a sufficient reason to review and overlapping reasons must not
//! manufacture confidence. A channel without enough support **abstains**, and an
//! abstention is not a zero that cancels another channel.
//!
//! Every rate an occurrence is judged against is **leave-one-out**: the thing
//! under judgment is removed from both numerator and denominator, so nothing
//! licenses itself at `1/1`. A single medial `*` therefore makes placement
//! *abstain* rather than conclude that medial `*` is the corpus's convention, and
//! rarity carries the finding instead.
//!
//! ## Discourse, and the one legitimate seam effect
//!
//! Verse and chapter markers are reference plumbing (repo `CLAUDE.md`): discourse
//! flows across them and resets only at a **book** boundary. The single seam
//! effect is glyph adjacency — a mark opening verse N is not *attached* to the last
//! letter of verse N−1 — so a seam reads as **spaced continuity**. Three
//! consequences shape this substrate:
//!
//! 1. a nonletter run never spans a seam, because a seam is a spaced break;
//! 2. the only context a chapter cannot resolve alone is the outer context of a
//!    run touching its **first** or **last** grapheme;
//! 3. and that context is `Spaced` whenever a neighbouring chapter exists in the
//!    book, `Boundary` (abstain) only at a true **book** edge — the previous
//!    chapter's *content* never matters, only its existence.
//!
//! So mapping is predecessor-free and marks at most two edges
//! [`NeighbourClass::Deferred`]; ordered reduction resolves the leading edge from
//! [`NonletterBoundary::seen_previous`] and routes the trailing edge's resolution
//! into its owning chapter through the driver's `carry_out`.
//!
//! `seen_previous` is explicit and must **not** be inferred from an empty pending
//! slot: a chapter containing no candidate at all carries no pending tail yet
//! still proves a neighbour exists, and inferring would silently resolve the next
//! chapter's leading edge as `Boundary` — visible only in corpora that happen to
//! hold a punctuation-free chapter.
//!
//! ## Ownership at an exact span
//!
//! Controls, zero-width/format hazards, invalid code points **and a combining mark
//! with no base** are excluded from candidacy, so deterministic hygiene and this
//! rule can never both own a span. `punct.bracket-balance` keeps established
//! structural violations; `uni.mixed-normalization` keeps equivalence claims
//! (identity here is exact raw grapheme bytes, so two normalization-equivalent
//! forms are two identities); `uni.rare-glyph` stays the Unicode **Letter** lane;
//! the census stays exhaustive and knob-free. There is no generic span deduper.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::charclass::class_of;
use crate::config::NonletterUsageConfig;
use crate::corpus::{Corpus, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{Finding, FindingArgs, NonletterForm, NonletterReason, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::grapheme::{self, GSpan};
use crate::span::Span;

pub const NONLETTER_USAGE_ANOMALY: RuleId = RuleId::NonletterUsageAnomaly;

// ───────────────────────────────────────────────────────────────────────────
// Classification
// ───────────────────────────────────────────────────────────────────────────

/// What one grapheme cluster is, for this rule's purposes. Classified by the
/// cluster's **base** scalar, so a cluster with an alphabetic base is context and
/// its combining marks stay part of it — never candidates of their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Alphabetic base — context, never a candidate.
    Alpha,
    /// Whitespace — context, and what makes a neighbour "spaced".
    Space,
    /// Deterministic hygiene's domain: control, zero-width/format, invalid code
    /// point. Excluded from candidacy so the two rules cannot both own a span.
    Hygiene,
    /// A combining mark with no alphabetic base — `uni.combining-mark-without-base`
    /// owns it, so it is excluded from candidacy too.
    BaselessMark,
    /// A visible nonalphabetic grapheme: the candidate domain.
    Candidate(CandClass),
}

/// A candidate's fine class. Only the **Nd** distinction is load-bearing
/// (calibration addendum §B1/§B2):
///
/// - which numbers a translation happens to write is compositional, not an
///   orthographic convention, so Nd digits pool into one participant for the
///   directed-pair channel and read a pooled class run count for rarity;
/// - **No**/**Nl** (`²`, `½`, Roman numerals) deliberately do **not** pool: a
///   superscript numeral is a glyph choice, and an odd numeral appearing once is
///   exactly the rare-identity case this rule exists to surface.
///
/// The probe found a real defect here: `is_numeric()` is the fused `NUMERIC` bit
/// covering all of **N\***, so classifying on it pooled `²` into the digit
/// participant and cost it both its own identity and its ability to fire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CandClass {
    /// Unicode **Nd** only.
    Digit,
    /// **No** / **Nl**.
    Numeral,
    Quote,
    Punct,
    Symbol,
    Other,
}

/// Classify one grapheme cluster by its base scalar.
fn classify(cluster: &str) -> Kind {
    let Some(base) = cluster.chars().next() else {
        return Kind::Space;
    };
    let cl = class_of(base);
    if cl.is_control() || cl.is_zero_width_format() || cl.is_invalid_codepoint() {
        return Kind::Hygiene;
    }
    if cl.is_whitespace() {
        return Kind::Space;
    }
    if cl.is_alphabetic() {
        return Kind::Alpha;
    }
    if cl.is_mark() {
        return Kind::BaselessMark;
    }
    Kind::Candidate(if cl.is_quote() {
        CandClass::Quote
    } else if cl.is_decimal_digit() {
        // Nd ONLY — see `CandClass::Numeral`.
        CandClass::Digit
    } else if cl.is_numeric() {
        CandClass::Numeral
    } else if cl.is_punctuation() {
        CandClass::Punct
    } else if cl.is_symbol() {
        CandClass::Symbol
    } else {
        CandClass::Other
    })
}

/// The candidate class of a grapheme known to be a candidate.
fn cand_class(cluster: &str) -> CandClass {
    match classify(cluster) {
        Kind::Candidate(c) => c,
        // A retained run and a judge key hold only candidates; anything else here
        // would mean a span and the text it addresses came from different content.
        _ => unreachable!("expected a candidate grapheme"),
    }
}

/// The pooled directed-pair participant every **Nd** digit collapses to.
///
/// A `\u{1}`-prefixed sentinel rather than a bare `#`: a control scalar classifies
/// as [`Kind::Hygiene`] and can therefore never be a candidate identity, so the
/// pooled key cannot collide with a literal `#` written in the text. (The
/// calibration probe used a bare `#`, which collides wherever a corpus writes a
/// literal `#` beside another nonletter — rare, and strictly a defect.)
const DIGIT_POOL_KEY: &str = "\u{1}#";

/// The pair participant a grapheme collapses to: every **Nd** digit becomes one
/// participant, everything else keeps its exact bytes.
fn pool_key(glyph: &str, class: CandClass) -> &str {
    match class {
        CandClass::Digit => DIGIT_POOL_KEY,
        _ => glyph,
    }
}

/// The outer content class on one logical side of a candidate. **Logical**
/// start/end, never visual left/right (plan §0.6), so findings do not move when
/// text direction does.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub(crate) enum NeighbourClass {
    /// Directly attached to an alphabetic grapheme.
    Letter,
    /// Directly attached to a digit.
    Digit,
    /// Whitespace on this side, or a verse/chapter seam (which reads as spaced).
    Spaced,
    /// The interior of a nonletter run — no outer context. Abstains: excluded from
    /// the side's denominator entirely, which is what stops `word."` from
    /// manufacturing two misleading medial topologies while leaving an isolated
    /// `wo"rd` as `Both`.
    Internal,
    /// A book edge with no neighbour across the seam. Abstains.
    #[default]
    Boundary,
    /// The chapter could not resolve this side: it is the chapter's first or last
    /// grapheme. Present **only** inside a chapter observation's sites, never in a
    /// tally; ordered reduction replaces it with `Spaced` or `Boundary`.
    Deferred,
}

/// The number of *observable* neighbour classes — the width of both side marginal
/// tables.
const OBSERVABLE: usize = 3;

impl NeighbourClass {
    /// Whether this side counts as attached to CONTENT — the input to topology.
    fn attached(self) -> bool {
        matches!(self, Self::Letter | Self::Digit)
    }

    /// Whether this side carries a judgeable observation at all.
    fn observable(self) -> bool {
        matches!(self, Self::Letter | Self::Digit | Self::Spaced)
    }

    /// This class's slot in a side marginal table, for an observable class.
    fn slot(self) -> Option<usize> {
        match self {
            Self::Letter => Some(0),
            Self::Digit => Some(1),
            Self::Spaced => Some(2),
            _ => None,
        }
    }

    /// Packed 3-bit code, for a retained run site's outer byte.
    fn code(self) -> u8 {
        match self {
            Self::Letter => 0,
            Self::Digit => 1,
            Self::Spaced => 2,
            Self::Internal => 3,
            Self::Boundary => 4,
            Self::Deferred => 5,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Letter,
            1 => Self::Digit,
            2 => Self::Spaced,
            3 => Self::Internal,
            4 => Self::Boundary,
            _ => Self::Deferred,
        }
    }

    /// The published form name for a side marginal finding.
    fn form(self) -> NonletterForm {
        match self {
            Self::Letter => NonletterForm::Letter,
            Self::Digit => NonletterForm::Digit,
            _ => NonletterForm::Spaced,
        }
    }
}

/// The bounded four-state outer attachment topology (settled decision, plan §0.6).
/// Load-bearing for direction-ambiguous marks: a straight `"` is commonly
/// `EndOnly` opening and `StartOnly` closing, so **both** side marginals look
/// ordinary while `wo"rd`'s `Both` stays rare — without deciding whether the quote
/// opened or closed anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Topology {
    Neither,
    StartOnly,
    EndOnly,
    Both,
}

const TOPOLOGIES: usize = 4;

/// The **coarse outer content class** the four-state topology tally is conditioned
/// on, matching `punct.spacing-anomaly`'s precedent: a mark's binary was judged
/// against the pool of its own neighbour-content class, never against every
/// occurrence of the mark. Three closed values, deliberately coarser than the fine
/// [`NeighbourClass`] the raw observation retains.
///
/// Derived from the occurrence's outer sides jointly, because topology is a joint
/// statement: `Letter` when either side touches a letter, `Digit` when neither does
/// but one touches a digit, `Detached` when neither side touches content at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TopoClass {
    Letter,
    Digit,
    Detached,
}

const TOPO_CLASSES: usize = 3;

impl TopoClass {
    fn of(start: NeighbourClass, end: NeighbourClass) -> Self {
        if start == NeighbourClass::Letter || end == NeighbourClass::Letter {
            Self::Letter
        } else if start == NeighbourClass::Digit || end == NeighbourClass::Digit {
            Self::Digit
        } else {
            Self::Detached
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// The conditioned topology cell an occurrence falls in: `class · TOPOLOGIES +
/// state`. The one place the two axes are combined, so the map, the book fold and
/// the judge cannot disagree about the layout.
fn topo_cell(class: TopoClass, t: Topology) -> usize {
    class.slot() * TOPOLOGIES + t.slot()
}

impl Topology {
    /// The four-state topology, or `None` when the candidate has no outer context
    /// on EITHER side.
    ///
    /// Collapsing that case into `Neither` was **falsified** by the probe:
    /// `Neither` then meant both "detached from content on both sides" (` , ` — the
    /// classic orphaned mark) and "surrounded by other nonletters" (`?!"`'s `!`),
    /// which have different priors. Pooled together, an interior occurrence of a
    /// glyph that normally sits at a run edge read as a unique topology and fired
    /// at 0-of-1,601. An interior side already abstains on the per-side marginals;
    /// topology abstains for the same reason when BOTH sides do.
    fn of(start: NeighbourClass, end: NeighbourClass) -> Option<Self> {
        if !start.observable() && !end.observable() {
            return None;
        }
        Some(match (start.attached(), end.attached()) {
            (false, false) => Self::Neither,
            (true, false) => Self::StartOnly,
            (false, true) => Self::EndOnly,
            (true, true) => Self::Both,
        })
    }

    fn slot(self) -> usize {
        self as usize
    }

    fn form(self) -> NonletterForm {
        match self {
            Self::Neither => NonletterForm::Neither,
            Self::StartOnly => NonletterForm::StartOnly,
            Self::EndOnly => NonletterForm::EndOnly,
            Self::Both => NonletterForm::Both,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// The retained observation
// ───────────────────────────────────────────────────────────────────────────

/// Same-glyph run-length histogram width: index `n-1` counts runs of exactly `n`
/// of the identity, capped at `6+`.
const RUN_SLOTS: usize = 6;

/// The fixed integer counters one identity carries, as **one array** so an
/// exactly-subtractable book replacement is a single elementwise loop rather than
/// a field list a new counter could silently fall off.
const COUNTERS: usize = 27;
/// Maximal nonletter runs this identity appears in — the rarity numerator basis.
const C_RUNS: usize = 0;
/// Raw occurrences. Retained for census parity and for a future alternate judge;
/// the shipped judge reads run memberships.
const C_COUNT: usize = 1;
/// Start-side marginals, `OBSERVABLE` wide.
const C_START: usize = 2;
/// End-side marginals, `OBSERVABLE` wide.
const C_END: usize = 5;
/// Four-state topology counts, **conditioned on the coarse outer content class** —
/// `TOPO_CLASSES · TOPOLOGIES` wide, laid out by [`topo_cell`].
const C_TOPO: usize = 8;
/// Same-glyph run-length histogram, `RUN_SLOTS` wide.
const C_SAME_RUN: usize = 20;
/// Occurrences of this identity that lead SOME nonletter — the directed-pair
/// channel's conditional ("given a run continues") denominator.
const C_PAIR_LEADS: usize = 26;

/// One identity's integer tallies as a chapter or a book holds them. Sorted
/// slices, so `Eq` is deterministic and a book replacement is a merge-join.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Tally {
    counters: [u64; COUNTERS],
    /// Directed pairs this identity leads: pooled follower key → count, sorted.
    pairs: Box<[(Box<str>, u64)]>,
}

/// The same tallies as the corpus aggregate holds them: maps, so a book's addend
/// is added and subtracted key by key and bit-exactly.
#[derive(Default, Clone)]
pub(crate) struct CorpusTally {
    counters: [u64; COUNTERS],
    pairs: BTreeMap<Box<str>, u64>,
}

impl CorpusTally {
    /// Whether every counter and every pair fell to zero — an identity in this
    /// state is dropped, so "absent" and "all-zero" are one state.
    fn is_zero(&self) -> bool {
        self.counters.iter().all(|&c| c == 0) && self.pairs.is_empty()
    }
}

/// One retained candidate site: a **maximal nonletter run**, which is also the
/// coalesced finding span.
///
/// One record per run, not per occurrence, and the run's members are re-derived by
/// segmenting the run's own few bytes at materialization. That is an indexed
/// lookup, not a re-walk: the slice starts and ends on cluster boundaries the
/// full-text segmentation already chose, so no cluster straddles either edge and
/// segmenting the slice yields exactly the members the map saw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct RunSite {
    /// Verse index within the chapter, plus the run's byte span in that verse.
    addr: SiteAddr,
    /// Packed outer contexts: start class in bits 0..3, end class in bits 3..6.
    outer: u8,
}

impl RunSite {
    fn pack(local: LocalKeyIdx, span: Span, start: NeighbourClass, end: NeighbourClass) -> Self {
        RunSite {
            addr: SiteAddr::pack(local, span),
            outer: start.code() | (end.code() << 3),
        }
    }

    fn outer_start(self) -> NeighbourClass {
        NeighbourClass::from_code(self.outer & 0b111)
    }

    fn outer_end(self) -> NeighbourClass {
        NeighbourClass::from_code((self.outer >> 3) & 0b111)
    }
}

/// The chapter's at-most-two deferred outer contexts, and the identities they
/// belong to.
///
/// The identities are recorded **here** because reduction and the book fold have
/// no text: a resolved edge must be tallied under its own identity, and recovering
/// that from the site's byte span would need the chapter's text, which is long
/// gone by then. Widening [`RunSite`] to carry an identity per site instead would
/// multiply retained memory by the identity length across every occurrence,
/// against the measurement that justified compact sites at all (packet §3).
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct DeferredEdges {
    /// The occurrence at the chapter's **first** grapheme, when that grapheme is a
    /// candidate: its identity, and its END side's class — `None` when the end is
    /// this chapter's trailing deferred edge too (one single-member run is the
    /// chapter's whole content).
    lead: Option<(Box<str>, Option<NeighbourClass>)>,
    /// The occurrence at the chapter's **last** grapheme, on the same terms: its
    /// identity and its START side's class, `None` in that same case.
    tail: Option<(Box<str>, Option<NeighbourClass>)>,
}

/// One chapter's whole nonletter observation. Shared by `Arc` with its reduced
/// result and its book contribution rather than deep-copied: reduction adds only
/// the two resolved outer classes, so the body itself never changes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct NonletterChapterObs {
    token: Box<str>,
    body: Arc<NonletterChapterBody>,
}

#[derive(Default, PartialEq, Eq, Debug)]
pub(crate) struct NonletterChapterBody {
    /// Per-identity tallies, sorted by identity. **Excludes** the contributions a
    /// deferred edge owns (its side marginal and its topology), which
    /// [`fold_book`](NonletterUsageSubstrate::fold_book) adds once the edge has
    /// resolved.
    tallies: Box<[(Box<str>, Tally)]>,
    /// Candidate occurrences in this chapter — the absolute-rarity channel's
    /// corpus exposure addend.
    exposure: u64,
    /// Maximal nonletter runs containing at least one **Nd** digit — the pooled
    /// digit class's run count (addendum §B2).
    digit_class_runs: u64,
    /// One record per maximal run, in scan order: verse order, then byte offset.
    /// That is exactly the within-rule emission order the final stable sort
    /// preserves.
    sites: Box<[RunSite]>,
    edges: DeferredEdges,
}

/// One chapter's reduced result: the shared body plus the two outer classes the
/// chapter could not resolve alone.
///
/// Both default to `Boundary`, which is the book-edge answer, and a neighbouring
/// chapter overwrites the one it neighbours with `Spaced`. That is why
/// [`finish_book`](NonletterUsageSubstrate::finish_book) has nothing to do: an
/// unresolved trailing edge already *is* the book-edge resolution, so there is no
/// dangling state to clean up and no third value to reason about.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct NonletterReduced {
    token: Box<str>,
    body: Arc<NonletterChapterBody>,
    /// The resolved outer START class of the chapter's leading run edge.
    lead: NeighbourClass,
    /// The resolved outer END class of the chapter's trailing run edge.
    tail: NeighbourClass,
}

/// The boundary state carried across chapters. Resets at book boundaries only —
/// `Default` is the book-start state.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct NonletterBoundary {
    /// A previous chapter exists in this book. FALSE only at book start — this is
    /// what makes a leading edge `Spaced` rather than `Boundary`, and it must not
    /// be inferred from [`pending`](Self::pending), because a candidate-free
    /// chapter carries no pending tail yet still proves a neighbour exists.
    seen_previous: bool,
    /// The previous chapter's opaque token, when that chapter left a deferred
    /// trailing edge — so the resolution folds into ITS reduced result. It never
    /// travels further than one chapter: the very existence of this chapter is the
    /// resolution.
    pending: Option<Box<str>>,
}

/// A book's folded contribution: exactly subtractable integer tables plus its
/// chapters' reduced results, which own the retained run sites materialization
/// walks.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct NonletterBookContribution {
    tallies: Arc<Vec<(Box<str>, Tally)>>,
    exposure: u64,
    digit_class_runs: u64,
    chapters: Vec<NonletterReduced>,
}

/// One book's addend as the corpus aggregate holds it: its identity-sorted tally
/// table (shared by `Arc` with the contribution it was folded from, so the
/// aggregate never needs its own copy) and its two corpus scalars.
type NonletterAddend = (Arc<Vec<(Box<str>, Tally)>>, u64, u64);

/// The corpus aggregate: per-book addends so a replacement subtracts exactly what
/// it added, plus the summed per-identity tallies and the two corpus scalars every
/// judged rate reads.
#[derive(Default)]
pub(crate) struct NonletterCorpusStats {
    per_book: BTreeMap<Box<str>, NonletterAddend>,
    tallies: BTreeMap<Box<str>, CorpusTally>,
    exposure: u64,
    digit_class_runs: u64,
}

/// The judge key: one candidate identity, as exact raw grapheme bytes.
pub(crate) type NonletterKey = Box<str>;

// ───────────────────────────────────────────────────────────────────────────
// The chapter map
// ───────────────────────────────────────────────────────────────────────────

/// One identity's tallies under construction.
#[derive(Default)]
struct TallyBuilder {
    counters: [u64; COUNTERS],
    pairs: BTreeMap<Box<str>, u64>,
}

/// The outer class a non-candidate neighbour presents. A hygiene cluster or a
/// baseless mark is not content to be attached to, so it reads as spaced rather
/// than inventing a class the judge would have to pool.
fn outer_of(kind: Kind) -> NeighbourClass {
    match kind {
        Kind::Alpha => NeighbourClass::Letter,
        Kind::Space | Kind::Hygiene | Kind::BaselessMark => NeighbourClass::Spaced,
        Kind::Candidate(CandClass::Digit | CandClass::Numeral) => NeighbourClass::Digit,
        // Unreachable: a candidate neighbour would be inside the run.
        Kind::Candidate(_) => NeighbourClass::Spaced,
    }
}

fn map_nonletter_chapter(chapter: &crate::substrate::ChapterView<'_>) -> NonletterChapterObs {
    let graphemes = chapter.graphemes();
    let mut tallies: BTreeMap<Box<str>, TallyBuilder> = BTreeMap::new();
    let mut sites: Vec<RunSite> = Vec::new();
    let mut edges = DeferredEdges::default();
    let mut exposure = 0u64;
    let mut digit_class_runs = 0u64;
    // One reusable classification buffer for the whole chapter: the run scan and
    // the per-member tallying both read it, so each cluster is classified once.
    let mut kinds: Vec<Kind> = Vec::new();
    let last_verse = chapter.texts.len().saturating_sub(1);

    for (vi, text) in chapter.texts.iter().enumerate() {
        let local = LocalKeyIdx::from_usize(vi);
        let spans = graphemes.verse(vi);
        kinds.clear();
        kinds.extend(spans.iter().map(|g| classify(g.slice(text))));

        let mut i = 0usize;
        while i < spans.len() {
            if !matches!(kinds[i], Kind::Candidate(_)) {
                i += 1;
                continue;
            }
            let run_start = i;
            while i < spans.len() && matches!(kinds[i], Kind::Candidate(_)) {
                i += 1;
            }
            let run_end = i; // exclusive
            let len = run_end - run_start;

            // The run's outer contexts. A verse seam inside the chapter reads as
            // `Spaced`; the chapter's own first/last grapheme is `Deferred`. Note
            // an empty neighbouring verse still supplies the seam, so only the
            // chapter's true first/last grapheme can be deferred.
            let before = if run_start > 0 {
                outer_of(kinds[run_start - 1])
            } else if vi > 0 {
                NeighbourClass::Spaced
            } else {
                NeighbourClass::Deferred
            };
            let after = if run_end < spans.len() {
                outer_of(kinds[run_end])
            } else if vi < last_verse {
                NeighbourClass::Spaced
            } else {
                NeighbourClass::Deferred
            };

            let span = Span {
                start: spans[run_start].start,
                end: spans[run_end - 1].start + spans[run_end - 1].len,
            };
            sites.push(RunSite::pack(local, span, before, after));

            let glyph_at = |k: usize| spans[k].slice(text);
            let class_at = |k: usize| match kinds[k] {
                Kind::Candidate(c) => c,
                _ => unreachable!("a run holds only candidates"),
            };
            let all_same = (run_start..run_end).all(|k| glyph_at(k) == glyph_at(run_start));

            for offset in 0..len {
                let k = run_start + offset;
                let glyph = glyph_at(k);
                let start = if offset == 0 {
                    before
                } else {
                    NeighbourClass::Internal
                };
                let end = if offset + 1 == len {
                    after
                } else {
                    NeighbourClass::Internal
                };

                let entry = tallies.entry(Box::from(glyph)).or_default();
                entry.counters[C_COUNT] += 1;
                if let Some(slot) = start.slot() {
                    entry.counters[C_START + slot] += 1;
                }
                if let Some(slot) = end.slot() {
                    entry.counters[C_END + slot] += 1;
                }
                // A deferred side leaves the topology to the book fold, the only
                // place both of this occurrence's sides are known.
                if start != NeighbourClass::Deferred
                    && end != NeighbourClass::Deferred
                    && let Some(t) = Topology::of(start, end)
                {
                    entry.counters[C_TOPO + topo_cell(TopoClass::of(start, end), t)] += 1;
                }
                if offset + 1 < len {
                    let next = glyph_at(k + 1);
                    let key = pool_key(next, class_at(k + 1));
                    *entry.pairs.entry(Box::from(key)).or_default() += 1;
                    entry.counters[C_PAIR_LEADS] += 1;
                }
                // The continuation histogram speaks only for a run that is
                // entirely ONE glyph — exactly the `::` vs `:::` case directed
                // pairs cannot separate — and is recorded once, on the run's first
                // member, so one run yields one continuation signal.
                if offset == 0 && all_same {
                    entry.counters[C_SAME_RUN + len.min(RUN_SLOTS) - 1] += 1;
                }

                if start == NeighbourClass::Deferred {
                    edges.lead = Some((
                        Box::from(glyph),
                        (end != NeighbourClass::Deferred).then_some(end),
                    ));
                }
                if end == NeighbourClass::Deferred {
                    edges.tail = Some((
                        Box::from(glyph),
                        (start != NeighbourClass::Deferred).then_some(start),
                    ));
                }
            }

            // Run memberships: once per run per DISTINCT identity in it, so
            // wreckage cannot inflate its own recurrence by being long.
            for glyph in (run_start..run_end)
                .map(glyph_at)
                .collect::<BTreeSet<&str>>()
            {
                tallies
                    .get_mut(glyph)
                    .expect("every run member was just inserted")
                    .counters[C_RUNS] += 1;
            }
            if (run_start..run_end).any(|k| class_at(k) == CandClass::Digit) {
                digit_class_runs += 1;
            }
            exposure += len as u64;
        }
    }

    NonletterChapterObs {
        token: Box::from(chapter.chapter),
        body: Arc::new(NonletterChapterBody {
            tallies: tallies
                .into_iter()
                .map(|(g, b)| {
                    (
                        g,
                        Tally {
                            counters: b.counters,
                            pairs: b.pairs.into_iter().collect(),
                        },
                    )
                })
                .collect(),
            exposure,
            digit_class_runs,
            sites: sites.into_boxed_slice(),
            edges,
        }),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Judging
// ───────────────────────────────────────────────────────────────────────────

/// The linear recurrence knee the shipped rare-glyph and spacing rules use (ADR
/// 0050/0051): `minority` occurrences of the thing under judgment, decaying to
/// zero by `k`. Always fed the LEAVE-ONE-OUT count, so a singleton scores 1.
fn knee(minority_loo: u64, k: f64) -> f64 {
    (1.0 - (minority_loo as f64 / k)).clamp(0.0, 1.0)
}

/// Wilson lower bound on the MAJORITY share — "how confidently is the other form
/// the convention here". Leave-one-out already applied; an empty pool is not a
/// convention.
fn dominance(majority_loo: u64, total_loo: u64, z: f64) -> f64 {
    if total_loo == 0 {
        return 0.0;
    }
    wilson_lower_bound(majority_loo, total_loo, z)
}

/// One judged channel: its score and the leave-one-out evidence behind it, so a
/// score can never be read without the counts that produced it.
#[derive(Clone, Copy, Default, Debug)]
struct Channel {
    score: f64,
    count: u64,
    total: u64,
}

/// A channel that abstains carries `None` — never a zero, which would cancel
/// another well-supported channel through the `max`.
type Judged = Option<Channel>;

/// One identity's corpus verdict: every score that does not depend on an
/// occurrence's own context, computed once per identity per analyze.
#[derive(Clone, Default, Debug)]
pub(crate) struct IdentityVerdict {
    /// Absolute rarity is per-identity by construction: its numerator is the
    /// identity's own run memberships (or the pooled Nd class's), and leave-one-out
    /// always removes exactly the one run under judgment.
    rarity: Judged,
    start: [Judged; OBSERVABLE],
    end: [Judged; OBSERVABLE],
    /// Conditioned topology cells, laid out by [`topo_cell`]. A class-conditioned
    /// cell is smaller than the pooled table was, so the **pool floor** is what
    /// protects it: a thin cell abstains rather than inferring a convention from a
    /// handful of occurrences (the plan's named topology-fragmentation risk).
    topology: [Judged; TOPO_CLASSES * TOPOLOGIES],
    /// The pooled follower keys this identity has been seen leading, sorted.
    pairs: Box<[(Box<str>, Channel)]>,
    /// The score any pairing this identity has NEVER led takes. With the
    /// adjudicated `k = 2` this is very nearly the channel's whole range:
    /// dominance is uninformative at these denominators, so the channel is honestly
    /// binary — an unseen pairing, and little else — and the plan's own canonical
    /// message ("`. → ,` occurs here but nowhere else") is an unseen-pairing claim.
    pair_unseen: Judged,
    /// Same-glyph continuation by run length `2..=6+` (index `len - 2`).
    continuation: [Judged; RUN_SLOTS - 1],
}

impl IdentityVerdict {
    /// This identity's verdict for one directed pairing, by pooled follower key.
    /// A follower whose own verdict abstained falls through to the unseen verdict,
    /// which abstains under the identical support gate — so the two answers agree.
    fn pair(&self, key: &str) -> Judged {
        match self.pairs.binary_search_by(|(f, _)| (**f).cmp(key)) {
            Ok(i) => Some(self.pairs[i].1),
            Err(_) => self.pair_unseen,
        }
    }
}

/// The sanitised judging knobs, resolved once per analyze.
struct Knobs {
    floor: f64,
    rarity_min_exposure: u64,
    rarity_k: f64,
    placement_min_pool: u64,
    placement_knee: Knee,
    placement_z: f64,
    sequence_min_leads: u64,
    sequence_knee: Knee,
    sequence_z: f64,
    continuation_min_support: u64,
}

impl Knobs {
    fn of(cfg: &NonletterUsageConfig) -> Self {
        Knobs {
            floor: f64::from(clamp_unit(cfg.emit_score_min)),
            rarity_min_exposure: u64::from(cfg.rarity_min_exposure),
            rarity_k: clamp_count(cfg.rarity_k),
            placement_min_pool: u64::from(cfg.placement_min_pool),
            placement_knee: Knee::of(cfg.placement_k, cfg.placement_rate_per_10k),
            placement_z: clamp_z(cfg.placement_z),
            sequence_min_leads: u64::from(cfg.sequence_min_leads),
            sequence_knee: Knee::of(cfg.sequence_k, cfg.sequence_rate_per_10k),
            sequence_z: clamp_z(cfg.sequence_z),
            continuation_min_support: u64::from(cfg.continuation_min_support),
        }
    }
}

/// ADR 0050's **opportunity-proportional** recurrence knee:
/// `K = base + slope · N / 10 000`, where `N` is the judged pool's opportunity
/// volume.
///
/// The absolute base is the tolerance at negligible volume, and the whole
/// tolerance for a thin identity. The proportional term is what a flat knee gets
/// wrong: slips accumulate with volume, so a translation that writes ten times the
/// commas honestly accrues about ten times the comma slips, and a flat knee
/// silences exactly the slip clouds a large translation produces. The migration
/// ledger caught that empirically — see [`NonletterUsageConfig`]'s two rate knobs.
#[derive(Clone, Copy)]
struct Knee {
    base: f64,
    slope: f64,
}

impl Knee {
    fn of(base: f32, rate_per_10k: f32) -> Self {
        Knee {
            base: clamp_count(base),
            // A negative or NaN rate degrades to the flat knee rather than to a
            // knee that shrinks with volume, which would be non-monotone nonsense.
            slope: f64::from(rate_per_10k).max(0.0),
        }
    }

    /// This knee's width at a judged pool of `n`.
    fn at(self, n: u64) -> f64 {
        self.base + self.slope * n as f64 / 10_000.0
    }
}

/// One `(form, pool)` binary's leave-one-out verdict, shared by every
/// distribution-shaped channel: side marginals, topology, directed pairs and the
/// continuation histogram all ask the same question of a different table.
///
/// Leave-one-out drops the occurrence under judgment from BOTH the form's count
/// and the pool, so a form seen only here reads as `0 of n-1` rather than `1 of n`
/// — which is what stops a candidate licensing itself at `1/1`. A pool that falls
/// below its support floor **abstains** rather than hallucinating a convention.
fn judged_form(mine: u64, pool: u64, min_pool: u64, z: f64, knee_of: Knee) -> Judged {
    let total_loo = pool.saturating_sub(1);
    if total_loo < min_pool {
        return None;
    }
    let mine_loo = mine.saturating_sub(1).min(total_loo);
    Some(Channel {
        score: dominance(total_loo - mine_loo, total_loo, z)
            * knee(mine_loo, knee_of.at(total_loo)),
        count: mine_loo,
        total: total_loo,
    })
}

fn judge_identity(kn: &Knobs, key: &str, stats: &NonletterCorpusStats) -> IdentityVerdict {
    let Some(t) = stats.tallies.get(key) else {
        return IdentityVerdict::default();
    };
    let class = match classify(key) {
        Kind::Candidate(c) => c,
        // A judge key is always a candidate identity the map recorded.
        _ => return IdentityVerdict::default(),
    };

    // ── Channel 1: absolute rarity ────────────────────────────────────────
    // "Is this grapheme itself unusually rare in this translation?" The numerator
    // is its own RUN-membership recurrence, leave-one-out; its SUPPORT is corpus
    // exposure, not its own count — one `$` in a large corpus is well-supported
    // rarity, one `$` in a tiny corpus is thin, and that is why the exposure gate
    // is the thing that abstains.
    //
    // Run memberships rather than occurrences repairs identity-level
    // self-licensing: in `WA-as-ulb` all 11 occurrences of `*` ARE the two junk
    // runs `*******` and `****`, so occurrence counting read `*` as recurring 11
    // times and `knee(10, k=8) = 0` silenced obvious wreckage. Counting runs reads
    // it as appearing in 2 places, and since findings are coalesced per run,
    // leave-one-out honestly excludes the whole run under judgment.
    let rarity = (stats.exposure >= kn.rarity_min_exposure).then(|| {
        let basis = if class == CandClass::Digit {
            stats.digit_class_runs
        } else {
            t.counters[C_RUNS]
        };
        let count = basis.saturating_sub(1);
        Channel {
            score: knee(count, kn.rarity_k),
            count,
            total: stats.exposure,
        }
    });

    // ── Channel 2: placement ──────────────────────────────────────────────
    // "Given an established grapheme, is its logical start/end attachment unusual
    // HERE?" Three sub-components combined with `max`: they describe one
    // correlated occurrence, so overlapping reasons must not inflate the score.
    let pool = |base: usize, width: usize| -> u64 { t.counters[base..base + width].iter().sum() };
    let start_pool = pool(C_START, OBSERVABLE);
    let end_pool = pool(C_END, OBSERVABLE);
    // One pool per conditioned class, not one pool across the whole table.
    let topo_pools: [u64; TOPO_CLASSES] =
        std::array::from_fn(|c| pool(C_TOPO + c * TOPOLOGIES, TOPOLOGIES));
    let placement = |base: usize, pool: u64, slot: usize| {
        judged_form(
            t.counters[base + slot],
            pool,
            kn.placement_min_pool,
            kn.placement_z,
            kn.placement_knee,
        )
    };
    let start = std::array::from_fn(|slot| placement(C_START, start_pool, slot));
    let end = std::array::from_fn(|slot| placement(C_END, end_pool, slot));
    let topology =
        std::array::from_fn(|cell| placement(C_TOPO, topo_pools[cell / TOPOLOGIES], cell));

    // ── Channel 3: sequence ───────────────────────────────────────────────
    // "Are these individually ordinary graphemes placed beside one another in a
    // pairing this translation does not use?" Directed pairs, never exact
    // maximal-run strings — which would fragment evidence and stop natural
    // pairings generalizing. Nd digits pool, because numeric grouping is a
    // nonletter run: keyed exactly, a comma's pair table splits across all ten
    // digits and a corpus that groups numbers constantly still has `, → 9` as a
    // singleton, which the probe measured firing at 0-of-54,722 (§7.2).
    let leads = t.counters[C_PAIR_LEADS];
    let pair_of = |mine: u64| {
        judged_form(
            mine,
            leads,
            kn.sequence_min_leads,
            kn.sequence_z,
            kn.sequence_knee,
        )
    };
    let pairs: Box<[(Box<str>, Channel)]> = t
        .pairs
        .iter()
        .filter_map(|(f, &n)| pair_of(n).map(|c| (f.clone(), c)))
        .collect();
    let pair_unseen = pair_of(0);

    let cont_pool = pool(C_SAME_RUN, RUN_SLOTS);
    let continuation = std::array::from_fn(|i| {
        judged_form(
            t.counters[C_SAME_RUN + i + 1],
            cont_pool,
            kn.continuation_min_support,
            kn.sequence_z,
            kn.sequence_knee,
        )
    });

    IdentityVerdict {
        rarity,
        start,
        end,
        topology,
        pairs,
        pair_unseen,
        continuation,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// The substrate
// ───────────────────────────────────────────────────────────────────────────

/// The `uni.nonletter-usage-anomaly` observation substrate. Sole consumer: the
/// rule of the same name.
pub(crate) struct NonletterUsageSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <NonletterUsageSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// Walk two identity-sorted tally tables together, calling `f(identity, old, new)`
/// once per identity present in either. The one place a book's tally replacement
/// is applied, so the subtract and the add cannot disagree about which identities
/// they touched.
fn merge_tallies(
    old: &[(Box<str>, Tally)],
    new: &[(Box<str>, Tally)],
    mut f: impl FnMut(&str, Option<&Tally>, Option<&Tally>),
) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some((a, o)), Some((b, n))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    f(a, Some(o), None);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    f(b, None, Some(n));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    f(a, Some(o), Some(n));
                    i += 1;
                    j += 1;
                }
            },
            (Some((a, o)), None) => {
                f(a, Some(o), None);
                i += 1;
            }
            (None, Some((b, n))) => {
                f(b, None, Some(n));
                j += 1;
            }
            (None, None) => unreachable!("loop guard"),
        }
    }
}

/// Add one resolved deferred edge's contribution into the book's tally table.
fn add_edge(
    into: &mut BTreeMap<Box<str>, Tally>,
    glyph: &str,
    side: Option<(usize, NeighbourClass)>,
    topology: Option<(TopoClass, Topology)>,
) {
    let e = into.entry(Box::from(glyph)).or_default();
    if let Some((base, class)) = side
        && let Some(slot) = class.slot()
    {
        e.counters[base + slot] += 1;
    }
    if let Some((class, t)) = topology {
        e.counters[C_TOPO + topo_cell(class, t)] += 1;
    }
}

/// The tally contributions one chapter's resolved deferred edges produce. Called
/// from the book fold, the only place both an edge's identity and both of its
/// sides are known.
fn fold_edges(reduced: &NonletterReduced, into: &mut BTreeMap<Box<str>, Tally>) {
    let edges = &reduced.body.edges;
    // The LEADING edge's occurrence: its start is this chapter's resolved lead,
    // and its end is either locally known or this chapter's resolved tail (the one
    // single-member run that is the chapter's whole content).
    if let Some((glyph, end_known)) = &edges.lead {
        let start = reduced.lead;
        let end = end_known.unwrap_or(reduced.tail);
        add_edge(
            into,
            glyph,
            Some((C_START, start)),
            Topology::of(start, end).map(|t| (TopoClass::of(start, end), t)),
        );
    }
    // The TRAILING edge's occurrence — the SAME occurrence when its start is
    // `None`, in which case the topology was already added above.
    if let Some((glyph, start_known)) = &edges.tail {
        let end = reduced.tail;
        add_edge(
            into,
            glyph,
            Some((C_END, end)),
            start_known
                .and_then(|start| Topology::of(start, end).map(|t| (TopoClass::of(start, end), t))),
        );
    }
}

impl crate::substrate::ObservationSubstrate for NonletterUsageSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::NonletterUsage;
    // Bump on any observation/reduction schema change. 2: the topology tally gained
    // its coarse outer-content-class axis.
    const SCHEMA_STAMP: u64 = 2;
    type Pairing = crate::substrate::NoReference;
    // The candidate atom is one extended grapheme cluster, so the chapter's
    // grapheme spans (and the tape they are segmented from) are the whole input.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::GRAPHEMES;

    type Key = NonletterKey;
    type BoundaryState = NonletterBoundary;
    type ChapterObservation = NonletterChapterObs;
    type ReducedChapter = NonletterReduced;
    type BookContribution = NonletterBookContribution;
    type CorpusStats = NonletterCorpusStats;
    // Every `NonletterUsageConfig` field is read at judge — the floor, the four
    // support gates, the three knees and the two z's — so a knob change, and
    // therefore a Review Depth move, maps and reduces nothing.
    type ExtractorConfig = ();
    // Candidate identities are their own text; nothing to name through a table.
    type Symbols = ();
    type JudgeConfig = NonletterUsageConfig;
    type EntryOutcome = IdentityVerdict;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> NonletterChapterObs {
        map_nonletter_chapter(chapter)
    }

    fn pending_owner(state: &NonletterBoundary) -> Option<&str> {
        state.pending.as_deref()
    }

    fn reduce_chapter(
        observation: &NonletterChapterObs,
        entering: &NonletterBoundary,
        carry_out: &mut NonletterReduced,
    ) -> (NonletterReduced, NonletterBoundary) {
        // The previous chapter's trailing edge, if it left one: this chapter's very
        // existence resolves it, because a seam reads as spaced continuity whatever
        // the neighbouring chapter contains.
        if entering.pending.is_some() {
            carry_out.tail = NeighbourClass::Spaced;
        }
        let this = NonletterReduced {
            token: observation.token.clone(),
            body: Arc::clone(&observation.body),
            // A previous chapter in the book makes the leading edge spaced; book
            // start leaves it a boundary, which abstains.
            lead: if entering.seen_previous {
                NeighbourClass::Spaced
            } else {
                NeighbourClass::Boundary
            },
            // The book-edge answer, overwritten by the next chapter if there is
            // one — which is why `finish_book` has nothing to do.
            tail: NeighbourClass::Boundary,
        };
        let leaving = NonletterBoundary {
            seen_previous: true,
            pending: observation
                .body
                .edges
                .tail
                .is_some()
                .then(|| observation.token.clone()),
        };
        (this, leaving)
    }

    /// Nothing to resolve. A trailing deferred edge's book-edge answer is the value
    /// [`reduce_chapter`](Self::reduce_chapter) already left in place, so there is
    /// no dangling state at a book edge and no third value to reason about.
    fn finish_book(_leaving: &NonletterBoundary, _carry_out: &mut NonletterReduced) {}

    fn fold_book(reduced: &[NonletterReduced], _symbols: &()) -> NonletterBookContribution {
        let mut tallies: BTreeMap<Box<str>, Tally> = BTreeMap::new();
        for r in reduced {
            for (glyph, t) in r.body.tallies.iter() {
                let e = tallies.entry(glyph.clone()).or_default();
                for (x, y) in e.counters.iter_mut().zip(t.counters) {
                    *x += y;
                }
                if !t.pairs.is_empty() {
                    let mut merged: BTreeMap<Box<str>, u64> = e.pairs.iter().cloned().collect();
                    for (f, n) in t.pairs.iter() {
                        *merged.entry(f.clone()).or_default() += n;
                    }
                    e.pairs = merged.into_iter().collect();
                }
            }
        }
        // Now add what the map deliberately left out: the side marginal and the
        // topology of each chapter's at-most-two resolved deferred edges.
        for r in reduced {
            fold_edges(r, &mut tallies);
        }
        NonletterBookContribution {
            tallies: Arc::new(tallies.into_iter().collect()),
            exposure: reduced.iter().map(|r| r.body.exposure).sum(),
            digit_class_runs: reduced.iter().map(|r| r.body.digit_class_runs).sum(),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut NonletterCorpusStats,
        slug: &str,
        old: Option<&NonletterBookContribution>,
        new: Option<&NonletterBookContribution>,
    ) -> Vec<NonletterKey> {
        let empty: Vec<(Box<str>, Tally)> = Vec::new();
        merge_tallies(
            old.map_or(&empty[..], |c| &c.tallies[..]),
            new.map_or(&empty[..], |c| &c.tallies[..]),
            |glyph, o, n| {
                let e = stats.tallies.entry(Box::from(glyph)).or_default();
                for (i, slot) in e.counters.iter_mut().enumerate() {
                    let sub = o.map_or(0, |t| t.counters[i]);
                    let add = n.map_or(0, |t| t.counters[i]);
                    *slot = *slot + add - sub;
                }
                for (f, count) in o.into_iter().flat_map(|t| t.pairs.iter()) {
                    let p = e.pairs.entry(f.clone()).or_default();
                    *p -= count;
                    if *p == 0 {
                        e.pairs.remove(f);
                    }
                }
                for (f, count) in n.into_iter().flat_map(|t| t.pairs.iter()) {
                    *e.pairs.entry(f.clone()).or_default() += count;
                }
                if e.is_zero() {
                    stats.tallies.remove(glyph);
                }
            },
        );
        if let Some(o) = old {
            stats.exposure -= o.exposure;
            stats.digit_class_runs -= o.digit_class_runs;
        }
        match new {
            Some(c) => {
                stats.exposure += c.exposure;
                stats.digit_class_runs += c.digit_class_runs;
                stats.per_book.insert(
                    Box::from(slug),
                    (Arc::clone(&c.tallies), c.exposure, c.digit_class_runs),
                );
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // EMPTY, and honestly so: every judged rate reads a corpus-global
        // denominator (`exposure` for rarity, the identity's own corpus-wide pools
        // for placement and sequence), so a book replacement that moves a single
        // count moves either nothing or every key — never a subset, which is the
        // one answer that would be wrong. This substrate's consumer rebuilds its
        // whole partition from the analyze's findings, so the delta has no reader;
        // the same structural reason punct-only, repeated-run and casing give.
        Vec::new()
    }

    fn judge(
        cfg: &NonletterUsageConfig,
        key: &NonletterKey,
        stats: &NonletterCorpusStats,
    ) -> IdentityVerdict {
        judge_identity(&Knobs::of(cfg), key, stats)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Materialization — one finding per maximal run
// ───────────────────────────────────────────────────────────────────────────

/// One member of a maximal run, derived from the run's own text.
struct Member<'a> {
    glyph: &'a str,
    start: NeighbourClass,
    end: NeighbourClass,
    /// The follower this member leads, when it is not the run's last: its exact
    /// text (for a truthful message) and its pooled key (for the pair table).
    leads: Option<(&'a str, &'a str)>,
    /// The whole run is repetitions of THIS glyph and this is its first member —
    /// the only position the bounded continuation component speaks for. Carries the
    /// capped run length.
    continuation: Option<usize>,
}

/// Derive run member `i` from the run's own bytes and its two outer contexts.
fn member_at<'a>(
    run: &'a str,
    spans: &[GSpan],
    i: usize,
    all_same: bool,
    outer_start: NeighbourClass,
    outer_end: NeighbourClass,
) -> Member<'a> {
    let len = spans.len();
    let glyph = |k: usize| spans[k].slice(run);
    Member {
        glyph: glyph(i),
        start: if i == 0 {
            outer_start
        } else {
            NeighbourClass::Internal
        },
        end: if i + 1 == len {
            outer_end
        } else {
            NeighbourClass::Internal
        },
        leads: (i + 1 < len).then(|| {
            let next = glyph(i + 1);
            (next, pool_key(next, cand_class(next)))
        }),
        continuation: (i == 0 && all_same && len >= 2).then_some(len.min(RUN_SLOTS)),
    }
}

/// Canonical channel order — the primary-reason priority, and the order the `also`
/// list is rendered in.
///
/// Rarity first: "this grapheme appears in only two places" is the simplest and
/// most certain claim available, and it is the only channel that can reach a clean
/// 1.0. **Topology before the two side marginals**, because it is the strictly more
/// specific statement — it names both sides at once — and the three routinely tie
/// exactly (`th3e`'s `3` is unique on its start side, its end side and its
/// topology all at once, against the same 800-occurrence pool). On that tie the
/// marginal is implied by the topology but not the reverse, so the topology is the
/// message worth publishing.
fn reason_rank(r: NonletterReason) -> u8 {
    match r {
        NonletterReason::Rarity => 0,
        NonletterReason::Topology => 1,
        NonletterReason::Start => 2,
        NonletterReason::End => 3,
        NonletterReason::Pair => 4,
        NonletterReason::Continuation => 5,
    }
}

const REASONS: [NonletterReason; 6] = [
    NonletterReason::Rarity,
    NonletterReason::Topology,
    NonletterReason::Start,
    NonletterReason::End,
    NonletterReason::Pair,
    NonletterReason::Continuation,
];

/// Visit every channel this member is judged by, in canonical order. `max`
/// composition means the caller simply keeps the largest; an abstaining channel is
/// never visited, so it can never be read as a zero.
fn for_each_channel(
    member: &Member<'_>,
    v: &IdentityVerdict,
    mut f: impl FnMut(NonletterReason, NonletterForm, Channel),
) {
    let mut emit = |reason, form, judged: Judged| {
        if let Some(c) = judged {
            f(reason, form, c);
        }
    };
    emit(NonletterReason::Rarity, NonletterForm::None, v.rarity);
    if let Some(slot) = member.start.slot() {
        emit(NonletterReason::Start, member.start.form(), v.start[slot]);
    }
    if let Some(slot) = member.end.slot() {
        emit(NonletterReason::End, member.end.form(), v.end[slot]);
    }
    if let Some(t) = Topology::of(member.start, member.end) {
        emit(
            NonletterReason::Topology,
            t.form(),
            v.topology[topo_cell(TopoClass::of(member.start, member.end), t)],
        );
    }
    if let Some((_, key)) = member.leads {
        emit(NonletterReason::Pair, NonletterForm::None, v.pair(key));
    }
    if let Some(len) = member.continuation {
        emit(
            NonletterReason::Continuation,
            NonletterForm::None,
            v.continuation[len - 2],
        );
    }
}

impl NonletterBookContribution {
    /// Emit this book's findings: **one per maximal nonletter run** whose strongest
    /// channel clears the floor, rebasing each chapter-local address through its
    /// chapter's current base.
    ///
    /// Several firing members of one run coalesce into one finding at the run's
    /// span. The primary explanation is the strongest channel, ties broken by the
    /// canonical channel order and then by member position; every other channel
    /// that also cleared the floor travels in the args, so no violated fact is
    /// lost.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<NonletterKey, IdentityVerdict>,
        floor: f64,
        out: &mut Vec<Finding>,
    ) {
        let texts = corpus.texts();
        // Positional zip is truncating: a missing or extra trailing chapter would
        // silently DROP findings rather than fail. Chapter cardinality is the
        // alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        let mut spans: Vec<GSpan> = Vec::new();
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.body.sites.iter() {
                let (local, span) = site.addr.unpack();
                let text = &texts[block.range.start + usize::from(local.get())];
                let run = span.slice(text);
                let resolve = |c: NeighbourClass, resolved: NeighbourClass| {
                    if c == NeighbourClass::Deferred {
                        resolved
                    } else {
                        c
                    }
                };
                let outer_start = resolve(site.outer_start(), chapter.lead);
                let outer_end = resolve(site.outer_end(), chapter.tail);
                grapheme::segment(run, &mut spans);
                let all_same = (0..spans.len()).all(|i| spans[i].slice(run) == spans[0].slice(run));

                // The winning channel, held as indices so nothing is allocated for
                // a run that does not emit.
                let mut best: Option<(usize, NonletterReason, NonletterForm, Channel)> = None;
                let mut also_mask = 0u8;
                for i in 0..spans.len() {
                    let member = member_at(run, &spans, i, all_same, outer_start, outer_end);
                    // Every member's identity was counted by the same chapter map
                    // that produced this address, so it is in the aggregate and has
                    // a verdict.
                    let v = verdicts
                        .get(member.glyph)
                        .expect("every retained run member is a judged key");
                    for_each_channel(&member, v, |reason, form, channel| {
                        if channel.score < floor {
                            return;
                        }
                        also_mask |= 1 << reason_rank(reason);
                        let better = match &best {
                            None => true,
                            Some((bi, breason, _, bc)) => {
                                channel.score > bc.score
                                    || (channel.score == bc.score
                                        && (reason_rank(reason), i) < (reason_rank(*breason), *bi))
                            }
                        };
                        if better {
                            best = Some((i, reason, form, channel));
                        }
                    });
                }
                let Some((mi, reason, form, channel)) = best else {
                    continue;
                };
                let member = member_at(run, &spans, mi, all_same, outer_start, outer_end);
                let partner = match reason {
                    NonletterReason::Pair => member.leads.map_or("", |(next, _)| next),
                    _ => "",
                };
                let also: Vec<NonletterReason> = REASONS
                    .into_iter()
                    .filter(|&r| r != reason && also_mask & (1 << reason_rank(r)) != 0)
                    .collect();
                let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: NONLETTER_USAGE_ANOMALY,
                    severity: Severity::Info,
                    range: span,
                    score: Some(channel.score as f32),
                    args: Some(FindingArgs::NonletterUsage {
                        glyph: member.glyph.to_string(),
                        reason,
                        form,
                        partner: partner.to_string(),
                        count: sat(channel.count),
                        total: sat(channel.total),
                        also,
                    }),
                });
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Review Depth
// ───────────────────────────────────────────────────────────────────────────

/// The fleet-calibrated Review Depth profile (Gate 1 decision 7, owner-ratified).
///
/// Depth chooses a minimum **unusualness** and a minimum **support**, and support
/// relaxes faster than unusualness (ADR 0070): the strict end wants a strong
/// convention contradiction backed by a lot of evidence, the exploratory end
/// admits thinner evidence well before it admits a weaker contradiction. The three
/// unusualness anchors are the adjudicated ones — 0 → 0.90, 50 → 0.75, 100 → 0.50
/// — and depth 50 is exactly the calibrated midpoint on every knob, so the
/// packet's measured depth-50 volume is the shipped default's volume.
///
/// The knees are the model, not the policy, and do not move with depth.
pub fn config_at_review_depth(depth: crate::review_depth::ReviewDepth) -> NonletterUsageConfig {
    use crate::review_depth::{interpolate_f32, interpolate_u32};
    NonletterUsageConfig {
        emit_score_min: interpolate_f32(depth, &[(0, 0.90), (50, 0.75), (100, 0.50)]),
        rarity_min_exposure: interpolate_u32(depth, &[(0, 4_000), (50, 2_000), (100, 500)]),
        placement_min_pool: interpolate_u32(depth, &[(0, 60), (50, 30), (100, 8)]),
        sequence_min_leads: interpolate_u32(depth, &[(0, 200), (50, 100), (100, 25)]),
        continuation_min_support: interpolate_u32(depth, &[(0, 200), (50, 100), (100, 25)]),
        ..NonletterUsageConfig::default()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Drive
// ───────────────────────────────────────────────────────────────────────────

/// Plan this substrate's share of the analysis: enrol it in the chapter-outer
/// schedule for exactly the chapters whose observation input stamp moved. When
/// inactive, drop the cached products so an edit while it is disabled does no work
/// for it, and enrol nothing.
pub(crate) fn plan_nonletter_usage<'a>(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<NonletterUsageSubstrate>,
    schedule: &mut crate::schedule::Schedule<'a>,
) -> Option<crate::schedule::SubstratePlan<'a, NonletterUsageSubstrate>> {
    use crate::substrate::ObservationInputStamp;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return None;
    }
    Some(
        schedule.enrol::<NonletterUsageSubstrate>(cache, |_slug, c| {
            ObservationInputStamp::target_only::<NonletterUsageSubstrate>(c.hash, &())
        }),
    )
}

/// Reduce, judge and materialize `uni.nonletter-usage-anomaly` from the
/// observations the chapter-outer scheduler mapped.
pub(crate) fn finish_nonletter_usage(
    cache: &mut crate::substrate::SubstrateCache<NonletterUsageSubstrate>,
    corpus: &Corpus,
    cfg: &NonletterUsageConfig,
    plan: crate::schedule::SubstratePlan<'_, NonletterUsageSubstrate>,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{DrivePhase, DriveProbe};
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::NonletterUsage);
    let layout = corpus.book_layout();
    let crate::schedule::SubstratePlan { stamped, mut slots } = plan;
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| slots.take(bi, i));
    }
    probe.mark(DrivePhase::Reduce);
    // Judge every identity in the aggregate. Each is named by at least one retained
    // run member, so this is exactly the key set that can emit — and there is no
    // key-discovery phase, because the aggregate's key set already IS it.
    let kn = Knobs::of(cfg);
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<NonletterKey, IdentityVerdict> = stats
        .tallies
        .keys()
        .map(|g| (g.clone(), judge_identity(&kn, g, stats)))
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, &verdicts, kn.floor, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// The whole substrate on its own, over one caller-held cache — the shape the
/// per-rule convenience entry point and its tests use. Same planning pass, same
/// chapter task, same `finish_*`; only the participation mask is narrower.
pub(crate) fn drive_nonletter_usage(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<NonletterUsageSubstrate>,
    corpus: &Corpus,
    cfg: &NonletterUsageConfig,
    out: &mut Vec<Finding>,
) {
    let mut schedule = crate::schedule::Schedule::new(corpus);
    let Some(mut plan) = plan_nonletter_usage(active, cache, &mut schedule) else {
        return;
    };
    schedule.run_solo::<NonletterUsageSubstrate>(&mut plan, &(), &(), |_, _| None);
    finish_nonletter_usage(cache, corpus, cfg, plan, out);
}

/// Every maximal visible-nonletter run this rule OBSERVES in a corpus, as
/// `(global verse index, verse-local byte span)` in scan order — the candidate
/// domain itself, independent of any judgment.
///
/// It exists because the migration ledger's central question is *observability*,
/// not emission: "is there any span the three retired rules flag where this rule
/// sees no candidate at all?" A judged run set cannot answer that, because a run
/// every channel abstains on emits nothing at any floor while still being fully
/// observed. This is also the extractor a census lane would mirror, so it is the
/// one honest place both readings come from.
pub fn nonletter_candidate_runs(corpus: &Corpus) -> Vec<(crate::corpus::KeyIdx, Span)> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut sink = Vec::new();
    drive_nonletter_usage(
        true,
        &mut cache,
        corpus,
        &NonletterUsageConfig::default(),
        &mut sink,
    );
    let mut out = Vec::new();
    for book in corpus.book_layout() {
        let Some(contrib) = cache.book_contribution(&book.slug) else {
            continue;
        };
        for (chapter, block) in contrib.chapters.iter().zip(&book.chapters) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.body.sites.iter() {
                let (local, span) = site.addr.unpack();
                out.push((rebase(base, local), span));
            }
        }
    }
    out
}

/// `uni.nonletter-usage-anomaly` findings for a whole corpus at a given config,
/// via the observation substrate over a fresh transient cache — the single
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
pub fn nonletter_usage_findings(corpus: &Corpus, cfg: &NonletterUsageConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_nonletter_usage(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::RuleId;

    /// Ordinary English conventions: `,` twice, an opening `"`, and a `."` run.
    /// Five candidate occurrences per verse, so `N = 800` filler verses supply 4,000
    /// exposure — comfortably past the 2,000 gate, which is what makes a *silence*
    /// in these anchors a convention rather than an exposure-gate abstention. An
    /// earlier draft of the calibration probe got that wrong and its silences proved
    /// nothing.
    const EN: &str = "And he said unto them, \"Go ye into all the world, and preach.\"";

    /// The same, plus ordinary digits in conventional positions, so the digit
    /// anchors are judged against a corpus where digits ARE common.
    const EN_NUM: &str =
        "And he said unto them, \"Go ye 3 days into all 7 lands, and preach 40 years.\"";

    /// Ethiopic filler establishing `፡` as a word separator and `።` as a terminal.
    const AM: &str = "ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ኵሉ፡ ዓለም፡ ወስብኩ፡ ወንጌለ፡ ለኵሉ፡ ፍጥረት።";

    const N: usize = 800;

    /// A filler establishing a medial convention while carrying ordinary
    /// punctuation, so exposure is high and a silence is a real silence.
    fn medial(mark: char) -> String {
        format!("And the wor{mark}d of the lo{mark}rd came, and they hea{mark}rd, and said.")
    }

    /// `filler` repeated `n` times, then the probe verses — all in `GEN 1`, so every
    /// probe sits in one chapter of one book.
    fn synth(filler: &str, n: usize, probes: &[&str]) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1usize;
        for _ in 0..n {
            keys.push(format!("GEN 1:{v}"));
            texts.push(filler.to_string());
            v += 1;
        }
        for p in probes {
            keys.push(format!("GEN 1:{v}"));
            texts.push((*p).to_string());
            v += 1;
        }
        Corpus::try_from_parts(keys, texts).expect("synthetic corpus is well formed")
    }

    /// A corpus from explicit `(book, chapter, verse, text)` rows, in presented
    /// order.
    fn rows(rows: &[(&str, &str, u16, &str)]) -> Corpus {
        let keys = rows
            .iter()
            .map(|&(b, c, v, _)| format!("{b} {c}:{v}"))
            .collect();
        let texts = rows.iter().map(|&(_, _, _, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).expect("synthetic corpus is well formed")
    }

    /// A config with the floor removed, so every judged run emits and a test can
    /// read the raw composed score. Support gates are untouched: a fully abstaining
    /// occurrence still produces NO finding, which is exactly how these tests tell
    /// an abstention apart from an established-convention zero.
    fn open() -> NonletterUsageConfig {
        NonletterUsageConfig {
            emit_score_min: 0.0,
            ..NonletterUsageConfig::default()
        }
    }

    /// The finding covering the last verse's `needle`, if any.
    fn probe_finding(corpus: &Corpus, cfg: &NonletterUsageConfig, needle: &str) -> Option<Finding> {
        let last = corpus.keys().len() - 1;
        let text = &corpus.texts()[last];
        let at = text
            .find(needle)
            .expect("the probe text contains the needle") as u32;
        nonletter_usage_findings(corpus, cfg)
            .into_iter()
            .find(|f| f.key_idx.get() as usize == last && f.range.start <= at && at < f.range.end)
    }

    /// The composed score and primary reason for the last verse's `needle`.
    fn judged(
        corpus: &Corpus,
        cfg: &NonletterUsageConfig,
        needle: &str,
    ) -> Option<(f32, NonletterReason, NonletterForm, u32, u32)> {
        probe_finding(corpus, cfg, needle).map(|f| {
            let (reason, form, count, total) = match f.args {
                Some(FindingArgs::NonletterUsage {
                    reason,
                    form,
                    count,
                    total,
                    ..
                }) => (reason, form, count, total),
                other => panic!("expected nonletter args, got {other:?}"),
            };
            (f.score.expect("scored"), reason, form, count, total)
        })
    }

    fn score(corpus: &Corpus, needle: &str) -> Option<f32> {
        judged(corpus, &open(), needle).map(|(s, ..)| s)
    }

    fn close(actual: f32, want: f32) {
        assert!(
            (actual - want).abs() < 5e-3,
            "score {actual} is not ~{want}"
        );
    }

    /// The corpus aggregate this rule judges from — white-box, so the boundary and
    /// pooling contracts can be pinned on the counters themselves rather than
    /// inferred from a score.
    fn tallies(corpus: &Corpus) -> BTreeMap<Box<str>, CorpusTally> {
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(
            true,
            &mut cache,
            corpus,
            &NonletterUsageConfig::default(),
            &mut out,
        );
        cache.corpus_stats().tallies.clone()
    }

    // ── Channel 1: absolute rarity, and self-licensing ────────────────────

    /// The singleton ladder, and that it decays monotonically: a glyph seen in one
    /// place is a rare slip, one seen in four places is on its way to being the
    /// corpus's own convention. `knee(0, 8)`, `knee(1, 8)`, `knee(3, 8)`.
    #[test]
    fn rarity_decays_monotonically_with_the_places_a_glyph_appears() {
        close(
            score(&synth(EN, N, &["procrastinate ~ my case"]), "~").unwrap(),
            1.000,
        );
        close(
            score(
                &synth(EN, N, &["procrastinate ~ my case", "another ~ here"]),
                "~",
            )
            .unwrap(),
            0.875,
        );
        close(
            score(
                &synth(EN, N, &["a ~ here", "b ~ there", "c ~ again", "d ~ more"]),
                "~",
            )
            .unwrap(),
            0.625,
        );
    }

    /// Every named rare-identity anchor the plan lists fires through rarity, at a
    /// clean 1.0 — including the ones no shipped rule could observe at all: a
    /// symbol, a spacing acute, a superscript numeral, an emoji, and a curly quote
    /// in a straight-quote translation.
    #[test]
    fn a_lone_visible_nonletter_fires_through_rarity() {
        for (probe, needle) in [
            ("the price was $ high", "$"),
            ("he wrote { on the wall", "{"),
            ("he said \u{00B4} softly", "\u{00B4}"),
            ("some 50 % of them", "%"),
            ("the second\u{00B2} book", "\u{00B2}"),
            ("he smiled \u{1F600} at them", "\u{1F600}"),
            ("he said \u{201C}go\u{201D} to them", "\u{201C}"),
            ("he was mov$ing away", "$"),
        ] {
            let (s, reason, ..) = judged(&synth(EN, N, &[probe]), &open(), needle)
                .unwrap_or_else(|| panic!("{probe:?} produced no finding"));
            close(s, 1.000);
            assert_eq!(reason, NonletterReason::Rarity, "{probe:?}");
        }
    }

    /// SUPPORT, not the glyph's own count, is what rarity abstains on. One `~` in a
    /// translation of 41 candidate occurrences is thin evidence, so every channel
    /// abstains and there is no finding at all — distinct from an
    /// established-convention zero, which does emit at an open floor.
    #[test]
    fn a_singleton_in_a_tiny_corpus_abstains_entirely() {
        assert!(
            probe_finding(&synth(EN, 8, &["procrastinate ~ my case"]), &open(), "~").is_none(),
            "a singleton in a tiny corpus must abstain, not score"
        );
    }

    /// The `*******` / `****` case the occurrence basis silenced. `*` occurs 11
    /// times but in only TWO places, so run-membership counting reads
    /// `knee(1, 8) = 0.875` and both runs fire — where occurrence counting read
    /// `knee(10, 8) = 0` and let obvious wreckage through. This is the defect
    /// decision 5 repaired, and it is identity-level self-licensing.
    #[test]
    fn wreckage_cannot_inflate_its_own_rarity_by_being_long() {
        let corpus = synth(EN, N, &["the border town *******", "he was ashamed ****"]);
        let found = nonletter_usage_findings(&corpus, &NonletterUsageConfig::default());
        let starred: Vec<&Finding> = found
            .iter()
            .filter(|f| {
                let text = &corpus.texts()[f.key_idx.get() as usize];
                f.range.slice(text).starts_with('*')
            })
            .collect();
        assert_eq!(starred.len(), 2, "both junk runs fire, one finding each");
        for f in &starred {
            close(f.score.unwrap(), 0.875);
            match &f.args {
                Some(FindingArgs::NonletterUsage { reason, count, .. }) => {
                    assert_eq!(*reason, NonletterReason::Rarity);
                    // Leave-one-out excludes the whole run under judgment, so the
                    // honest message is "appears in only 2 places".
                    assert_eq!(*count, 1);
                }
                other => panic!("{other:?}"),
            }
        }
        // And the finding is ONE per maximal run, spanning the whole run.
        let text = &corpus.texts()[starred[0].key_idx.get() as usize];
        assert_eq!(starred[0].range.slice(text), "*******");
    }

    // ── Channel 2: placement and topology ─────────────────────────────────

    /// `th3e` — the case the idea document was written for and no shipped rule
    /// provides. The digit is common (digits appear in 2,400 runs), so rarity is
    /// silent; the *placement* is unique, so a placement channel carries it.
    ///
    /// MEASURED CONSEQUENCE OF CLASS-CONDITIONED TOPOLOGY: the score is unchanged,
    /// but the channel that names it moved from `Topology` to `Start`. Conditioning
    /// puts this `Both` occurrence in the identity's `Letter` cell, where it is the
    /// *only* member — this translation writes no other letter-adjacent `3` — so
    /// that cell falls under the pool floor and honestly abstains, and the
    /// class-pooled start marginal becomes the witness. The plan's canonical wording
    /// for this case ("attached to letters at both ends") is therefore no longer
    /// what ships; see the epic progress log's Entry 15.
    #[test]
    fn a_common_digit_in_an_unusual_placement_fires_through_placement() {
        let (s, reason, form, count, total) =
            judged(&synth(EN_NUM, N, &["he entered th3e house"]), &open(), "3").unwrap();
        close(s, 0.999);
        assert_eq!(reason, NonletterReason::Start);
        assert_eq!(form, NonletterForm::Letter);
        assert_eq!(
            (count, total),
            (0, 800),
            "judged against every other occurrence of this digit"
        );
    }

    /// `wo.rd` and `wo"rd`. `wo"rd` is the case the four-state topology exists for:
    /// the straight quote's two one-sided forms are both ordinary (opening
    /// `EndOnly`, closing `Neither`), so only the `Both` state is rare — and the
    /// rule reaches it without deciding whether the quote opened or closed anything.
    #[test]
    fn a_common_mark_inside_a_word_fires_through_topology() {
        for needle in [".", "\""] {
            let probe = format!("he saw the wo{needle}rd there");
            let (s, reason, form, count, _) =
                judged(&synth(EN, N, &[&probe]), &open(), needle).unwrap();
            close(s, 0.999);
            assert_eq!(reason, NonletterReason::Topology, "{needle}");
            assert_eq!(form, NonletterForm::Both, "{needle}");
            assert_eq!(count, 0, "{needle}: seen nowhere else");
        }
    }

    /// A detached mark, and a phrase-ending mark at the start of a verse — both
    /// against a translation that always attaches the mark.
    ///
    /// Same measured consequence as `th3e`: conditioning isolates these in the
    /// identity's `Detached` cell, whose only possible state IS `Neither`, so the
    /// cell is degenerate as well as thin and abstains. The score is unchanged and
    /// the start marginal names it.
    #[test]
    fn detached_and_verse_leading_marks_fire_through_placement() {
        for probe in ["he went out . and returned", ". and then he went out"] {
            let (s, reason, form, ..) = judged(&synth(EN, N, &[probe]), &open(), ".").unwrap();
            close(s, 0.999);
            assert_eq!(reason, NonletterReason::Start, "{probe:?}");
            assert_eq!(form, NonletterForm::Spaced, "{probe:?}");
        }
    }

    /// A CANDIDATE CANNOT LICENSE ITSELF AT 1/1. One medial `*` gives placement a
    /// pool of exactly one; leave-one-out empties it, so placement **abstains**
    /// rather than concluding that medial `*` is this translation's convention, and
    /// rarity carries the finding instead. This is also why an abstention must not
    /// be a zero: a zero here would have cancelled the rarity evidence through the
    /// `max`.
    #[test]
    fn a_single_medial_mark_does_not_license_itself() {
        let (s, reason, ..) = judged(&synth(EN, N, &["a new wor*d came"]), &open(), "*").unwrap();
        close(s, 1.000);
        assert_eq!(
            reason,
            NonletterReason::Rarity,
            "placement must abstain at a pool of one"
        );
    }

    /// DIRECTIVE 2's WITNESS. Class-conditioned topology cells are smaller, so the
    /// **pool floor** is what protects them: a thin cell abstains, never infers. And
    /// the two anchors topology exists for must survive the conditioning.
    ///
    /// `wo"rd` survives because the quote's `Letter` cell holds BOTH its ordinary
    /// `EndOnly` opening form and the rare `Both` — conditioning does not separate
    /// them, so the contrast that makes `wo"rd` visible is intact. The glottal-stop
    /// case survives for the mirror reason: where `Both` is the dominant form in its
    /// own cell, dominance collapses and the rule stays silent with no allow-list.
    #[test]
    fn a_conditioned_topology_cell_abstains_rather_than_inferring() {
        // `wo"rd`: the conditioned cell keeps the contrast, so topology still names it.
        let (s, reason, form, count, _) =
            judged(&synth(EN, N, &["he saw the wo\"rd there"]), &open(), "\"").unwrap();
        close(s, 0.999);
        assert_eq!(reason, NonletterReason::Topology);
        assert_eq!(form, NonletterForm::Both);
        assert_eq!(count, 0);

        // The glottal-stop shape: `Both` established as the convention in its own
        // cell — silent, with no language allow-list and no script special-casing.
        let glottal = synth(
            "ru'ux ri' k'aslemal ri' xtz'ib'aj ri' chupam ri' wuj ri'.",
            N,
            &["ja ri' xub'ij chi re"],
        );
        close(score(&glottal, "'").unwrap(), 0.0);

        // A single medial mark still cannot license itself, now per conditioned
        // cell: its `Letter` cell holds exactly one occurrence, leave-one-out empties
        // it, and it abstains rather than declaring medial `*` the convention.
        let (s, reason, ..) = judged(&synth(EN, N, &["a new wor*d came"]), &open(), "*").unwrap();
        close(s, 1.000);
        assert_eq!(reason, NonletterReason::Rarity);
    }

    /// DIRECTIVE 1's WITNESS. The schema stamp is what invalidates a cached
    /// observation when the observation's SHAPE changes, and it is folded per
    /// substrate — so a bump re-maps exactly this substrate's chapters and cannot
    /// touch another's. Pinned on the stamp itself rather than on a rebuild, because
    /// `SCHEMA_STAMP` is a compile-time const: what a test can honestly check is
    /// that the stamp carries it and that a mismatch reads as stale.
    #[test]
    fn a_schema_stamp_bump_invalidates_exactly_this_substrate() {
        use crate::substrate::{ObservationInputStamp, ObservationSubstrate};
        let corpus = synth(EN, 4, &["a ~ b"]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(
            true,
            &mut cache,
            &corpus,
            &NonletterUsageConfig::default(),
            &mut out,
        );
        let book = &corpus.book_layout()[0];
        let chapter = &book.chapters[0];
        let current =
            ObservationInputStamp::target_only::<NonletterUsageSubstrate>(chapter.hash, &());
        assert!(
            cache.observation_is_current(&book.slug, &chapter.chapter, &current),
            "the freshly mapped chapter is current at its own stamp"
        );
        // The same chapter under a DIFFERENT schema stamp is stale — which is the
        // whole mechanism, and it is scoped to this substrate because the stamp is
        // built from `S::SCHEMA_STAMP`.
        // `reference` is module-private on purpose (only the two gated constructors
        // may choose it), so the bump is expressed by mutating the public field.
        let mut bumped = current;
        bumped.schema_stamp = NonletterUsageSubstrate::SCHEMA_STAMP + 1;
        assert!(
            !cache.observation_is_current(&book.slug, &chapter.chapter, &bumped),
            "a schema-stamp bump must read every cached observation as stale"
        );
        // And a stamp mismatch really does re-map, rather than being reported stale
        // and then reused.
        cache.reset_probes();
        let mut again = Vec::new();
        drive_nonletter_usage(
            true,
            &mut cache,
            &corpus,
            &NonletterUsageConfig::default(),
            &mut again,
        );
        assert_eq!(
            cache.mapped, 0,
            "an unbumped stamp reuses every observation"
        );
        assert_eq!(again, out);
    }

    /// The same shapes, established by the translation, go quiet — and quiet because
    /// the CONVENTION is established, not because a channel abstained: the finding
    /// is still produced at an open floor, with a score of zero.
    #[test]
    fn an_established_convention_is_silent_rather_than_abstaining() {
        for (filler, probe, needle) in [
            (medial('*'), "a new wor*d came".to_string(), "*"),
            (medial('"'), "a new wo\"rd came".to_string(), "\""),
        ] {
            let corpus = synth(&filler, N, &[&probe]);
            let (s, ..) = judged(&corpus, &open(), needle)
                .unwrap_or_else(|| panic!("{probe:?}: expected a scored zero, not an abstention"));
            close(s, 0.0);
            // And at the shipped floor it does not surface at all.
            assert!(probe_finding(&corpus, &NonletterUsageConfig::default(), needle).is_none());
        }
    }

    /// Ethiopic conventions, learned with no allow-list and no script special
    /// casing: the word separator `፡` and a detached terminal `።` are both this
    /// translation's own house style, so both are silent.
    #[test]
    fn ethiopic_conventions_are_learned_not_listed() {
        let sep = synth(AM, N, &["ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም።"]);
        close(score(&sep, "\u{1361}").unwrap(), 0.0);
        let detached = synth("ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም ። ወስብኩ፡ ወንጌለ ።", N, &["ወይቤሎሙ፡ ሑሩ ።"]);
        close(score(&detached, "\u{1362}").unwrap(), 0.0);
    }

    // ── Channel 3: sequence and continuation ──────────────────────────────

    /// An unseen directed pairing fires; an established one is silent. `k = 2` makes
    /// the channel honestly binary, which is exactly the claim its message makes.
    #[test]
    fn directed_pairs_fire_only_on_a_pairing_this_translation_never_writes() {
        // A filler where `.` always leads a closing curly quote and `,` always
        // stands detached, so BOTH members of the probe's `.,` run sit in their own
        // established placements and only the PAIRING is new — otherwise the
        // coalesced run would fire on a placement instead, which is correct but
        // would not exercise this channel.
        let filler = "And he said.\u{201D} Go ye , and preach.\u{201D}";
        let unseen = synth(filler, N, &["And he said., Go ye"]);
        let (s, reason, _, count, total) = judged(&unseen, &open(), ".,").unwrap();
        close(s, 0.999);
        assert_eq!(reason, NonletterReason::Pair);
        assert_eq!(
            (count, total),
            (0, 1600),
            "seen nowhere else, against every other run this mark leads"
        );
        match probe_finding(&unseen, &open(), ".,").unwrap().args {
            Some(FindingArgs::NonletterUsage { partner, .. }) => assert_eq!(partner, ","),
            other => panic!("{other:?}"),
        }

        // The established pairing is silent, and silent because the convention IS
        // established: the finding is still produced at an open floor, scoring zero.
        let established = synth(filler, N, &["And he said.\u{201D} Go ye"]);
        close(score(&established, ".").unwrap(), 0.0);
    }

    /// `word?!` over an established `?` and an established `!`: neither glyph is
    /// rare and neither edge is unusual, but the *pairing* is new.
    #[test]
    fn an_unseen_pairing_of_two_established_marks_fires() {
        let corpus = synth(
            "And he said, \"Why?\" And they said, \"Go!\" And he preached.",
            N,
            &["and he said, \"Why?!\" go"],
        );
        let (s, reason, ..) = judged(&corpus, &open(), "?!").unwrap();
        close(s, 0.999);
        assert_eq!(reason, NonletterReason::Pair);
    }

    /// The bounded continuation component earns its production state: `:::` over an
    /// established `::` is a run length this translation never writes, and directed
    /// pairs cannot reach it because BOTH edges of `:::` are the familiar `: → :`.
    #[test]
    fn continuation_reaches_a_run_length_pairs_cannot() {
        let filler = "And he said:: go ye into all the world:: and preach:: to them.";
        let longer = synth(filler, N, &["he said::: go"]);
        let (s, reason, ..) = judged(&longer, &open(), ":::").unwrap();
        close(s, 1.000);
        assert_eq!(reason, NonletterReason::Continuation);
        // The established doubling is silent, and so is a doubled Ethiopic terminal.
        close(
            score(&synth(filler, N, &["he said:: go"]), "::").unwrap(),
            0.0,
        );
        close(
            score(
                &synth(
                    "ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም።። ወስብኩ፡ ወንጌለ፡ ለኵሉ።።",
                    N,
                    &["ወይቤሎሙ፡ ሑሩ።።"],
                ),
                "\u{1362}\u{1362}",
            )
            .unwrap(),
            0.0,
        );
    }

    /// `..` over a translation whose only period run is `."`: the pair channel
    /// carries it, because `. → .` is a pairing this translation never writes even
    /// though `. → "` is its convention.
    #[test]
    fn a_doubled_mark_fires_through_the_pair_channel() {
        let (s, reason, ..) = judged(
            &synth(EN, N, &["and he preached unto them.."]),
            &open(),
            "..",
        )
        .unwrap();
        close(s, 0.999);
        assert_eq!(reason, NonletterReason::Pair);
    }

    // ── Digit pooling: the division of labour ─────────────────────────────

    /// **Nd** digits pool into one class identity for rarity, so which numbers a
    /// translation happens to write is not mistaken for an orthographic convention.
    /// The predicted division of labour, all four cases:
    ///
    /// - a stray digit in a digit-free translation fires through CLASS rarity;
    /// - an ordinary digit where numbers are common is silent;
    /// - `th3e` still fires, through placement (pinned separately above);
    /// - numeric grouping is silent.
    #[test]
    fn nd_digits_pool_for_rarity_but_placement_still_fires() {
        let (s, reason, ..) =
            judged(&synth(EN, N, &["there were 7 of them"]), &open(), "7").unwrap();
        close(s, 1.000);
        assert_eq!(reason, NonletterReason::Rarity);

        close(
            score(&synth(EN_NUM, N, &["and 3 more came"]), "3").unwrap(),
            0.0,
        );

        // Numeric grouping is silent because the digit class is established, not
        // because a channel abstained — the run is still judged, and scores zero.
        close(
            score(
                &synth(EN_NUM, N, &["there were 1,000 of them and 2,000 more"]),
                "1,000",
            )
            .unwrap(),
            0.0,
        );
    }

    /// **No**/**Nl** numerals deliberately do NOT pool. `is_numeric()` is the fused
    /// `NUMERIC` bit over all of `N*`, and classifying on it pooled `²` into the
    /// digit participant — costing it both its own identity and its ability to fire.
    /// Each keeps its identity and fires in a digit-rich translation.
    #[test]
    fn superscript_and_vulgar_numerals_keep_their_own_identity() {
        for (probe, needle) in [
            ("the second\u{00B2} book", "\u{00B2}"),
            ("about \u{00BD} of them", "\u{00BD}"),
        ] {
            let (s, reason, ..) = judged(&synth(EN_NUM, N, &[probe]), &open(), needle)
                .unwrap_or_else(|| panic!("{probe:?} produced no finding"));
            close(s, 1.000);
            assert_eq!(reason, NonletterReason::Rarity, "{probe:?}");
        }
    }

    /// The pooled pair key is a sentinel a real grapheme cannot equal, so a literal
    /// `#` beside another nonletter is its own pair participant and not confused
    /// with a digit. (The calibration probe used a bare `#`, which collides.)
    #[test]
    fn the_pooled_digit_key_cannot_collide_with_a_literal_hash() {
        assert_eq!(pool_key("7", CandClass::Digit), DIGIT_POOL_KEY);
        assert_eq!(pool_key("#", CandClass::Symbol), "#");
        assert_ne!(pool_key("#", CandClass::Symbol), DIGIT_POOL_KEY);
        assert!(matches!(classify(DIGIT_POOL_KEY), Kind::Hygiene));
    }

    // ── Candidate domain edges ────────────────────────────────────────────

    /// A cluster with an alphabetic base is CONTEXT: its combining marks stay part
    /// of it and never become candidates, whichever raw encoding the translation
    /// used. That is also where the `uni.mixed-normalization` overlap dissolves —
    /// both forms are alphabetic here, so neither is a candidate and the two rules
    /// cannot both own the span.
    #[test]
    fn an_alphabetic_base_with_combining_marks_is_never_a_candidate() {
        assert!(matches!(classify("e\u{0301}"), Kind::Alpha));
        assert!(matches!(classify("\u{00E9}"), Kind::Alpha));
        let corpus = synth(EN, N, &["caf\u{00E9} and cafe\u{0301} both"]);
        let found = nonletter_usage_findings(&corpus, &open());
        let last = corpus.keys().len() - 1;
        assert!(
            found.iter().all(|f| f.key_idx.get() as usize != last),
            "an accented word is context, not a candidate"
        );
    }

    /// Hygiene's domain and a baseless combining mark are excluded from candidacy,
    /// so deterministic hygiene and this rule can never both own a span.
    #[test]
    fn hygiene_and_baseless_marks_are_excluded_from_candidacy() {
        assert!(matches!(classify("\u{0301}"), Kind::BaselessMark));
        assert!(matches!(classify("\u{0000}"), Kind::Hygiene));
        assert!(matches!(classify("\u{200B}"), Kind::Hygiene));
        assert!(matches!(classify("\u{FFFD}"), Kind::Hygiene));
        let corpus = synth(EN, N, &["a \u{0301} and \u{0000} and \u{FFFD} here"]);
        let last = corpus.keys().len() - 1;
        assert!(
            nonletter_usage_findings(&corpus, &open())
                .iter()
                .all(|f| f.key_idx.get() as usize != last),
            "hygiene's domain produces no candidate"
        );
    }

    // ── Boundaries: the one legitimate seam effect ─────────────────────────

    /// A CHAPTER SEAM IS NOT A DISCOURSE RESET. Splitting the same text across two
    /// chapters must produce exactly the same findings, at exactly the same verses,
    /// with exactly the same scores — the deferred-edge machinery's whole purpose.
    ///
    /// The split falls between `T3` and `T4`, and `T3` ends with a candidate while
    /// `T4` begins with one, so both deferred edges are exercised.
    #[test]
    fn a_chapter_seam_reads_as_spaced_continuity() {
        let verses = [
            "And he said unto them, \"Go ye.\"",
            "And they went out, and preached.",
            "And he wrote it down.",
            "\"Behold,\" he said, \"the word.\"",
            "And the people heard him, and believed.",
            "And it came to pass, and they rejoiced.",
        ];
        let one: Vec<(&str, &str, u16, &str)> = verses
            .iter()
            .enumerate()
            .map(|(i, t)| ("GEN", "1", i as u16 + 1, *t))
            .collect();
        let two: Vec<(&str, &str, u16, &str)> = verses
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if i < 3 {
                    ("GEN", "1", i as u16 + 1, *t)
                } else {
                    ("GEN", "2", i as u16 - 2, *t)
                }
            })
            .collect();
        let a = rows(&one);
        let b = rows(&two);
        assert_eq!(tallies(&a).len(), tallies(&b).len());
        for (glyph, ta) in tallies(&a) {
            let tb = tallies(&b)
                .remove(&glyph)
                .unwrap_or_else(|| panic!("{glyph:?} vanished across the chapter split"));
            assert_eq!(
                ta.counters, tb.counters,
                "{glyph:?}'s counters moved across a chapter seam"
            );
            assert_eq!(ta.pairs, tb.pairs, "{glyph:?}'s pairs moved");
        }
        let render = |c: &Corpus| -> Vec<String> {
            nonletter_usage_findings(c, &open())
                .into_iter()
                .map(|f| {
                    let text = &c.texts()[f.key_idx.get() as usize];
                    format!("{}|{:?}", f.range.slice(text), f.score)
                })
                .collect()
        };
        assert_eq!(render(&a), render(&b));
    }

    /// A BOOK boundary is the real reset: a run touching a book edge has no
    /// neighbour across the seam and that side abstains. Splitting the same verses
    /// into two books therefore withdraws exactly two side observations — the
    /// trailing edge of the first book and the leading edge of the second — while
    /// the chapter split above withdraws none.
    #[test]
    fn a_book_boundary_abstains_where_a_chapter_seam_does_not() {
        // The seam falls between verse 1 and verse 2: verse 1 ENDS with a quote
        // (the trailing deferred edge) and verse 2 BEGINS with one (the leading
        // deferred edge), so both edges are exercised. Verse 3 ends on a letter so
        // the corpus's own final edge is not in play.
        let verses = [
            "And he said unto them, \"Go ye.\"",
            "And they went out, and preached.\"",
            "\"Behold, he said, the word.",
            "And the people heard him, and believed",
        ];
        let chaptered = rows(&[
            ("GEN", "1", 1, verses[0]),
            ("GEN", "1", 2, verses[1]),
            ("GEN", "2", 1, verses[2]),
            ("GEN", "2", 2, verses[3]),
        ]);
        let split = rows(&[
            ("GEN", "1", 1, verses[0]),
            ("GEN", "1", 2, verses[1]),
            ("EXO", "1", 1, verses[2]),
            ("EXO", "1", 2, verses[3]),
        ]);
        // Verse 2 ends with `.` inside a `."` run; verse 3 opens with `"`.
        let quote_ends = |c: &Corpus| tallies(c)[&Box::from("\"")].counters[C_END + 2];
        let quote_starts = |c: &Corpus| tallies(c)[&Box::from("\"")].counters[C_START + 2];
        assert_eq!(
            quote_ends(&chaptered) - quote_ends(&split),
            1,
            "the first book's trailing quote loses its spaced end observation"
        );
        assert_eq!(
            quote_starts(&chaptered) - quote_starts(&split),
            1,
            "the second book's leading quote loses its spaced start observation"
        );
    }

    /// A chapter with no candidate at all still proves a neighbour exists, so the
    /// NEXT chapter's leading edge reads as spaced rather than as a book edge. This
    /// is why the boundary carries an explicit `seen_previous` instead of inferring
    /// it from an empty pending slot — the defect would be invisible except in
    /// corpora that happen to hold a punctuation-free chapter.
    #[test]
    fn a_candidate_free_chapter_still_proves_a_neighbour_exists() {
        let with_gap = rows(&[
            ("GEN", "1", 1, "and he said unto them go ye"),
            ("GEN", "2", 1, "no punctuation at all here"),
            ("GEN", "3", 1, "\"behold he said the word"),
        ]);
        let without_gap = rows(&[
            ("GEN", "1", 1, "and he said unto them go ye"),
            ("GEN", "2", 1, "\"behold he said the word"),
        ]);
        // In both, the leading `"` of the last chapter is spaced across the seam.
        assert_eq!(
            tallies(&with_gap)[&Box::from("\"")].counters[C_START + 2],
            1
        );
        assert_eq!(
            tallies(&without_gap)[&Box::from("\"")].counters[C_START + 2],
            1
        );
        // And a book-initial one is NOT — the contrast that makes the assertion
        // above meaningful.
        let at_book_start = rows(&[("GEN", "1", 1, "\"behold he said the word")]);
        assert_eq!(
            tallies(&at_book_start)[&Box::from("\"")].counters[C_START + 2],
            0
        );
    }

    /// A chapter whose entire content is one single-member nonletter run: BOTH of
    /// its outer contexts are deferred at once, and both resolve — spaced from its
    /// neighbours, or a book edge when it has none.
    #[test]
    fn one_run_can_be_a_whole_chapter_and_both_edges_still_resolve() {
        let middle = rows(&[
            ("GEN", "1", 1, "and he said unto them"),
            ("GEN", "2", 1, "."),
            ("GEN", "3", 1, "and they went out"),
        ]);
        let t = &tallies(&middle)[&Box::from(".")];
        assert_eq!(t.counters[C_START + 2], 1, "spaced start across the seam");
        assert_eq!(t.counters[C_END + 2], 1, "spaced end across the seam");
        assert_eq!(
            t.counters[C_TOPO + topo_cell(TopoClass::Detached, Topology::Neither)],
            1,
            "attached to content on neither side"
        );

        let alone = rows(&[("GEN", "1", 1, ".")]);
        let t = &tallies(&alone)[&Box::from(".")];
        assert_eq!(t.counters[C_START + 2], 0, "no neighbour at a book edge");
        assert_eq!(t.counters[C_END + 2], 0);
        assert!(
            t.counters[C_TOPO..C_TOPO + TOPO_CLASSES * TOPOLOGIES]
                .iter()
                .all(|&c| c == 0),
            "both sides unobservable ⇒ topology abstains rather than reading Neither"
        );
    }

    /// A run-INTERIOR candidate has no outer context on either side, and topology
    /// abstains rather than reading `Neither`. Collapsing the two was falsified on
    /// the fleet: `?!"`'s `!` scored 0.999 on evidence `0/1601` because `Neither`
    /// then pooled "detached from content" with "surrounded by other nonletters".
    #[test]
    fn a_run_interior_candidate_abstains_on_topology() {
        assert_eq!(
            Topology::of(NeighbourClass::Internal, NeighbourClass::Internal),
            None
        );
        let corpus = synth(EN, N, &["he said, \"Why?!\u{201D} and left"]);
        // The `!` sits between `?` and `”`: interior on both sides, so it
        // contributes no topology at all.
        let t = &tallies(&corpus)[&Box::from("!")];
        assert!(
            t.counters[C_TOPO..C_TOPO + TOPO_CLASSES * TOPOLOGIES]
                .iter()
                .all(|&c| c == 0)
        );
    }

    // ── Coalescing, ordering and the args ─────────────────────────────────

    /// ONE finding per maximal run, spanning the whole run, however many of its
    /// members fire — and every other channel that cleared the floor travels in the
    /// args so no violated fact is lost to the `max`.
    #[test]
    fn a_run_coalesces_into_one_finding_carrying_every_violated_reason() {
        let corpus = synth(EN, N, &["he said unto them~$ go ye"]);
        let last = corpus.keys().len() - 1;
        let found: Vec<Finding> = nonletter_usage_findings(&corpus, &open())
            .into_iter()
            .filter(|f| f.key_idx.get() as usize == last)
            .collect();
        assert_eq!(found.len(), 1, "one run, one finding: {found:?}");
        let text = &corpus.texts()[last];
        assert_eq!(found[0].range.slice(text), "~$");
        match &found[0].args {
            Some(FindingArgs::NonletterUsage { reason, also, .. }) => {
                assert_eq!(*reason, NonletterReason::Rarity);
                assert!(
                    also.windows(2)
                        .all(|w| reason_rank(w[0]) < reason_rank(w[1])),
                    "the also list is deterministic and deduplicated: {also:?}"
                );
                assert!(!also.contains(reason));
            }
            other => panic!("{other:?}"),
        }
    }

    /// Findings come out in the final stable order — verse, then byte offset — which
    /// is the scan order the retained sites are recorded in.
    #[test]
    fn findings_are_emitted_in_corpus_scan_order() {
        let corpus = synth(EN, N, &["a ~ b $ c { d"]);
        let found = nonletter_usage_findings(&corpus, &NonletterUsageConfig::default());
        assert!(
            found
                .windows(2)
                .all(|w| (w[0].key_idx, w[0].range.start) <= (w[1].key_idx, w[1].range.start))
        );
        assert!(
            found
                .iter()
                .all(|f| f.code == RuleId::NonletterUsageAnomaly)
        );
        assert!(found.iter().all(|f| f.severity == Severity::Info));
    }

    /// The published message names the habit and the leave-one-out counts, never a
    /// confidence adjective — and it is truthful about the pooled digit: the partner
    /// rendered is the digit actually written.
    #[test]
    fn the_message_names_the_habit_and_the_counts() {
        let corpus = synth(EN_NUM, N, &["he entered th3e house"]);
        let f = probe_finding(&corpus, &open(), "3").unwrap();
        let rendered = crate::catalog::message(RuleId::NonletterUsageAnomaly, f.args.as_ref());
        assert_eq!(
            rendered,
            "\u{2018}3\u{2019} is attached to a word at the start here; this translation writes \
             it that way in 0 of 800 other places."
        );
        let rare = synth(EN, N, &["procrastinate ~ my case"]);
        let f = probe_finding(&rare, &open(), "~").unwrap();
        assert_eq!(
            crate::catalog::message(RuleId::NonletterUsageAnomaly, f.args.as_ref()),
            "\u{2018}~\u{2019} appears in only one place in this translation."
        );
    }

    // ── Monotonicity, config isolation and resident equivalence ───────────

    /// THE PERMANENT GATE AGAINST THE FLAT KNEE. A slip cloud whose size grew with
    /// the translation's volume must survive the recurrence knee — and must NOT
    /// survive it when the opportunity-proportional term is switched off.
    ///
    /// This is the defect the migration ledger's obligation (b) caught, in
    /// synthetic form so it cannot come back outside a fleet run. `engwebster`
    /// writes `-` attached 3,430 times and spaced 19 — a 5.5-per-1,000 slip cloud,
    /// every member a broken hyphenation (`life -time`, `high -ways`) that ADR 0054
    /// shipped as a finding. Under a flat knee of 8 all 19 scored **0.173**;
    /// ADR 0050's proportional knee is what readmits them.
    ///
    /// Both the slip count and the volume are DERIVED from the shipped config, so
    /// the test keeps its meaning through a recalibration: it asks only that the
    /// proportional term do real work, never that it be any particular size.
    #[test]
    fn a_slip_cloud_that_grew_with_volume_survives_the_recurrence_knee() {
        let cfg = NonletterUsageConfig::default();
        assert!(
            cfg.placement_rate_per_10k > 0.0,
            "a flat placement knee silences volume-grown slip clouds — ADR 0050"
        );
        // Two slips past the flat knee's base, so a flat knee scores exactly zero.
        let slips = cfg.placement_k.ceil() as usize + 2;
        // The pool volume at which the proportional term lifts that same cloud
        // clear of the floor: knee(slips - 1, K) >= target  =>  K >= (slips-1)/(1-target).
        let target = f64::from(cfg.emit_score_min) + 0.05;
        let need_knee = (slips as f64 - 1.0) / (1.0 - target);
        let need_pool = ((need_knee - f64::from(cfg.placement_k))
            / f64::from(cfg.placement_rate_per_10k)
            * 10_000.0)
            .ceil()
            .max(0.0) as usize;
        // Filler establishing the attached form; ten hyphens a verse keeps the
        // verse count down.
        const PER_VERSE: usize = 10;
        const FILLER: &str = "a-b c-d e-f g-h i-j k-l m-n o-p q-r s-t.";
        let verses = need_pool / PER_VERSE + 1;
        let probes: Vec<&str> = (0..slips).map(|_| "the high -way there").collect();
        let corpus = synth(FILLER, verses, &probes);

        let hit = judged(&corpus, &cfg, "-way")
            .expect("the slip cloud must produce a finding at the shipped floor");
        assert_eq!(hit.1, NonletterReason::Topology);
        assert!(
            hit.0 >= cfg.emit_score_min,
            "the slip cloud scored {} against a floor of {}",
            hit.0,
            cfg.emit_score_min
        );

        // The same cloud under a FLAT knee: silenced, which is the regression.
        let flat = NonletterUsageConfig {
            placement_rate_per_10k: 0.0,
            ..cfg
        };
        let flat_score = judged(&corpus, &flat, "-way").map_or(0.0, |h| h.0);
        assert!(
            flat_score < cfg.emit_score_min,
            "a flat knee must NOT reach the floor here ({flat_score}); if it does, \
             this witness has stopped testing the proportional term"
        );
    }

    /// REMOVAL MONOTONICITY: correcting one of two anomalous occurrences must make
    /// the remaining one MORE suspicious, never less. Clean-as-you-go sharpens the
    /// signal; a non-monotone denominator accident would punish the translator for
    /// fixing something.
    #[test]
    fn removing_one_occurrence_raises_the_survivor() {
        let two = score(&synth(EN, N, &["a ~ here", "b ~ there"]), "~").unwrap();
        let one = score(&synth(EN, N, &["a plain here", "b ~ there"]), "~").unwrap();
        assert!(one > two, "{one} must exceed {two}");
    }

    /// A judging-knob change re-judges from RETAINED observations: zero chapters
    /// mapped, zero reduced. This is what makes the Review Depth slider cheap on the
    /// resident path, and it holds because the substrate has no extraction config at
    /// all.
    #[test]
    fn a_judging_only_change_maps_and_reduces_nothing() {
        let corpus = synth(EN, N, &["procrastinate ~ my case"]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(true, &mut cache, &corpus, &open(), &mut out);
        assert!(cache.mapped > 0, "the cold call maps");

        let mut rejudged = Vec::new();
        drive_nonletter_usage(
            true,
            &mut cache,
            &corpus,
            &NonletterUsageConfig::default(),
            &mut rejudged,
        );
        assert_eq!(cache.mapped, 0, "a judging knob maps nothing");
        assert_eq!(cache.reduced, 0, "a judging knob reduces nothing");
        assert!(!rejudged.is_empty(), "and the findings still move");
        assert!(rejudged.len() < out.len());
    }

    /// The Review Depth profile is monotone in every knob, its midpoint IS the
    /// shipped default, and support relaxes faster than unusualness (ADR 0070).
    #[test]
    fn the_review_depth_profile_is_monotone_and_relaxes_support_faster() {
        let at = |d: u8| config_at_review_depth(crate::review_depth::ReviewDepth::new(d).unwrap());
        assert_eq!(at(50), NonletterUsageConfig::default());
        let profile: Vec<NonletterUsageConfig> = [0, 25, 50, 75, 100].into_iter().map(at).collect();
        for pair in profile.windows(2) {
            assert!(pair[0].emit_score_min >= pair[1].emit_score_min);
            assert!(pair[0].rarity_min_exposure >= pair[1].rarity_min_exposure);
            assert!(pair[0].placement_min_pool >= pair[1].placement_min_pool);
            assert!(pair[0].sequence_min_leads >= pair[1].sequence_min_leads);
            assert!(pair[0].continuation_min_support >= pair[1].continuation_min_support);
            // The knees are the model, not the policy — depth moves the floor and
            // the support gates, never the recurrence shape.
            assert_eq!(pair[0].rarity_k, pair[1].rarity_k);
            assert_eq!(pair[0].placement_k, pair[1].placement_k);
            assert_eq!(
                pair[0].placement_rate_per_10k,
                pair[1].placement_rate_per_10k
            );
            assert_eq!(pair[0].sequence_k, pair[1].sequence_k);
            assert_eq!(pair[0].sequence_rate_per_10k, pair[1].sequence_rate_per_10k);
        }
        // Support relaxes FASTER: from the midpoint to the exploratory end every
        // support gate falls by a larger fraction than the unusualness floor does.
        let mid = &profile[2];
        let broad = &profile[4];
        let floor_ratio = f64::from(broad.emit_score_min) / f64::from(mid.emit_score_min);
        for (a, b) in [
            (broad.rarity_min_exposure, mid.rarity_min_exposure),
            (broad.placement_min_pool, mid.placement_min_pool),
            (broad.sequence_min_leads, mid.sequence_min_leads),
            (broad.continuation_min_support, mid.continuation_min_support),
        ] {
            assert!(
                f64::from(a) / f64::from(b) < floor_ratio,
                "{a}/{b} must relax faster than the floor's {floor_ratio}"
            );
        }
        // Depth moves volume monotonically, with no cliff or dead range.
        let corpus = synth(EN, N, &["a ~ b $ c wo.rd d", "he entered th3e house"]);
        let volumes: Vec<usize> = profile
            .iter()
            .map(|c| nonletter_usage_findings(&corpus, c).len())
            .collect();
        assert!(
            volumes.windows(2).all(|w| w[0] <= w[1]),
            "depth volume must be monotone: {volumes:?}"
        );
    }

    /// A resident cache's answer always equals a cold analysis of the same corpus,
    /// under randomized edits across three chapters — the aggregate under test is the
    /// incrementally maintained one, and the deferred edges resolve through the
    /// replay window rather than a rebuild.
    #[test]
    fn resident_equals_cold_under_randomized_edits() {
        let shapes = [
            "and he said unto them, \"go ye\"",
            "wo.rd here",
            "th3e house",
            ".",
            "",
            "a ~ b",
            "he said::: go",
            "plain words only",
        ];
        let mut cells: Vec<(u16, u16, String)> = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=5u16 {
                cells.push((ch, v, shapes[0].to_string()));
            }
        }
        let build = |cells: &[(u16, u16, String)]| {
            let keys = cells
                .iter()
                .map(|(c, v, _)| format!("GEN {c}:{v}"))
                .collect();
            let texts = cells.iter().map(|(_, _, t)| t.clone()).collect();
            Corpus::try_from_parts(keys, texts).unwrap()
        };
        let render = |c: &Corpus, f: &[Finding]| -> Vec<String> {
            f.iter()
                .map(|f| {
                    format!(
                        "{}|{}..{}|{:?}|{:?}",
                        c.key(f.key_idx),
                        f.range.start,
                        f.range.end,
                        f.score,
                        f.args
                    )
                })
                .collect()
        };
        let cfg = open();
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(true, &mut cache, &build(&cells), &cfg, &mut out);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let ci = (state >> 33) as usize % cells.len();
            let si = (state >> 11) as usize % shapes.len();
            cells[ci].2 = shapes[si].to_string();
            let corpus = build(&cells);
            let mut inc = Vec::new();
            drive_nonletter_usage(true, &mut cache, &corpus, &cfg, &mut inc);
            inc.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
            assert_eq!(
                render(&corpus, &inc),
                render(&corpus, &nonletter_usage_findings(&corpus, &cfg)),
                "step {step}: the resident answer diverged from cold"
            );
        }
    }

    /// Removing a book withdraws its evidence exactly, and can make a surviving
    /// occurrence newly unusual — the aggregate is integer-exact under replacement.
    #[test]
    fn removing_a_book_withdraws_its_evidence_exactly() {
        let both = rows(&[
            ("GEN", "1", 1, "a ~ here and , there"),
            ("EXO", "1", 1, "b ~ there and , here"),
        ]);
        let gen_only = rows(&[("GEN", "1", 1, "a ~ here and , there")]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(true, &mut cache, &both, &open(), &mut out);
        cache.remove_book("EXO");
        let mut after = Vec::new();
        drive_nonletter_usage(true, &mut cache, &gen_only, &open(), &mut after);
        after.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        assert_eq!(
            after,
            nonletter_usage_findings(&gen_only, &open()),
            "the incrementally withdrawn aggregate equals a cold one"
        );
    }

    /// A disabled consumer costs nothing and leaves no product behind.
    #[test]
    fn a_disabled_consumer_maps_nothing() {
        let corpus = synth(EN, 4, &["a ~ b"]);
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut out = Vec::new();
        drive_nonletter_usage(
            false,
            &mut cache,
            &corpus,
            &NonletterUsageConfig::default(),
            &mut out,
        );
        assert!(out.is_empty());
        assert_eq!(cache.mapped, 0);
    }

    /// The rule runs through `analyze` at shipped defaults — it is DEFAULT-ON,
    /// because it replaces two default-on rules and shipping it off would be a
    /// silent coverage regression.
    #[test]
    fn the_rule_is_default_on_through_analyze() {
        let corpus = synth(EN, N, &["procrastinate ~ my case"]);
        let cfg = crate::Config::v1_defaults();
        assert!(cfg.is_enabled(RuleId::NonletterUsageAnomaly));
        let found = crate::analyze_with_config(&corpus, None, &cfg);
        assert!(
            found
                .iter()
                .any(|f| f.code == RuleId::NonletterUsageAnomaly),
            "default-on and firing through analyze"
        );
        let mut off = cfg.clone();
        off.rules.insert(RuleId::NonletterUsageAnomaly, false);
        assert!(
            crate::analyze_with_config(&corpus, None, &off)
                .iter()
                .all(|f| f.code != RuleId::NonletterUsageAnomaly)
        );
    }

    /// The packed 16-byte wire record round-trips, and the digest is the finding's
    /// own leave-one-out count pair.
    #[cfg(feature = "serde")]
    #[test]
    fn the_wire_digest_is_the_leave_one_out_count_pair() {
        let args = FindingArgs::NonletterUsage {
            glyph: "3".into(),
            reason: NonletterReason::Topology,
            form: NonletterForm::Both,
            partner: String::new(),
            count: 0,
            total: 2400,
            also: vec![NonletterReason::Start, NonletterReason::End],
        };
        let json = serde_json::to_value(&args).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "nonletter-usage",
                "glyph": "3",
                "reason": "topology",
                "form": "both",
                "partner": "",
                "count": 0,
                "total": 2400,
                "also": ["start", "end"],
            })
        );
    }
}
