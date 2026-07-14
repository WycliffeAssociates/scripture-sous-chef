//! `cargo test -p ssc-core` coverage for the terminal_strength SPIKE sweep
//! harness (shortlist 2/3). The G²/Fisher fixtures now live with the graduated
//! `analysis::association` module (ADR 0052); here the terminal witnesses are
//! exercised on a hand-built Corpus only (synthetic tests, never corpus
//! fixtures — MEMORY).

#[path = "../dev/terminal.rs"]
mod terminal;

use ssc_core::Corpus;

/// Cycle `templates`, one verse each, `reps` cycles — a synthetic corpus.
/// `Corpus` requires contiguous, in-order construction, so build the
/// `keys`/`texts` vectors directly in verse order rather than inserting into
/// a map.
fn cycle(book: &str, templates: &[&str], reps: u16) -> Corpus {
    let mut keys = Vec::new();
    let mut texts = Vec::new();
    let mut v = 1u16;
    for _ in 0..reps {
        for t in templates {
            keys.push(format!("{book} 1:{v}"));
            texts.push((*t).to_string());
            v += 1;
        }
    }
    Corpus::try_from_parts(keys, texts).unwrap()
}

/// W1: a mark that reliably precedes a capitalized lexicon-lowercase word earns
/// high case-trust; a list separator between the same words does not.
#[test]
fn case_witness_separates_terminal_from_separator() {
    // Each verse ends with '.', so the next verse's `The` is forced-upper across
    // the seam; `the` recurs mid-flow ("saw the gate") ⇒ lexicon-lowercase. The
    // list verse gives ',' a genuinely different aftermath (item words that
    // never follow a period) so the reshuffle guard can bite.
    let c = cycle(
        "GEN",
        &[
            "The men saw the gate.",
            "The priest gave wood, stone, iron, gold, bronze, silver.",
        ],
        120,
    );
    let c = terminal::analyze_corpus("syn".into(), &c, true);
    let dot = c
        .trust
        .classes
        .get(&terminal::ClassKey {
            mark: '.',
            quoted: false,
        })
        .expect("'.' class present");
    assert!(
        dot.s_case > 0.9,
        "'.' case-trust {} should be high",
        dot.s_case
    );
    // The genealogy guard: ',' trust stays low under variant B (the case
    // witness sees the lowercase follower; the reshuffle guard doesn't rescue).
    if let Some(cm) = c.trust.classes.get(&terminal::ClassKey {
        mark: ',',
        quoted: false,
    }) {
        assert!(cm.trust_b < 0.5, "',' trust_B {} should be low", cm.trust_b);
        assert!(dot.trust_b > cm.trust_b, "'.' outranks ',' on trust");
    }
}

/// A caseless corpus produces no case witness, and the walk stays silent
/// (no bicameral evidence) — trust rests entirely on the reshuffle witness.
#[test]
fn caseless_corpus_has_no_case_witness() {
    let keys: Vec<String> = (1..=40u16).map(|v| format!("GEN 1:{v}")).collect();
    let texts: Vec<String> = (1..=40u16)
        .map(|_| "उसने कहा। वे चले गए। फिर वह चला गया।".to_string())
        .collect();
    let c = Corpus::try_from_parts(keys, texts).unwrap();
    let c = terminal::analyze_corpus("syn".into(), &c, true);
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
    // 300 clean verses, then one extra verse (verse 301, the corpus's actual
    // next verse) carrying the anomaly — same intent as the old sparse
    // `VerseMap` insert at an arbitrary unused verse number 500, just appended
    // contiguously as `Corpus` requires.
    let mut keys: Vec<String> = (1..=300u16).map(|v| format!("GEN 1:{v}")).collect();
    let mut texts: Vec<String> = (1..=300u16).map(|_| "we saw Jesus".to_string()).collect();
    keys.push("GEN 1:301".to_string());
    texts.push("we saw jesus".to_string());
    let c = Corpus::try_from_parts(keys, texts).unwrap();
    let c = terminal::analyze_corpus("syn".into(), &c, true);
    assert_eq!(c.base_i, 1, "one intrinsic anomaly at baseline");
    // No terminal habit exists (no '.'), so trust changes no verdict.
    assert_eq!(
        c.tr_i, c.base_i,
        "trust wiring leaves the intrinsic count unchanged"
    );
}
