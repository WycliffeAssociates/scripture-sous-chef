// ═══════════════════════════════════════════════════════════════════════════
// `uni.nonletter-usage-anomaly` PROBE — dev-only. No live RuleId, config,
// catalog or wire behavior; this module only measures.
//
// Epic plan §9. The rule's claim is that a visible nonalphabetic grapheme is
// being used in a way this translation's own conventions do not establish. Three
// independently sufficient channels are proposed, composed with `max`:
//
//     score = max(absolute_rarity, placement_anomaly, sequence_anomaly)
//
// This probe reports each channel SEPARATELY, before composition, because a
// plausible combined score can hide a broken component. Every rate a candidate
// occurrence is judged against is computed LEAVE-ONE-OUT: the occurrence under
// judgment is removed from the convention evidence used to judge it, so nothing
// can license itself at 1/1.
//
// ── The observation model, and why it is shaped this way ───────────────────
//
// The atom is one UAX #29 extended grapheme cluster (`ssc_core::grapheme`, the
// same segmenter the engine ships). A cluster whose base scalar is alphabetic is
// alphabetic CONTEXT — its combining marks stay part of it and never become
// candidates. Whitespace is context. Controls, zero-width/format hazards and
// invalid code points belong to deterministic hygiene and are excluded from
// candidacy (counted separately so the exclusion is visible). A standalone
// combining mark with no base is recorded in its own bucket: hygiene owns any
// live finding for it, but its volume is a candidate-domain question.
//
// Discourse flows across verse seams (repo `CLAUDE.md`): the walk is BOOK-outer,
// not verse-local, and state resets only at a book boundary. The one legitimate
// seam effect is glyph adjacency — a mark opening verse N is not *attached* to
// the last letter of verse N−1 — so a verse seam reads as SPACED continuity, and
// a true book edge supplies no neighbour and abstains.
//
// Placement reads the content OUTSIDE the contiguous nonletter run. A candidate
// at a run edge has an outer context on that side; on an interior side it has
// none, and that side abstains. This is what stops `word."` from manufacturing
// two misleading medial topologies while leaving an isolated `wo"rd` as `Both`.
// Relationships INSIDE a run are the sequence channel's business, as directed
// grapheme pairs — never as an exact maximal-run string, which would fragment
// evidence and stop natural pairings generalizing.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ssc_core::charclass::class_of;
use ssc_core::grapheme;
use ssc_core::key::parse_key;
use ssc_core::{Corpus, Finding, RuleId};

use crate::vref_io::load_corpus;

// ───────────────────────────────────────────────────────────────────────────
// Classification
// ───────────────────────────────────────────────────────────────────────────

/// What one grapheme cluster is, for this rule's purposes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Base scalar is alphabetic — context, never a candidate.
    Alpha,
    /// Whitespace — context, and what makes a neighbour "spaced".
    Space,
    /// Deterministic hygiene's domain: control, zero-width/format, invalid code
    /// point. Excluded from candidacy so the two rules cannot both own a span.
    Hygiene,
    /// A combining mark with no alphabetic base. Observable, but hygiene owns the
    /// live finding (`uni.combining-mark-without-base`).
    BaselessMark,
    /// A visible nonalphabetic grapheme: the candidate domain.
    Candidate(CandClass),
}

/// The fine class retained on a candidate. The judge is expected to POOL these;
/// the probe keeps them fine so the pooling question can be measured rather than
/// assumed (plan §9 / the idea's open question 2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum CandClass {
    Digit,
    Quote,
    Punct,
    Symbol,
    Other,
}

impl CandClass {
    const ALL: [Self; 5] = [
        Self::Digit,
        Self::Quote,
        Self::Punct,
        Self::Symbol,
        Self::Other,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Digit => "digit",
            Self::Quote => "quote",
            Self::Punct => "punct",
            Self::Symbol => "symbol",
            Self::Other => "other",
        }
    }
}

/// The neighbour class retained on a placement observation. Fine, for the same
/// reason `CandClass` is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum NeighbourClass {
    /// Attached directly to an alphabetic grapheme.
    Letter,
    /// Attached directly to a digit.
    Digit,
    /// Whitespace on this side, or a verse seam (which reads as spaced).
    Spaced,
    /// This side is the interior of a nonletter run — no outer context. Abstains:
    /// excluded from the side's denominator entirely.
    Internal,
    /// A book edge with no neighbour across the seam. Abstains.
    Boundary,
}

impl NeighbourClass {
    /// Whether this side counts as attached to CONTENT — the input to topology.
    fn attached(self) -> bool {
        matches!(self, Self::Letter | Self::Digit)
    }
    /// Whether this side carries a judgeable observation at all.
    fn observable(self) -> bool {
        !matches!(self, Self::Internal | Self::Boundary)
    }
    fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Digit => "digit",
            Self::Spaced => "spaced",
            Self::Internal => "internal",
            Self::Boundary => "boundary",
        }
    }
}

/// The bounded four-state outer attachment topology (a settled decision, plan
/// §0.6). Necessary for direction-ambiguous marks: a straight `"` is commonly
/// `EndOnly` when opening and `StartOnly` when closing, so BOTH side marginals
/// look ordinary while `wo"rd`'s `Both` stays rare — without deciding whether the
/// quote opened or closed anything.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Topology {
    Neither,
    StartOnly,
    EndOnly,
    Both,
}

impl Topology {
    const ALL: [Self; 4] = [Self::Neither, Self::StartOnly, Self::EndOnly, Self::Both];

    /// The four-state topology, or `None` when the candidate has no outer context
    /// on EITHER side — the interior of a nonletter run.
    ///
    /// PROBE FINDING: collapsing that case into `Neither` was wrong. `Neither`
    /// then meant both "detached from content on both sides" (` , ` — the classic
    /// orphaned mark) and "surrounded by other nonletters" (`?!"`'s `!`), which
    /// are different phenomena with different priors. Pooled together, an interior
    /// occurrence of a glyph that normally sits at a run edge scored as a rare
    /// topology and fired: `!` in `?!"` read as 0-of-1,601. An interior side
    /// already abstains on the per-side marginals; topology must abstain for the
    /// same reason when BOTH sides are interior.
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
    fn label(self) -> &'static str {
        match self {
            Self::Neither => "Neither",
            Self::StartOnly => "StartOnly",
            Self::EndOnly => "EndOnly",
            Self::Both => "Both",
        }
    }
}

/// The pair participant a grapheme collapses to under `PairKeying::PoolDigits`:
/// every digit becomes `#`, everything else keeps its exact bytes.
fn pool_key(g: &str) -> Box<str> {
    match classify(g) {
        Kind::Candidate(CandClass::Digit) => Box::from("#"),
        _ => Box::from(g),
    }
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
    } else if cl.is_decimal_digit() || cl.is_numeric() {
        CandClass::Digit
    } else if cl.is_punctuation() {
        CandClass::Punct
    } else if cl.is_symbol() {
        CandClass::Symbol
    } else {
        CandClass::Other
    })
}

// ───────────────────────────────────────────────────────────────────────────
// Observations
// ───────────────────────────────────────────────────────────────────────────

/// One candidate occurrence, as the walk sees it.
#[derive(Clone)]
struct Occurrence {
    /// The candidate's exact grapheme bytes — identity is never one `char`.
    glyph: Box<str>,
    class: CandClass,
    start: NeighbourClass,
    end: NeighbourClass,
    /// The directed pair this occurrence LEADS, if the next grapheme is also a
    /// candidate in the same run.
    leads: Option<Box<str>>,
    /// This occurrence's position in its maximal run, and the run's length.
    run_index: u32,
    run_len: u32,
    /// Whether the whole run is repetitions of THIS glyph. Only then does the
    /// continuation component have a comparable population.
    same_run: bool,
    /// The run's exact text — used only to group a coalesced sample and to key
    /// the continuation histogram, never as a primary statistical identity.
    run: Box<str>,
    /// The run's own start byte within its verse, so a coalesced emitted span
    /// needs no reconstruction from `run`'s scalars (which would be wrong as soon
    /// as a run member is a multi-scalar grapheme).
    run_byte: u32,
    /// Where to find it again, for samples.
    key: Box<str>,
    /// The verse text, for rendering a sample in context.
    verse: usize,
    byte: u32,
}

/// Per-identity tallies for one corpus.
#[derive(Default, Clone)]
struct GlyphObs {
    count: u64,
    /// Start/end marginals, keyed by observable neighbour class only.
    start_forms: BTreeMap<NeighbourClass, u64>,
    end_forms: BTreeMap<NeighbourClass, u64>,
    /// Four-state topology counts.
    topology: BTreeMap<Topology, u64>,
    /// Directed pairs this glyph leads: `glyph -> next` counts.
    pairs: BTreeMap<Box<str>, u64>,
    /// The same, with every digit collapsed to one participant.
    pairs_pooled: BTreeMap<Box<str>, u64>,
    /// Occurrences of this glyph that lead SOME nonletter — the conditional
    /// denominator alternative for the pair channel.
    pair_leads: u64,
    /// Which books it appears in (breadth).
    books: BTreeSet<Box<str>>,
    /// Same-glyph run-length histogram, for the continuation question: index
    /// `n-1` counts runs of exactly `n` of this glyph. Capped at 6+.
    same_runs: [u64; 6],
    /// The number of maximal nonletter runs this identity appears in — counted
    /// ONCE per run however many times the identity occurs inside it. The
    /// run-membership rarity basis (decision 5).
    run_memberships: u64,
}

/// One corpus's whole observation set.
struct CorpusObs {
    #[allow(dead_code)] // carried for report headers; the fleet row keeps its own
    id: String,
    verses: usize,
    /// Exposure denominators the absolute-rarity channel might use.
    total_graphemes: u64,
    visible_graphemes: u64,
    alpha_graphemes: u64,
    candidate_occurrences: u64,
    /// Excluded-domain volumes, so the candidate-domain edges are visible.
    hygiene_graphemes: u64,
    baseless_marks: u64,
    /// Per-class candidate occurrence totals.
    class_totals: BTreeMap<CandClass, u64>,
    glyphs: BTreeMap<Box<str>, GlyphObs>,
    occurrences: Vec<Occurrence>,
    /// Retained-observation cost, in bytes, and the chapter count it spans.
    chapters: usize,
}

