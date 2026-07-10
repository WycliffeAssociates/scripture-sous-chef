//! `cargo test -p ssc-core` coverage for the terminal_strength SPIKE dev code
//! (shortlist 2/3). The ported `association` fixtures (Dunning G² + Fisher,
//! textbook values) live in `dev/association.rs` under `#[cfg(test)]` and run
//! here; the terminal witnesses are exercised on a hand-built VerseMap only
//! (synthetic tests, never corpus fixtures — MEMORY).

#[path = "../dev/association.rs"]
mod association;

#[path = "../dev/terminal.rs"]
mod terminal;

use ssc_core::{Sid, VerseMap};

fn sid(book: &str, v: u16) -> Sid {
    Sid::parse(&format!("{book} 1:{v}")).unwrap()
}

/// Cycle `templates`, one verse each, `reps` cycles — a synthetic corpus.
fn cycle(book: &str, templates: &[&str], reps: u16) -> VerseMap {
    let mut out = VerseMap::new();
    let mut v = 1u16;
    for _ in 0..reps {
        for t in templates {
            out.insert(sid(book, v), (*t).to_string());
            v += 1;
        }
    }
    out
}

/// W1: a mark that reliably precedes a capitalized lexicon-lowercase word earns
/// high case-trust; a list separator between the same words does not.
#[test]
fn case_witness_separates_terminal_from_separator() {
    // Each verse ends with '.', so the next verse's `The` is forced-upper across
    // the seam; `the` recurs mid-flow ("saw the gate") ⇒ lexicon-lowercase. The
    // list verse gives ',' a genuinely different aftermath (item words that
    // never follow a period) so the reshuffle guard can bite.
    let vm = cycle(
        "GEN",
        &[
            "The men saw the gate.",
            "The priest gave wood, stone, iron, gold, bronze, silver.",
        ],
        120,
    );
    let c = terminal::analyze_corpus("syn".into(), &vm, true);
    let dot = c
        .trust
        .classes
        .get(&terminal::ClassKey { mark: '.', quoted: false })
        .expect("'.' class present");
    assert!(dot.s_case > 0.9, "'.' case-trust {} should be high", dot.s_case);
    // The genealogy guard: ',' trust stays low under variant B (the case
    // witness sees the lowercase follower; the reshuffle guard doesn't rescue).
    if let Some(cm) = c.trust.classes.get(&terminal::ClassKey { mark: ',', quoted: false }) {
        assert!(cm.trust_b < 0.5, "',' trust_B {} should be low", cm.trust_b);
        assert!(dot.trust_b > cm.trust_b, "'.' outranks ',' on trust");
    }
}

/// A caseless corpus produces no case witness, and the walk stays silent
/// (no bicameral evidence) — trust rests entirely on the reshuffle witness.
#[test]
fn caseless_corpus_has_no_case_witness() {
    let mut vm = VerseMap::new();
    for v in 1..=40u16 {
        vm.insert(sid("GEN", v), "उसने कहा। वे चले गए। फिर वह चला गया।".to_string());
    }
    let c = terminal::analyze_corpus("syn".into(), &vm, true);
    assert!(!c.bicameral, "Devanagari corpus is caseless");
    for t in c.trust.classes.values() {
        assert!(!t.s_case_seen, "no case witness in a caseless corpus");
    }
}

/// The baseline scenario reproduces ADR 0051: a capitalized word written once
/// lowercase mid-flow surfaces as an intrinsic finding, and trust wiring does
/// not resurrect anything the floor already suppressed here.
#[test]
fn baseline_reproduces_an_intrinsic_finding() {
    let mut vm = cycle("GEN", &["we saw Jesus"], 300);
    vm.insert(sid("GEN", 500), "we saw jesus".to_string());
    let c = terminal::analyze_corpus("syn".into(), &vm, true);
    assert_eq!(c.base_i, 1, "one intrinsic anomaly at baseline");
    // No terminal habit exists (no '.'), so trust changes no verdict.
    assert_eq!(c.tr_i, c.base_i, "trust wiring leaves the intrinsic count unchanged");
}
