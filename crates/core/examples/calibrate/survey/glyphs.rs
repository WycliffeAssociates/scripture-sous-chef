// ═══════════════════════════════════════════════════════════════════════════
// Rare-glyph calibration. The inventory counts every scalar so a future census
// can reuse this walk. The spike's candidate rows are deliberately narrower:
// visible letters, numbers, punctuation, and symbols only.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::Corpus;
use ssc_core::charclass::class_of;
use ssc_core::token::tokenize;

use super::shared::rarity_abs;
use crate::vref_io::load_corpus;

const GLYPH_ABS_KS: [f64; 6] = [2.0, 4.0, 8.0, 16.0, 32.0, 64.0];
const GLYPH_RATE_PER_10K: [f64; 6] = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0];
const GLYPH_SWEEP_FLOOR: f64 = 0.95;
const GLYPH_HIST_LABELS: [&str; 8] = ["1", "2", "3-4", "5-8", "9-16", "17-32", "33-64", "65+"];
// Round 3: alphabet closure is now a LETTER-SCALAR share (hapax L-scalar types /
// all L-scalar occurrences), which is far smaller than the round-2 word-hapax
// share, so the self-disable sweep uses finer low-end steps: 0.001% … 2%.
const CLOSURE_SCALAR_SHARES: [f64; 8] = [0.00001, 0.0001, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02];
// Round 3: knee ≤1–5 was conjecture; sweep ≤1 through ≤8 to see where the
// retained set stops being flat.
const LETTER_RARE_MAX_COUNTS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// Representative closure threshold and knee used only to pick retained review
/// samples for the human adjudication table (not a frozen knob).
const RETAINED_SAMPLE_THRESHOLD: f64 = 0.001;
const RETAINED_SAMPLE_KNEE: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlyphLane {
    Letter,
    Number,
    Punctuation,
    Symbol,
}

impl GlyphLane {
    const ALL: [Self; 4] = [Self::Letter, Self::Number, Self::Punctuation, Self::Symbol];

    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Punctuation => 2,
            Self::Symbol => 3,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "L",
            Self::Number => "N",
            Self::Punctuation => "P",
            Self::Symbol => "S",
        }
    }
}

/// The visible candidate lanes. Marks, separators, controls, and formats are
/// inventoried but never enter the spike's rarity sweeps.
fn glyph_lane(c: char) -> Option<GlyphLane> {
    let cl = class_of(c);
    if cl.is_mark()
        || cl.is_whitespace()
        || cl.is_control()
        || cl.is_zero_width_format()
        || cl.is_invalid_codepoint()
    {
        return None;
    }
    if cl.is_numeric() {
        Some(GlyphLane::Number)
    } else if cl.is_alphabetic() {
        Some(GlyphLane::Letter)
    } else if cl.is_punctuation() {
        Some(GlyphLane::Punctuation)
    } else if cl.is_symbol() {
        Some(GlyphLane::Symbol)
    } else {
        None
    }
}

/// UAX #29 tokens that consist only of letters and their combining marks.
/// Numeric references and mixed `q1`-style tokens do not establish alphabet
/// closure or lexical concentration.
pub(crate) fn is_letter_token(word: &str) -> bool {
    let mut has_letter = false;
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_alphabetic() && !cl.is_mark() {
            has_letter = true;
        } else if !cl.is_mark() {
            return false;
        }
    }
    has_letter
}

/// Round-5 titlecase-shape + forced-position facts for one letter-token
/// occurrence. `titlecase` is the name-shape test — uppercase first letter AND
/// at least one following lowercase letter (round 4 used bare capital-initial,
/// which leaked on lone capitals `Q`/`I` and all-caps common words like `YÖ`);
/// `forced` is the casing machinery's forced-position test (book-initial, or a
/// word that consumed a bare attached terminal — verse-initial is NOT forced,
/// per `CLAUDE.md`). Only consulted for hapax words, so recording each word's
/// latest occurrence is sufficient.
#[derive(Clone, Copy)]
struct WordShape {
    titlecase: bool,
    forced: bool,
}

