// ═══════════════════════════════════════════════════════════════════════════
// THE MIGRATION LEDGER — dev-only. The gate that must pass before
// `punct.spacing-anomaly`, `punct.adjacency-anomaly` and `lex.punct-only-token`
// are deleted (epic plan §11.1/§13 Phase E, §14.3).
//
// The probe's ledger measured a MODEL. This one measures the SHIPPED RULE: it
// calls `nonletter_usage_findings` and `nonletter_candidate_runs` — the same
// public surfaces the engine judges through — against the three retired rules at
// their shipped defaults, over the full 1,504-corpus fleet.
//
// Every old finding is classified into exactly one disposition:
//
//   preserved            the new rule emits at an overlapping span
//   coalesced            the new rule emits, but over the whole maximal run
//                        (the old span differed by the whitespace it included)
//   intentionally-moved  the new rule OBSERVES a candidate there and declines
//   lost                 the new rule observes no candidate at all
//
// `lost` is the only disposition that can block deletion, and it is why this
// tool reads `nonletter_candidate_runs` rather than a judged run set: a run every
// channel abstains on emits nothing at any floor while still being fully
// observed, so "emits nothing" and "sees nothing" are different answers and only
// the second is a coverage loss.
//
// It also discharges the two obligations attached to the sequence-`k=2` ruling
// (progress log Entry 9):
//
//   (a) the adjacency findings that moved SPECIFICALLY because k=2 admits only an
//       unseen pairing — i.e. those the same rule at k=8 would have emitted —
//       sampled with context so a reader can confirm they are conventions rather
//       than systematic errors;
//   (b) the corpora ADR 0024 and ADR 0054 name as adjudicated multilingual wins,
//       reported per corpus so each win is visibly preserved or visibly accepted
//       drift.
// ═══════════════════════════════════════════════════════════════════════════

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use ssc_core::config::{
    NonletterUsageConfig, PunctOnlyTokenConfig, PunctuationAdjacencyConfig,
    PunctuationSpacingConfig,
};
use ssc_core::{Corpus, Finding, RuleId, Span};

use crate::vref_io::load_corpus;

