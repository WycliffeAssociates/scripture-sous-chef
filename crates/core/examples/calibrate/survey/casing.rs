// ═══════════════════════════════════════════════════════════════════════════
// Casing two-factor calibration (ADR 0051). Consumes the real
// `signals::casing::evaluate` (one classified `SiteEval` per lowercase site,
// with each channel's dominance/minority/opportunities), then sweeps the
// absolute recurrence knee `k` and the emission floor over those factors —
// `score = dominance × rarity(minority, k)` — exactly as the shipped rules do
// at the frozen knobs. The rules apply floor 0.95 / k 32; this reports the
// grid around that so the packet volumes stay reproducible.
// ═══════════════════════════════════════════════════════════════════════════

use std::path::Path;

use ssc_core::Corpus;
use ssc_core::rule::StatefulRule;
use ssc_core::signals::casing::{PosClass, SiteEval, evaluate};

use super::shared::{PACKET_FLOORS, PACKET_KS, REF_FLOOR, REF_K, rarity_abs};
use crate::vref_io::load_corpus;

/// Named review anchors tracked across the fleet — `(corpus, sid, lowercased
/// word)`; the ADR 0051 adjudicated true/false positives.
const ANCHORS: &[(&str, &str, &str)] = &[
    ("swhulb", "LUK 8:44", "yesu"),        // TP intrinsic
    ("WA-fr-ulb", "JHN 13:2", "jésus"),    // TP intrinsic
    ("spaRV1909", "1SA 7:8", "filisteos"), // TP intrinsic
    ("vie1934", "MAT 24:24", "christ"),    // TP intrinsic (min 2)
    ("eng-web", "3MA 6:9", "gentiles"),    // TP-ish intrinsic
    ("eng-kjv", "SIR 7:5", "justify"),     // TP positional (cross-seam)
    ("WA-en-ulb", "LAM 1:22", "deal"),     // TP positional (min 2)
    ("fraLSG", "ACT 19:13", "juifs"),      // FP intrinsic (French adjective)
    ("porblt", "MAT 24:24", "messias"),    // FP intrinsic (generic plural)
    ("deu1912", "PHM 1:9", "alter"),       // FP intrinsic (adj/noun homograph)
    ("ind", "DEU 14:12", "rajawali"),      // FP positional (list colon)
    ("nld", "GEN 6:19", "mannetje"),       // FP positional (list colon)
];

/// A site's two channel scores at knee `k` (0 where the channel is absent).
fn site_scores(s: &SiteEval, k: f64) -> (f64, f64) {
    let intr = s
        .intrinsic
        .map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
    let pos = s
        .positional
        .map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
    (intr, pos)
}

/// The site's quadrant (`None` = not a clean anomaly candidate).
fn site_quad(s: &SiteEval) -> Option<&'static str> {
    match (s.intrinsic.is_some(), s.positional.is_some()) {
        (true, true) => Some("both"),
        (true, false) => Some("intrinsic"),
        (false, true) => Some("positional"),
        (false, false) => None,
    }
}

fn pos_glyph(pos: PosClass) -> Option<char> {
    match pos {
        PosClass::ForcedAfterTerminal(ck) => Some(ck.mark),
        _ => None,
    }
}

/// One tracked anchor's factors, so its score is recomputable at any k.
#[derive(Clone)]
struct AnchorHit {
    corpus: String,
    sid: String,
    word: String,
    quad: &'static str,
    intr: Option<(f64, u64, u64)>,
    pos: Option<(f64, u64, u64)>,
}

impl AnchorHit {
    fn score(&self, k: f64) -> f64 {
        let i = self.intr.map_or(0.0, |(d, m, _)| d * rarity_abs(m, k));
        let p = self.pos.map_or(0.0, |(d, m, _)| d * rarity_abs(m, k));
        i.max(p)
    }
}

/// One surfaced site sampled for review.
struct CasingSample {
    sid: String,
    quad: &'static str,
    word: String,
    glyph: Option<char>,
    dom: f64,
    minority: u64,
    opps: u64,
    score: f64,
    ctx: String,
}

/// Per-corpus casing result. Grids are `[knee][floor]`, fleet-summable.
pub(crate) struct CasingCorpus {
    id: String,
    verses: usize,
    sites: u64,
    grid_intr: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_pos: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_both: Vec<[u64; PACKET_FLOORS.len()]>,
    hist: [u64; 40],
    ref_intrinsic: u64,
    ref_positional: u64,
    ref_both: u64,
    anchors: Vec<AnchorHit>,
    samples: Vec<CasingSample>,
}

