// ═══════════════════════════════════════════════════════════════════════════
// Mixed-case word SPIKE (plan rule 3). A word written in an internal-capital
// shape (`wOrd`, `McDonald`, `kiSwahili`, `LORDs`) is a slip unless it is a
// convention the recurrence machinery excuses. Harness-only: no production
// rule, `RuleStats`, or `CasingConfig` is touched (READ-ONLY). The token unit
// is the plain UAX #29 letter-run word (`is_letter_token`), so `Obed-Edom`
// splits into two Titlecase tokens and never reads as one mixed-case word —
// this is deliberately NOT the hyphen-merged `compound_words` the casing walk
// uses, matching the plan's "letter-run tokens" unit for rule 3. Sweep
// constants (PACKET_KS/PACKET_FLOORS/REF_K/REF_FLOOR) and `rarity_abs` are
// shared with the casing spike; `sig_wilson_lb` is the harness-local Wilson.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::Corpus;
use ssc_core::charclass::class_of;
use ssc_core::token::tokenize;

use super::casing::casing_ctx;
use super::glyphs::{glyph_advance_gap, is_letter_token};
use super::shared::{PACKET_FLOORS, PACKET_KS, REF_FLOOR, REF_K, rarity_abs, sig_wilson_lb};
use crate::vref_io::load_corpus;

const MC_Z: f64 = 1.96;
/// Corpora sampled in the fleet report for the convention-class adjudication.
const MC_MAJOR: &[&str] = &[
    "eng-web",
    "eng-kjv",
    "engwebster",
    "WA-en-ulb",
    "spaRV1909",
    "WA-es-419-ulb",
    "fraLSG",
    "WA-fr-ulb",
    "porblt",
    "deu1912",
    "swhulb",
    "WA-sw-ulb",
    "WA-bem-reg",
    "ind",
    "nld",
    "vie1934",
    "tglulb",
    "ron1924",
    "engojb",
];

/// A word's observed case shape over its **cased** letters (marks and caseless
/// letters ignored). `OtherMixed` is the `wOrd` candidate: it has both cases and
/// is neither Titlecase nor ALLCAPS, so it necessarily carries an *internal*
/// capital (an uppercase letter that is not the first cased letter).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum McShape {
    Lower,
    Title,
    AllCaps,
    OtherMixed,
}

/// Classify a letter-run token by the case sequence of its cased letters.
/// `None` = no cased letter (caseless script / marks only): no shape, not a
/// candidate, not counted in the cased denominator. A single cased letter is
/// Lower or AllCaps, never OtherMixed (the single-letter guard the plan warns
/// about: a lone `I`/`A` must not read as mixed). Combining marks and non-cased
/// letters inside a cased word are skipped, so an intra-word caseless glyph
/// cannot by itself manufacture a mixed shape.
fn mc_classify(word: &str) -> Option<McShape> {
    let mut cases: Vec<bool> = Vec::new(); // true = uppercase
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_uppercase() {
            cases.push(true);
        } else if cl.is_lowercase() {
            cases.push(false);
        }
    }
    if cases.is_empty() {
        return None;
    }
    let up = cases.iter().filter(|&&u| u).count();
    let n = cases.len();
    Some(if up == 0 {
        McShape::Lower
    } else if up == n {
        McShape::AllCaps
    } else if cases[0] && cases[1..].iter().all(|&u| !u) {
        McShape::Title
    } else {
        McShape::OtherMixed
    })
}

/// True iff the first *cased* letter of a mixed word is uppercase — the
/// boundary axis against casing v2. An upper-first OtherMixed word (`McDonald`,
/// `LORDs`) has an uppercase first letter, so casing (which fires only on a
/// lowercase word-start) never sees it: it is unambiguously mixed-case's. A
/// lower-first OtherMixed word (`wOrd`, `kiSwahili`) shares casing's
/// lowercase-site domain and is the overlap class.
fn mc_first_cased_upper(word: &str) -> bool {
    word.chars()
        .find_map(|c| {
            let cl = class_of(c);
            if cl.is_uppercase() {
                Some(true)
            } else if cl.is_lowercase() {
                Some(false)
            } else {
                None
            }
        })
        .unwrap_or(false)
}