impl CorpusObs {
    /// The bytes a substrate would retain for these observations, estimated from
    /// the same shapes the production substrate would hold: per-identity tables
    /// plus one compact site record per occurrence.
    fn retained_bytes(&self) -> usize {
        let per_glyph: usize = self
            .glyphs
            .iter()
            .map(|(g, o)| {
                g.len()
                    + 16 // Box<str> header
                    + 8 // count
                    + (o.start_forms.len() + o.end_forms.len() + o.topology.len()) * 16
                    + o.pairs.keys().map(|p| p.len() + 24).sum::<usize>()
                    + o.books.iter().map(|b| b.len() + 16).sum::<usize>()
                    + 48 // same_runs + pair_leads
            })
            .sum();
        // A retained site is a packed (local verse idx, byte offset, glyph id,
        // start/end class, topology) record — 8 bytes is the realistic target.
        per_glyph + self.occurrences.len() * 8
    }
}

/// Walk one corpus BOOK-outer, collecting every observation.
fn observe(id: String, corpus: &Corpus) -> CorpusObs {
    let keys = corpus.keys();
    let texts = corpus.texts();
    let mut obs = CorpusObs {
        id,
        verses: keys.len(),
        total_graphemes: 0,
        visible_graphemes: 0,
        alpha_graphemes: 0,
        candidate_occurrences: 0,
        hygiene_graphemes: 0,
        baseless_marks: 0,
        class_totals: BTreeMap::new(),
        glyphs: BTreeMap::new(),
        occurrences: Vec::new(),
        chapters: 0,
    };

    // Book runs, derived from the keys (the layout is crate-private).
    let mut book_starts: Vec<(usize, &str)> = Vec::new();
    let mut chapters: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (i, key) in keys.iter().enumerate() {
        let Ok(parts) = parse_key(key) else { continue };
        chapters.insert((parts.book, parts.chapter));
        if book_starts.last().map(|(_, b)| *b) != Some(parts.book) {
            book_starts.push((i, parts.book));
        }
    }
    obs.chapters = chapters.len();
    let bounds: Vec<(usize, usize, &str)> = book_starts
        .iter()
        .enumerate()
        .map(|(bi, &(start, slug))| {
            let end = book_starts.get(bi + 1).map_or(keys.len(), |&(s, _)| s);
            (start, end, slug)
        })
        .collect();

    // The flattened book walk. `Cell` is one grapheme with its verse-local
    // address; a verse seam is represented by a synthetic `Space` cell so
    // adjacency across the seam reads as SPACED rather than attached — the one
    // legitimate seam effect (`CLAUDE.md`). Nothing else resets at a seam.
    struct Cell {
        kind: Kind,
        text: Box<str>,
        verse: usize,
        byte: u32,
        seam: bool,
    }

    let mut buf: Vec<grapheme::GSpan> = Vec::new();
    for &(start, end, slug) in &bounds {
        let mut cells: Vec<Cell> = Vec::new();
        for (vi, text) in texts.iter().enumerate().take(end).skip(start) {
            if vi > start {
                cells.push(Cell {
                    kind: Kind::Space,
                    text: Box::from(""),
                    verse: vi,
                    byte: 0,
                    seam: true,
                });
            }
            grapheme::segment(text, &mut buf);
            for g in &buf {
                let cluster = g.slice(text);
                cells.push(Cell {
                    kind: classify(cluster),
                    text: Box::from(cluster),
                    verse: vi,
                    byte: g.start,
                    seam: false,
                });
            }
        }

        // Tally exposure.
        for c in &cells {
            if c.seam {
                continue;
            }
            obs.total_graphemes += 1;
            match c.kind {
                Kind::Alpha => {
                    obs.alpha_graphemes += 1;
                    obs.visible_graphemes += 1;
                }
                Kind::Candidate(_) => obs.visible_graphemes += 1,
                Kind::BaselessMark => {
                    obs.baseless_marks += 1;
                    obs.visible_graphemes += 1;
                }
                Kind::Hygiene => obs.hygiene_graphemes += 1,
                Kind::Space => {}
            }
        }

        // Maximal candidate runs. A run is broken by ANY non-candidate cell,
        // including the synthetic seam cell, so a run never spans a verse seam.
        let is_cand = |c: &Cell| matches!(c.kind, Kind::Candidate(_));
        let mut i = 0usize;
        while i < cells.len() {
            if !is_cand(&cells[i]) {
                i += 1;
                continue;
            }
            let run_start = i;
            while i < cells.len() && is_cand(&cells[i]) {
                i += 1;
            }
            let run_end = i; // exclusive
            let run_len = (run_end - run_start) as u32;
            let run_text: String = cells[run_start..run_end]
                .iter()
                .map(|c| &*c.text)
                .collect::<String>();

            // The run's outer contexts. `None` on either side means a book edge.
            let outer = |at: Option<&Cell>| -> NeighbourClass {
                match at {
                    None => NeighbourClass::Boundary,
                    Some(c) if c.seam => NeighbourClass::Spaced,
                    Some(c) => match c.kind {
                        Kind::Alpha => NeighbourClass::Letter,
                        Kind::Space => NeighbourClass::Spaced,
                        // A hygiene cluster or a baseless mark is not content to
                        // be attached to; treat it as spaced rather than inventing
                        // a class the judge would have to pool.
                        Kind::Hygiene | Kind::BaselessMark => NeighbourClass::Spaced,
                        Kind::Candidate(CandClass::Digit) => NeighbourClass::Digit,
                        // Unreachable: a candidate neighbour would be in the run.
                        Kind::Candidate(_) => NeighbourClass::Spaced,
                    },
                }
            };
            let before = outer(run_start.checked_sub(1).map(|k| &cells[k]));
            let after = outer(cells.get(run_end));

            for (offset, cell) in cells[run_start..run_end].iter().enumerate() {
                let Kind::Candidate(class) = cell.kind else {
                    unreachable!("a run holds only candidates")
                };
                // Outer context only where this candidate sits at a run edge; an
                // interior side abstains.
                let start = if offset == 0 {
                    before
                } else {
                    NeighbourClass::Internal
                };
                let end = if offset + 1 == run_len as usize {
                    after
                } else {
                    NeighbourClass::Internal
                };
                let leads = cells
                    .get(run_start + offset + 1)
                    .filter(|_| offset + 1 < run_len as usize)
                    .map(|c| c.text.clone());

                let entry = obs.glyphs.entry(cell.text.clone()).or_default();
                entry.count += 1;
                entry.books.insert(Box::from(slug));
                if start.observable() {
                    *entry.start_forms.entry(start).or_default() += 1;
                }
                if end.observable() {
                    *entry.end_forms.entry(end).or_default() += 1;
                }
                if let Some(t) = Topology::of(start, end) {
                    *entry.topology.entry(t).or_default() += 1;
                }
                if let Some(next) = leads.as_ref() {
                    *entry.pairs.entry(next.clone()).or_default() += 1;
                    *entry.pairs_pooled.entry(pool_key(next)).or_default() += 1;
                    entry.pair_leads += 1;
                }

                obs.candidate_occurrences += 1;
                *obs.class_totals.entry(class).or_default() += 1;
                obs.occurrences.push(Occurrence {
                    glyph: cell.text.clone(),
                    class,
                    start,
                    end,
                    leads,
                    run_index: offset as u32,
                    run_len,
                    same_run: cells[run_start..run_end]
                        .iter()
                        .all(|c| c.text == cells[run_start].text),
                    run: Box::from(run_text.as_str()),
                    run_byte: cells[run_start].byte,
                    key: Box::from(&*keys[cell.verse]),
                    verse: cell.verse,
                    byte: cell.byte,
                });
            }

            // Run memberships: once per run per distinct identity in it.
            for glyph in cells[run_start..run_end]
                .iter()
                .map(|c| c.text.clone())
                .collect::<BTreeSet<Box<str>>>()
            {
                obs.glyphs
                    .get_mut(&glyph)
                    .expect("every run member was just inserted")
                    .run_memberships += 1;
            }

            // The same-glyph run-length histogram, for the continuation question.
            // Only a run that is entirely ONE glyph contributes: that is exactly
            // the `::` vs `:::` case pairs cannot separate.
            let all_same = cells[run_start..run_end]
                .iter()
                .all(|c| c.text == cells[run_start].text);
            if all_same {
                let slot = ((run_len as usize).min(6)) - 1;
                obs.glyphs
                    .get_mut(&cells[run_start].text)
                    .expect("just inserted")
                    .same_runs[slot] += 1;
            }
        }
    }
    obs
}

// ───────────────────────────────────────────────────────────────────────────
// Channels — each scored independently, every rate leave-one-out
// ───────────────────────────────────────────────────────────────────────────

/// The linear recurrence knee the shipped rare-glyph and spacing rules use
/// (ADR 0050/0051): `minority` occurrences of the thing under judgment, decaying
/// to zero by `k`. Fed the LEAVE-ONE-OUT count, so a singleton scores 1.0.
fn knee(minority_loo: u64, k: f64) -> f64 {
    (1.0 - (minority_loo as f64 / k)).clamp(0.0, 1.0)
}

/// Wilson lower bound on the MAJORITY share — "how confidently is the other form
/// the convention here". `k` successes of `n`, leave-one-out already applied.
fn wilson_lb(k: u64, n: u64, z: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let nf = n as f64;
    let p = (k as f64 / nf).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / nf;
    let center = (p + z2 / (2.0 * nf)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}

/// A judged occurrence, one score per channel plus the evidence behind each.
struct Scored {
    rarity: f64,
    placement: f64,
    sequence: f64,
    /// Which channel the `max` would have chosen, and the composed score.
    primary: &'static str,
    max: f64,
    /// Support figures, so an abstention is distinguishable from a zero.
    rarity_abstained: bool,
    placement_abstained: bool,
    sequence_abstained: bool,
    /// The raw evidence behind each channel, leave-one-out, so a score can never
    /// be read without the counts that produced it.
    ev_glyph: (u64, u64),
    ev_start: (u64, u64),
    ev_end: (u64, u64),
    ev_topo: (u64, u64),
    ev_pair: (u64, u64),
    ev_run: (u64, u64),
}

/// Tunables the probe sweeps rather than freezes.
/// What the absolute-rarity channel counts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RarityBasis {
    /// Raw occurrences of the identity.
    Occurrences,
    /// The number of maximal nonletter RUNS the identity appears in — option (d).
    ///
    /// The defect this repairs is identity-level self-licensing: wreckage inflates
    /// its own rarity count past the knee. In `WA-as-ulb` all 11 occurrences of `*`
    /// ARE the two junk runs (`*******` and `****`), so occurrence counting reads
    /// `*` as recurring 11 times and `knee(10, k=8) = 0`. Counting run memberships
    /// reads it as appearing in 2 places, and since findings are already coalesced
    /// per run, leave-one-out excludes the whole run under judgment.
    RunMemberships,
}

