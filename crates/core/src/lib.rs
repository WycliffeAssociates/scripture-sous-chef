//! `scc-core` — public engine contract.
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
pub mod verse;

pub use config::{Config, ExceptionSet};
pub use diagnostics::{Diagnostics, Finding, RuleId, Severity};
pub use project::{NamedCorpus, Project};
pub use sid::{BookId, Sid};
pub use verse::{Token, TokenKind, Verse};

/// Run all enabled signals against `project` and return the accumulated
/// diagnostics. Borrows from each verse's precomputed views, so the result
/// is bounded by `'src` (the lifetime of the ingested verse text).
///
/// v0 implementation: a flat loop over verses calling each signal as a
/// free function, with an `ExceptionSet` filter. Becomes a real
/// pipeline once the `Rule` trait lands (see `crate::rule`) and once
/// score combination (γ in `rule.rs`) is wired.
pub fn analyze<'src>(project: &'src Project<'src>) -> Diagnostics<'src> {
    let mut diags = Diagnostics::default();
    for verse in project.target.verses.values() {
        for f in signals::hygiene::tab_in_body(verse) {
            if !project.exceptions.contains(f.rule_id, f.sid) {
                diags.push(f);
            }
        }
    }
    diags
}