/// The corpora ADR 0024 and ADR 0054 name by hand as adjudicated multilingual
/// wins — the obligation (b) roster. Each row is `(corpus id, what the ADR
/// adjudicated)`.
const ADJUDICATED: [(&str, &str); 5] = [
    (
        "WA-ne-udb",
        "ADR 0054: `,`/`!` anchors kept AND the 40 verse-final dandas kept at ~0.549",
    ),
    (
        "engwebster",
        "ADR 0054: spaced period-typography collapses; the genuine spaced-`!` slips kept",
    ),
    (
        "WA-kmr-IQ-badini-reg",
        "ADR 0054: the 1,289 spaced ` \u{060C}` convention collapses; slips kept",
    ),
    (
        "WA-pa-ulb",
        "ADR 0054: the spaced `? !` convention collapses; slips kept",
    ),
    (
        "ayn_reg",
        "ADR 0024: the moderate-frequency Arabic `\u{06D4}\u{06D4}` convention stays suppressed",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Disposition {
    Preserved,
    Coalesced,
    Moved,
    Lost,
}

impl Disposition {
    fn label(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Coalesced => "coalesced",
            Self::Moved => "intentionally-moved",
            Self::Lost => "lost",
        }
    }
}

/// The three retired rules' findings at shipped defaults, tagged by rule.
fn old_findings(corpus: &Corpus) -> Vec<Finding> {
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

fn overlaps(a: Span, b: Span) -> bool {
    a.start < b.end && b.start < a.end
}

/// One corpus's ledger row plus its samples.
struct CorpusLedger {
    id: String,
    /// `[rule index][disposition]` counts, rule index in `OLD_RULES` order.
    counts: [[u64; 4]; 3],
    new_findings: usize,
    /// Old adjacency findings the same rule at `sequence_k = 8` WOULD have
    /// emitted — the population obligation (a) is about.
    k2_moved: u64,
    /// Up to three sampled `k=2` movers, rendered with context.
    k2_samples: Vec<String>,
    /// Present only for a corpus on the obligation (b) roster.
    adjudicated: Option<AdjudicatedRow>,
}

struct AdjudicatedRow {
    note: &'static str,
    old_total: u64,
    preserved: u64,
    coalesced: u64,
    moved: u64,
    lost: u64,
    samples: Vec<String>,
}

const OLD_RULES: [(&str, RuleId); 3] = [
    (
        "punct.adjacency-anomaly",
        RuleId::PunctuationAdjacencyAnomaly,
    ),
    ("lex.punct-only-token", RuleId::PunctOnlyToken),
    ("punct.spacing-anomaly", RuleId::PunctuationSpacingAnomaly),
];

fn rule_index(code: RuleId) -> usize {
    OLD_RULES
        .iter()
        .position(|&(_, r)| r == code)
        .expect("only the three retired rules enter the ledger")
}

/// A short window of the verse around a span, escaped so a TSV row stays one row.
fn context(text: &str, span: Span) -> String {
    let b = span.start as usize;
    let lo = text[..b.min(text.len())]
        .char_indices()
        .rev()
        .nth(26)
        .map_or(0, |(i, _)| i);
    let hi = text[b.min(text.len())..]
        .char_indices()
        .nth(28)
        .map_or(text.len(), |(i, _)| b + i);
    let mut out = String::new();
    for c in text[lo..hi].chars() {
        match c {
            '\t' | '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{{{:04X}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    format!("\u{2026}{out}\u{2026}")
}

fn ledger_for(id: &str, corpus: &Corpus, tuned: NonletterUsageConfig) -> CorpusLedger {
    let default = tuned;
    let new = ssc_core::signals::nonletter_usage::nonletter_usage_findings(corpus, &default);
    // Every span the rule OBSERVES, judgment aside. This — not a judged run set —
    // is what makes `lost` a real coverage answer.
    let observed = ssc_core::signals::nonletter_usage::nonletter_candidate_runs(corpus);

    // The obligation (a) counterfactual: the same rule with the sequence knee at
    // the packet's original k = 8, so pairings seen 2..7 still count as unseen.
    let at_k8 = ssc_core::signals::nonletter_usage::nonletter_usage_findings(
        corpus,
        &NonletterUsageConfig {
            sequence_k: 8.0,
            ..default
        },
    );

    // Index the three span sets by verse, so classification is a small local scan
    // rather than a cross product over the corpus.
    let mut by_verse_new: BTreeMap<u32, Vec<Span>> = BTreeMap::new();
    for f in &new {
        by_verse_new
            .entry(f.key_idx.get())
            .or_default()
            .push(f.range);
    }
    let mut by_verse_obs: BTreeMap<u32, Vec<Span>> = BTreeMap::new();
    for (k, s) in &observed {
        by_verse_obs.entry(k.get()).or_default().push(*s);
    }
    let mut by_verse_k8: BTreeMap<u32, Vec<Span>> = BTreeMap::new();
    for f in &at_k8 {
        by_verse_k8
            .entry(f.key_idx.get())
            .or_default()
            .push(f.range);
    }

    let mut counts = [[0u64; 4]; 3];
    let mut k2_moved = 0u64;
    let mut k2_samples: Vec<String> = Vec::new();
    let mut adj_samples: Vec<String> = Vec::new();
    let mut adj = ADJUDICATED
        .iter()
        .find(|&&(c, _)| c == id)
        .map(|&(_, note)| AdjudicatedRow {
            note,
            old_total: 0,
            preserved: 0,
            coalesced: 0,
            moved: 0,
            lost: 0,
            samples: Vec::new(),
        });

    for f in old_findings(corpus) {
        let v = f.key_idx.get();
        let empty: Vec<Span> = Vec::new();
        let news = by_verse_new.get(&v).unwrap_or(&empty);
        let obs = by_verse_obs.get(&v).unwrap_or(&empty);
        let hit = news.iter().copied().find(|s| overlaps(*s, f.range));
        let disposition = match hit {
            Some(s) if s == f.range => Disposition::Preserved,
            Some(_) => Disposition::Coalesced,
            None if obs.iter().any(|s| overlaps(*s, f.range)) => Disposition::Moved,
            None => Disposition::Lost,
        };
        counts[rule_index(f.code)][disposition as usize] += 1;

        // Obligation (a): an adjacency finding the new rule declines at k = 2 but
        // WOULD have emitted at k = 8 moved specifically because the pairing was
        // already seen 2..7 times — i.e. because this translation already writes
        // it, which is the definition of a convention.
        if f.code == RuleId::PunctuationAdjacencyAnomaly
            && disposition == Disposition::Moved
            && by_verse_k8
                .get(&v)
                .is_some_and(|ss| ss.iter().any(|s| overlaps(*s, f.range)))
        {
            k2_moved += 1;
            if k2_samples.len() < 3 {
                let text = corpus.text(f.key_idx);
                k2_samples.push(format!(
                    "{}\t{}\t{}",
                    corpus.key(f.key_idx),
                    f.range.slice(text),
                    context(text, f.range)
                ));
            }
        }

        if let Some(row) = adj.as_mut() {
            row.old_total += 1;
            match disposition {
                Disposition::Preserved => row.preserved += 1,
                Disposition::Coalesced => row.coalesced += 1,
                Disposition::Moved => row.moved += 1,
                Disposition::Lost => row.lost += 1,
            }
            if adj_samples.len() < 6 {
                let text = corpus.text(f.key_idx);
                adj_samples.push(format!(
                    "{}\t{}\t{}\t{}\t{}",
                    f.code.code(),
                    disposition.label(),
                    corpus.key(f.key_idx),
                    f.range.slice(text),
                    context(text, f.range)
                ));
            }
        }
    }
    if let Some(row) = adj.as_mut() {
        row.samples = adj_samples;
    }

    CorpusLedger {
        id: id.to_string(),
        counts,
        new_findings: new.len(),
        k2_moved,
        k2_samples,
        adjudicated: adj,
    }
}

/// Run the ledger over a whole vref directory and print the durable TSV to
/// stdout. Rayon-parallel per corpus; each corpus is independent.
pub(crate) fn nonletter_ledger_fleet(dir: &Path, tuned: NonletterUsageConfig) {
    use rayon::prelude::*;
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = files.len();
    let rows: Vec<CorpusLedger> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let out = ledger_for(&id, &load_corpus(path), tuned);
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("{n}/{total}");
            }
            out
        })
        .collect();

    println!("# nonletter-usage migration ledger — the SHIPPED rule vs the three retired rules");
    println!("# corpora={total}");
    println!(
        "# knobs: floor={} sequence_k={} placement_k={} placement_min_pool={}",
        tuned.emit_score_min, tuned.sequence_k, tuned.placement_k, tuned.placement_min_pool
    );

    // ── Fleet totals, per retired rule and overall.
    let mut fleet = [[0u64; 4]; 3];
    let mut new_total = 0usize;
    let mut k2_total = 0u64;
    for r in &rows {
        for (i, per_rule) in r.counts.iter().enumerate() {
            for (d, n) in per_rule.iter().enumerate() {
                fleet[i][d] += n;
            }
        }
        new_total += r.new_findings;
        k2_total += r.k2_moved;
    }
    println!();
    println!("## fleet ledger");
    println!("retired_rule\ttotal\tpreserved\tcoalesced\tintentionally-moved\tlost");
    let mut grand = [0u64; 4];
    for (i, (name, _)) in OLD_RULES.iter().enumerate() {
        let t: u64 = fleet[i].iter().sum();
        for (d, n) in fleet[i].iter().enumerate() {
            grand[d] += n;
        }
        println!(
            "{name}\t{t}\t{}\t{}\t{}\t{}",
            fleet[i][0], fleet[i][1], fleet[i][2], fleet[i][3]
        );
    }
    let grand_total: u64 = grand.iter().sum();
    println!(
        "ALL\t{grand_total}\t{}\t{}\t{}\t{}",
        grand[0], grand[1], grand[2], grand[3]
    );
    println!();
    println!("new_rule_findings_at_defaults\t{new_total}");
    println!(
        "lost_share\t{:.6}",
        if grand_total == 0 {
            0.0
        } else {
            grand[3] as f64 / grand_total as f64
        }
    );

    // ── Obligation (a).
    println!();
    println!("## obligation (a) — adjacency findings that moved because sequence k = 2");
    println!("# an old adjacency finding the rule declines at k=2 but WOULD emit at k=8:");
    println!("# the pairing was already seen 2..7 times in this translation.");
    println!("k2_specific_movers\t{k2_total}");
    println!(
        "share_of_adjacency_moved\t{:.6}",
        if fleet[0][2] == 0 {
            0.0
        } else {
            k2_total as f64 / fleet[0][2] as f64
        }
    );
    println!("corpus\tsid\tpattern\tcontext");
    let mut sampled: Vec<&CorpusLedger> = rows.iter().filter(|r| r.k2_moved > 0).collect();
    sampled.sort_by(|a, b| b.k2_moved.cmp(&a.k2_moved).then(a.id.cmp(&b.id)));
    for r in sampled.iter().take(40) {
        for s in &r.k2_samples {
            println!("{}\t{s}", r.id);
        }
    }

    // ── Obligation (b).
    println!();
    println!("## obligation (b) — the ADR 0024 / ADR 0054 adjudicated multilingual wins");
    println!("corpus\ttotal\tpreserved\tcoalesced\tintentionally-moved\tlost\tadjudication");
    for (id, _) in ADJUDICATED {
        match rows.iter().find(|r| r.id == id) {
            Some(r) => {
                let a = r.adjudicated.as_ref().expect("roster corpus carries a row");
                println!(
                    "{id}\t{}\t{}\t{}\t{}\t{}\t{}",
                    a.old_total, a.preserved, a.coalesced, a.moved, a.lost, a.note
                );
            }
            None => println!("{id}\tMISSING FROM THE FLEET\t-\t-\t-\t-"),
        }
    }
    println!();
    println!("rule\tdisposition\tcorpus\tsid\tspan\tcontext");
    for (id, _) in ADJUDICATED {
        if let Some(r) = rows.iter().find(|r| r.id == id)
            && let Some(a) = r.adjudicated.as_ref()
        {
            for s in &a.samples {
                let mut it = s.splitn(3, '\t');
                let rule = it.next().unwrap_or("");
                let disp = it.next().unwrap_or("");
                println!("{rule}\t{disp}\t{id}\t{}", it.next().unwrap_or(""));
            }
        }
    }

    // ── Per-corpus rows, the durable body.
    println!();
    println!("## per corpus");
    println!(
        "corpus\tnew_findings\tadj_total\tadj_preserved\tadj_coalesced\tadj_moved\tadj_lost\t\
         punctonly_total\tpunctonly_preserved\tpunctonly_coalesced\tpunctonly_moved\t\
         punctonly_lost\tspacing_total\tspacing_preserved\tspacing_coalesced\tspacing_moved\t\
         spacing_lost\tk2_movers"
    );
    let mut sorted: Vec<&CorpusLedger> = rows.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for r in sorted {
        print!("{}\t{}", r.id, r.new_findings);
        for per_rule in &r.counts {
            let t: u64 = per_rule.iter().sum();
            print!(
                "\t{t}\t{}\t{}\t{}\t{}",
                per_rule[0], per_rule[1], per_rule[2], per_rule[3]
            );
        }
        println!("\t{}", r.k2_moved);
    }
}
