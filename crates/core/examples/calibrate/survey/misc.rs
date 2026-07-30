use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::config::{
    BracketBalanceConfig, CasingConfig, MixedScriptConfig, PunctOnlyTokenConfig,
    PunctuationAdjacencyConfig, PunctuationSpacingConfig, RepeatedCharacterRunConfig,
};
use ssc_core::signals::bracket_balance::bracket_findings;
use ssc_core::signals::lexical::{punct_only_findings, repeated_run_findings};
use ssc_core::signals::punctuation::adjacency_findings;
use ssc_core::{
    BracketMeasure, Config, Corpus, Finding, FindingArgs, RuleId, analyze, analyze_with_config,
};

use crate::vref_io::load_corpus;

/// Per-verse batch over one corpus with the shipped defaults: counts per
/// rule, worst book per rule, and a few sample slices per rule.
pub(crate) fn batch(dir: &Path) {
    let t0 = std::time::Instant::now();
    let target = load_corpus(dir);
    let t_load = t0.elapsed();
    let t1 = std::time::Instant::now();
    let findings = analyze(&target, None);
    let t_analyze = t1.elapsed();
    eprintln!(
        "{} verses | load+parse {:?} | analyze {:?} ({:.1} µs/verse)",
        target.len(),
        t_load,
        t_analyze,
        t_analyze.as_secs_f64() * 1e6 / target.len() as f64
    );

    let mut by_rule: BTreeMap<RuleId, Vec<&ssc_core::Finding>> = BTreeMap::new();
    for f in &findings {
        by_rule.entry(f.code).or_default().push(f);
    }
    println!("total findings: {}\n", findings.len());
    for (rule, fs) in &by_rule {
        let mut per_book: BTreeMap<String, usize> = BTreeMap::new();
        for f in fs {
            let book = ssc_core::key::parse_key(target.key(f.key_idx))
                .unwrap()
                .book
                .to_string();
            *per_book.entry(book).or_default() += 1;
        }
        let (worst_book, worst) = per_book
            .iter()
            .max_by_key(|&(_, n)| *n)
            .map(|(b, n)| (b.clone(), *n))
            .unwrap();
        println!("{rule}: {} (worst book {worst_book}: {worst})", fs.len());
        for f in fs.iter().take(5) {
            let key = target.key(f.key_idx);
            let text = target.text(f.key_idx);
            let slice: String = f.range.slice(text).chars().take(40).collect();
            let ctx_start = text[..f.range.start as usize]
                .char_indices()
                .rev()
                .nth(19)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let ctx: String = text[ctx_start..].chars().take(60).collect();
            println!("    {:<10} [{slice}] …{ctx}", key);
        }
    }
}

/// One finding sampled for the fleet report: enough to render a "what this
/// looks like in real text" row without shipping the corpus.
struct FleetSample {
    corpus: String,
    sid: String,
    score: Option<f32>,
    slice: String,
    ctx: String,
}

/// Per-corpus tally from one fleet pass.
struct FleetRow {
    id: String,
    verses: usize,
    chars: usize,
    /// Findings the shipped floor would show the user, per `RuleId::ALL` slot.
    surfaced: Vec<u32>,
    /// All scored sites at floor zero (== `surfaced` for unscored rules).
    sites: Vec<u32>,
    /// Score histogram per rule, aligned with that rule's bucket edges.
    hists: Vec<Vec<u64>>,
    /// ≤ 2 best surfaced samples per rule (corpus diversity cap).
    samples: Vec<Vec<FleetSample>>,
}