/// Advance the pending-terminal machine over a gap (all scalars between two
/// letter tokens), mirroring `casing::advance_gap`. The pending state is
/// `None` = no terminal seen; `Some(false)` = a bare/quoted terminal is
/// pending; `Some(true)` = a non-quote intervening punctuation collapsed the
/// boundary to mid-flow (`...`).
pub(crate) fn glyph_advance_gap(gap: &str, pending: &mut Option<bool>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some(collapsed) if !cl.is_quote() => *collapsed = true,
                Some(_) => {}
                None if *prev_letter => *pending = Some(false),
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Walk each book in canonical order, mirroring `casing::walk_book`'s pending-
/// terminal machine (carried across verse seams, reset per book; the book's
/// first word is forced), and record each letter token's capital-initial +
/// forced facts. Keyed by the same lowercase letter-token key the lexical
/// machinery uses, so the round-4 proper-noun test can look up a rare glyph's
/// hapax container. Only pure-letter tokens are recorded, matching the tokens
/// that feed `letter_words`/`glyph_words` (a hyphen-joined name is two ordinary
/// letter tokens in both, never one compound span).
fn letter_word_shapes(map: &Corpus) -> BTreeMap<String, WordShape> {
    let mut shapes: BTreeMap<String, WordShape> = BTreeMap::new();
    for group in &ssc_core::corpus::by_book(map) {
        let mut pending: Option<bool> = None;
        let mut book_initial = true;
        for text in group.texts {
            let mut prev_letter = false;
            let mut cursor = 0usize;
            for token in tokenize(text) {
                let word = token.span.slice(text);
                if !is_letter_token(word) {
                    // Not a word for the casing walk; its text stays in the gap
                    // the next letter token sees (cursor deliberately unmoved).
                    continue;
                }
                glyph_advance_gap(
                    &text[cursor..token.span.start as usize],
                    &mut pending,
                    &mut prev_letter,
                );
                let mut word_chars = word.chars();
                let first = word_chars.next().unwrap();
                // Titlecase shape: uppercase first letter AND >=1 following
                // lowercase letter. Spares genuine names (Quirinius, Roma) while
                // returning lone capitals and all-caps tokens to retained.
                let titlecase = class_of(first).is_uppercase()
                    && word_chars.any(|c| class_of(c).is_lowercase());
                let forced = book_initial || matches!(pending.take(), Some(false));
                book_initial = false;
                shapes.insert(word.to_lowercase(), WordShape { titlecase, forced });
                prev_letter = word
                    .chars()
                    .next_back()
                    .is_some_and(|c| class_of(c).is_alphabetic());
                cursor = token.span.end as usize;
            }
            glyph_advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
        }
    }
    shapes
}

fn letter_round2(
    inventory: &BTreeMap<char, u64>,
    word_tokens: BTreeMap<String, u64>,
    glyph_words: BTreeMap<char, BTreeMap<String, u64>>,
    shapes: &BTreeMap<String, WordShape>,
) -> LetterRound2 {
    let tokens: u64 = word_tokens.values().sum();
    let hapax_types = word_tokens.values().filter(|&&count| count == 1).count() as u64;
    // Letter-scalar closure straight off the inventory the harness already built.
    let mut letter_scalars = 0u64;
    let mut hapax_letter_scalars = 0u64;
    for (&glyph, &count) in inventory {
        if glyph_lane(glyph) == Some(GlyphLane::Letter) {
            letter_scalars += count;
            if count == 1 {
                hapax_letter_scalars += 1;
            }
        }
    }
    let mut rare = Vec::new();
    for (&glyph, &count) in inventory {
        if glyph_lane(glyph) != Some(GlyphLane::Letter)
            || count > *LETTER_RARE_MAX_COUNTS.last().unwrap()
        {
            continue;
        }
        let Some(words) = glyph_words.get(&glyph) else {
            rare.push(LetterRare {
                glyph,
                count,
                lexical_word: None,
                lexical_word_tokens: 0,
                proper_noun_shape: false,
            });
            continue;
        };
        let accounted: u64 = words.values().sum();
        let dominant = words.iter().max_by_key(|(_, occurrences)| **occurrences);
        let (lexical_word, lexical_word_tokens) = match dominant {
            Some((word, &occurrences))
                if accounted == count
                    && occurrences == count
                    && word_tokens.get(word).copied().unwrap_or(0) >= 2 =>
            {
                (Some(word.clone()), word_tokens[word])
            }
            _ => (None, 0),
        };
        // Round-5 proper-noun-shape discount: only where the recurring-word
        // lexical discount did NOT already fire. It applies when the glyph's
        // sole containing word type is a hapax (occurs once) AND that lone
        // occurrence is titlecase-shaped (upper first + >=1 following lower) AND
        // at a non-forced (mid-flow) position. A capital at a forced position is
        // capitalised for position reasons — shape says nothing — so no discount
        // there (the flag survives). The titlecase test (round 5, was bare
        // capital-initial) returns lone capitals and all-caps tokens to
        // retained. Bicameral-only by construction: `titlecase` is false for
        // caseless scripts, so the branch never fires for them.
        let proper_noun_shape = lexical_word.is_none()
            && words.len() == 1
            && accounted == count
            && words.values().next().is_some_and(|&occ| occ == count)
            && words
                .keys()
                .next()
                .and_then(|word| {
                    (word_tokens.get(word).copied().unwrap_or(0) == 1)
                        .then(|| shapes.get(word))
                        .flatten()
                })
                .is_some_and(|shape| shape.titlecase && !shape.forced);
        rare.push(LetterRare {
            glyph,
            count,
            lexical_word,
            lexical_word_tokens,
            proper_noun_shape,
        });
    }
    rare.sort_by_key(|candidate| (candidate.count, candidate.glyph));
    LetterRound2 {
        tokens,
        types: word_tokens.len() as u64,
        hapax_types,
        letter_scalars,
        hapax_letter_scalars,
        rare,
    }
}

fn glyph_count_bucket(count: u64) -> usize {
    match count {
        0 => unreachable!("inventory entries have nonzero counts"),
        1 => 0,
        2 => 1,
        3..=4 => 2,
        5..=8 => 3,
        9..=16 => 4,
        17..=32 => 5,
        33..=64 => 6,
        _ => 7,
    }
}

fn glyph_rarity_abs(count: u64, knee: f64) -> f64 {
    rarity_abs(count, knee)
}

/// A rate-shaped knee: one occurrence remains fully rare, then the knee grows
/// with opportunities in the glyph's own category lane.
fn glyph_rarity_rate(count: u64, lane_total: u64, rate_per_10k: f64) -> f64 {
    let knee = 1.0 + rate_per_10k * lane_total as f64 / 10_000.0;
    rarity_abs(count, knee)
}

#[derive(Clone, Copy)]
struct GlyphCandidate {
    glyph: char,
    lane: GlyphLane,
    count: u64,
    lane_total: u64,
}

#[derive(Clone, Copy, Default)]
struct GlyphSweep {
    types: u64,
    sites: u64,
}

#[derive(Clone)]
struct GlyphSample {
    corpus: String,
    sid: String,
    glyph: char,
    lane: GlyphLane,
    count: u64,
    lane_total: u64,
    context: String,
}

/// One very-rare letter's lexical evidence. A concentration discount is only
/// justified when every scalar occurrence is accounted for by one repeatedly
/// observed, case-folded word type.
struct LetterRare {
    glyph: char,
    count: u64,
    lexical_word: Option<String>,
    lexical_word_tokens: u64,
    /// Round-5: the glyph's sole container is a titlecase-shaped hapax word at a
    /// non-forced position, so its capital is shape (a name), not position.
    proper_noun_shape: bool,
}

struct LetterRound2 {
    // Word-level machinery, retained unchanged for the lexical-concentration
    // discount and for the round-2/round-3 flip comparison.
    tokens: u64,
    types: u64,
    hapax_types: u64,
    // Round-3 alphabet-closure gate: letter-SCALAR closure. `letter_scalars` is
    // total GC-L scalar occurrences; `hapax_letter_scalars` is the number of L
    // scalar types seen exactly once. Their ratio is the hapax-letter-type
    // occurrence share (each hapax type contributes exactly one occurrence).
    letter_scalars: u64,
    hapax_letter_scalars: u64,
    rare: Vec<LetterRare>,
}

impl LetterRound2 {
    /// Letter-SCALAR closure (round 3): hapax L-scalar occurrence share. ~0 for
    /// closed alphabets (English/Bemba), materially nonzero for open inventories
    /// (CJK). This is the alphabet-closure gate, not vocabulary closure.
    fn closure(&self) -> f64 {
        self.hapax_letter_scalars as f64 / self.letter_scalars.max(1) as f64
    }

    /// Round-2 metric, kept only to report which corpora flip open under the
    /// round-3 scalar closure that were closed under word-hapax share.
    fn word_hapax_share(&self) -> f64 {
        self.hapax_types as f64 / self.tokens.max(1) as f64
    }
}

pub(crate) struct GlyphCorpus {
    id: String,
    verses: usize,
    scalar_count: u64,
    inventory: BTreeMap<char, u64>,
    lane_totals: [u64; 4],
    count_hist: [[u64; GLYPH_HIST_LABELS.len()]; 4],
    abs_sweeps: Vec<[GlyphSweep; 4]>,
    rate_sweeps: Vec<[GlyphSweep; 4]>,
    decomposed_pairs: BTreeMap<String, u64>,
    samples: Vec<GlyphSample>,
    letter_round2: LetterRound2,
    retained_samples: Vec<GlyphSample>,
    proper_samples: Vec<GlyphSample>,
}

/// The fleet keeps calibration rollups, not a corpus's full scalar inventory.
/// This permits corpus-level parallelism without retaining all 1,504 maps.
struct GlyphFleetSummary {
    id: String,
    scalar_count: u64,
    lane_totals: [u64; 4],
    count_hist: [[u64; GLYPH_HIST_LABELS.len()]; 4],
    abs_sweeps: Vec<[GlyphSweep; 4]>,
    rate_sweeps: Vec<[GlyphSweep; 4]>,
    decomposed_pairs: BTreeMap<String, u64>,
    samples: Vec<GlyphSample>,
    letter_round2: LetterRound2,
    retained_samples: Vec<GlyphSample>,
    proper_samples: Vec<GlyphSample>,
}

impl From<GlyphCorpus> for GlyphFleetSummary {
    fn from(corpus: GlyphCorpus) -> Self {
        Self {
            id: corpus.id,
            scalar_count: corpus.scalar_count,
            lane_totals: corpus.lane_totals,
            count_hist: corpus.count_hist,
            abs_sweeps: corpus.abs_sweeps,
            rate_sweeps: corpus.rate_sweeps,
            decomposed_pairs: corpus.decomposed_pairs,
            samples: corpus.samples,
            letter_round2: corpus.letter_round2,
            retained_samples: corpus.retained_samples,
            proper_samples: corpus.proper_samples,
        }
    }
}

fn glyph_candidates(
    inventory: &BTreeMap<char, u64>,
    lane_totals: &[u64; 4],
) -> Vec<GlyphCandidate> {
    inventory
        .iter()
        .filter_map(|(&glyph, &count)| {
            glyph_lane(glyph).map(|lane| GlyphCandidate {
                glyph,
                lane,
                count,
                lane_total: lane_totals[lane.index()],
            })
        })
        .collect()
}

fn glyph_sweep(
    candidates: &[GlyphCandidate],
    score: impl Fn(GlyphCandidate) -> f64,
) -> [GlyphSweep; 4] {
    candidates
        .iter()
        .copied()
        .fold([GlyphSweep::default(); 4], |mut out, candidate| {
            if score(candidate) >= GLYPH_SWEEP_FLOOR {
                let lane = &mut out[candidate.lane.index()];
                lane.types += 1;
                lane.sites += candidate.count;
            }
            out
        })
}

fn glyph_sweep_total(sweep: &[GlyphSweep; 4]) -> GlyphSweep {
    sweep.iter().fold(GlyphSweep::default(), |mut total, lane| {
        total.types += lane.types;
        total.sites += lane.sites;
        total
    })
}

fn glyph_context(text: &str, start: usize, end: usize) -> String {
    let before = text[..start]
        .char_indices()
        .rev()
        .nth(22)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let after = text[end..]
        .char_indices()
        .nth(22)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[before..after].replace(['\t', '\n'], " ")
}

/// Pick one source occurrence for the strongest rare candidates. The samples
/// are review leads, not stored rule sites: a production rule will forward or
/// re-scan its own spans under the stateful protocol.
fn glyph_samples(id: &str, map: &Corpus, candidates: &[GlyphCandidate]) -> Vec<GlyphSample> {
    let mut ranked: Vec<GlyphCandidate> = candidates
        .iter()
        .copied()
        .filter(|c| glyph_rarity_abs(c.count, 32.0) >= GLYPH_SWEEP_FLOOR)
        .collect();
    ranked.sort_by_key(|c| (std::cmp::Reverse(c.lane_total), c.count, c.glyph));

    let mut wanted = BTreeMap::new();
    for lane in GlyphLane::ALL {
        for candidate in ranked
            .iter()
            .copied()
            .filter(|candidate| candidate.lane == lane)
            .take(6)
        {
            wanted.insert(candidate.glyph, candidate);
        }
    }
    let mut samples = Vec::new();
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        for (start, glyph) in text.char_indices() {
            let Some(candidate) = wanted.remove(&glyph) else {
                continue;
            };
            samples.push(GlyphSample {
                corpus: id.to_string(),
                sid: sid.to_string(),
                glyph,
                lane: candidate.lane,
                count: candidate.count,
                lane_total: candidate.lane_total,
                context: glyph_context(text, start, start + glyph.len_utf8()),
            });
            if wanted.is_empty() {
                return samples;
            }
        }
    }
    samples.sort_by_key(|sample| {
        (
            sample.lane.index(),
            std::cmp::Reverse(sample.lane_total),
            sample.count,
            sample.glyph,
        )
    });
    samples
}