/// Run the real casing model over one corpus and roll up the sweep grids,
/// reference-setting counts, histogram, tracked anchors, and samples.
pub(crate) fn analyze_casing(id: String, map: &Corpus) -> CasingCorpus {
    let books = ssc_core::corpus::by_book(map);
    // Production knobs (ADR 0051 floor/k/z + ADR 0052 trust gate 0.90). The
    // sweep below varies floor/k around the reference cell; the trust gate and
    // discount are baked into the returned factors.
    let sites = evaluate(&books, &ssc_core::config::CasingConfig::default());

    let nk = PACKET_KS.len();
    let mut grid_intr = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_pos = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_both = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist = [0u64; 40];
    let (mut ref_i, mut ref_p, mut ref_b) = (0u64, 0u64, 0u64);
    let mut anchors: Vec<AnchorHit> = Vec::new();
    let mut samples: Vec<CasingSample> = Vec::new();
    let anchor_corpus = ANCHORS.iter().any(|a| a.0 == id);
    let mut n_sites = 0u64;

    for s in &sites {
        let Some(quad) = site_quad(s) else { continue };
        n_sites += 1;
        let text = map.text(s.key_idx);
        let word = text[s.start as usize..s.end as usize].to_lowercase();

        // Sweep grids.
        for (ki, &k) in PACKET_KS.iter().enumerate() {
            let (is, ps) = site_scores(s, k);
            let surf = is.max(ps);
            for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                if surf >= fl {
                    match quad {
                        "intrinsic" => grid_intr[ki][fi] += 1,
                        "positional" => grid_pos[ki][fi] += 1,
                        _ => grid_both[ki][fi] += 1,
                    }
                }
            }
        }

        // Reference setting (k=32, floor 0.95): counts, histogram, samples.
        let (is, ps) = site_scores(s, REF_K);
        let surf = is.max(ps);
        hist[(surf.clamp(0.0, 0.999_999) * 40.0) as usize] += 1;
        if surf >= REF_FLOOR {
            match quad {
                "intrinsic" => ref_i += 1,
                "positional" => ref_p += 1,
                _ => ref_b += 1,
            }
            if samples.len() < 400 {
                let (dom, min, opp) = if ps >= is {
                    let f = s.positional.unwrap();
                    (f.dominance, f.minority, f.opportunities)
                } else {
                    let f = s.intrinsic.unwrap();
                    (f.dominance, f.minority, f.opportunities)
                };
                samples.push(CasingSample {
                    sid: map.key(s.key_idx).to_string(),
                    quad,
                    word: text[s.start as usize..s.end as usize].to_string(),
                    glyph: pos_glyph(s.pos),
                    dom,
                    minority: min,
                    opps: opp,
                    score: surf,
                    ctx: casing_ctx(text, s.start as usize, s.end as usize),
                });
            }
        }

        // Anchor capture.
        if anchor_corpus
            && ANCHORS
                .iter()
                .any(|&(ac, asid, aw)| ac == id && asid == map.key(s.key_idx) && aw == word)
        {
            anchors.push(AnchorHit {
                corpus: id.clone(),
                sid: map.key(s.key_idx).to_string(),
                word,
                quad,
                intr: s
                    .intrinsic
                    .map(|f| (f.dominance, f.minority, f.opportunities)),
                pos: s
                    .positional
                    .map(|f| (f.dominance, f.minority, f.opportunities)),
            });
        }
    }

    CasingCorpus {
        id,
        verses: map.len(),
        sites: n_sites,
        grid_intr,
        grid_pos,
        grid_both,
        hist,
        ref_intrinsic: ref_i,
        ref_positional: ref_p,
        ref_both: ref_b,
        anchors,
        samples,
    }
}

/// ~24 chars of lead-in plus the flagged word, whitespace flattened.
pub(crate) fn casing_ctx(text: &str, start: usize, end: usize) -> String {
    let ctx_start = text[..start]
        .char_indices()
        .rev()
        .nth(23)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let ctx_end = text[end..]
        .char_indices()
        .nth(24)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[ctx_start..ctx_end].replace(['\t', '\n'], " ")
}

fn print_casing_grid(name: &str, grid: &[[u64; PACKET_FLOORS.len()]]) {
    println!("  [{name}] rows = floor, cols = k");
    print!("    {:>6}", "fl\\k");
    for k in PACKET_KS {
        print!("  {:>8}", format!("k={k:.0}"));
    }
    println!();
    for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
        print!("    {fl:>6.2}");
        for row in grid {
            print!("  {:>8}", row[fi]);
        }
        println!();
    }
}

fn print_casing_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!("\nscore histogram at ref knee (k=32) — {total} sites, 40 buckets:");
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>7} {bar}", lo + 0.025);
    }
}

fn print_casing_samples(samples: &[&CasingSample]) {
    for s in samples {
        println!(
            "    {:<11} {:<10} [{}] g={} dom={:.3} min={} opp={} score={:.3} | {}",
            s.sid,
            s.quad,
            s.word,
            s.glyph
                .map(|c| format!("{c:?}"))
                .unwrap_or_else(|| "^".to_string()),
            s.dom,
            s.minority,
            s.opps,
            s.score,
            s.ctx,
        );
    }
}