/// One word type's shape profile within a corpus (raw counts). `other_first_upper`
/// and `other_forced` split the OtherMixed occurrences by the two axes the spike
/// adjudicates (casing overlap, and position-independence).
#[derive(Clone, Copy, Default)]
struct McProfile {
    lower: u64,
    title: u64,
    allcaps: u64,
    other: u64,
    other_first_upper: u64,
    other_forced: u64,
}

impl McProfile {
    fn total(&self) -> u64 {
        self.lower + self.title + self.allcaps + self.other
    }
    fn not_other(&self) -> u64 {
        self.total() - self.other
    }
}

/// One OtherMixed occurrence, retained (capped) only to draw review samples;
/// the volume grids come analytically from the per-type profiles, not these.
struct McCand {
    sid: String,
    start: u32,
    end: u32,
    key: String,
    first_upper: bool,
    forced: bool,
}

/// One sampled OtherMixed site for the review tables.
struct McSample {
    sid: String,
    word: String,
    route: &'static str,
    other: u64,
    word_total: u64,
    dom: f64,
    score: f64,
    first_upper: bool,
    forced: bool,
    ctx: String,
}

/// Per-corpus mixed-case result. Grids are `[knee][floor]`, fleet-summable.
pub(crate) struct McCorpus {
    id: String,
    verses: usize,
    cased_tokens: u64,
    other_tokens: u64,
    other_types: u64,
    hapax_other_types: u64,
    recurring_other_types: u64,
    corpus_dominance: f64,
    // Route 4 — position: OtherMixed vs all-cased at forced / mid positions.
    cased_forced: u64,
    other_forced: u64,
    cased_mid: u64,
    other_mid: u64,
    // Boundary vs casing v2: OtherMixed occurrences by first-cased-letter case.
    other_first_upper: u64,
    other_first_lower: u64,
    // Of the ref-flagged (route A) OtherMixed sites, the same two splits.
    flagged_first_upper: u64,
    flagged_forced: u64,
    // Volume grids: sites (occurrences) surfaced. [knee][floor].
    grid_within: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_fallback: Vec<[u64; PACKET_FLOORS.len()]>,
    hist_within: [u64; 40],
    ref_within: u64,
    ref_fallback: u64,
    flagged_samples: Vec<McSample>,
    excused_samples: Vec<McSample>,
    fallback_samples: Vec<McSample>,
}

/// Walk each book mirroring `casing::walk_book`'s pending-terminal machine
/// (carried across verse seams, reset per book, book-initial forced), over the
/// plain UAX letter-run tokens. Returns the per-type profiles, corpus counters,
/// and a capped list of OtherMixed occurrences for sampling.
fn mc_walk(
    map: &Corpus,
) -> (
    BTreeMap<String, McProfile>,
    [u64; 4], // cased_forced, other_forced, cased_mid, other_mid
    Vec<McCand>,
) {
    let mut profiles: BTreeMap<String, McProfile> = BTreeMap::new();
    let mut counters = [0u64; 4];
    let mut cands: Vec<McCand> = Vec::new();
    const CAND_CAP: usize = 8000;

    for group in &ssc_core::corpus::by_book(map) {
        let mut pending: Option<bool> = None;
        let mut book_initial = true;
        for (sid, text) in group.keys.iter().zip(group.texts) {
            let mut prev_letter = false;
            let mut cursor = 0usize;
            for token in tokenize(text) {
                let word = token.span.slice(text);
                if !is_letter_token(word) {
                    continue; // its text stays in the gap the next word sees
                }
                glyph_advance_gap(
                    &text[cursor..token.span.start as usize],
                    &mut pending,
                    &mut prev_letter,
                );
                let forced = book_initial || matches!(pending.take(), Some(false));
                book_initial = false;
                prev_letter = word
                    .chars()
                    .next_back()
                    .is_some_and(|c| class_of(c).is_alphabetic());
                cursor = token.span.end as usize;

                let Some(shape) = mc_classify(word) else {
                    continue;
                };
                let entry = profiles.entry(word.to_lowercase()).or_default();
                let is_forced = forced;
                if is_forced {
                    counters[0] += 1;
                } else {
                    counters[2] += 1;
                }
                match shape {
                    McShape::Lower => entry.lower += 1,
                    McShape::Title => entry.title += 1,
                    McShape::AllCaps => entry.allcaps += 1,
                    McShape::OtherMixed => {
                        entry.other += 1;
                        let first_upper = mc_first_cased_upper(word);
                        if first_upper {
                            entry.other_first_upper += 1;
                        }
                        if is_forced {
                            entry.other_forced += 1;
                            counters[1] += 1;
                        } else {
                            counters[3] += 1;
                        }
                        if cands.len() < CAND_CAP {
                            cands.push(McCand {
                                sid: sid.clone(),
                                start: token.span.start,
                                end: token.span.end,
                                key: word.to_lowercase(),
                                first_upper,
                                forced: is_forced,
                            });
                        }
                    }
                }
            }
            glyph_advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
        }
    }
    (profiles, counters, cands)
}