/// Review leads for rare letter glyphs (count ≤ knee) that survive the lexical-
/// concentration discount, split into two adjudication sets so a human can judge
/// signal quality on the set the rule would keep in a closed-alphabet corpus:
/// `(proper_killed, retained)`. `proper_killed` is what the round-4 proper-noun-
/// shape discount removes (expect Quirinius-class names); `retained` is what
/// survives all four factors (expect script-intrusion typos). Whether the corpus
/// itself clears closure is decided at fleet time.
fn glyph_retained_samples(
    id: &str,
    map: &Corpus,
    round2: &LetterRound2,
) -> (Vec<GlyphSample>, Vec<GlyphSample>) {
    // glyph -> (count, is_proper_killed)
    let mut wanted: BTreeMap<char, (u64, bool)> = BTreeMap::new();
    for candidate in round2
        .rare
        .iter()
        .filter(|c| c.count <= RETAINED_SAMPLE_KNEE && c.lexical_word.is_none())
    {
        wanted.insert(
            candidate.glyph,
            (candidate.count, candidate.proper_noun_shape),
        );
    }
    let (mut proper, mut retained) = (Vec::new(), Vec::new());
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        if wanted.is_empty() {
            break;
        }
        for (start, glyph) in text.char_indices() {
            let Some((count, is_proper)) = wanted.remove(&glyph) else {
                continue;
            };
            let sample = GlyphSample {
                corpus: id.to_string(),
                sid: sid.to_string(),
                glyph,
                lane: GlyphLane::Letter,
                count,
                lane_total: round2.letter_scalars,
                context: glyph_context(text, start, start + glyph.len_utf8()),
            };
            if is_proper {
                proper.push(sample);
            } else {
                retained.push(sample);
            }
        }
    }
    proper.sort_by_key(|sample| (sample.count, sample.glyph));
    retained.sort_by_key(|sample| (sample.count, sample.glyph));
    (proper, retained)
}

