//! `ssc-core` — public engine contract.
//!
//! Locked types only. Bodies are deliberately `todo!()` until we commit to
//! implementation. The shape here is what every signal in METHODS.md §3 will
//! consume; if a rule cannot be expressed against these types, the API is
//! wrong and should be revised before we build behind it.

pub mod analysis;
pub mod config;
pub mod diagnostics;
pub mod discourse;
pub mod profile;
pub mod project;
pub mod rule;
pub mod script;
pub mod sid;
pub mod signals;
pub mod unicode;
pub mod verse;

pub use config::{Config, ExceptionSet};
pub use diagnostics::{Diagnostics, Finding, RuleId, Severity};
pub use project::{NamedCorpus, Project};
pub use sid::{BookId, Sid};
pub use verse::{Token, TokenKind, Verse};

/// Run all enabled rules against `project` and return the accumulated
/// diagnostics. Borrows from each verse's precomputed views, so the
/// result is bounded by `'src` (the lifetime of the ingested verse
/// text).
///
/// Walks `rule::default_rules()`. For a custom rule set (testing,
/// embedding), call `analyze_with` instead.
///
/// Currently sequential. Parallel rule dispatch is forward-compatible
/// via the `Rule: Sync` bound — see `rule.rs` for the three-layer
/// parallelism note.
pub fn analyze<'src>(project: &'src Project<'src>) -> Diagnostics<'src> {
    analyze_with(project, &rule::default_rules())
}

/// Like `analyze`, but with a caller-supplied rule registry.
pub fn analyze_with<'src>(
    project: &'src Project<'src>,
    rules: &[Box<dyn rule::Rule>],
) -> Diagnostics<'src> {
    let mut diags = Diagnostics::default();

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

    for r in rules {
        let id = r.id();
        if enabled.get(&id) == Some(&false) {
            continue;
        }
        for mut f in r.check(project) {
            if project.exceptions.contains(f.rule_id, f.sid) {
                continue;
            }
            if let Some(&sev) = severity_override.get(&f.rule_id) {
                f.severity = sev;
            }
            diags.push(f);
        }
    }
    diags
}