pub(crate) fn analyze_mixedcase(id: String, map: &Corpus) -> McCorpus {
    let (profiles, counters, cands) = mc_walk(map);
    let [cased_forced, other_forced, cased_mid, other_mid] = counters;
    let cased_tokens: u64 = profiles.values().map(McProfile::total).sum();
    let other_tokens: u64 = profiles.values().map(|p| p.other).sum();
    let other_first_upper: u64 = profiles.values().map(|p| p.other_first_upper).sum();
    // Corpus-level "tokens are not other-mixed" dominance — the route-B factor.
    let corpus_dominance = sig_wilson_lb(
        cased_tokens.saturating_sub(other_tokens),
        cased_tokens,
        MC_Z,
    );

    let nk = PACKET_KS.len();
    let mut grid_within = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_fallback = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist_within = [0u64; 40];
    let (mut ref_within, mut ref_fallback) = (0u64, 0u64);
    let (mut other_types, mut hapax_other_types, mut recurring_other_types) = (0u64, 0u64, 0u64);
    let (mut flagged_first_upper, mut flagged_forced) = (0u64, 0u64);

    // Category sets for sampling, decided at the reference cell (k=32, 0.95).
    use std::collections::{HashMap, HashSet};
    let mut flagged: HashSet<String> = HashSet::new();
    let mut fallback_keys: HashSet<String> = HashSet::new();
    // Excused = recurring OtherMixed (other >= 2) that route A leaves silent.
    let mut excused: Vec<(String, u64, u64)> = Vec::new();

    for (key, p) in &profiles {
        if p.other == 0 {
            continue;
        }
        other_types += 1;
        if p.total() == 1 {
            hapax_other_types += 1;
        }
        if p.other >= 2 {
            recurring_other_types += 1;
        }
        let dom = sig_wilson_lb(p.not_other(), p.total(), MC_Z);

        // Route A (within-word): each OtherMixed occurrence surfaces iff the
        // type's score clears the floor.
        for (ki, &k) in PACKET_KS.iter().enumerate() {
            let sc = dom * rarity_abs(p.other, k);
            for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                if sc >= fl {
                    grid_within[ki][fi] += p.other;
                }
            }
        }
        let sc_ref = dom * rarity_abs(p.other, REF_K);
        hist_within[(sc_ref.clamp(0.0, 0.999_999) * 40.0) as usize] += p.other;
        if sc_ref >= REF_FLOOR {
            ref_within += p.other;
            flagged.insert(key.clone());
            flagged_first_upper += p.other_first_upper;
            flagged_forced += p.other_forced;
        } else if p.other >= 2 {
            excused.push((key.clone(), p.other, p.total()));
        }

        // Route B (corpus fallback) — hapax OtherMixed words only (route A is
        // structurally silent for them: not_other == 0 ⇒ dominance 0). Score is
        // corpus_dominance × rarity(1, k) = corpus_dominance (knee-independent).
        if p.total() == 1 {
            for (ki, _k) in PACKET_KS.iter().enumerate() {
                for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                    if corpus_dominance >= fl {
                        grid_fallback[ki][fi] += 1;
                    }
                }
            }
            if corpus_dominance >= REF_FLOOR {
                ref_fallback += 1;
                fallback_keys.insert(key.clone());
            }
        }
    }

    excused.sort_by_key(|(_, other, _)| std::cmp::Reverse(*other));
    let excused_set: HashSet<&str> = excused
        .iter()
        .take(30)
        .map(|(k, _, _)| k.as_str())
        .collect();

    // Draw review samples from the capped candidate list.
    let (mut flagged_samples, mut fallback_samples, mut excused_samples) =
        (Vec::new(), Vec::new(), Vec::new());
    let mut excused_seen: HashSet<String> = HashSet::new();
    let key_to_text: HashMap<&str, &str> = map
        .keys()
        .iter()
        .zip(map.texts())
        .map(|(k, t)| (k.as_str(), t.as_str()))
        .collect();
    for c in &cands {
        let Some(p) = profiles.get(&c.key) else {
            continue;
        };
        let text = key_to_text[c.sid.as_str()];
        let word = text[c.start as usize..c.end as usize].to_string();
        let dom = sig_wilson_lb(p.not_other(), p.total(), MC_Z);
        let mk = |route: &'static str, score: f64| McSample {
            sid: c.sid.to_string(),
            word: word.clone(),
            route,
            other: p.other,
            word_total: p.total(),
            dom,
            score,
            first_upper: c.first_upper,
            forced: c.forced,
            ctx: casing_ctx(text, c.start as usize, c.end as usize),
        };
        if flagged.contains(&c.key) && flagged_samples.len() < 60 {
            flagged_samples.push(mk("within", dom * rarity_abs(p.other, REF_K)));
        } else if fallback_keys.contains(&c.key) && fallback_samples.len() < 60 {
            fallback_samples.push(mk("fallback", corpus_dominance));
        } else if excused_set.contains(c.key.as_str())
            && excused_seen.insert(c.key.clone())
            && excused_samples.len() < 40
        {
            excused_samples.push(mk("excused", dom * rarity_abs(p.other, REF_K)));
        }
    }

    McCorpus {
        id,
        verses: map.len(),
        cased_tokens,
        other_tokens,
        other_types,
        hapax_other_types,
        recurring_other_types,
        corpus_dominance,
        cased_forced,
        other_forced,
        cased_mid,
        other_mid,
        other_first_upper,
        other_first_lower: other_tokens.saturating_sub(other_first_upper),
        flagged_first_upper,
        flagged_forced,
        grid_within,
        grid_fallback,
        hist_within,
        ref_within,
        ref_fallback,
        flagged_samples,
        excused_samples,
        fallback_samples,
    }
}