pub(crate) fn analyze_glyphs(id: String, map: &Corpus) -> GlyphCorpus {
    let mut inventory: BTreeMap<char, u64> = BTreeMap::new();
    let mut lane_totals = [0u64; 4];
    let mut decomposed_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_words: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_glyph_words: BTreeMap<char, BTreeMap<String, u64>> = BTreeMap::new();
    let mut scalar_count = 0u64;

    for text in map.texts() {
        let mut previous: Option<char> = None;
        for glyph in text.chars() {
            scalar_count += 1;
            *inventory.entry(glyph).or_default() += 1;
            if let Some(lane) = glyph_lane(glyph) {
                lane_totals[lane.index()] += 1;
            }

            // This is a dependency-free preflight for the normalization seam:
            // record immediately attached base+mark pairs. Canonical equivalence
            // still needs a normalizer before composed and decomposed forms can
            // be joined as one abstract glyph.
            if class_of(glyph).is_mark()
                && let Some(base) = previous
                && !class_of(base).is_mark()
            {
                *decomposed_pairs
                    .entry(format!("{base}{glyph}"))
                    .or_default() += 1;
            }
            previous = Some(glyph);
        }

        for token in tokenize(text) {
            let word = token.span.slice(text);
            if !is_letter_token(word) {
                continue;
            }
            let key = word.to_lowercase();
            *letter_words.entry(key.clone()).or_default() += 1;
            for glyph in word
                .chars()
                .filter(|&glyph| glyph_lane(glyph) == Some(GlyphLane::Letter))
            {
                *letter_glyph_words
                    .entry(glyph)
                    .or_default()
                    .entry(key.clone())
                    .or_default() += 1;
            }
        }
    }

    let candidates = glyph_candidates(&inventory, &lane_totals);
    let mut count_hist = [[0u64; GLYPH_HIST_LABELS.len()]; 4];
    for candidate in &candidates {
        count_hist[candidate.lane.index()][glyph_count_bucket(candidate.count)] += 1;
    }
    let abs_sweeps = GLYPH_ABS_KS
        .iter()
        .map(|&k| glyph_sweep(&candidates, |c| glyph_rarity_abs(c.count, k)))
        .collect();
    let rate_sweeps = GLYPH_RATE_PER_10K
        .iter()
        .map(|&rate| {
            glyph_sweep(&candidates, |c| {
                glyph_rarity_rate(c.count, c.lane_total, rate)
            })
        })
        .collect();
    let samples = glyph_samples(&id, map, &candidates);
    let shapes = letter_word_shapes(map);
    let letter_round2 = letter_round2(&inventory, letter_words, letter_glyph_words, &shapes);
    let (proper_samples, retained_samples) = glyph_retained_samples(&id, map, &letter_round2);

    GlyphCorpus {
        id,
        verses: map.len(),
        scalar_count,
        inventory,
        lane_totals,
        count_hist,
        abs_sweeps,
        rate_sweeps,
        decomposed_pairs,
        samples,
        letter_round2,
        retained_samples,
        proper_samples,
    }
}

fn glyph_label(glyph: char) -> String {
    format!("{} U+{:04X}", glyph.escape_default(), glyph as u32)
}

fn print_glyph_sweeps(abs: &[[GlyphSweep; 4]], rate: &[[GlyphSweep; 4]]) {
    println!(
        "\nrecurrence sweeps (rows surface raw rarity >= {GLYPH_SWEEP_FLOOR:.2}; types / sites):"
    );
    let describe = |sweep: &[GlyphSweep; 4]| {
        let total = glyph_sweep_total(sweep);
        let lanes = GlyphLane::ALL
            .iter()
            .map(|lane| {
                let s = sweep[lane.index()];
                format!("{} {}/{}", lane.label(), s.types, s.sites)
            })
            .collect::<Vec<_>>()
            .join("  ");
        format!("total {}/{}  {lanes}", total.types, total.sites)
    };
    println!("  absolute knee:");
    for (&k, row) in GLYPH_ABS_KS.iter().zip(abs) {
        println!("    K={k:>5.1}: {}", describe(row));
    }
    println!("  rate knee (K = 1 + rate × lane opportunities / 10k):");
    for (&rate, row) in GLYPH_RATE_PER_10K.iter().zip(rate) {
        println!("    r={rate:>5.2}: {}", describe(row));
    }
}

fn print_glyph_histogram(hist: &[[u64; GLYPH_HIST_LABELS.len()]; 4]) {
    println!("\ncandidate type-count histogram (number of glyph types):");
    print!("  {:<5}", "lane");
    for label in GLYPH_HIST_LABELS {
        print!(" {:>7}", label);
    }
    println!();
    for lane in GlyphLane::ALL {
        print!("  {:<5}", lane.label());
        for n in hist[lane.index()] {
            print!(" {n:>7}");
        }
        println!();
    }
}

fn print_glyph_samples(samples: &[GlyphSample]) {
    for sample in samples {
        let per_10k = sample.count as f64 * 10_000.0 / sample.lane_total.max(1) as f64;
        println!(
            "  {:<18} {:<10} {:<15} {} count={} lane_n={} rate={per_10k:.3}/10k | {}",
            sample.corpus,
            sample.sid,
            sample.lane.label(),
            glyph_label(sample.glyph),
            sample.count,
            sample.lane_total,
            sample.context,
        );
    }
}