/// How the pair channel keys its participants.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PairKeying {
    /// Every grapheme is its own pair participant.
    Exact,
    /// All digits collapse to one participant (`#`). Numeric grouping (`1,000`,
    /// `3,930`) is a nonletter run, so exact keying splits one convention across
    /// ten digits and makes the uncommon ones look unseen.
    PoolDigits,
}

/// The pair-channel denominator under test.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PairDenominator {
    /// Every occurrence of the lead glyph — broad, realizable, monotone.
    AllLeadOccurrences,
    /// Only the lead occurrences that actually lead SOME nonletter — the
    /// "given a run continues" conditional.
    LeadsARun,
}

#[derive(Clone, Copy)]
struct Knobs {
    /// What absolute rarity counts (decision 5).
    rarity_basis: RarityBasis,
    /// The continuation component's own support floor, separate from the pair
    /// channel's — option (a) lowers this so a glyph whose only occurrences are
    /// the anomaly can still be judged on run length.
    continuation_min_support: u64,
    /// Absolute rarity: the recurrence knee, and the minimum corpus exposure
    /// (candidate occurrences) below which rarity abstains — the "one `$` in a
    /// tiny corpus is thin evidence" gate.
    rarity_k: f64,
    rarity_min_exposure: u64,
    /// Placement: minimum observable pool on a side (or in the topology table)
    /// below which that component abstains rather than inventing a convention.
    placement_min_pool: u64,
    placement_z: f64,
    placement_k: f64,
    /// Sequence: minimum lead opportunities for the pair channel to speak.
    pair_min_leads: u64,
    pair_z: f64,
    pair_k: f64,
    pair_keying: PairKeying,
    pair_denominator: PairDenominator,
}

/// The Gate-1-adjudicated knobs (progress log Entry 7): rarity exposure >= 2000
/// with k = 8 on the run-membership basis, placement pool >= 30 with k = 8,
/// sequence pooled-digits / leads-a-run with leads >= 100 and k = 2.
const DEFAULT_KNOBS: Knobs = Knobs {
    rarity_basis: RarityBasis::RunMemberships,
    continuation_min_support: 100,
    rarity_k: 8.0,
    rarity_min_exposure: 2_000,
    placement_min_pool: 30,
    placement_z: 1.0,
    placement_k: 8.0,
    pair_min_leads: 100,
    pair_z: 1.0,
    pair_k: 2.0,
    pair_keying: PairKeying::PoolDigits,
    pair_denominator: PairDenominator::LeadsARun,
};

/// The pre-decision-5 baseline, for the option (a)/(b)/(d) comparison.
const OCCURRENCE_BASIS: Knobs = Knobs {
    rarity_basis: RarityBasis::Occurrences,
    ..DEFAULT_KNOBS
};

/// Option (a): keep occurrence-based rarity but lower the continuation
/// component's support floor so run length can carry a low-frequency glyph.
/// Option (b) — "run length exceeds this identity's observed maximum" — collapses
/// into the same measurement: both reduce to letting the run-length histogram
/// speak on a tiny population, and the histogram already IS the comparison
/// against the identity's other run lengths.
const OPTION_A: Knobs = Knobs {
    rarity_basis: RarityBasis::Occurrences,
    continuation_min_support: 2,
    ..DEFAULT_KNOBS
};