fn print_mc_grid(name: &str, grid: &[[u64; PACKET_FLOORS.len()]]) {
    println!("  [{name}] rows = floor, cols = k");
    print!("    {:>6}", "fl\\k");
    for k in PACKET_KS {
        print!("  {:>10}", format!("k={k:.0}"));
    }
    println!();
    for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
        print!("    {fl:>6.2}");
        for row in grid {
            print!("  {:>10}", row[fi]);
        }
        println!();
    }
}

fn print_mc_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!(
        "\nroute-A score histogram at ref knee (k=32) — {total} OtherMixed sites, 40 buckets:"
    );
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>8} {bar}", lo + 0.025);
    }
}

fn print_mc_samples(samples: &[&McSample]) {
    for s in samples {
        println!(
            "    {:<12} {:<9} [{}] {} {} dom={:.3} other={} wtot={} score={:.3} | {}",
            s.sid,
            s.route,
            s.word,
            if s.first_upper { "Up1" } else { "lo1" },
            if s.forced { "FORCED" } else { "mid" },
            s.dom,
            s.other,
            s.word_total,
            s.score,
            s.ctx,
        );
    }
}

pub(crate) fn mixedcase_single_report(c: &McCorpus) {
    println!(
        "=== mixed-case word SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    println!(
        "cased letter-run tokens: {}  |  OtherMixed tokens: {} ({:.4}%)  types: {} (hapax {}, recurring>=2 {})",
        c.cased_tokens,
        c.other_tokens,
        c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
        c.other_types,
        c.hapax_other_types,
        c.recurring_other_types,
    );
    println!(
        "corpus 'not-other-mixed' dominance (route-B factor): {:.4}",
        c.corpus_dominance
    );
    println!(
        "\nposition (route 4): OtherMixed rate  forced {:.4}% ({}/{})  vs  mid {:.4}% ({}/{})",
        c.other_forced as f64 * 100.0 / c.cased_forced.max(1) as f64,
        c.other_forced,
        c.cased_forced,
        c.other_mid as f64 * 100.0 / c.cased_mid.max(1) as f64,
        c.other_mid,
        c.cased_mid,
    );
    println!(
        "boundary vs casing v2: OtherMixed first-letter  upper {} (casing-invisible)  lower {} (overlaps casing)",
        c.other_first_upper, c.other_first_lower
    );
    println!(
        "\nreference cell (k=32, floor 0.95): route-A within {}  |  route-B hapax-fallback {}",
        c.ref_within, c.ref_fallback
    );
    println!(
        "  of ref-flagged route-A sites: first-upper {}  forced-position {}",
        c.flagged_first_upper, c.flagged_forced
    );
    println!("\n-- route-A (within-word) volume sweep --");
    print_mc_grid("within-word", &c.grid_within);
    println!("\n-- route-B (corpus hapax fallback) volume sweep --");
    print_mc_grid("hapax-fallback", &c.grid_fallback);
    print_mc_hist(&c.hist_within);

    let mut fs: Vec<&McSample> = c.flagged_samples.iter().collect();
    fs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop route-A flagged samples:");
    print_mc_samples(&fs.iter().take(25).copied().collect::<Vec<_>>());
    println!("\nexcused recurring-convention samples (route A silent):");
    print_mc_samples(&c.excused_samples.iter().take(25).collect::<Vec<_>>());
    println!("\nroute-B hapax-fallback samples:");
    print_mc_samples(&c.fallback_samples.iter().take(25).collect::<Vec<_>>());
}