#[derive(Clone, Copy, Default)]
struct LetterRound2Tally {
    base: GlyphSweep,
    closure_killed: GlyphSweep,
    lexical_killed: GlyphSweep,
    proper_killed: GlyphSweep,
    retained: GlyphSweep,
}

fn add_glyph_sweep(total: &mut GlyphSweep, add: GlyphSweep) {
    total.types += add.types;
    total.sites += add.sites;
}

fn add_letter_round2_tally(total: &mut LetterRound2Tally, add: LetterRound2Tally) {
    add_glyph_sweep(&mut total.base, add.base);
    add_glyph_sweep(&mut total.closure_killed, add.closure_killed);
    add_glyph_sweep(&mut total.lexical_killed, add.lexical_killed);
    add_glyph_sweep(&mut total.proper_killed, add.proper_killed);
    add_glyph_sweep(&mut total.retained, add.retained);
}

fn letter_round2_tally(
    round2: &LetterRound2,
    max_count: u64,
    closed_alphabet: bool,
) -> LetterRound2Tally {
    let mut out = LetterRound2Tally::default();
    for candidate in round2
        .rare
        .iter()
        .filter(|candidate| candidate.count <= max_count)
    {
        let candidate_sweep = GlyphSweep {
            types: 1,
            sites: candidate.count,
        };
        add_glyph_sweep(&mut out.base, candidate_sweep);
        if !closed_alphabet {
            add_glyph_sweep(&mut out.closure_killed, candidate_sweep);
        } else if candidate.lexical_word.is_some() {
            add_glyph_sweep(&mut out.lexical_killed, candidate_sweep);
        } else if candidate.proper_noun_shape {
            add_glyph_sweep(&mut out.proper_killed, candidate_sweep);
        } else {
            add_glyph_sweep(&mut out.retained, candidate_sweep);
        }
    }
    out
}

fn kill_rate(killed: u64, base: u64) -> f64 {
    killed as f64 * 100.0 / base.max(1) as f64
}

fn print_letter_round2_single(round2: &LetterRound2) {
    println!("\nround 3 letter evidence:");
    println!(
        "  L scalars={}  hapax L scalars={}  scalar closure={:.4}%  (word types={}, round-2 word-hapax share={:.3}%)",
        round2.letter_scalars,
        round2.hapax_letter_scalars,
        round2.closure() * 100.0,
        round2.types,
        round2.word_hapax_share() * 100.0,
    );
    println!("  small-knee candidates assuming this corpus clears closure:");
    for max_count in LETTER_RARE_MAX_COUNTS {
        let tally = letter_round2_tally(round2, max_count, true);
        println!(
            "    <= {max_count}: base {}/{}  lexical-discount {}/{} ({:.1}%)  proper-noun {}/{} ({:.1}%)  retained {}/{}",
            tally.base.types,
            tally.base.sites,
            tally.lexical_killed.types,
            tally.lexical_killed.sites,
            kill_rate(tally.lexical_killed.sites, tally.base.sites),
            tally.proper_killed.types,
            tally.proper_killed.sites,
            kill_rate(tally.proper_killed.sites, tally.base.sites),
            tally.retained.types,
            tally.retained.sites,
        );
    }
    let lexical: Vec<_> = round2
        .rare
        .iter()
        .filter(|candidate| candidate.lexical_word.is_some())
        .collect();
    println!(
        "  lexical-concentration discounts (first {} of {}):",
        lexical.len().min(20),
        lexical.len()
    );
    for candidate in lexical.iter().take(20) {
        println!(
            "    {:<15} count={} word={} ({} tokens)",
            glyph_label(candidate.glyph),
            candidate.count,
            candidate.lexical_word.as_deref().unwrap_or_default(),
            candidate.lexical_word_tokens,
        );
    }
    let proper: Vec<_> = round2
        .rare
        .iter()
        .filter(|candidate| candidate.proper_noun_shape)
        .collect();
    println!(
        "  proper-noun-shape discounts (first {} of {}):",
        proper.len().min(20),
        proper.len()
    );
    for candidate in proper.iter().take(20) {
        println!(
            "    {:<15} count={} (titlecase-shaped hapax word at a non-forced position)",
            glyph_label(candidate.glyph),
            candidate.count,
        );
    }
}

pub(crate) fn glyph_single_report(corpus: &GlyphCorpus) {
    println!(
        "=== RARE-GLYPH SPIKE: {} ({} verses) ===",
        corpus.id, corpus.verses
    );
    println!(
        "raw scalar inventory: {} occurrences / {} distinct scalars",
        corpus.scalar_count,
        corpus.inventory.len()
    );
    println!("candidate lane opportunities:");
    for lane in GlyphLane::ALL {
        let types = corpus
            .inventory
            .keys()
            .filter(|&&c| glyph_lane(c) == Some(lane))
            .count();
        println!(
            "  {}  {:>10} occurrences / {:>5} glyph types",
            lane.label(),
            corpus.lane_totals[lane.index()],
            types
        );
    }
    print_glyph_histogram(&corpus.count_hist);
    print_glyph_sweeps(&corpus.abs_sweeps, &corpus.rate_sweeps);
    print_letter_round2_single(&corpus.letter_round2);

    let mut candidates = glyph_candidates(&corpus.inventory, &corpus.lane_totals);
    candidates.sort_by_key(|c| (c.count, std::cmp::Reverse(c.lane_total), c.glyph));
    println!(
        "\nrarest candidate glyphs (first {} of {}):",
        candidates.len().min(120),
        candidates.len()
    );
    println!(
        "  {:<15} {:<5} {:>8} {:>12} {:>14}",
        "glyph", "lane", "count", "lane total", "rate /10k"
    );
    for candidate in candidates.iter().take(120) {
        let rate = candidate.count as f64 * 10_000.0 / candidate.lane_total.max(1) as f64;
        println!(
            "  {:<15} {:<5} {:>8} {:>12} {:>14.3}",
            glyph_label(candidate.glyph),
            candidate.lane.label(),
            candidate.count,
            candidate.lane_total,
            rate,
        );
    }

    let mut decomposed: Vec<_> = corpus.decomposed_pairs.iter().collect();
    decomposed.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    println!("\ndecomposed base+mark preflight (top 20; canonical pairing not yet joined):");
    if decomposed.is_empty() {
        println!("  none");
    } else {
        for (pair, count) in decomposed.iter().take(20) {
            println!("  {pair:?}  {count}");
        }
    }
    println!("\nsample high-rarity candidates (absolute K=32):");
    print_glyph_samples(&corpus.samples);
}