/// Fleet survey: every rule over every vref corpus in `dir`, with all
/// emission floors zeroed so the score histograms include the sub-floor mass
/// the shipped floors suppress. Writes a self-contained HTML report to `out`
/// (per-corpus rates, per-rule score distributions with the shipped floor
/// marked, and sample findings).
pub(crate) fn fleet(dir: &Path, out: &Path) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;

    let n_rules = RuleId::ALL.len();

    // Measurement config: everything on, every floor at zero. Surfaced-vs-not
    // is then recomputed against the shipped floors below, so one pass yields
    // both the full distribution and the user-facing volume.
    let mut cfg = Config::all();
    cfg.bracket_balance.emit_score_min = 0.0;
    cfg.casing.emit_score_min = 0.0;
    cfg.punctuation_adjacency.emit_score_min = 0.0;
    cfg.punctuation_spacing.emit_score_min = 0.0;
    cfg.repeated_character_run.emit_score_min = 0.0;
    cfg.punct_only_token.emit_score_min = 0.0;
    cfg.mixed_script.emit_score_min = 0.0;

    let floors: Vec<Option<f32>> = RuleId::ALL
        .iter()
        .map(|id| match id {
            RuleId::BracketBalance => Some(BracketBalanceConfig::default().emit_score_min),
            RuleId::SentenceInitialLowercase => Some(CasingConfig::default().emit_score_min),
            RuleId::PunctuationAdjacencyAnomaly => {
                Some(PunctuationAdjacencyConfig::default().emit_score_min)
            }
            RuleId::PunctuationSpacingAnomaly => {
                Some(PunctuationSpacingConfig::default().emit_score_min)
            }
            RuleId::RepeatedCharacterRun => {
                Some(RepeatedCharacterRunConfig::default().emit_score_min)
            }
            RuleId::PunctOnlyToken => Some(PunctOnlyTokenConfig::default().emit_score_min),
            RuleId::MixedScriptInToken => Some(MixedScriptConfig::default().emit_score_min),
            _ => None,
        })
        .collect();

    // Histogram bucket edges per rule: 40 uniform buckets plus the shipped
    // floor as an extra edge, so below-floor vs surfaced is exact per bucket.
    let edges: Vec<Vec<f32>> = floors
        .iter()
        .map(|floor| {
            let mut e: Vec<f32> = (0..=40).map(|i| i as f32 / 40.0).collect();
            if let Some(f) = floor
                && e.iter().all(|x| (x - f).abs() > 1e-6)
            {
                e.push(*f);
                e.sort_by(|a, b| a.partial_cmp(b).unwrap());
            }
            e
        })
        .collect();

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let rows: Vec<FleetRow> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let verses = map.len();
            let chars = map.texts().iter().map(|t| t.chars().count()).sum();
            let findings = if verses == 0 {
                Vec::new()
            } else {
                analyze_with_config(&map, None, &cfg)
            };

            let mut surfaced = vec![0u32; n_rules];
            let mut sites = vec![0u32; n_rules];
            let mut hists: Vec<Vec<u64>> = edges.iter().map(|e| vec![0u64; e.len() - 1]).collect();
            let mut samples: Vec<Vec<FleetSample>> = (0..n_rules).map(|_| Vec::new()).collect();

            for f in &findings {
                let ri = RuleId::ALL.iter().position(|r| *r == f.code).unwrap();
                sites[ri] += 1;
                if let Some(s) = f.score {
                    let e = &edges[ri];
                    let b = e.partition_point(|x| *x <= s.clamp(0.0, 0.999_999)) - 1;
                    hists[ri][b.min(e.len() - 2)] += 1;
                }
                let shown = f.score.is_none_or(|s| s >= floors[ri].unwrap_or(0.0));
                if !shown {
                    continue;
                }
                surfaced[ri] += 1;
                // Keep the 2 best surfaced samples per rule per corpus.
                let sv = &mut samples[ri];
                let better_than =
                    |x: &FleetSample| f.score.unwrap_or(0.0) > x.score.unwrap_or(f32::INFINITY);
                if sv.len() < 2 || sv.iter().any(better_than) {
                    let text = map.text(f.key_idx);
                    let sample = FleetSample {
                        corpus: id.clone(),
                        sid: map.key(f.key_idx).to_string(),
                        score: f.score,
                        slice: display_slice(f.range.slice(text), 24),
                        ctx: fleet_context(text, f.range.start as usize),
                    };
                    if sv.len() < 2 {
                        sv.push(sample);
                    } else if let Some((i, _)) = sv.iter().enumerate().min_by(|a, b| {
                        a.1.score
                            .unwrap_or(0.0)
                            .partial_cmp(&b.1.score.unwrap_or(0.0))
                            .unwrap()
                    }) {
                        sv[i] = sample;
                    }
                }
            }

            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(100) {
                eprintln!("  …{n}/{total}");
            }
            FleetRow {
                id,
                verses,
                chars,
                surfaced,
                sites,
                hists,
                samples,
            }
        })
        .collect();

    // Fleet-wide aggregates.
    let mut sites_total = vec![0u64; n_rules];
    let mut surfaced_total = vec![0u64; n_rules];
    let mut corpora_hit = vec![0u32; n_rules];
    for row in &rows {
        for ri in 0..n_rules {
            sites_total[ri] += row.sites[ri] as u64;
            surfaced_total[ri] += row.surfaced[ri] as u64;
            corpora_hit[ri] += (row.surfaced[ri] > 0) as u32;
        }
    }

    let corpora_json: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "v": r.verses, "ch": r.chars, "c": r.surfaced,
            })
        })
        .collect();

    let mut hist_total: Vec<Vec<u64>> = edges.iter().map(|e| vec![0u64; e.len() - 1]).collect();
    let mut samples_all: Vec<Vec<FleetSample>> = (0..n_rules).map(|_| Vec::new()).collect();
    for row in rows {
        for (ri, h) in row.hists.into_iter().enumerate() {
            for (b, n) in h.into_iter().enumerate() {
                hist_total[ri][b] += n;
            }
        }
        for (ri, s) in row.samples.into_iter().enumerate() {
            samples_all[ri].extend(s);
        }
    }
    for sv in &mut samples_all {
        sv.sort_by(|a, b| {
            b.score
                .unwrap_or(0.0)
                .partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap()
                .then_with(|| a.corpus.cmp(&b.corpus))
                .then_with(|| a.sid.cmp(&b.sid))
        });
        sv.truncate(8);
    }

    let rules_json: Vec<serde_json::Value> = RuleId::ALL
        .iter()
        .enumerate()
        .map(|(ri, id)| {
            let scored = hist_total[ri].iter().any(|&n| n > 0);
            let hist: Option<Vec<serde_json::Value>> = scored.then(|| {
                edges[ri]
                    .windows(2)
                    .zip(&hist_total[ri])
                    .map(|(w, &n)| serde_json::json!({"lo": w[0], "hi": w[1], "n": n}))
                    .collect()
            });
            serde_json::json!({
                "code": id.code(),
                "sites": sites_total[ri],
                "surfaced": surfaced_total[ri],
                "corpora_hit": corpora_hit[ri],
                "floor": floors[ri],
                "scored": scored,
                "hist": hist,
                "samples": samples_all[ri].iter().map(|s| serde_json::json!({
                    "corpus": s.corpus, "sid": s.sid, "score": s.score,
                    "slice": s.slice, "ctx": s.ctx,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let data = serde_json::json!({
        "corpus_count": corpora_json.len(),
        "rules": rules_json,
        "corpora": corpora_json,
    });
    // `</` must not appear inside the inline <script> payload; `<\/` is the
    // same string after JSON unescaping.
    let payload = data.to_string().replace("</", "<\\/");
    let html =
        include_str!("../../fleet_report_template.html").replace("__FLEET_DATA__", &payload);
    std::fs::write(out, html).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
}

/// Printable preview of a finding slice: invisibles made visible, whitespace
/// flattened, capped at `max` chars.
pub(crate) fn display_slice(s: &str, max: usize) -> String {
    s.chars()
        .take(max)
        .map(|c| match c {
            '\u{200B}' => '·',
            '\t' | '\n' => ' ',
            c if c.is_control() => '⌧',
            c => c,
        })
        .collect()
}

/// ~20 chars of lead-in plus the finding neighbourhood, for the samples table.
fn fleet_context(text: &str, start: usize) -> String {
    let ctx_start = text[..start]
        .char_indices()
        .rev()
        .nth(19)
        .map(|(i, _)| i)
        .unwrap_or(0);
    display_slice(&text[ctx_start..], 64)
}

/// Redundant-ZWSP report (ADR 0027). The rule is deterministic and default-on, so
/// there is nothing to calibrate — this just reports how much U+200B a corpus
/// carries, how many runs are redundant (doubled U+200B), and confirms
/// deterministic hygiene still flags no U+200B.
pub(crate) fn zwsp_calib(dir: &Path) {
    let target = load_corpus(dir);
    eprintln!("{} verses", target.len());

    let raw: usize = target
        .texts()
        .iter()
        .map(|t| t.matches('\u{200B}').count())
        .sum();

    let f = analyze(&target, None);
    // Deterministic hygiene must still flag zero U+200B (checked by slicing the
    // char, not just the rule id — hyg.zero-width-misuse still owns BOM/bidi/WJ).
    let hyg_zwsp = f
        .iter()
        .filter(|f| f.code == RuleId::ZeroWidthMisuse)
        .filter(|f| {
            target
                .text(f.key_idx)
                .get(f.range.start as usize..f.range.end as usize)
                == Some("\u{200B}")
        })
        .count();
    let redundant: Vec<_> = f
        .iter()
        .filter(|f| f.code == RuleId::RedundantZeroWidthSpace)
        .collect();
    println!(
        "U+200B raw={raw}  redundant runs flagged={}  (hyg U+200B flags: {hyg_zwsp}, must be 0)",
        redundant.len()
    );
    for fd in redundant.iter().take(10) {
        let t = target.text(fd.key_idx);
        let n = t
            .get(fd.range.start as usize..fd.range.end as usize)
            .unwrap_or("")
            .matches('\u{200B}')
            .count();
        println!("  {}  run of {n} U+200B", target.key(fd.key_idx));
    }
}

/// Repeated-character-run calibration at floor zero. The scored distribution
/// comes from the production rule; the TSV joins each site to the human-readable
/// recurrence signals needed for typo/convention spot checks.
pub(crate) fn repeat_calib(dir: &Path, cfg: RepeatedCharacterRunConfig) {
    use std::collections::{HashMap, HashSet};

    use ssc_core::grapheme::segment;
    use ssc_core::signals::lexical::scan_repeated_character_run;
    use ssc_core::token::tokenize;

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);

    // Corpus pass for explanatory TSV columns. Production scoring performs its
    // own reduction below; keeping this throwaway join separate prevents the
    // calibration harness from becoming rule infrastructure.
    let mut word_freq: HashMap<String, usize> = HashMap::new();
    let mut cluster_runs: HashMap<String, usize> = HashMap::new();
    let mut cluster_types: HashMap<String, HashSet<String>> = HashMap::new();
    let mut total_tokens = 0usize;
    let mut lexical_units = 0usize;
    let mut tokens_with_run = 0usize;
    let mut graphemes = Vec::new();
    let mut word_graphemes = Vec::new();

    for text in target.texts() {
        lexical_units += text.split_whitespace().count();
        let tokens = tokenize(text);
        total_tokens += tokens.len();
        graphemes.clear();
        segment(text, &mut graphemes);
        let raw_runs = scan_repeated_character_run(text, &graphemes);
        for run in &raw_runs {
            *cluster_runs
                .entry(run.slice(text).graphemes_first().to_lowercase())
                .or_default() += 1;
        }
        for tok in tokens {
            let word = tok.span.slice(text);
            if word.chars().take(3).count() < 3 {
                continue;
            }
            let folded = word.to_lowercase();
            word_graphemes.clear();
            segment(&folded, &mut word_graphemes);
            if scan_repeated_character_run(&folded, &word_graphemes).is_empty() {
                continue;
            }
            *word_freq.entry(folded.clone()).or_default() += 1;
            let runs: Vec<_> = raw_runs
                .iter()
                .filter(|run| tok.span.start <= run.start && run.end <= tok.span.end)
                .collect();
            if runs.is_empty() {
                continue;
            }
            tokens_with_run += 1;
            let mut seen = HashSet::new();
            for r in runs {
                // Cluster = first grapheme of the run, folded.
                let cluster = r.slice(text).graphemes_first().to_lowercase();
                if seen.insert(cluster.clone()) {
                    cluster_types
                        .entry(cluster)
                        .or_default()
                        .insert(folded.clone());
                }
            }
        }
    }

    let scoring = RepeatedCharacterRunConfig {
        emit_score_min: 0.0,
        ..cfg
    };
    let t0 = std::time::Instant::now();
    let repeat = repeated_run_findings(&target, &scoring);
    eprintln!(
        "{corpus}: repeat map+reduce+judge {:?}; rate={} K={}",
        t0.elapsed(),
        cfg.convention_rate_per_10k,
        cfg.word_recurrence_k
    );
    report_scored("lex.repeated-character-run", &target, &repeat);

    println!(
        "corpus\tsid\tword\tcluster\trun_len\tword_freq\tcluster_runs\tcluster_rate_per_10k\tsame_run_types\ttokens_with_run\tlexical_units\tscore"
    );
    for f in &repeat {
        let text = target.text(f.key_idx);
        let word = tokenize(text)
            .iter()
            .find(|t| t.span.start <= f.range.start && f.range.end <= t.span.end)
            .map(|t| t.span.slice(text).to_string())
            .unwrap_or_default();
        let run_str = f.range.slice(text);
        graphemes.clear();
        segment(run_str, &mut graphemes);
        let run_len = graphemes.len();
        let cluster = run_str.graphemes_first().to_lowercase();
        let folded = word.to_lowercase();
        println!(
            "{corpus}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.4}\t{}\t{}\t{}\t{:.6}",
            target.key(f.key_idx),
            word,
            cluster,
            run_len,
            word_freq.get(&folded).copied().unwrap_or(0),
            cluster_runs.get(&cluster).copied().unwrap_or(0),
            cluster_runs.get(&cluster).copied().unwrap_or(0) as f64 * 10_000.0
                / lexical_units.max(1) as f64,
            cluster_types.get(&cluster).map(|s| s.len()).unwrap_or(0),
            tokens_with_run,
            lexical_units,
            f.score.unwrap_or(0.0),
        );
    }
    eprintln!(
        "{corpus}: {} verses, {} UAX tokens, {} lexical units, {} tokens-with-run ({:.2}/10k UAX tokens), {} findings",
        target.len(),
        total_tokens,
        lexical_units,
        tokens_with_run,
        tokens_with_run as f64 * 10_000.0 / total_tokens.max(1) as f64,
        repeat.len()
    );
}

trait GraphemesFirst {
    fn graphemes_first(&self) -> &str;
}
impl GraphemesFirst for str {
    fn graphemes_first(&self) -> &str {
        use unicode_segmentation::UnicodeSegmentation;
        self.graphemes(true).next().unwrap_or("")
    }
}

/// Punct-only-token signal exploration: every finding the shipped rule
/// produces, with the exact flagged chunk, how many times that same chunk is
/// flagged corpus-wide (pattern recurrence — the candidate convention signal),
/// and a little context for eyeballing.
pub(crate) fn punct_only_calib(dir: &Path) {
    use std::collections::HashMap;

    use ssc_core::signals::lexical::scan_punct_only_token;

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);

    // Pass 1: count every flagged chunk pattern corpus-wide.
    let mut pattern_count: HashMap<String, usize> = HashMap::new();
    let mut per_verse: Vec<(&str, &str, Vec<ssc_core::Span>)> = Vec::new();
    for (key, text) in target.keys().iter().zip(target.texts()) {
        let spans = scan_punct_only_token(text);
        if spans.is_empty() {
            continue;
        }
        for s in &spans {
            *pattern_count.entry(s.slice(text).to_string()).or_default() += 1;
        }
        per_verse.push((key.as_str(), text.as_str(), spans));
    }
    let total: usize = pattern_count.values().sum();

    // Pass 2: emit per-finding rows.
    println!("corpus\tsid\tchunk\tchunk_count\ttotal_findings\tverses\tcontext");
    for (key, text, spans) in &per_verse {
        for s in spans {
            let chunk = s.slice(text);
            let ctx_start = text[..s.start as usize]
                .char_indices()
                .rev()
                .nth(19)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let ctx: String = text[ctx_start..]
                .chars()
                .take(20 + chunk.chars().count() + 20)
                .collect::<String>()
                .replace(['\t', '\n'], " ");
            println!(
                "{corpus}\t{key}\t{chunk}\t{}\t{total}\t{}\t{ctx}",
                pattern_count[chunk],
                target.len(),
            );
        }
    }
    let mut top: Vec<_> = pattern_count.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    let head: Vec<String> = top
        .iter()
        .take(6)
        .map(|(p, n)| format!("[{p}]x{n}"))
        .collect();
    eprintln!(
        "{corpus}: {} verses, {total} candidates, {} distinct patterns | {}",
        target.len(),
        pattern_count.len(),
        head.join(" ")
    );

    // Production score distribution at floor 0, and the shipped-floor count.
    let findings = punct_only_findings(
        &target,
        &PunctOnlyTokenConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    );
    report_scored("lex.punct-only-token", &target, &findings);
    let shipped = PunctOnlyTokenConfig::default().emit_score_min;
    let surfaced = findings
        .iter()
        .filter(|f| f.score.unwrap_or(0.0) >= shipped)
        .count();
    eprintln!("{corpus}: surfaced at shipped floor {shipped}: {surfaced}");
}

/// Bracket-balance calibration (ADR 0037) at floor 0. Reports the production
/// score distribution, per-family tallies (which delimiter families the corpus
/// uses, how often each pairs, and how many orphans / long spans each yields),
/// and a sample of orphan findings with their full `DelimObservation`
/// inventories rendered readably — the audit view ADR 0037 findings carry.
pub(crate) fn bracket_calib(dir: &Path) {
    use ssc_core::charclass::{bracket_close_of, bracket_open_of, class_of};

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);
    eprintln!("{corpus}: {} verses", target.len());

    // Floor-0 run of the production rule: every orphan and every long-span pair
    // surfaces, so the score distribution shows the sub-floor mass too.
    let t0 = std::time::Instant::now();
    let findings = bracket_findings(
        &target,
        &BracketBalanceConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    );
    eprintln!("bracket check: {:?}", t0.elapsed());
    report_scored("punct.bracket-balance", &target, &findings);

    // Per-family event tally over the whole corpus, using the same family
    // classification the rule uses (family key = the pair's open glyph).
    #[derive(Default)]
    struct Fam {
        open: char,
        close: char,
        opens: u64,
        closes: u64,
    }
    let mut fams: BTreeMap<char, Fam> = BTreeMap::new();
    for text in target.texts() {
        for c in text.chars() {
            if !class_of(c).is_punctuation() {
                continue;
            }
            let (family, is_open, open_glyph, close_glyph) =
                if let Some(close) = bracket_close_of(c) {
                    (c, true, c, close)
                } else if let Some(open) = bracket_open_of(c) {
                    (open, false, open, c)
                } else {
                    continue;
                };
            let e = fams.entry(family).or_default();
            e.open = open_glyph;
            e.close = close_glyph;
            if is_open {
                e.opens += 1;
            } else {
                e.closes += 1;
            }
        }
    }

    // Orphan / long-span counts per family, read off the floor-0 findings. The
    // finding's own slice is the anchor glyph (the orphan for Pairing, the
    // opener for ShortSpan); its family is that glyph or its opener.
    let mut orphans: BTreeMap<char, u64> = BTreeMap::new();
    let mut long_spans: BTreeMap<char, u64> = BTreeMap::new();
    for f in &findings {
        let text = target.text(f.key_idx);
        let glyph = f.range.slice(text).chars().next().unwrap();
        let family = bracket_close_of(glyph)
            .map(|_| glyph)
            .or_else(|| bracket_open_of(glyph))
            .unwrap_or(glyph);
        match &f.args {
            Some(FindingArgs::BracketWindow {
                measure: BracketMeasure::Pairing,
                ..
            }) => {
                *orphans.entry(family).or_default() += 1;
            }
            Some(FindingArgs::BracketWindow {
                measure: BracketMeasure::ShortSpan,
                ..
            }) => {
                *long_spans.entry(family).or_default() += 1;
            }
            _ => {}
        }
    }

    println!("\nper-family tally (family = open glyph; events = opens + closes):");
    println!(
        "  {:^9} {:>8} {:>7} {:>7} {:>9} {:>7} {:>9}",
        "pair", "events", "opens", "closes", "orphans", "long", "pair_rate"
    );
    let mut rows: Vec<&Fam> = fams.values().collect();
    rows.sort_by_key(|f| std::cmp::Reverse(f.opens + f.closes));
    for f in rows {
        let events = f.opens + f.closes;
        let orph = orphans.get(&f.open).copied().unwrap_or(0);
        let long = long_spans.get(&f.open).copied().unwrap_or(0);
        // Descriptive pairing rate == matched_events / events == (events −
        // orphan_events) / events (each orphan is one unmatched event).
        let rate = (events.saturating_sub(orph)) as f64 / events.max(1) as f64 * 100.0;
        println!(
            "  {}…{}  U+{:04X}  {:>8} {:>7} {:>7} {:>9} {:>7} {:>8.1}%",
            f.open, f.close, f.open as u32, events, f.opens, f.closes, orph, long, rate
        );
    }

    // ~20 sample orphan findings with their DelimObservation inventories, so
    // the family collisions (quote-role glyphs vs real brackets) are eyeballable.
    println!("\nsample findings (up to 20) with delimiter inventories:");
    let mut samples: Vec<&Finding> = findings.iter().collect();
    samples.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap()
    });
    for f in samples.iter().take(20) {
        let text = target.text(f.key_idx);
        let glyph = f.range.slice(text);
        let (measure, window, majority, total) = match &f.args {
            Some(FindingArgs::BracketWindow {
                measure,
                window,
                majority,
                total,
            }) => (*measure, window, *majority, *total),
            _ => continue,
        };
        let kind = match measure {
            BracketMeasure::Pairing => "orphan",
            BracketMeasure::ShortSpan => "long-span",
        };
        println!(
            "  {:<10} score={:.3} {kind} [{glyph}] {majority}/{total}",
            target.key(f.key_idx),
            f.score.unwrap_or(0.0),
        );
        // Render the inventory compactly: glyph + role + matched flag, grouped
        // so the reviewer sees what surrounds the orphan.
        let inv: String = window
            .iter()
            .map(|o| {
                let role = match o.role {
                    ssc_core::DelimRole::Open => 'o',
                    ssc_core::DelimRole::Close => 'c',
                };
                let mark = if o.matched { '=' } else { '!' };
                format!("{}{role}{mark}", o.glyph)
            })
            .collect::<Vec<_>>()
            .join(" ");
        let inv: String = inv.chars().take(160).collect();
        println!("      inv: {inv}");
    }
}

/// Punctuation adjacency calibration (ADR 0024) at floor 0.
pub(crate) fn punct_calib(dir: &Path) {
    let target = load_corpus(dir);
    eprintln!("{} verses", target.len());
    let cfg = PunctuationAdjacencyConfig {
        emit_score_min: 0.0,
        ..Default::default()
    };
    let t0 = std::time::Instant::now();
    let findings = adjacency_findings(&target, &cfg);
    eprintln!("punct map+reduce+judge: {:?}", t0.elapsed());
    report_scored("punct.adjacency-anomaly", &target, &findings);

    // How many the shipped default config surfaces (default-on rule).
    let shipped = analyze_with_config(&target, None, &Config::v1_defaults());
    let shipped_n = shipped
        .iter()
        .filter(|f| f.code == RuleId::PunctuationAdjacencyAnomaly)
        .count();
    println!("\nshipped default surfaces: {shipped_n}");
}

/// Punctuation-spacing knee/floor sweep + regression over the vref fleet
/// (ADR 0054). Drives the **production** `punct.spacing-anomaly` rule (the new
/// 16-cell signature model) under a grid of `(minority_recurrence_k,
/// minority_rate_per_10k)` at floor 0.5, reporting the fleet-wide finding total
/// and the six ADR 0050 calibration corpora at each cell. This is the
/// before/after regression counter and the ADR 0054 knee-sweep evidence: the
/// shipped `(32, 40)` cell is the one to compare against the old binary rule's
/// 3,928 fleet findings, and the six named corpora must keep their kept-sites.
pub(crate) fn spacing_fleet_sweep(dir: &Path) {
    use rayon::prelude::*;

    // ADR 0050 regression corpora (short id → file stem).
    const REG: &[&str] = &[
        "engwebster",
        "WA-kmr-IQ-badini-reg",
        "udu",
        "WA-ne-udb",
        "WA-pa-ulb",
        "mya",
    ];
    // (k, rate) grid; floor fixed at the shipped 0.5. The shipped cell is (32,40).
    const KS: &[f32] = &[16.0, 32.0, 64.0];
    const RATES: &[f32] = &[0.0, 20.0, 40.0, 80.0];

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total_corpora = files.len();
    eprintln!("spacing sweep fleet: {total_corpora} corpora");

    let count = |k: f32, rate: f32, map: &Corpus| -> usize {
        let cfg = PunctuationSpacingConfig {
            emit_score_min: 0.5,
            confidence_z: 1.96,
            minority_recurrence_k: k,
            minority_rate_per_10k: rate,
        };
        ssc_core::signals::punctuation::spacing_findings(map, &cfg).len()
    };

    // Per-corpus: for each (k, rate) cell, the finding count. Reduce fleet in
    // parallel, summing per cell.
    let per_corpus: Vec<(String, Vec<usize>)> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let mut cells = Vec::new();
            for &k in KS {
                for &rate in RATES {
                    cells.push(count(k, rate, &map));
                }
            }
            (id, cells)
        })
        .collect();

    let ncells = KS.len() * RATES.len();
    let mut totals = vec![0usize; ncells];
    let mut corpora_with = vec![0usize; ncells];
    for (_, cells) in &per_corpus {
        for (i, &n) in cells.iter().enumerate() {
            totals[i] += n;
            if n > 0 {
                corpora_with[i] += 1;
            }
        }
    }

    println!("=== punct.spacing-anomaly fleet knee/floor sweep (floor 0.5, z 1.96) ===");
    println!(
        "production per-side (left/right) rule; cells = total fleet findings (corpora with ≥1)"
    );
    print!("      {:>6}", "k\\rate");
    for &rate in RATES {
        print!("  {:>14}", format!("{rate:.0}/10k"));
    }
    println!();
    for (ki, &k) in KS.iter().enumerate() {
        print!("      {k:>6.0}");
        for ri in 0..RATES.len() {
            let i = ki * RATES.len() + ri;
            print!("  {:>14}", format!("{} ({})", totals[i], corpora_with[i]));
        }
        println!();
    }

    println!("\n-- six ADR 0050 regression corpora, findings per (k, rate) cell --");
    print!("  {:<24}", "corpus");
    for &k in KS {
        for &rate in RATES {
            print!("  {:>8}", format!("{k:.0}/{rate:.0}"));
        }
    }
    println!();
    for &id in REG {
        if let Some((_, cells)) = per_corpus.iter().find(|(cid, _)| cid == id) {
            print!("  {id:<24}");
            for &n in cells {
                print!("  {n:>8}");
            }
            println!();
        } else {
            println!("  {id:<24}  (absent)");
        }
    }
    let shipped_idx = KS.iter().position(|&k| k == 32.0).unwrap() * RATES.len()
        + RATES.iter().position(|&r| r == 40.0).unwrap();
    println!(
        "\nshipped cell (k=32, rate=40/10k, floor 0.5): {} fleet findings across {} corpora",
        totals[shipped_idx], corpora_with[shipped_idx]
    );
}