/// Detailed single-corpus casing report.
pub(crate) fn casing_single_report(c: &CasingCorpus) {
    println!("=== casing (ADR 0051): {} ({} verses) ===", c.id, c.verses);
    println!("classifiable lowercase sites: {}", c.sites);
    println!(
        "\nreference setting (k=32, floor 0.95): surfaced {} (intrinsic {}, positional {}, both {})",
        c.ref_intrinsic + c.ref_positional + c.ref_both,
        c.ref_intrinsic,
        c.ref_positional,
        c.ref_both
    );
    println!("\nsurfaced-site volume sweep:");
    print_casing_grid("intrinsic", &c.grid_intr);
    print_casing_grid("positional", &c.grid_pos);
    print_casing_grid("both-quadrant", &c.grid_both);
    print_casing_hist(&c.hist);

    let mut s: Vec<&CasingSample> = c.samples.iter().collect();
    s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop surfaced samples (ref knee):");
    print_casing_samples(&s.iter().take(20).copied().collect::<Vec<_>>());
    println!("\nnear-floor surfaced samples:");
    print_casing_samples(&s.iter().rev().take(10).copied().collect::<Vec<_>>());
}

/// Fleet aggregate over every vref corpus in `dir`.
pub(crate) fn casing_fleet(dir: &Path) {
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
    eprintln!("casing fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<CasingCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = if map.is_empty() {
                analyze_casing(id, &Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap())
            } else {
                analyze_casing(id, &map)
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(100) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("casing fleet evaluate: {:?}", t0.elapsed());

    // Fleet aggregates.
    let nk = PACKET_KS.len();
    let (mut ref_i, mut ref_p, mut ref_b) = (0u64, 0u64, 0u64);
    let mut hist = [0u64; 40];
    let mut corpora_with_ref = 0u32;
    for c in &corpora {
        ref_i += c.ref_intrinsic;
        ref_p += c.ref_positional;
        ref_b += c.ref_both;
        for (h, ch) in hist.iter_mut().zip(&c.hist) {
            *h += ch;
        }
        if c.ref_intrinsic + c.ref_positional + c.ref_both > 0 {
            corpora_with_ref += 1;
        }
    }

    println!(
        "=== CASING TWO-FACTOR (ADR 0051) — fleet aggregate ({} corpora) ===",
        corpora.len()
    );
    println!(
        "\n-- reference setting (k=32, floor 0.95) --\n  surfaced: {}  (intrinsic {ref_i}, positional {ref_p}, both {ref_b})  across {corpora_with_ref} corpora",
        ref_i + ref_p + ref_b
    );

    // Packet 1 — per-channel volume, affected corpora, top-5 corpus share.
    // `chan`: 0 = intrinsic, 1 = positional, 2 = both.
    let channel_cell = |chan: u8, ki: usize, fi: usize| -> (u64, u32, f64) {
        let mut counts: Vec<u64> = corpora
            .iter()
            .map(|c| match chan {
                0 => c.grid_intr[ki][fi],
                1 => c.grid_pos[ki][fi],
                _ => c.grid_both[ki][fi],
            })
            .filter(|&n| n > 0)
            .collect();
        let total: u64 = counts.iter().sum();
        let affected = counts.len() as u32;
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top5: u64 = counts.iter().take(5).sum();
        (
            total,
            affected,
            if total > 0 {
                top5 as f64 / total as f64
            } else {
                0.0
            },
        )
    };
    println!(
        "\n-- packet 1: per-channel surfaced volume | total (affected corpora; top-5 share) --"
    );
    for (chan, name) in [(0u8, "intrinsic"), (1, "positional"), (2, "both-quadrant")] {
        println!("  [{name}]  rows = floor, cols = k");
        print!("    {:>6}", "fl\\k");
        for k in PACKET_KS {
            print!("  {:>22}", format!("k={k:.0}"));
        }
        println!();
        for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
            print!("    {fl:>6.2}");
            for ki in 0..nk {
                let (t, a, sh) = channel_cell(chan, ki, fi);
                print!("  {:>22}", format!("{t} ({a}; {:.0}%)", sh * 100.0));
            }
            println!();
        }
    }

    // Packet 2 — anchor fates.
    let all_anchors: Vec<&AnchorHit> = corpora.iter().flat_map(|c| c.anchors.iter()).collect();
    println!("\n-- packet 2: anchor fates — factors, score@k, alive floors at k=32 --");
    for &(ac, asid, aw) in ANCHORS {
        match all_anchors
            .iter()
            .find(|h| h.corpus == ac && h.sid == asid && h.word == aw)
        {
            Some(h) => {
                let (s8, s16, s32) = (h.score(8.0), h.score(16.0), h.score(32.0));
                let alive: Vec<String> = PACKET_FLOORS
                    .iter()
                    .filter(|&&fl| s32 >= fl)
                    .map(|fl| format!("{fl:.2}"))
                    .collect();
                let ifac = h
                    .intr
                    .map(|(d, m, o)| format!("i(d{d:.3} m{m} o{o})"))
                    .unwrap_or_default();
                let pfac = h
                    .pos
                    .map(|(d, m, o)| format!("p(d{d:.3} m{m} o{o})"))
                    .unwrap_or_default();
                println!(
                    "  {ac:<11} {asid:<9} {aw:<11} {:<11} {ifac} {pfac}  s@8={s8:.3} @16={s16:.3} @32={s32:.3}  alive≥[{}]",
                    h.quad,
                    if alive.is_empty() {
                        "dead@0.80+".to_string()
                    } else {
                        alive.join(",")
                    },
                );
            }
            None => println!(
                "  {ac:<11} {asid:<9} {aw:<11} — not captured (not a lowercase anomaly candidate)"
            ),
        }
    }

    print_casing_hist(&hist);

    // Noisiest corpora at the reference setting.
    let mut ranked: Vec<&CasingCorpus> = corpora
        .iter()
        .filter(|c| c.ref_intrinsic + c.ref_positional + c.ref_both > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.ref_intrinsic + c.ref_positional + c.ref_both));
    println!("\n-- top-15 noisiest corpora (ref setting) --");
    for c in ranked.iter().take(15) {
        println!(
            "  {:<24} surfaced {:>6}  (i {}, p {}, b {})",
            c.id,
            c.ref_intrinsic + c.ref_positional + c.ref_both,
            c.ref_intrinsic,
            c.ref_positional,
            c.ref_both
        );
    }

    // Samples from major-language corpora.
    const MAJOR: &[&str] = &[
        "eng-web",
        "eng-kjv",
        "engwebster",
        "WA-en-ulb",
        "spaRV1909",
        "WA-es-419-ulb",
        "fraLSG",
        "WA-fr-ulb",
        "porblt",
        "ita1885",
        "ron1924",
        "deu1912",
        "swhulb",
        "WA-sw-ulb",
        "ind",
        "nld",
        "vie1934",
        "tglulb",
    ];
    println!("\n-- surfaced samples from major-language corpora (ref knee) --");
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples.is_empty() {
            continue;
        }
        let mut s: Vec<&CasingSample> = c.samples.iter().collect();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        println!(
            "  [{}] surfaced {} (i {}, p {}, b {}):",
            c.id,
            c.ref_intrinsic + c.ref_positional + c.ref_both,
            c.ref_intrinsic,
            c.ref_positional,
            c.ref_both
        );
        print_casing_samples(&s.iter().take(3).copied().collect::<Vec<_>>());
    }
}