/// Fleet report: workers drop each raw inventory after deriving a compact
/// summary. The aggregate keeps only reproducible rollups and bounded samples.
pub(crate) fn glyph_fleet(dir: &Path) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("rare-glyph fleet: {total} corpora in {}", dir.display());

    let mut lane_totals = [0u64; 4];
    let mut count_hist = [[0u64; GLYPH_HIST_LABELS.len()]; 4];
    let mut abs_sweeps = vec![[GlyphSweep::default(); 4]; GLYPH_ABS_KS.len()];
    let mut rate_sweeps = vec![[GlyphSweep::default(); 4]; GLYPH_RATE_PER_10K.len()];
    let mut noisiest: Vec<(String, [u64; 4], [u64; 4], u64)> = Vec::new();
    let mut samples = Vec::new();
    let mut decomposed: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut round2 = vec![
        vec![LetterRound2Tally::default(); LETTER_RARE_MAX_COUNTS.len()];
        CLOSURE_SCALAR_SHARES.len()
    ];
    let mut open_corpora = vec![0u64; CLOSURE_SCALAR_SHARES.len()];
    // (id, L scalars, hapax L scalars, scalar closure ppm, word-hapax share ppm)
    let mut closure_rows: Vec<(String, u64, u64, u64, u64)> = Vec::new();
    // Round-3 sanity checks: corpora that flip open (closed word-hapax → open
    // scalar closure), retained review leads, and lexical-kill mechanism leads.
    let mut flips: Vec<(String, u64, u64)> = Vec::new();
    let mut retained_samples: Vec<GlyphSample> = Vec::new();
    let mut proper_samples: Vec<GlyphSample> = Vec::new();
    let mut lexical_kill_leads: Vec<(String, char, String, u64)> = Vec::new();
    let t0 = std::time::Instant::now();
    let done = AtomicUsize::new(0);
    let corpora: Vec<GlyphFleetSummary> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let summary = GlyphFleetSummary::from(analyze_glyphs(id, &load_corpus(path)));
            let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
            if completed.is_multiple_of(100) {
                eprintln!("  …{completed}/{total}");
            }
            summary
        })
        .collect();
    eprintln!("rare-glyph fleet analyze: {:?}", t0.elapsed());

    for corpus in corpora {
        for lane in GlyphLane::ALL {
            lane_totals[lane.index()] += corpus.lane_totals[lane.index()];
            for (sum, value) in count_hist[lane.index()]
                .iter_mut()
                .zip(corpus.count_hist[lane.index()])
            {
                *sum += value;
            }
        }
        for (sum, value) in abs_sweeps.iter_mut().zip(&corpus.abs_sweeps) {
            for (sum, value) in sum.iter_mut().zip(value) {
                sum.types += value.types;
                sum.sites += value.sites;
            }
        }
        for (sum, value) in rate_sweeps.iter_mut().zip(&corpus.rate_sweeps) {
            for (sum, value) in sum.iter_mut().zip(value) {
                sum.types += value.types;
                sum.sites += value.sites;
            }
        }
        let abs_ref = corpus.abs_sweeps[4].map(|sweep| sweep.sites); // K=32
        let rate_ref = corpus.rate_sweeps[3].map(|sweep| sweep.sites); // 2/10k
        noisiest.push((corpus.id.clone(), abs_ref, rate_ref, corpus.scalar_count));
        let closure = corpus.letter_round2.closure();
        let word_hapax = corpus.letter_round2.word_hapax_share();
        closure_rows.push((
            corpus.id.clone(),
            corpus.letter_round2.letter_scalars,
            corpus.letter_round2.hapax_letter_scalars,
            (closure * 1_000_000.0).round() as u64,
            (word_hapax * 1_000_000.0).round() as u64,
        ));
        // Flip = closed under the round-2 word-hapax gate (>0.5%, the round-2
        // representative), open under the round-3 scalar gate (≤0.1%).
        if word_hapax > 0.005 && closure <= RETAINED_SAMPLE_THRESHOLD {
            flips.push((
                corpus.id.clone(),
                (word_hapax * 1_000_000.0).round() as u64,
                (closure * 1_000_000.0).round() as u64,
            ));
        }
        for (threshold_index, &threshold) in CLOSURE_SCALAR_SHARES.iter().enumerate() {
            let open = closure <= threshold;
            if open {
                open_corpora[threshold_index] += 1;
            }
            for (knee_index, &max_count) in LETTER_RARE_MAX_COUNTS.iter().enumerate() {
                add_letter_round2_tally(
                    &mut round2[threshold_index][knee_index],
                    letter_round2_tally(&corpus.letter_round2, max_count, open),
                );
            }
        }
        // Lexical-kill mechanism leads at knee ≤1: count==1 letter scalars whose
        // occurrence folds into a repeated word type. Uppercase glyph here proves
        // the suspected uppercase-folds-into-repeated-lowercase-word mechanism.
        if closure <= RETAINED_SAMPLE_THRESHOLD {
            for cand in corpus
                .letter_round2
                .rare
                .iter()
                .filter(|c| c.count == 1 && c.lexical_word.is_some())
            {
                if lexical_kill_leads.len() < 20 {
                    lexical_kill_leads.push((
                        corpus.id.clone(),
                        cand.glyph,
                        cand.lexical_word.clone().unwrap_or_default(),
                        cand.lexical_word_tokens,
                    ));
                }
            }
            retained_samples.extend(corpus.retained_samples.iter().cloned());
            proper_samples.extend(corpus.proper_samples.iter().cloned());
        }
        samples.extend(corpus.samples);
        for (pair, &count) in &corpus.decomposed_pairs {
            let row = decomposed.entry(pair.clone()).or_default();
            row.0 += count;
            row.1 += 1;
        }
    }
    eprintln!("rare-glyph fleet tally: {:?}", t0.elapsed());

    println!("=== RARE-GLYPH SPIKE — fleet aggregate ({total} corpora) ===");
    println!("candidate lane opportunities:");
    for lane in GlyphLane::ALL {
        println!("  {}  {}", lane.label(), lane_totals[lane.index()]);
    }
    print_glyph_histogram(&count_hist);
    print_glyph_sweeps(&abs_sweeps, &rate_sweeps);

    println!("\nround 3 L-only stack (base is the small absolute knee; all counts are sites):");
    println!(
        "  closure threshold is hapax L-scalar types / all L-scalar occurrences (letter-SCALAR closure)."
    );
    for (threshold_index, &threshold) in CLOSURE_SCALAR_SHARES.iter().enumerate() {
        println!(
            "  scalar closure <= {:.4}%: {}/{} corpora open the L lane",
            threshold * 100.0,
            open_corpora[threshold_index],
            total
        );
        for (knee_index, &max_count) in LETTER_RARE_MAX_COUNTS.iter().enumerate() {
            let tally = round2[threshold_index][knee_index];
            println!(
                "    <= {max_count}: base {:>6}; closure -{:>6} ({:>5.1}%); lexical -{:>6} ({:>5.1}%); proper-noun -{:>6} ({:>5.1}%); keep {:>6}",
                tally.base.sites,
                tally.closure_killed.sites,
                kill_rate(tally.closure_killed.sites, tally.base.sites),
                tally.lexical_killed.sites,
                kill_rate(tally.lexical_killed.sites, tally.base.sites),
                tally.proper_killed.sites,
                kill_rate(tally.proper_killed.sites, tally.base.sites),
                tally.retained.sites,
            );
        }
    }

    // Highest scalar closure = open-inventory corpora that self-silence.
    closure_rows.sort_by_key(|(_, _, _, closure_ppm, _)| std::cmp::Reverse(*closure_ppm));
    println!("\nhighest letter-SCALAR closure (open-inventory self-disable, stay closed):");
    for (id, scalars, hapaxes, closure_ppm, word_ppm) in closure_rows.iter().take(20) {
        println!(
            "  {id:<24} {}/{} = {:.4}%  (word-hapax {:.3}%)",
            hapaxes,
            scalars,
            *closure_ppm as f64 / 10_000.0,
            *word_ppm as f64 / 10_000.0,
        );
    }

    // Sanity: corpora that flip open under scalar closure but were closed under
    // the round-2 word-hapax gate — the agglutinative Latin-script class.
    flips.sort_by_key(|(_, word_ppm, _)| std::cmp::Reverse(*word_ppm));
    println!(
        "\nflip-open corpora (word-hapax >0.5% [closed in round 2] but scalar closure <=0.1% [open now]): {} total",
        flips.len()
    );
    for (id, word_ppm, closure_ppm) in flips.iter().take(25) {
        println!(
            "  {id:<24} word-hapax {:.3}%  scalar closure {:.4}%",
            *word_ppm as f64 / 10_000.0,
            *closure_ppm as f64 / 10_000.0,
        );
    }

    // Sanity: confirm the mechanism of the knee≤1 lexical kills.
    println!(
        "\nlexical kills at knee<=1 (count==1 L scalar folding into a repeated word type): {} leads",
        lexical_kill_leads.len()
    );
    for (id, glyph, word, word_tokens) in lexical_kill_leads.iter().take(20) {
        let upper = glyph.is_uppercase();
        println!(
            "  {id:<20} {} -> word {word:?} ({word_tokens} tokens){}",
            glyph_label(*glyph),
            if upper {
                "  [uppercase → folds to repeated lowercase]"
            } else {
                ""
            },
        );
    }

    // Round-5 proper-noun-kill table: ~20 diverse sites the shape discount
    // removes (letter, count<=3, non-lexical, titlecase-shaped hapax at a
    // non-forced position). Expect Quirinius-class names; the round-4 leaks
    // (lone capitals, all-caps common words) should no longer appear here.
    proper_samples.sort_by_key(|s| (s.corpus.clone(), s.count, s.glyph));
    proper_samples.dedup_by(|a, b| a.corpus == b.corpus && a.glyph == b.glyph);
    let mut proper_diverse: Vec<GlyphSample> = Vec::new();
    let mut proper_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for sample in &proper_samples {
        let seen = proper_per_corpus.entry(sample.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            proper_diverse.push(sample.clone());
        }
    }
    println!(
        "\nround-5 proper-noun-kill table ({} of {} proper-shape leads; closure<={:.3}%, knee<={}):",
        proper_diverse.len().min(20),
        proper_samples.len(),
        RETAINED_SAMPLE_THRESHOLD * 100.0,
        RETAINED_SAMPLE_KNEE,
    );
    print_glyph_samples(&proper_diverse.into_iter().take(20).collect::<Vec<_>>());

    // Retained review table: ~30 diverse retained sites (letter, count<=3, not
    // lexical, not proper-noun-shape) in corpora open at the representative
    // closure threshold — what survives all four factors.
    retained_samples.sort_by_key(|s| (s.corpus.clone(), s.count, s.glyph));
    retained_samples.dedup_by(|a, b| a.corpus == b.corpus && a.glyph == b.glyph);
    let mut diverse: Vec<GlyphSample> = Vec::new();
    let mut per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for sample in &retained_samples {
        let seen = per_corpus.entry(sample.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            diverse.push(sample.clone());
        }
    }
    println!(
        "\nretained review table ({} of {} retained leads; closure<={:.3}%, knee<={}, non-lexical, non-proper-noun — survives all four factors):",
        diverse.len().min(30),
        retained_samples.len(),
        RETAINED_SAMPLE_THRESHOLD * 100.0,
        RETAINED_SAMPLE_KNEE,
    );
    print_glyph_samples(&diverse.into_iter().take(30).collect::<Vec<_>>());

    noisiest.sort_by_key(|(_, abs, rate, _)| {
        (
            std::cmp::Reverse(abs.iter().sum::<u64>()),
            std::cmp::Reverse(rate.iter().sum::<u64>()),
        )
    });
    println!("\nnoisiest corpora (raw-rarity reference: absolute K=32, rate=2/10k):");
    for (id, abs, rate, scalars) in noisiest.iter().take(20) {
        println!(
            "  {id:<24} abs L/N/P/S={}/{}/{}/{}  rate={}/{}/{}/{}  raw {scalars:>9} scalars",
            abs[0], abs[1], abs[2], abs[3], rate[0], rate[1], rate[2], rate[3],
        );
    }

    let mut decomposed: Vec<_> = decomposed.into_iter().collect();
    decomposed.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    println!("\ndecomposed base+mark preflight across the fleet (top 20):");
    for (pair, (count, corpora)) in decomposed.iter().take(20) {
        println!("  {pair:?}  {count:>8} occurrences in {corpora} corpora");
    }

    println!("\nreview samples by lane (absolute K=32):");
    for lane in GlyphLane::ALL {
        let mut lane_samples: Vec<_> = samples
            .iter()
            .filter(|sample| sample.lane == lane)
            .cloned()
            .collect();
        lane_samples.sort_by_key(|sample| {
            (
                std::cmp::Reverse(sample.lane_total),
                sample.count,
                sample.glyph,
            )
        });
        println!("  [{}]", lane.label());
        print_glyph_samples(&lane_samples.into_iter().take(12).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    fn one_verse(text: &str) -> Corpus {
        Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec![text.to_string()]).unwrap()
    }

    #[test]
    fn visible_candidate_lanes_cover_stated_examples_only() {
        assert_eq!(glyph_lane('q'), Some(GlyphLane::Letter));
        assert_eq!(glyph_lane('¹'), Some(GlyphLane::Number));
        assert_eq!(glyph_lane('“'), Some(GlyphLane::Punctuation));
        assert_eq!(glyph_lane('='), Some(GlyphLane::Symbol));
        assert_eq!(glyph_lane('\u{301}'), None);
        assert_eq!(glyph_lane(' '), None);
        assert_eq!(glyph_lane('\u{FFFD}'), None);
    }

    #[test]
    fn rate_knee_expands_with_lane_volume() {
        assert_eq!(glyph_rarity_abs(1, 32.0), 1.0);
        assert!(glyph_rarity_rate(32, 500_000, 2.0) > glyph_rarity_abs(32, 32.0));
    }

    #[test]
    fn closure_uses_hapax_letter_scalar_share() {
        // "alpha alpha alpha": a×6, l×3, p×3, h×3 — no scalar seen once.
        let closed = analyze_glyphs("closed".to_string(), &one_verse("alpha alpha alpha"));
        assert_eq!(closed.letter_round2.hapax_letter_scalars, 0);
        assert_eq!(closed.letter_round2.letter_scalars, 15);
        assert_eq!(closed.letter_round2.closure(), 0.0);

        // "alpha beta gamma": a×5, m×2 repeat; l,p,h,b,e,t,g each once (7 hapax
        // scalars) of 14 L occurrences → 0.5. Scalar closure, not word closure:
        // even with three distinct (word-hapax=1.0) word types the alphabet is
        // half-closed.
        let open = analyze_glyphs("open".to_string(), &one_verse("alpha beta gamma"));
        assert_eq!(open.letter_round2.hapax_letter_scalars, 7);
        assert_eq!(open.letter_round2.letter_scalars, 14);
        assert_eq!(open.letter_round2.closure(), 0.5);
        assert_eq!(open.letter_round2.word_hapax_share(), 1.0);
    }

    #[test]
    fn lexical_discount_requires_one_repeated_word_type() {
        let concentrated = analyze_glyphs("concentrated".to_string(), &one_verse("Xerxes Xerxes"));
        let x = concentrated
            .letter_round2
            .rare
            .iter()
            .find(|candidate| candidate.glyph == 'X')
            .unwrap();
        assert_eq!(x.lexical_word.as_deref(), Some("xerxes"));
        assert_eq!(x.lexical_word_tokens, 2);

        let scattered = analyze_glyphs("scattered".to_string(), &one_verse("Xenon Xylophone"));
        let x = scattered
            .letter_round2
            .rare
            .iter()
            .find(|candidate| candidate.glyph == 'X')
            .unwrap();
        assert!(x.lexical_word.is_none());
    }

    fn rare(corpus: &GlyphCorpus, glyph: char) -> &LetterRare {
        corpus
            .letter_round2
            .rare
            .iter()
            .find(|c| c.glyph == glyph)
            .unwrap()
    }

    #[test]
    fn proper_noun_shape_discounts_titlecase_hapax_at_midflow() {
        // `Q` occurs once, inside the hapax name `Quirinius`, mid-flow (a common
        // word precedes it, no terminal). Its lone container is titlecase-shaped
        // (upper first + following lower) and at a non-forced position ⇒
        // proper-noun-shape discount fires. The recurring-word lexical discount
        // does not (the container is a hapax).
        let map = one_verse("in the days of Quirinius the governor");
        let corpus = analyze_glyphs("quirinius".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(q.lexical_word.is_none());
        assert!(q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_lone_capital_token() {
        // Round-5 tightening: a lone one-letter uppercase token (`Q` standing
        // alone mid-flow, the round-4 WA-dje MAT 11:4 leak) is capital-initial
        // but NOT titlecase (no following lowercase letter), so the discount no
        // longer fires — the stray capital stays flagged (the safe direction).
        let map = one_verse("he said to them Q go and tell the news");
        let corpus = analyze_glyphs("lone-capital".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(q.lexical_word.is_none());
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_all_caps_token() {
        // Round-5 tightening: an all-caps token carrying a stray glyph (the
        // Spanish `YÖ`-for-`YO` leak, WA-es-419 ZEC 3:4) is capital-initial but
        // has no following lowercase letter, so it is not titlecase and the
        // discount does not fire — the genuine typo stays flagged.
        let map = one_verse("and the voice cried YÖ am the one who speaks");
        let corpus = analyze_glyphs("all-caps".to_string(), &map);
        let o = rare(&corpus, 'Ö');
        assert!(o.lexical_word.is_none());
        assert!(!o.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_capital_at_a_forced_position() {
        // Same name, but now the word after a bare terminal: the capital is
        // position-forced, so shape says nothing and the discount must NOT fire
        // (conservative — the flag survives).
        let map = one_verse("it happened then. Quirinius ruled the land");
        let corpus = analyze_glyphs("forced".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_book_initial_capital() {
        // Book-initial is forced with no terminal glyph (CLAUDE.md), so a rare
        // glyph inside the very first word gets no shape discount.
        let map = one_verse("Quirinius governed the far country");
        let corpus = analyze_glyphs("book-initial".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_ignores_lowercase_script_intrusion() {
        // A stray `q` intruding into an otherwise-lowercase word is not capital-
        // initial, so the shape branch never fires — script-intrusion typos in
        // ordinary lowercase words stay flagged (bicameral-only by construction
        // also means caseless scripts, which have no uppercase, never qualify).
        let map = one_verse("she walked into the woqden house today");
        let corpus = analyze_glyphs("intrusion".to_string(), &map);
        let q = rare(&corpus, 'q');
        assert!(!q.proper_noun_shape);
    }
}