/// Shared score-distribution report for the corpus-relative rules: total
/// scored sites, how many clear a ladder of floors, and the top/bottom samples
/// with their exact slice and a little context.
fn report_scored(name: &str, target: &Corpus, findings: &[Finding]) {
    println!("\n{name}: {} scored sites (floor 0)", findings.len());
    for floor in [0.5_f32, 0.7, 0.9, 0.99] {
        let n = findings
            .iter()
            .filter(|f| f.score.unwrap_or(0.0) >= floor)
            .count();
        println!("  ≥ {floor:>4}: {n}");
    }
    // 10-bucket histogram of evidence scores — shows the sub-floor mass.
    let mut buckets = [0usize; 10];
    for f in findings {
        let s = f.score.unwrap_or(0.0).clamp(0.0, 0.999_999);
        buckets[(s * 10.0) as usize] += 1;
    }
    println!("  score histogram (each row = 0.1 wide):");
    for (i, &n) in buckets.iter().enumerate() {
        let lo = i as f32 / 10.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("    [{lo:.1},{:.1}) {n:>6} {bar}", lo + 0.1);
    }
    let mut by_score: Vec<&Finding> = findings.iter().collect();
    by_score.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap()
    });
    println!("  top 10 by score:");
    print_scored(target, by_score.iter().take(10).copied());
    println!("  bottom 5 by score:");
    print_scored(target, by_score.iter().rev().take(5).copied());
}

fn print_scored<'a>(target: &Corpus, findings: impl Iterator<Item = &'a Finding>) {
    for f in findings {
        let text = target.text(f.key_idx);
        let slice: String = f.range.slice(text).chars().take(16).collect();
        let ctx_start = text[..f.range.start as usize]
            .char_indices()
            .rev()
            .nth(14)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let ctx: String = text[ctx_start..].chars().take(44).collect();
        println!(
            "    {:<10} score={:.3} [{}] …{}",
            target.key(f.key_idx),
            f.score.unwrap_or(0.0),
            slice.replace('\u{200B}', "·"),
            ctx.replace('\u{200B}', "·")
        );
    }
}