pub(crate) fn mixedcase_fleet(dir: &Path) {
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
    eprintln!("mixedcase fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<McCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = analyze_mixedcase(id, &map);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("mixedcase fleet evaluate: {:?}", t0.elapsed());

    let nk = PACKET_KS.len();
    let mut grid_within = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_fallback = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist = [0u64; 40];
    let (mut ref_within, mut ref_fallback) = (0u64, 0u64);
    let (mut cased, mut other) = (0u64, 0u64);
    let (mut cf, mut of, mut cm, mut om) = (0u64, 0u64, 0u64, 0u64);
    let (mut fup, mut flo) = (0u64, 0u64);
    let (mut fl_up, mut fl_forced) = (0u64, 0u64);
    let mut corpora_with_ref = 0u32;
    for c in &corpora {
        for ki in 0..nk {
            for fi in 0..PACKET_FLOORS.len() {
                grid_within[ki][fi] += c.grid_within[ki][fi];
                grid_fallback[ki][fi] += c.grid_fallback[ki][fi];
            }
        }
        for (h, ch) in hist.iter_mut().zip(&c.hist_within) {
            *h += ch;
        }
        ref_within += c.ref_within;
        ref_fallback += c.ref_fallback;
        cased += c.cased_tokens;
        other += c.other_tokens;
        cf += c.cased_forced;
        of += c.other_forced;
        cm += c.cased_mid;
        om += c.other_mid;
        fup += c.other_first_upper;
        flo += c.other_first_lower;
        fl_up += c.flagged_first_upper;
        fl_forced += c.flagged_forced;
        if c.ref_within + c.ref_fallback > 0 {
            corpora_with_ref += 1;
        }
    }

    println!(
        "=== MIXED-CASE WORD SPIKE — fleet aggregate ({} corpora) ===",
        corpora.len()
    );
    println!(
        "\ncased letter-run tokens {cased}  |  OtherMixed {other} ({:.4}% of cased)",
        other as f64 * 100.0 / cased.max(1) as f64
    );
    println!(
        "\n-- reference cell (k=32, floor 0.95) --\n  route-A within {ref_within}  |  route-B hapax-fallback {ref_fallback}  across {corpora_with_ref} corpora"
    );
    println!(
        "  of ref-flagged route-A sites: first-letter-upper {fl_up} (casing-invisible), forced-position {fl_forced}"
    );

    println!("\n-- route 4 (position independence): OtherMixed rate forced vs mid --");
    println!(
        "  forced {:.4}% ({of}/{cf})   mid {:.4}% ({om}/{cm})   ratio forced/mid = {:.3}",
        of as f64 * 100.0 / cf.max(1) as f64,
        om as f64 * 100.0 / cm.max(1) as f64,
        (of as f64 / cf.max(1) as f64) / (om as f64 / cm.max(1) as f64).max(1e-12),
    );
    println!(
        "\n-- boundary vs casing v2: all OtherMixed by first-cased-letter --\n  first-upper {fup} (casing-invisible: casing fires only on lowercase word-starts)  first-lower {flo} (overlaps casing's lowercase-site domain)"
    );

    println!("\n-- route-A (within-word) volume sweep (surfaced OtherMixed sites) --");
    print_mc_grid("within-word", &grid_within);
    println!("\n-- route-B (corpus hapax fallback) volume sweep --");
    print_mc_grid("hapax-fallback", &grid_fallback);
    print_mc_hist(&hist);

    // Noisiest corpora by route-A ref volume, with storm diagnosis inputs.
    let mut ranked: Vec<&McCorpus> = corpora
        .iter()
        .filter(|c| c.ref_within + c.ref_fallback > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.ref_within + c.ref_fallback));
    println!("\n-- top-20 noisiest corpora (ref cell) --");
    println!(
        "  {:<20} {:>8} {:>8} {:>9} {:>10} {:>8}",
        "corpus", "withinA", "hapaxB", "other%", "corpusDom", "hapaxTy"
    );
    for c in ranked.iter().take(20) {
        println!(
            "  {:<20} {:>8} {:>8} {:>8.3}% {:>10.4} {:>8}",
            c.id,
            c.ref_within,
            c.ref_fallback,
            c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
            c.corpus_dominance,
            c.hapax_other_types,
        );
    }

    // Convention-class adjudication across major-language corpora.
    println!(
        "\n-- convention adjudication (major corpora): flagged (route A), excused (recurring), hapax (route B) --"
    );
    for c in &corpora {
        if !MC_MAJOR.contains(&c.id.as_str()) {
            continue;
        }
        println!(
            "\n[{}] other {} ({:.3}%) corpusDom {:.4} | ref withinA {} hapaxB {}",
            c.id,
            c.other_tokens,
            c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
            c.corpus_dominance,
            c.ref_within,
            c.ref_fallback,
        );
        let mut fs: Vec<&McSample> = c.flagged_samples.iter().collect();
        fs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        if !fs.is_empty() {
            println!("  flagged (route A):");
            print_mc_samples(&fs.iter().take(4).copied().collect::<Vec<_>>());
        }
        if !c.excused_samples.is_empty() {
            println!("  excused (recurring convention):");
            print_mc_samples(&c.excused_samples.iter().take(4).collect::<Vec<_>>());
        }
        if !c.fallback_samples.is_empty() {
            println!("  hapax (route B only):");
            print_mc_samples(&c.fallback_samples.iter().take(4).collect::<Vec<_>>());
        }
    }
}