/// Casing stats-size probe: reduce every corpus with the real
/// `SentenceInitialLowercase` rule and report the serialized `CasingStats`
/// JSON byte size (the wire size the shell round-trips) — p50/p90/max plus a
/// few named corpora.
pub(crate) fn casing_size(dir: &Path) {
    use rayon::prelude::*;
    use ssc_core::config::CasingConfig;
    use ssc_core::signals::casing::SentenceInitialLowercase;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let rule = SentenceInitialLowercase {
        cfg: CasingConfig::default(),
    };
    let mut rows: Vec<(String, usize)> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let books = ssc_core::corpus::by_book(&map);
            let (stats, _) = rule.reduce(&books, None, None);
            // The monolithic serialized `Stats` wire was retired (granularity-
            // spine Phase A step 5); this size survey measures the inner
            // `CasingStats` aggregate directly, which still derives serde.
            let bytes = match &stats {
                ssc_core::RuleStats::Casing(cs) => {
                    serde_json::to_string(cs).map(|s| s.len()).unwrap_or(0)
                }
                _ => 0,
            };
            (id, bytes)
        })
        .collect();
    rows.sort_by_key(|r| r.1);
    let n = rows.len();
    let pct = |q: f64| rows[((n - 1) as f64 * q) as usize].1;
    println!("casing CasingStats JSON size over {n} corpora:");
    println!(
        "  p50 {} B  p90 {} B  max {} B",
        pct(0.5),
        pct(0.9),
        pct(1.0)
    );
    println!("  largest: {} ({} B)", rows[n - 1].0, rows[n - 1].1);
    for id in ["eng-kjv", "deu1912", "swhulb", "vie1934"] {
        if let Some((_, b)) = rows.iter().find(|r| r.0 == id) {
            println!("  {id}: {b} B");
        }
    }
}