/// Score one occurrence, each channel independently.
fn score(occ: &Occurrence, obs: &CorpusObs, kn: Knobs) -> Scored {
    let g = &obs.glyphs[&occ.glyph];

    // ── Channel 1: absolute rarity ────────────────────────────────────────
    // "Is this grapheme itself unusually rare in this translation?" The
    // numerator is the glyph's own recurrence, LEAVE-ONE-OUT: this occurrence
    // does not count as evidence that the glyph is established. Its SUPPORT is
    // corpus exposure, not the glyph's own count — one `$` in a large corpus is
    // well-supported rarity; one `$` in a tiny corpus is thin.
    let rarity_abstained = obs.candidate_occurrences < kn.rarity_min_exposure;
    // Leave-one-out on the selected basis. On the run-membership basis the unit
    // excluded is the whole RUN under judgment, which is sound because a finding
    // is already coalesced per run — so one run is one piece of evidence, and
    // wreckage can no longer inflate its own recurrence by being long.
    let rarity_numerator = match kn.rarity_basis {
        RarityBasis::Occurrences => g.count,
        RarityBasis::RunMemberships => g.run_memberships,
    }
    .saturating_sub(1);
    let rarity = if rarity_abstained {
        0.0
    } else {
        knee(rarity_numerator, kn.rarity_k)
    };

    // ── Channel 2: placement ──────────────────────────────────────────────
    // "Given an established grapheme, is its logical start/end attachment
    // unusual HERE?" Three sub-components, combined with `max` (they describe one
    // correlated occurrence, so overlapping reasons must not inflate the score).
    // Each abstains below a minimum pool rather than hallucinating a convention.
    // Returns `(score, (mine_loo, total_loo))` so the evidence travels with the
    // score. Leave-one-out drops this occurrence from BOTH the form's count and
    // the pool, so a form seen only here reads as 0 of n-1 rather than 1 of n.
    let side = |form: NeighbourClass,
                table: &BTreeMap<NeighbourClass, u64>|
     -> Option<(f64, (u64, u64))> {
        if !form.observable() {
            return None;
        }
        let total_loo = table.values().sum::<u64>().saturating_sub(1);
        if total_loo < kn.placement_min_pool {
            return None;
        }
        let mine_loo = table.get(&form).copied().unwrap_or(0).saturating_sub(1);
        let dominance = wilson_lb(total_loo - mine_loo, total_loo, kn.placement_z);
        Some((
            dominance * knee(mine_loo, kn.placement_k),
            (mine_loo, total_loo),
        ))
    };
    let topo = match Topology::of(occ.start, occ.end) {
        None => None,
        Some(mine) => {
            let table = &g.topology;
            let total_loo = table.values().sum::<u64>().saturating_sub(1);
            if total_loo < kn.placement_min_pool {
                None
            } else {
                let mine_loo = table.get(&mine).copied().unwrap_or(0).saturating_sub(1);
                let dominance = wilson_lb(total_loo - mine_loo, total_loo, kn.placement_z);
                Some((
                    dominance * knee(mine_loo, kn.placement_k),
                    (mine_loo, total_loo),
                ))
            }
        }
    };
    let start_part = side(occ.start, &g.start_forms);
    let end_part = side(occ.end, &g.end_forms);
    let parts: Vec<f64> = [start_part, end_part, topo]
        .into_iter()
        .flatten()
        .map(|(v, _)| v)
        .collect();
    let placement_abstained = parts.is_empty();
    let placement = parts.iter().copied().fold(0.0f64, f64::max);

    // ── Channel 3: sequence ───────────────────────────────────────────────
    // "Are these individually ordinary graphemes placed beside one another in a
    // pairing this translation does not use?" Directed pairs, never exact
    // maximal-run strings. Two denominators are reported by the sweep; the score
    // here uses the broad one (all lead occurrences), which is realizable and
    // monotone.
    let pair = occ.leads.as_ref().and_then(|next| {
        let leads_loo = match kn.pair_denominator {
            PairDenominator::AllLeadOccurrences => g.count,
            PairDenominator::LeadsARun => g.pair_leads,
        }
        .saturating_sub(1);
        if leads_loo < kn.pair_min_leads {
            return None;
        }
        let (table, key) = match kn.pair_keying {
            PairKeying::Exact => (&g.pairs, next.clone()),
            PairKeying::PoolDigits => (&g.pairs_pooled, pool_key(next)),
        };
        let mine_loo = table.get(&key).copied().unwrap_or(0).saturating_sub(1);
        let dominance = wilson_lb(leads_loo - mine_loo, leads_loo, kn.pair_z);
        Some((dominance * knee(mine_loo, kn.pair_k), (mine_loo, leads_loo)))
    });
    // The bounded continuation tiebreaker, on probation: a same-glyph run of
    // length L is unusual when this corpus's OTHER runs of that glyph are not
    // that long. Only evaluated for a same-glyph run, and only for its first
    // member, so one run yields one continuation signal.
    let continuation = (occ.run_index == 0 && occ.run_len >= 2 && occ.same_run)
        .then(|| {
            let hist = &g.same_runs;
            let total_loo = hist.iter().sum::<u64>().saturating_sub(1);
            if total_loo < kn.continuation_min_support {
                return None;
            }
            let slot = ((occ.run_len as usize).min(6)) - 1;
            let mine_loo = hist[slot].saturating_sub(1);
            let dominance = wilson_lb(total_loo - mine_loo, total_loo, kn.pair_z);
            Some((dominance * knee(mine_loo, kn.pair_k), (mine_loo, total_loo)))
        })
        .flatten();
    let seq_parts: Vec<f64> = [pair, continuation]
        .into_iter()
        .flatten()
        .map(|(v, _)| v)
        .collect();
    let sequence_abstained = seq_parts.is_empty();
    let sequence = seq_parts.iter().copied().fold(0.0f64, f64::max);

    // ── Composition: `max`, never noisy-OR ────────────────────────────────
    // The channels overlap heavily. A rare glyph in a novel position beside
    // another rare glyph is not three independent witnesses. Any one channel is a
    // sufficient reason to review, and the strongest sets the score. An
    // abstention is NOT a zero that cancels another channel — it simply does not
    // participate.
    let mut primary = "none";
    let mut max = 0.0f64;
    for (name, v, abstained) in [
        ("rarity", rarity, rarity_abstained),
        ("placement", placement, placement_abstained),
        ("sequence", sequence, sequence_abstained),
    ] {
        if !abstained && v > max {
            max = v;
            primary = name;
        }
    }
    Scored {
        rarity,
        placement,
        sequence,
        primary,
        max,
        rarity_abstained,
        placement_abstained,
        sequence_abstained,
        ev_glyph: (rarity_numerator, obs.candidate_occurrences),
        ev_start: start_part.map_or((0, 0), |(_, e)| e),
        ev_end: end_part.map_or((0, 0), |(_, e)| e),
        ev_topo: topo.map_or((0, 0), |(_, e)| e),
        ev_pair: pair.map_or((0, 0), |(_, e)| e),
        ev_run: continuation.map_or((0, 0), |(_, e)| e),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Reporting helpers
// ───────────────────────────────────────────────────────────────────────────

fn pct(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((s.len() as f64 - 1.0) * q).round() as usize;
    s[idx]
}

fn pct_u(v: &[u64], q: f64) -> u64 {
    if v.is_empty() {
        return 0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let idx = ((s.len() as f64 - 1.0) * q).round() as usize;
    s[idx]
}

/// Render a grapheme for a report: escape whitespace/invisibles so a TSV row
/// stays one row and an invisible candidate is still identifiable.
fn show(g: &str) -> String {
    let mut out = String::new();
    for c in g.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{{{:04X}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Render one leave-one-out evidence pair as `minority/pool`.
fn ev(pair: (u64, u64)) -> String {
    format!("{}/{}", pair.0, pair.1)
}

/// A short window of the verse around an occurrence, for a human sample.
fn context(text: &str, byte: u32) -> String {
    let b = byte as usize;
    let lo = text[..b.min(text.len())]
        .char_indices()
        .rev()
        .nth(24)
        .map_or(0, |(i, _)| i);
    let hi = text[b.min(text.len())..]
        .char_indices()
        .nth(25)
        .map_or(text.len(), |(i, _)| b + i);
    format!("…{}…", show(&text[lo..hi]).replace('\t', " "))
}

// ───────────────────────────────────────────────────────────────────────────
// Anchor cases — the plan's named probes, on synthetic corpora
// ───────────────────────────────────────────────────────────────────────────

/// Build a synthetic corpus: `filler` repeated to establish conventions, then the
/// probe verses appended.
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

/// One anchor row: what the probe verse is, and how each channel judged the
/// candidate the anchor is about.
struct Anchor {
    name: &'static str,
    probe: String,
    glyph: String,
    count: u64,
    exposure: u64,
    start: &'static str,
    end: &'static str,
    topology: &'static str,
    scored: Option<Scored>,
}

/// Judge the anchor's target glyph in the last verse of a synthetic corpus.
fn anchor(name: &'static str, corpus: &Corpus, probe: &str, target: &str, kn: Knobs) -> Anchor {
    let obs = observe("anchor".to_string(), corpus);
    let last = corpus.keys().len() - 1;
    let hit = obs
        .occurrences
        .iter()
        .find(|o| o.verse == last && &*o.glyph == target);
    let (glyph, count, start, end, topology, scored) = match hit {
        Some(o) => {
            let s = score(o, &obs, kn);
            (
                show(&o.glyph),
                obs.glyphs[&o.glyph].count,
                o.start.label(),
                o.end.label(),
                Topology::of(o.start, o.end).map_or("interior", Topology::label),
                Some(s),
            )
        }
        None => (show(target), 0, "-", "-", "-", None),
    };
    Anchor {
        name,
        probe: show(probe),
        glyph,
        count,
        exposure: obs.candidate_occurrences,
        start,
        end,
        topology,
        scored,
    }
}

fn anchor_table(kn: Knobs) -> Vec<Anchor> {
    // Filler establishing ordinary English conventions. `N` is chosen so every
    // filler clears the rarity channel's exposure gate — otherwise a "silence"
    // could be the exposure gate abstaining rather than a convention being
    // established, and the anchor would prove nothing.
    const N: usize = 800;
    const EN: &str = "And he said unto them, \"Go ye into all the world, and preach.\"";
    /// The same, plus ordinary digits in conventional positions, so the digit
    /// anchors are judged against a corpus where digits ARE common.
    const EN_NUM: &str =
        "And he said unto them, \"Go ye 3 days into all 7 lands, and preach 40 years.\"";
    /// Ethiopic filler establishing `፡` as a word separator and `።` as a
    /// terminal.
    const AM: &str = "ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ኵሉ፡ ዓለም፡ ወስብኩ፡ ወንጌለ፡ ለኵሉ፡ ፍጥረት።";
    /// A filler that establishes a medial convention AND carries ordinary
    /// punctuation, so exposure is high and the silence is a real silence.
    fn medial(mark: char) -> String {
        format!("And the wor{mark}d of the lo{mark}rd came, and they hea{mark}rd, and said.")
    }
    let mut out = Vec::new();

    // ── Channel 1: absolute rarity ────────────────────────────────────────
    // A visible nonletter this translation otherwise never uses, against
    // substantial exposure. This is the channel that stops a one-occurrence glyph
    // vanishing because its placement denominator is 1.
    for (name, probe, target) in [
        ("~ once", "procrastinate ~ my case", "~"),
        ("$ once", "the price was $ high", "$"),
        ("{ once", "he wrote { on the wall", "{"),
        (
            "U+00B4 spacing acute once",
            "he said \u{00B4} softly",
            "\u{00B4}",
        ),
        ("% once", "some 50 % of them", "%"),
        ("superscript 2 once", "the second\u{00B2} book", "\u{00B2}"),
        ("emoji once", "he smiled \u{1F600} at them", "\u{1F600}"),
        (
            "straight vs curly quote mixing",
            "he said \u{201C}go\u{201D} to them",
            "\u{201C}",
        ),
    ] {
        out.push(anchor(name, &synth(EN, N, &[probe]), probe, target, kn));
    }

    // ── Channel 2: placement / topology ───────────────────────────────────
    // A common glyph in an attachment this translation does not otherwise use.
    out.push(anchor(
        "th3e — common digit, Both topology",
        &synth(EN_NUM, N, &["he entered th3e house"]),
        "th3e",
        "3",
        kn,
    ));
    out.push(anchor(
        "1,000 numeric grouping (should stay quiet)",
        &synth(EN_NUM, N, &["there were 1,000 of them and 2,000 more"]),
        "1,000",
        ",",
        kn,
    ));
    out.push(anchor(
        "mov$ing — symbol, Both topology",
        &synth(EN, N, &["he was mov$ing away"]),
        "mov$ing",
        "$",
        kn,
    ));
    out.push(anchor(
        "wo.rd — common period, Both topology",
        &synth(EN, N, &["he saw the wo.rd there"]),
        "wo.rd",
        ".",
        kn,
    ));
    out.push(anchor(
        "wo\"rd — quote marginals ordinary, Both rare",
        &synth(EN, N, &["he saw the wo\"rd there"]),
        "wo\"rd",
        "\"",
        kn,
    ));
    out.push(anchor(
        "detached . (spaced both sides)",
        &synth(EN, N, &["he went out . and returned"]),
        "out . and",
        ".",
        kn,
    ));
    out.push(anchor(
        "phrase-ending . at text start",
        &synth(EN, N, &[". and then he went out"]),
        "leading .",
        ".",
        kn,
    ));

    // ── Convention silences ───────────────────────────────────────────────
    // The same shapes, but established by the corpus. These must go quiet, and
    // must go quiet because the CONVENTION is established — not because a channel
    // abstained for want of support.
    out.push(anchor(
        "medial * established -> silent",
        &synth(&medial('*'), N, &["a new wor*d came"]),
        "wor*d established",
        "*",
        kn,
    ));
    out.push(anchor(
        "medial quote established -> silent",
        &synth(&medial('"'), N, &["a new wo\"rd came"]),
        "wo\"rd established",
        "\"",
        kn,
    ));
    out.push(anchor(
        "Ethiopic word separator established -> silent",
        &synth(AM, N, &["ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም።"]),
        "Ethiopic \u{1361}",
        "\u{1361}",
        kn,
    ));
    out.push(anchor(
        "detached Ethiopic terminal established -> silent",
        &synth("ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም ። ወስብኩ፡ ወንጌለ ።", N, &["ወይቤሎሙ፡ ሑሩ ።"]),
        "detached \u{1362}",
        "\u{1362}",
        kn,
    ));

    // ── Channel 3: sequence ───────────────────────────────────────────────
    out.push(anchor(
        ". -> \" established -> silent",
        &synth(EN, N, &["and he said, \"go into the world.\""]),
        "word.\"",
        ".",
        kn,
    ));
    out.push(anchor(
        ". -> , unseen pairing",
        &synth(EN, N, &["and he said unto them., go ye"]),
        "word.,",
        ".",
        kn,
    ));
    out.push(anchor(
        "?! over established ? and !",
        &synth(
            "And he said, \"Why?\" And they said, \"Go!\" And he preached.",
            N,
            &["and he said, \"Why?! go\""],
        ),
        "word?!",
        "?",
        kn,
    ));
    // Doubled punctuation, established and not.
    out.push(anchor(
        ":: established -> silent",
        &synth(
            "And he said:: go ye into all the world:: and preach:: to them.",
            N,
            &["he said:: go"],
        ),
        ":: established",
        ":",
        kn,
    ));
    out.push(anchor(
        "::: over an established :: (continuation earns its keep)",
        &synth(
            "And he said:: go ye into all the world:: and preach:: to them.",
            N,
            &["he said::: go"],
        ),
        "::: over ::",
        ":",
        kn,
    ));
    out.push(anchor(
        "Amharic doubled terminal established -> silent",
        &synth(
            "ወይቤሎሙ፡ ሑሩ፡ ውስተ፡ ዓለም።። ወስብኩ፡ ወንጌለ፡ ለኵሉ።።",
            N,
            &["ወይቤሎሙ፡ ሑሩ።።"],
        ),
        "\u{1362}\u{1362} established",
        "\u{1362}",
        kn,
    ));
    out.push(anchor(
        ".. over an established single .",
        &synth(EN, N, &["and he preached unto them.."]),
        "word..",
        ".",
        kn,
    ));

    // ── Self-licensing and thin-corpus behavior (plan §8.3 items 4 and 7) ──
    out.push(anchor(
        "singleton ~ in a TINY corpus (must abstain)",
        &synth(EN, 8, &["procrastinate ~ my case"]),
        "~ tiny corpus",
        "~",
        kn,
    ));
    out.push(anchor(
        "seen-twice ~ (cannot license itself)",
        &synth(EN, N, &["procrastinate ~ my case", "another ~ here"]),
        "~ x2",
        "~",
        kn,
    ));
    out.push(anchor(
        "seen-4x ~",
        &synth(EN, N, &["a ~ here", "b ~ there", "c ~ again", "d ~ more"]),
        "~ x4",
        "~",
        kn,
    ));
    out.push(anchor(
        "single medial * (1/1 must NOT self-license)",
        &synth(EN, N, &["a new wor*d came"]),
        "wor*d once",
        "*",
        kn,
    ));
    out
}

// ───────────────────────────────────────────────────────────────────────────
// Old-rule overlap ledger
// ───────────────────────────────────────────────────────────────────────────

/// How one old finding relates to what the probe would surface at the same span.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Disposition {
    /// The probe surfaces a candidate whose span overlaps the old finding.
    Preserved,
    /// The probe surfaces something at that span, but coalesced into one span
    /// covering a whole nonletter run rather than the old rule's exact span.
    DuplicateCoalesced,
    /// The probe observes a candidate there but its channels do not reach the
    /// threshold — an intentional move, adjudicable per case.
    IntentionallyMoved,
    /// The probe has no candidate at that span at all — a genuine coverage LOSS.
    Lost,
}

impl Disposition {
    fn label(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::DuplicateCoalesced => "duplicate-coalesced",
            Self::IntentionallyMoved => "intentionally-moved",
            Self::Lost => "lost",
        }
    }
}

/// The three retired rules, in the order the ledger reports them.
const OLD_RULES: [(&str, RuleId); 3] = [
    (
        "punct.adjacency-anomaly",
        RuleId::PunctuationAdjacencyAnomaly,
    ),
    ("lex.punct-only-token", RuleId::PunctOnlyToken),
    ("punct.spacing-anomaly", RuleId::PunctuationSpacingAnomaly),
];

/// The three retired rules' findings for one corpus, at shipped defaults.
fn old_findings(corpus: &Corpus) -> Vec<Finding> {
    use ssc_core::config::{
        PunctOnlyTokenConfig, PunctuationAdjacencyConfig, PunctuationSpacingConfig,
    };
    let mut out = ssc_core::signals::punctuation::adjacency_findings(
        corpus,
        &PunctuationAdjacencyConfig::default(),
    );
    out.extend(ssc_core::signals::lexical::punct_only_findings(
        corpus,
        &PunctOnlyTokenConfig::default(),
    ));
    out.extend(ssc_core::signals::punctuation::spacing_findings(
        corpus,
        &PunctuationSpacingConfig::default(),
    ));
    out
}

// ───────────────────────────────────────────────────────────────────────────
// Per-corpus report
// ───────────────────────────────────────────────────────────────────────────

pub(crate) fn nonletter_single_report(id: &str, corpus: &Corpus) {
    let kn = DEFAULT_KNOBS;
    let obs = observe(id.to_string(), corpus);
    println!("# nonletter probe — {id}");
    println!(
        "verses={} chapters={} total_graphemes={} visible={} alpha={} candidates={} \
         hygiene={} baseless_marks={} distinct_glyphs={} retained_bytes={} bytes_per_chapter={}",
        obs.verses,
        obs.chapters,
        obs.total_graphemes,
        obs.visible_graphemes,
        obs.alpha_graphemes,
        obs.candidate_occurrences,
        obs.hygiene_graphemes,
        obs.baseless_marks,
        obs.glyphs.len(),
        obs.retained_bytes(),
        obs.retained_bytes() / obs.chapters.max(1),
    );

    println!("\n## candidate class totals");
    for c in CandClass::ALL {
        println!(
            "{}\t{}",
            c.label(),
            obs.class_totals.get(&c).copied().unwrap_or(0)
        );
    }

    println!("\n## top glyphs (count, books, topology spread)");
    let mut rows: Vec<(&Box<str>, &GlyphObs)> = obs.glyphs.iter().collect();
    rows.sort_by_key(|(_, o)| std::cmp::Reverse(o.count));
    println!("glyph\tcount\tbooks\tNeither\tStartOnly\tEndOnly\tBoth\tpairs\tsame_runs");
    for (g, o) in rows.iter().take(40) {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}",
            show(g),
            o.count,
            o.books.len(),
            o.topology.get(&Topology::Neither).copied().unwrap_or(0),
            o.topology.get(&Topology::StartOnly).copied().unwrap_or(0),
            o.topology.get(&Topology::EndOnly).copied().unwrap_or(0),
            o.topology.get(&Topology::Both).copied().unwrap_or(0),
            o.pairs.len(),
            o.same_runs,
        );
    }

    println!("\n## per-channel scored occurrences (score >= 0.50), top 60 by max");
    let mut scored: Vec<(&Occurrence, Scored)> = obs
        .occurrences
        .iter()
        .map(|o| (o, score(o, &obs, kn)))
        .filter(|(_, s)| s.max >= 0.50)
        .collect();
    scored.sort_by(|a, b| b.1.max.partial_cmp(&a.1.max).unwrap());
    println!(
        "key\tglyph\tstart\tend\ttopology\trarity\tplacement\tsequence\tprimary\tmax\tcontext"
    );
    for (o, s) in scored.iter().take(60) {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            o.key,
            show(&o.glyph),
            o.start.label(),
            o.end.label(),
            Topology::of(o.start, o.end).map_or("interior", Topology::label),
            if s.rarity_abstained {
                "abstain".to_string()
            } else {
                format!("{:.3}", s.rarity)
            },
            if s.placement_abstained {
                "abstain".to_string()
            } else {
                format!("{:.3}", s.placement)
            },
            if s.sequence_abstained {
                "abstain".to_string()
            } else {
                format!("{:.3}", s.sequence)
            },
            s.primary,
            s.max,
            ev(s.ev_glyph),
            ev(s.ev_start),
            ev(s.ev_end),
            ev(s.ev_topo),
            ev(s.ev_pair),
            ev(s.ev_run),
            context(&corpus.texts()[o.verse], o.byte),
        );
    }
    println!("\ntotal scored >=0.50: {}", scored.len());
}

// ───────────────────────────────────────────────────────────────────────────
// Fleet sweep
// ───────────────────────────────────────────────────────────────────────────

/// One corpus's fleet row.
struct FleetRow {
    id: String,
    verses: usize,
    chapters: usize,
    total_graphemes: u64,
    candidates: u64,
    distinct: usize,
    hygiene: u64,
    baseless: u64,
    retained: usize,
    /// Per-channel counts at the sweep's emission floor, and the composed count.
    rarity_hits: u64,
    placement_hits: u64,
    sequence_hits: u64,
    max_hits: u64,
    /// Channel counts at three floors, for the depth-anchor evidence.
    hits_by_floor: [u64; 3],
    /// Abstention rates — how often each channel had nothing to say.
    rarity_abstain: u64,
    placement_abstain: u64,
    sequence_abstain: u64,
    /// Topology distribution over all candidate occurrences, plus the
    /// run-interior occurrences that have no topology at all.
    topology: [u64; 4],
    topology_interior: u64,
    /// The same, restricted to the QUOTE class — the four-state evidence the
    /// topology decision actually rests on, since a quote is the
    /// direction-ambiguous case the state was introduced for.
    quote_topology: [u64; 4],
    quote_interior: u64,
    /// Per-quote-glyph four-state spread, for the quote-heavy samples.
    quote_glyphs: BTreeMap<Box<str>, [u64; 5]>,
    /// Singleton / seen-twice behavior.
    singleton_glyphs: usize,
    twice_glyphs: usize,
    singleton_hits: u64,
    twice_hits: u64,
    /// Composed hits by candidate class — the evidence for whether the judge can
    /// pool `Letter | Punctuation | Other` or whether Digit (or Quote) needs a
    /// pool of its own.
    class_hits: BTreeMap<CandClass, u64>,
    class_occ: BTreeMap<CandClass, u64>,
    /// Decision-5 comparison: composed hits at the three depth floors under
    /// (d) run memberships, the occurrence baseline, and option (a).
    basis_d: [u64; 3],
    basis_occ: [u64; 3],
    basis_a: [u64; 3],
    /// The two RETIRED DEFAULT-ON rules' finding counts for this corpus — the
    /// baseline decision 8's default-on check compares against.
    old_default_on: u64,
    /// Per-variant hit counts, `[at 0.50, at 0.90]` per sweep row.
    rarity_sweep: Vec<[u64; 2]>,
    placement_sweep: Vec<[u64; 2]>,
    pair_sweep: Vec<[u64; 2]>,
    /// Old-rule overlap ledger for this corpus, keyed by `(old rule, disposition)`
    /// so a loss can be attributed to the rule that owned it.
    old_total: u64,
    dispositions: BTreeMap<(RuleId, Disposition), u64>,
    /// Named examples of the two dispositions that need adjudication, with
    /// context: `(rule, disposition, key, span text, context)`.
    examples: Vec<(&'static str, Disposition, String, String, String)>,
}

const FLOORS: [f64; 3] = [0.50, 0.75, 0.90];

// ── Per-channel knob sweeps ────────────────────────────────────────────────
//
// The channels are scored INDEPENDENTLY, so each sweep varies one channel's
// knobs and ignores the others. `observe` runs once per corpus and every variant
// re-scores the same observations, so a wide sweep costs almost nothing on top of
// the walk.

/// `(recurrence knee k, minimum corpus exposure)`. Exposure is what separates
/// "one `$` in a large corpus is well-supported rarity" from "one `$` in a tiny
/// corpus is thin evidence".
const RARITY_SWEEP: &[(f64, u64)] = &[
    (2.0, 0),
    (2.0, 2_000),
    (4.0, 2_000),
    (8.0, 0),
    (8.0, 2_000),
    (8.0, 10_000),
    (16.0, 2_000),
    (32.0, 2_000),
];

/// `(minimum observable pool, recurrence knee k)`.
const PLACEMENT_SWEEP: &[(u64, f64)] = &[
    (10, 4.0),
    (30, 4.0),
    (30, 8.0),
    (30, 16.0),
    (100, 8.0),
    (100, 16.0),
    (300, 16.0),
];

/// `(keying, denominator, minimum leads, recurrence knee k)`.
const PAIR_SWEEP: &[(PairKeying, PairDenominator, u64, f64)] = &[
    (
        PairKeying::Exact,
        PairDenominator::AllLeadOccurrences,
        30,
        8.0,
    ),
    (PairKeying::Exact, PairDenominator::LeadsARun, 30, 8.0),
    (
        PairKeying::PoolDigits,
        PairDenominator::AllLeadOccurrences,
        30,
        8.0,
    ),
    (PairKeying::PoolDigits, PairDenominator::LeadsARun, 30, 2.0),
    (PairKeying::PoolDigits, PairDenominator::LeadsARun, 30, 8.0),
    (PairKeying::PoolDigits, PairDenominator::LeadsARun, 100, 8.0),
    (PairKeying::PoolDigits, PairDenominator::LeadsARun, 300, 8.0),
    (
        PairKeying::PoolDigits,
        PairDenominator::LeadsARun,
        300,
        32.0,
    ),
];
/// Verse count below which a corpus counts as SMALL for the maturity split.
const SMALL_VERSES: usize = 8_000;

fn fleet_row(id: String, corpus: &Corpus, kn: Knobs, with_overlap: bool) -> FleetRow {
    let obs = observe(id.clone(), corpus);
    let mut row = FleetRow {
        id,
        verses: obs.verses,
        chapters: obs.chapters,
        total_graphemes: obs.total_graphemes,
        candidates: obs.candidate_occurrences,
        distinct: obs.glyphs.len(),
        hygiene: obs.hygiene_graphemes,
        baseless: obs.baseless_marks,
        retained: obs.retained_bytes(),
        rarity_hits: 0,
        placement_hits: 0,
        sequence_hits: 0,
        max_hits: 0,
        hits_by_floor: [0; 3],
        rarity_abstain: 0,
        placement_abstain: 0,
        sequence_abstain: 0,
        topology: [0; 4],
        topology_interior: 0,
        quote_topology: [0; 4],
        quote_interior: 0,
        quote_glyphs: BTreeMap::new(),
        singleton_glyphs: obs.glyphs.values().filter(|o| o.count == 1).count(),
        twice_glyphs: obs.glyphs.values().filter(|o| o.count == 2).count(),
        singleton_hits: 0,
        twice_hits: 0,
        basis_d: [0; 3],
        basis_occ: [0; 3],
        basis_a: [0; 3],
        old_default_on: 0,
        rarity_sweep: vec![[0; 2]; RARITY_SWEEP.len()],
        placement_sweep: vec![[0; 2]; PLACEMENT_SWEEP.len()],
        pair_sweep: vec![[0; 2]; PAIR_SWEEP.len()],
        class_hits: BTreeMap::new(),
        class_occ: obs.class_totals.clone(),
        old_total: 0,
        dispositions: BTreeMap::new(),
        examples: Vec::new(),
    };

    // Spans the probe would emit at the reference floor, for the overlap ledger.
    // A run is coalesced into ONE span, per the plan's coalescing rule.
    let mut emitted: BTreeMap<usize, Vec<(u32, u32)>> = BTreeMap::new();
    let mut observed: BTreeMap<usize, Vec<(u32, u32)>> = BTreeMap::new();

    for occ in &obs.occurrences {
        // The sweeps. Each varies one channel and reads only that channel's
        // score, so all three share one `observe` pass.
        for (i, &(k, exposure)) in RARITY_SWEEP.iter().enumerate() {
            let v = score(
                occ,
                &obs,
                Knobs {
                    rarity_k: k,
                    rarity_min_exposure: exposure,
                    ..kn
                },
            );
            if !v.rarity_abstained {
                if v.rarity >= FLOORS[0] {
                    row.rarity_sweep[i][0] += 1;
                }
                if v.rarity >= FLOORS[2] {
                    row.rarity_sweep[i][1] += 1;
                }
            }
        }
        for (i, &(pool, k)) in PLACEMENT_SWEEP.iter().enumerate() {
            let v = score(
                occ,
                &obs,
                Knobs {
                    placement_min_pool: pool,
                    placement_k: k,
                    ..kn
                },
            );
            if !v.placement_abstained {
                if v.placement >= FLOORS[0] {
                    row.placement_sweep[i][0] += 1;
                }
                if v.placement >= FLOORS[2] {
                    row.placement_sweep[i][1] += 1;
                }
            }
        }
        for (i, &(keying, denom, leads, k)) in PAIR_SWEEP.iter().enumerate() {
            let v = score(
                occ,
                &obs,
                Knobs {
                    pair_keying: keying,
                    pair_denominator: denom,
                    pair_min_leads: leads,
                    pair_k: k,
                    ..kn
                },
            );
            if !v.sequence_abstained {
                if v.sequence >= FLOORS[0] {
                    row.pair_sweep[i][0] += 1;
                }
                if v.sequence >= FLOORS[2] {
                    row.pair_sweep[i][1] += 1;
                }
            }
        }
        // Decision-5 comparison: (d) run memberships vs the occurrence baseline
        // vs option (a) (occurrences + a lowered continuation support floor).
        for (variant, which) in [
            (DEFAULT_KNOBS, 0usize),
            (OCCURRENCE_BASIS, 1),
            (OPTION_A, 2),
        ] {
            let v = score(occ, &obs, variant);
            for (i, f) in FLOORS.iter().enumerate() {
                if v.max >= *f {
                    match which {
                        0 => row.basis_d[i] += 1,
                        1 => row.basis_occ[i] += 1,
                        _ => row.basis_a[i] += 1,
                    }
                }
            }
        }
        let s = score(occ, &obs, kn);
        let slot = Topology::of(occ.start, occ.end)
            .map(|t| Topology::ALL.iter().position(|x| *x == t).unwrap());
        match slot {
            Some(i) => row.topology[i] += 1,
            None => row.topology_interior += 1,
        }
        if occ.class == CandClass::Quote {
            match slot {
                Some(i) => row.quote_topology[i] += 1,
                None => row.quote_interior += 1,
            }
            let e = row.quote_glyphs.entry(occ.glyph.clone()).or_insert([0; 5]);
            e[slot.unwrap_or(4)] += 1;
        }
        if s.rarity_abstained {
            row.rarity_abstain += 1;
        }
        if s.placement_abstained {
            row.placement_abstain += 1;
        }
        if s.sequence_abstained {
            row.sequence_abstain += 1;
        }
        let g = &obs.glyphs[&occ.glyph];
        if !s.rarity_abstained && s.rarity >= FLOORS[0] {
            row.rarity_hits += 1;
        }
        if !s.placement_abstained && s.placement >= FLOORS[0] {
            row.placement_hits += 1;
        }
        if !s.sequence_abstained && s.sequence >= FLOORS[0] {
            row.sequence_hits += 1;
        }
        for (i, f) in FLOORS.iter().enumerate() {
            if s.max >= *f {
                row.hits_by_floor[i] += 1;
            }
        }
        if s.max >= FLOORS[0] {
            row.max_hits += 1;
            *row.class_hits.entry(occ.class).or_default() += 1;
            if g.count == 1 {
                row.singleton_hits += 1;
            }
            if g.count == 2 {
                row.twice_hits += 1;
            }
            // One coalesced span per maximal run (plan §7.5): several locally
            // firing members of one run are one finding, not several.
            emitted
                .entry(occ.verse)
                .or_default()
                .push((occ.run_byte, occ.run_byte + occ.run.len() as u32));
        }
        observed
            .entry(occ.verse)
            .or_default()
            .push((occ.byte, occ.byte + occ.glyph.len() as u32));
    }

    if with_overlap {
        let old = old_findings(corpus);
        row.old_total = old.len() as u64;
        row.old_default_on = old
            .iter()
            .filter(|f| {
                matches!(
                    f.code,
                    RuleId::PunctuationAdjacencyAnomaly | RuleId::PunctOnlyToken
                )
            })
            .count() as u64;
        for f in &old {
            let vi = f.key_idx.get() as usize;
            let (fs, fe) = (f.range.start, f.range.end);
            let overlaps = |v: Option<&Vec<(u32, u32)>>| {
                v.is_some_and(|spans| spans.iter().any(|&(s, e)| s < fe && fs < e))
            };
            let exact = |v: Option<&Vec<(u32, u32)>>| {
                v.is_some_and(|spans| spans.iter().any(|&(s, e)| s == fs && e == fe))
            };
            let d = if overlaps(emitted.get(&vi)) {
                if exact(emitted.get(&vi)) {
                    Disposition::Preserved
                } else {
                    Disposition::DuplicateCoalesced
                }
            } else if overlaps(observed.get(&vi)) {
                Disposition::IntentionallyMoved
            } else {
                Disposition::Lost
            };
            *row.dispositions.entry((f.code, d)).or_default() += 1;
            // Keep a bounded sample of the two dispositions that need a human
            // decision, with enough context to make one.
            if matches!(d, Disposition::Lost | Disposition::IntentionallyMoved)
                && row.examples.len() < 6
            {
                let name = OLD_RULES
                    .iter()
                    .find(|(_, r)| *r == f.code)
                    .map_or("?", |(n, _)| *n);
                let text = &corpus.texts()[vi];
                row.examples.push((
                    name,
                    d,
                    corpus.key(f.key_idx).to_string(),
                    show(&text[fs as usize..(fe as usize).min(text.len())]),
                    context(text, fs),
                ));
            }
        }
    }
    row
}

pub(crate) fn nonletter_fleet(dir: &Path, overlap: bool) {
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("nonletter fleet: {total} corpora (overlap ledger: {overlap})");

    let kn = DEFAULT_KNOBS;
    let done = std::sync::atomic::AtomicUsize::new(0);
    let rows: Vec<FleetRow> = {
        use rayon::prelude::*;
        files
            .par_iter()
            .map(|f| {
                let id = f.file_stem().unwrap().to_string_lossy().to_string();
                let r = fleet_row(id, &load_corpus(f), kn, overlap);
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n.is_multiple_of(100) {
                    eprintln!("{n}/{total}");
                }
                r
            })
            .collect()
    };

    // ── §9: eligibility, exclusions, coverage ─────────────────────────────
    println!("# nonletter fleet survey");
    println!("corpora={total}");
    let empty: Vec<&FleetRow> = rows.iter().filter(|r| r.candidates == 0).collect();
    println!(
        "corpora with zero candidates (excluded from rate stats)={}",
        empty.len()
    );
    let eligible: Vec<&FleetRow> = rows.iter().filter(|r| r.candidates > 0).collect();
    println!("eligible corpora={}", eligible.len());
    println!(
        "`<range>` placeholder lines are dropped by the vref reader before a Corpus exists, \
         so they never enter any denominator here."
    );

    // ── Equal-corpus opportunity distributions ────────────────────────────
    println!("\n## equal-corpus opportunity distributions (one value per corpus)");
    let f = |get: fn(&FleetRow) -> u64| -> (u64, u64, u64) {
        let v: Vec<u64> = eligible.iter().map(|r| get(r)).collect();
        (pct_u(&v, 0.50), pct_u(&v, 0.90), pct_u(&v, 0.99))
    };
    println!("metric\tp50\tp90\tp99");
    for (name, get) in [
        (
            "total_graphemes",
            (|r: &FleetRow| r.total_graphemes) as fn(&FleetRow) -> u64,
        ),
        ("candidate_occurrences", |r| r.candidates),
        ("distinct_glyphs", |r| r.distinct as u64),
        ("hygiene_graphemes", |r| r.hygiene),
        ("baseless_marks", |r| r.baseless),
        ("retained_bytes", |r| r.retained as u64),
        ("retained_bytes_per_chapter", |r| {
            (r.retained / r.chapters.max(1)) as u64
        }),
    ] {
        let (a, b, c) = f(get);
        println!("{name}\t{a}\t{b}\t{c}");
    }

    // ── Per-channel finding distributions, BEFORE composition ─────────────
    println!(
        "\n## per-channel finding counts per corpus (floor {:.2}) — equal-corpus",
        FLOORS[0]
    );
    println!("channel\tp50\tp90\tp99\tfleet_total\tcorpora_firing");
    for (name, get) in [
        (
            "absolute_rarity",
            (|r: &FleetRow| r.rarity_hits) as fn(&FleetRow) -> u64,
        ),
        ("placement", |r| r.placement_hits),
        ("sequence", |r| r.sequence_hits),
        ("max(composed)", |r| r.max_hits),
    ] {
        let v: Vec<u64> = eligible.iter().map(|r| get(r)).collect();
        println!(
            "{name}\t{}\t{}\t{}\t{}\t{}",
            pct_u(&v, 0.50),
            pct_u(&v, 0.90),
            pct_u(&v, 0.99),
            v.iter().sum::<u64>(),
            v.iter().filter(|x| **x > 0).count(),
        );
    }

    // ── Abstention rates: an abstention is not a zero ─────────────────────
    println!("\n## channel abstention share of candidate occurrences (equal-corpus median)");
    println!("channel\tp50_share\tp90_share");
    for (name, get) in [
        (
            "absolute_rarity",
            (|r: &FleetRow| r.rarity_abstain) as fn(&FleetRow) -> u64,
        ),
        ("placement", |r| r.placement_abstain),
        ("sequence", |r| r.sequence_abstain),
    ] {
        let v: Vec<f64> = eligible
            .iter()
            .map(|r| get(r) as f64 / r.candidates as f64)
            .collect();
        println!("{name}\t{:.4}\t{:.4}", pct(&v, 0.50), pct(&v, 0.90));
    }

    // ── Review Depth candidate anchors: counts by floor ───────────────────
    println!("\n## composed-score counts by emission floor (Review Depth anchor evidence)");
    println!("floor\tp50\tp90\tp99\tfleet_total");
    for (i, fl) in FLOORS.iter().enumerate() {
        let v: Vec<u64> = eligible.iter().map(|r| r.hits_by_floor[i]).collect();
        println!(
            "{fl:.2}\t{}\t{}\t{}\t{}",
            pct_u(&v, 0.50),
            pct_u(&v, 0.90),
            pct_u(&v, 0.99),
            v.iter().sum::<u64>()
        );
    }

    // ── Small vs mature ───────────────────────────────────────────────────
    println!("\n## small (<{SMALL_VERSES} verses) vs mature corpora");
    for (label, set) in [
        (
            "small",
            eligible
                .iter()
                .filter(|r| r.verses < SMALL_VERSES)
                .collect::<Vec<_>>(),
        ),
        (
            "mature",
            eligible
                .iter()
                .filter(|r| r.verses >= SMALL_VERSES)
                .collect::<Vec<_>>(),
        ),
    ] {
        let hits: Vec<u64> = set.iter().map(|r| r.max_hits).collect();
        let cand: Vec<u64> = set.iter().map(|r| r.candidates).collect();
        let rarity_abst: Vec<f64> = set
            .iter()
            .map(|r| r.rarity_abstain as f64 / r.candidates as f64)
            .collect();
        println!(
            "{label}\tcorpora={}\tcandidates_p50={}\thits_p50={}\thits_p90={}\t\
             rarity_abstain_p50={:.4}",
            set.len(),
            pct_u(&cand, 0.50),
            pct_u(&hits, 0.50),
            pct_u(&hits, 0.90),
            pct(&rarity_abst, 0.50),
        );
    }

    // ── Corpus-weighted tails: the absolute-rarity flood risk ─────────────
    println!("\n## corpus-weighted tail — the 20 corpora with the most composed hits");
    println!("id\tverses\tcandidates\tdistinct\trarity\tplacement\tsequence\tmax\tper_10k_cand");
    let mut tail: Vec<&FleetRow> = eligible.to_vec();
    tail.sort_by_key(|r| std::cmp::Reverse(r.max_hits));
    for r in tail.iter().take(20) {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}",
            r.id,
            r.verses,
            r.candidates,
            r.distinct,
            r.rarity_hits,
            r.placement_hits,
            r.sequence_hits,
            r.max_hits,
            r.max_hits as f64 * 10_000.0 / r.candidates as f64,
        );
    }

    // ── Decision 5: the rarity numerator basis ────────────────────────────
    println!(
        "\n## decision 5 — rarity numerator basis, composed volume at the adjudicated \
         depth floors\n(d) run memberships | baseline = raw occurrences | (a) occurrences \
         + continuation support floor 2"
    );
    println!(
        "variant\tdepth0(.90)_p50\tdepth50(.75)_p50\tdepth100(.50)_p50\tfleet@.90\tfleet@.75\tfleet@.50"
    );
    for (label, get) in [
        (
            "(d) run memberships",
            (|r: &FleetRow| &r.basis_d) as fn(&FleetRow) -> &[u64; 3],
        ),
        ("baseline occurrences", |r| &r.basis_occ),
        ("(a) occ + cont floor 2", |r| &r.basis_a),
    ] {
        let col = |i: usize| -> Vec<u64> { eligible.iter().map(|r| get(r)[i]).collect() };
        let (c50, c75, c90) = (col(0), col(1), col(2));
        println!(
            "{label}\t{}\t{}\t{}\t{}\t{}\t{}",
            pct_u(&c90, 0.50),
            pct_u(&c75, 0.50),
            pct_u(&c50, 0.50),
            c90.iter().sum::<u64>(),
            c75.iter().sum::<u64>(),
            c50.iter().sum::<u64>(),
        );
    }

    // ── Decision 8: the default-on volume check ───────────────────────────
    println!(
        "\n## decision 8 — default-on volume check\n(the two RETIRED default-on rules' \
         per-corpus counts vs this rule at the adjudicated depth-50 floor 0.75)"
    );
    let old_on: Vec<u64> = eligible.iter().map(|r| r.old_default_on).collect();
    let new_50: Vec<u64> = eligible.iter().map(|r| r.basis_d[1]).collect();
    println!("series\tp50\tp90\tp99\tfleet_total");
    println!(
        "retired default-on pair (adjacency + punct-only)\t{}\t{}\t{}\t{}",
        pct_u(&old_on, 0.50),
        pct_u(&old_on, 0.90),
        pct_u(&old_on, 0.99),
        old_on.iter().sum::<u64>()
    );
    println!(
        "uni.nonletter-usage-anomaly at depth 50\t{}\t{}\t{}\t{}",
        pct_u(&new_50, 0.50),
        pct_u(&new_50, 0.90),
        pct_u(&new_50, 0.99),
        new_50.iter().sum::<u64>()
    );
    let (a, b) = (pct_u(&old_on, 0.50), pct_u(&new_50, 0.50));
    println!(
        "p50 ratio new/retired = {:.2} (mediator's flag threshold: > 2.00)",
        b as f64 / a.max(1) as f64
    );

    // ── Knob sweeps, per channel, independently ───────────────────────────
    let sweep_table = |name: &str,
                       header: &str,
                       labels: Vec<String>,
                       get: &dyn Fn(&FleetRow) -> &Vec<[u64; 2]>| {
        println!("\n## {name} sweep (equal-corpus per-corpus counts)");
        println!("{header}\tp50@.50\tp90@.50\tp99@.50\tfleet@.50\tp90@.90\tfleet@.90");
        for (i, label) in labels.iter().enumerate() {
            let at50: Vec<u64> = eligible.iter().map(|r| get(r)[i][0]).collect();
            let at90: Vec<u64> = eligible.iter().map(|r| get(r)[i][1]).collect();
            println!(
                "{label}\t{}\t{}\t{}\t{}\t{}\t{}",
                pct_u(&at50, 0.50),
                pct_u(&at50, 0.90),
                pct_u(&at50, 0.99),
                at50.iter().sum::<u64>(),
                pct_u(&at90, 0.90),
                at90.iter().sum::<u64>(),
            );
        }
    };
    sweep_table(
        "absolute rarity",
        "knee_k/min_exposure",
        RARITY_SWEEP
            .iter()
            .map(|(k, e)| format!("k={k}/exp>={e}"))
            .collect(),
        &|r| &r.rarity_sweep,
    );
    sweep_table(
        "placement",
        "min_pool/knee_k",
        PLACEMENT_SWEEP
            .iter()
            .map(|(p, k)| format!("pool>={p}/k={k}"))
            .collect(),
        &|r| &r.placement_sweep,
    );
    sweep_table(
        "sequence (pairs + continuation)",
        "keying/denominator/min_leads/knee_k",
        PAIR_SWEEP
            .iter()
            .map(|(ky, d, l, k)| {
                format!(
                    "{}/{}/leads>={l}/k={k}",
                    match ky {
                        PairKeying::Exact => "exact",
                        PairKeying::PoolDigits => "pool#",
                    },
                    match d {
                        PairDenominator::AllLeadOccurrences => "all-lead",
                        PairDenominator::LeadsARun => "leads-run",
                    }
                )
            })
            .collect(),
        &|r| &r.pair_sweep,
    );

    // ── Per-class hit rates: the pooling question ─────────────────────────
    println!(
        "\n## composed hits by candidate class (fleet totals, floor {:.2})",
        FLOORS[0]
    );
    println!("class\toccurrences\thits\thits_per_10k_occ");
    for c in CandClass::ALL {
        let occ: u64 = rows
            .iter()
            .map(|r| r.class_occ.get(&c).copied().unwrap_or(0))
            .sum();
        let hits: u64 = rows
            .iter()
            .map(|r| r.class_hits.get(&c).copied().unwrap_or(0))
            .sum();
        println!(
            "{}\t{}\t{}\t{:.2}",
            c.label(),
            occ,
            hits,
            hits as f64 * 10_000.0 / occ.max(1) as f64
        );
    }

    // ── Quote topology across the fleet ───────────────────────────────────
    println!("\n## four-state topology distribution over all candidate occurrences");
    let mut tot = [0u64; 4];
    for r in &rows {
        for (i, t) in tot.iter_mut().enumerate() {
            *t += r.topology[i];
        }
    }
    let interior: u64 = rows.iter().map(|r| r.topology_interior).sum();
    let sum: u64 = tot.iter().sum::<u64>() + interior;
    println!("state\tfleet_count\tshare");
    for (i, t) in Topology::ALL.iter().enumerate() {
        println!(
            "{}\t{}\t{:.5}",
            t.label(),
            tot[i],
            tot[i] as f64 / sum.max(1) as f64
        );
    }
    println!(
        "(no topology: run-interior)\t{}\t{:.5}",
        interior,
        interior as f64 / sum.max(1) as f64
    );

    // ── Quote topology: the four-state evidence, on the ambiguous case ────
    println!(
        "\n## four-state topology restricted to the QUOTE class (the direction-ambiguous \
         case the state exists for)"
    );
    let mut qt = [0u64; 4];
    let mut qi = 0u64;
    for r in &rows {
        for (i, t) in qt.iter_mut().enumerate() {
            *t += r.quote_topology[i];
        }
        qi += r.quote_interior;
    }
    let qsum: u64 = qt.iter().sum::<u64>() + qi;
    println!("state\tfleet_count\tshare_of_quote_occurrences");
    for (i, t) in Topology::ALL.iter().enumerate() {
        println!(
            "{}\t{}\t{:.5}",
            t.label(),
            qt[i],
            qt[i] as f64 / qsum.max(1) as f64
        );
    }
    println!(
        "(no topology: run-interior)\t{}\t{:.5}",
        qi,
        qi as f64 / qsum.max(1) as f64
    );

    println!(
        "\n## per-quote-glyph four-state spread — the 6 most quote-heavy corpora\n\
         (a straight quote reads EndOnly opening and StartOnly closing, so BOTH \
         marginals look ordinary while `Both` stays rare — which is the whole \
         argument for keeping the state)"
    );
    let mut quoteful: Vec<&FleetRow> = eligible.to_vec();
    quoteful.sort_by_key(|r| std::cmp::Reverse(r.quote_topology.iter().sum::<u64>()));
    println!("corpus\tglyph\tNeither\tStartOnly\tEndOnly\tBoth\tinterior\tBoth_share");
    for r in quoteful.iter().take(6) {
        let mut gs: Vec<(&Box<str>, &[u64; 5])> = r.quote_glyphs.iter().collect();
        gs.sort_by_key(|(_, c)| std::cmp::Reverse(c.iter().sum::<u64>()));
        for (g, c) in gs.iter().take(4) {
            let tot: u64 = c.iter().sum();
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.5}",
                r.id,
                show(g),
                c[0],
                c[1],
                c[2],
                c[3],
                c[4],
                c[3] as f64 / tot.max(1) as f64
            );
        }
    }

    // ── Singleton / seen-twice ────────────────────────────────────────────
    println!("\n## singleton and seen-twice behavior (self-licensing)");
    let sg: Vec<u64> = eligible.iter().map(|r| r.singleton_glyphs as u64).collect();
    let tg: Vec<u64> = eligible.iter().map(|r| r.twice_glyphs as u64).collect();
    let sh: Vec<u64> = eligible.iter().map(|r| r.singleton_hits).collect();
    let th: Vec<u64> = eligible.iter().map(|r| r.twice_hits).collect();
    println!(
        "singleton_glyph_types p50={} p90={}; they produce hits p50={} p90={} (fleet {})",
        pct_u(&sg, 0.50),
        pct_u(&sg, 0.90),
        pct_u(&sh, 0.50),
        pct_u(&sh, 0.90),
        sh.iter().sum::<u64>()
    );
    println!(
        "seen_twice_glyph_types p50={} p90={}; they produce hits p50={} p90={} (fleet {})",
        pct_u(&tg, 0.50),
        pct_u(&tg, 0.90),
        pct_u(&th, 0.50),
        pct_u(&th, 0.90),
        th.iter().sum::<u64>()
    );

    // ── Old-rule overlap ledger ───────────────────────────────────────────
    if overlap {
        println!("\n## old-rule overlap ledger, PER RETIRED RULE (fleet totals)");
        let mut led: BTreeMap<(RuleId, Disposition), u64> = BTreeMap::new();
        let mut old_total = 0u64;
        for r in &rows {
            old_total += r.old_total;
            for (k, n) in &r.dispositions {
                *led.entry(*k).or_default() += n;
            }
        }
        println!("old findings (3 retired rules, shipped defaults) = {old_total}");
        println!("old_rule\ttotal\tpreserved\tcoalesced\tintentionally_moved\tlost\tlost_share");
        const DISP: [Disposition; 4] = [
            Disposition::Preserved,
            Disposition::DuplicateCoalesced,
            Disposition::IntentionallyMoved,
            Disposition::Lost,
        ];
        for (name, rule) in OLD_RULES {
            let g = |d: Disposition| led.get(&(rule, d)).copied().unwrap_or(0);
            let tot: u64 = DISP.iter().map(|&d| g(d)).sum();
            println!(
                "{name}\t{tot}\t{}\t{}\t{}\t{}\t{:.5}",
                g(Disposition::Preserved),
                g(Disposition::DuplicateCoalesced),
                g(Disposition::IntentionallyMoved),
                g(Disposition::Lost),
                g(Disposition::Lost) as f64 / tot.max(1) as f64,
            );
        }
        let all = |d: Disposition| -> u64 {
            OLD_RULES
                .iter()
                .map(|(_, r)| led.get(&(*r, d)).copied().unwrap_or(0))
                .sum()
        };
        println!("\ndisposition\tcount\tshare_of_all_old_findings");
        for d in DISP {
            let n = all(d);
            println!(
                "{}\t{}\t{:.5}",
                d.label(),
                n,
                n as f64 / old_total.max(1) as f64
            );
        }

        println!("\n## corpora with the most LOST old findings");
        let lost_of = |r: &FleetRow| -> u64 {
            OLD_RULES
                .iter()
                .map(|(_, rule)| {
                    r.dispositions
                        .get(&(*rule, Disposition::Lost))
                        .copied()
                        .unwrap_or(0)
                })
                .sum()
        };
        let mut worst: Vec<&FleetRow> = rows.iter().filter(|r| lost_of(r) > 0).collect();
        worst.sort_by_key(|r| std::cmp::Reverse(lost_of(r)));
        println!("id\told_total\tlost");
        for r in worst.iter().take(20) {
            println!("{}\t{}\t{}", r.id, r.old_total, lost_of(r));
        }

        println!("\n## named examples — LOST and INTENTIONALLY-MOVED old findings");
        println!("corpus\told_rule\tdisposition\tkey\tspan\tcontext");
        let mut shown = 0usize;
        for r in rows.iter().filter(|r| !r.examples.is_empty()) {
            for (name, d, key, span, ctx) in &r.examples {
                if *d == Disposition::Lost {
                    println!("{}\t{name}\t{}\t{key}\t{span}\t{ctx}", r.id, d.label());
                    shown += 1;
                }
            }
            if shown >= 40 {
                break;
            }
        }
        if shown == 0 {
            println!("(no LOST findings anywhere in the fleet)");
        }
        let mut shown = 0usize;
        for r in rows.iter().filter(|r| !r.examples.is_empty()) {
            for (name, d, key, span, ctx) in &r.examples {
                if *d == Disposition::IntentionallyMoved && shown < 40 {
                    println!("{}\t{name}\t{}\t{key}\t{span}\t{ctx}", r.id, d.label());
                    shown += 1;
                }
            }
            if shown >= 40 {
                break;
            }
        }
    }

    // ── Anchor cases ──────────────────────────────────────────────────────
    println!(
        "\n## named anchor cases (synthetic corpora; filler sized so every channel's \
         support gate is cleared, so a silence is a convention and not an abstention)"
    );
    println!(
        "anchor\tglyph\tcount\texposure\tstart\tend\ttopology\trarity\tplacement\tsequence\t\
         primary\tmax\tprobe"
    );
    for a in anchor_table(kn) {
        match a.scored {
            Some(s) => println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}",
                a.name,
                a.glyph,
                a.count,
                a.exposure,
                a.start,
                a.end,
                a.topology,
                if s.rarity_abstained {
                    "abstain".into()
                } else {
                    format!("{:.3}", s.rarity)
                },
                if s.placement_abstained {
                    "abstain".into()
                } else {
                    format!("{:.3}", s.placement)
                },
                if s.sequence_abstained {
                    "abstain".into()
                } else {
                    format!("{:.3}", s.sequence)
                },
                s.primary,
                s.max,
                a.probe,
            ),
            None => println!(
                "{}\t{}\t0\t{}\t-\t-\t-\tNO-CANDIDATE\t-\t-\t-\t-\t{}",
                a.name, a.glyph, a.exposure, a.probe
            ),
        }
    }
}