#[cfg(test)]
mod mixedcase_tests {
    use super::{McShape, mc_classify, mc_first_cased_upper};

    #[test]
    fn plain_shapes() {
        assert_eq!(mc_classify("word"), Some(McShape::Lower));
        assert_eq!(mc_classify("Word"), Some(McShape::Title));
        assert_eq!(mc_classify("WORD"), Some(McShape::AllCaps));
        assert_eq!(mc_classify("wOrd"), Some(McShape::OtherMixed));
    }

    #[test]
    fn single_letter_is_never_mixed() {
        assert_eq!(mc_classify("I"), Some(McShape::AllCaps));
        assert_eq!(mc_classify("a"), Some(McShape::Lower));
    }

    #[test]
    fn caseless_has_no_shape() {
        // Han / digits-only-ish: no cased letter ⇒ None (not a candidate).
        assert_eq!(mc_classify("好"), None);
        assert_eq!(mc_classify("1"), None);
    }

    #[test]
    fn convention_shapes_are_othermixed() {
        // McX names, class-prefix orthographies, inflected all-caps names.
        assert_eq!(mc_classify("McDonald"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("kiSwahili"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("iPhone"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("LORDs"), Some(McShape::OtherMixed));
        // Pure ALLCAPS YHWH stays AllCaps — not a mixed candidate at all.
        assert_eq!(mc_classify("LORD"), Some(McShape::AllCaps));
    }

    #[test]
    fn combining_marks_and_caseless_do_not_manufacture_mixing() {
        // Base + combining acute (decomposed é): still Lower.
        assert_eq!(mc_classify("cafe\u{0301}"), Some(McShape::Lower));
        // Title with a trailing combining mark stays Title.
        assert_eq!(mc_classify("A\u{0301}bc"), Some(McShape::Title));
    }

    #[test]
    fn first_cased_axis() {
        assert!(mc_first_cased_upper("McDonald"));
        assert!(mc_first_cased_upper("LORDs"));
        assert!(!mc_first_cased_upper("wOrd"));
        assert!(!mc_first_cased_upper("kiSwahili"));
    }
}
