//! `ssc-core` — public engine contract.

pub mod aggregate;
pub mod analysis;
pub mod config;
pub mod context;
pub mod diagnostics;
pub mod discourse;
pub mod profile;
pub mod project;
pub mod punctuation_class;
pub mod rule;
pub mod script;
pub mod sid;
pub mod signals;
pub mod unicode;
pub mod verse;

pub use config::{Config, ExceptionSet};
pub use context::AnalysisContext;
pub use diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Diagnostics, Finding, FindingId, Lane, RuleId, Severity,
};
pub use project::{NamedCorpus, Project};
pub use sid::{BookId, Sid};
pub use verse::{Token, TokenKind, Verse};

/// Run all enabled rules against `project` and return diagnostics.
/// Stats are discarded — call `analyze_with_stats` to keep them.
pub fn analyze<'src>(project: &'src Project<'src>) -> Diagnostics<'src> {
    run(project, &rule::default_rules()).0
}

/// Like `analyze`, but also returns per-rule debug statistics.
pub fn analyze_with_stats<'src>(project: &'src Project<'src>) -> (Diagnostics<'src>, AnalyzeStats) {
    run(project, &rule::default_rules())
}

fn run<'src>(
    project: &'src Project<'src>,
    rules: &[Box<dyn rule::Rule>],
) -> (Diagnostics<'src>, AnalyzeStats) {
    let mut diags = Diagnostics::default();
    let mut stats = AnalyzeStats::default();
    let context = AnalysisContext::build(project);
    stats.bootstrap = Some(context.bootstrap_stats.clone());

    let enabled: std::collections::HashMap<RuleId, bool> = project
        .config
        .rules
        .iter()
        .map(|r| (r.id, r.enabled))
        .collect();
    let severity_override: std::collections::HashMap<RuleId, Severity> = project
        .config
        .rules
        .iter()
        .filter_map(|r| r.severity.map(|s| (r.id, s)))
        .collect();

    let mut raw = Diagnostics::default();
    for r in rules {
        let id = r.id();
        if enabled.get(&id) == Some(&false) {
            continue;
        }
        for mut f in r.check(project, &context, &mut stats) {
            if let Some(&sev) = severity_override.get(&f.rule_id) {
                f.severity = sev;
            }
            raw.push(f);
        }
    }
    raw.assign_finding_ids();
    for f in raw.findings {
        if project.exceptions.contains(&f) {
            continue;
        }
        diags.push(f);
    }
    stats.lexicon = Some(context.lexicon.stats());
    (diags, stats)
}
