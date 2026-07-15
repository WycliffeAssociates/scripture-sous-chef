//! `cargo xtask survey-diff <baseline-dir> [current-dir]` — compare two
//! playground survey caches and print what moved.
//!
//! The standing regression loop for rule work: snapshot `cache/survey/`
//! before a change (`cp -R`), rebuild it after (`cargo refresh-survey
//! --rebuild` in the playground), then diff. Reads each cache's
//! `index.json` for the per-rule totals and, for every rule whose numbers
//! moved, the per-corpus files for the biggest per-corpus deltas — the
//! "which corpora and how hard" answer that decides whether a change is a
//! storm, a fix, or noise. `current-dir` defaults to the playground cache at
//! `../sousChefPlayground/cache/survey`.

use std::collections::BTreeMap;
use std::path::Path;

/// `rule code -> (total, corpora)` from a survey `index.json`.
fn load_index(dir: &Path) -> BTreeMap<String, (u64, u64)> {
    let path = dir.join("index.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    v["rules"]
        .as_array()
        .expect("index.json has a `rules` array")
        .iter()
        .map(|r| {
            (
                r["code"].as_str().expect("rule code").to_string(),
                (
                    r["total"].as_u64().unwrap_or(0),
                    r["corpora"].as_u64().unwrap_or(0),
                ),
            )
        })
        .collect()
}

/// `corpus -> count` for one rule, from a cache's per-corpus files.
fn per_corpus(dir: &Path, rule: &str) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json")
            || path.file_name().is_some_and(|n| n == "index.json")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(corpus) = v["corpus"].as_str() else {
            continue;
        };
        for rc in v["rule_counts"].as_array().into_iter().flatten() {
            if rc[0].as_str() == Some(rule)
                && let Some(n) = rc[1].as_u64()
            {
                out.insert(corpus.to_string(), n);
            }
        }
    }
    out
}

pub fn run(baseline: &Path, current: &Path) {
    let base = load_index(baseline);
    let cur = load_index(current);

    let mut codes: Vec<&String> = base.keys().chain(cur.keys()).collect();
    codes.sort();
    codes.dedup();

    println!(
        "{:<36} {:>9} {:>5}   {:>9} {:>5}   {:>8}",
        "rule", "baseline", "corp", "current", "corp", "delta"
    );
    let mut changed: Vec<&String> = Vec::new();
    let (mut tb, mut tc) = (0u64, 0u64);
    for code in codes {
        let (b, bc) = base.get(code).copied().unwrap_or((0, 0));
        let (c, cc) = cur.get(code).copied().unwrap_or((0, 0));
        tb += b;
        tc += c;
        let delta = c as i64 - b as i64;
        let marker = if delta != 0 || bc != cc { " *" } else { "" };
        println!("{code:<36} {b:>9} {bc:>5}   {c:>9} {cc:>5}   {delta:>+8}{marker}");
        if delta != 0 || bc != cc {
            changed.push(code);
        }
    }
    println!(
        "{:<36} {tb:>9}         {tc:>9}         {:>+8}",
        "TOTAL",
        tc as i64 - tb as i64
    );

    // Per-corpus breakdown for what moved: the biggest shifts first, so a
    // one-corpus storm is distinguishable from a broad drift at a glance.
    for code in changed {
        let b = per_corpus(baseline, code);
        let c = per_corpus(current, code);
        let mut deltas: Vec<(i64, &String)> = b
            .keys()
            .chain(c.keys())
            .map(|k| {
                (
                    c.get(k).copied().unwrap_or(0) as i64 - b.get(k).copied().unwrap_or(0) as i64,
                    k,
                )
            })
            .filter(|(d, _)| *d != 0)
            .collect();
        deltas.sort_by_key(|(d, k)| (-d.abs(), (*k).clone()));
        deltas.dedup();
        if deltas.is_empty() {
            continue;
        }
        println!("\n{code} — corpora that moved (largest first):");
        for (d, corpus) in deltas.iter().take(12) {
            let bn = b.get(*corpus).copied().unwrap_or(0);
            let cn = c.get(*corpus).copied().unwrap_or(0);
            println!("  {corpus:<32} {bn:>7} -> {cn:<7} ({d:+})");
        }
        if deltas.len() > 12 {
            println!("  … and {} more corpora", deltas.len() - 12);
        }
    }
}
