//! Content-derived analysis identity.
//!
//! Two opaque `u64` newtypes prove *what* an analysis result describes, so a
//! persisted packed-findings buffer can be validated against the current
//! inputs without re-running analysis (Phase A-W consumes these; this module
//! only defines them). Both are **content-derived and deterministic** — never
//! a counter or timestamp — so the same target/reference/config yields the
//! same id across sessions, instances, and edit-then-undo.
//!
//! - [`TargetContextId`] folds the target book hashes, the complete config,
//!   and the engine stamp, but **not** the reference. Its sole use is proving
//!   the unchanged half of the reference-present → reference-absent salvage.
//! - [`AnalysisId`] folds the target-context id, an explicit
//!   reference-present/absent tag, and the reference book hashes when present.
//!   It is the exact identity everywhere else.
//!
//! Both read the `Corpus`'s owned book hashes (`BookLayout`), so computing an
//! id is O(book count) and never walks verse text.

use xxhash_rust::xxh3::Xxh3;

use crate::config::Config;
use crate::corpus::Corpus;

/// Deterministic stamp of the analysis engine's *semantics*. It must change
/// whenever anything that can alter semantic findings, scores, args, order, or
/// rule interpretation changes — even when the wire layout is unchanged — so a
/// buffer from an older engine can never falsely match. It is **never** a
/// timestamp or random build id: bump it by hand on a semantic change.
///
/// By Phase F this folds the direct-lane schema stamp and every closed-registry
/// `RuleId` semantic/schema stamp (registry coverage tests pin the fold); until
/// then it is a single manually-maintained constant.
pub const ANALYSIS_ENGINE_STAMP: u64 = 1;

/// Opaque identity over the target corpus, the complete config, and the engine
/// stamp — excluding reference presence/content. Not a general weaker cache
/// key: its only public use is proving the unchanged half of the
/// reference-present → reference-absent persisted-findings salvage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetContextId(u64);

impl TargetContextId {
    /// The current `u64` wire representation (JS `bigint`).
    pub fn get(self) -> u64 {
        self.0
    }

    /// Fold the target's owned book hashes, the config fingerprint, and the
    /// engine stamp. Reads `BookLayout` hashes — no verse-text walk.
    pub fn compute(target: &Corpus, config: &Config) -> TargetContextId {
        TargetContextId(target_context_fold(target, config, ANALYSIS_ENGINE_STAMP))
    }
}

/// Opaque complete content identity. Folds the target-context id, an explicit
/// reference-present/absent tag, and (when present) the reference book hashes.
/// The presence tag stops "no reference" from aliasing an empty reference
/// corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalysisId(u64);

impl AnalysisId {
    /// The current `u64` wire representation (JS `bigint`).
    pub fn get(self) -> u64 {
        self.0
    }

    /// Fold the target-context id with the reference presence tag and reference
    /// book hashes. The stateless and resident paths compute the same id for
    /// the same target/reference/config input.
    pub fn compute(target: &Corpus, source: Option<&Corpus>, config: &Config) -> AnalysisId {
        let tcid = TargetContextId::compute(target, config);
        let mut h = Xxh3::new();
        h.update(b"ssc.analysis-id.v1");
        h.update(&tcid.0.to_le_bytes());
        match source {
            None => h.update(&[0u8]),
            Some(reference) => {
                h.update(&[1u8]);
                fold_book_leaves(&mut h, reference);
            }
        }
        AnalysisId(h.digest())
    }
}

/// The target-context fold, parameterized by the engine stamp so tests can
/// prove stamp sensitivity without mutating the const.
fn target_context_fold(target: &Corpus, config: &Config, engine_stamp: u64) -> u64 {
    let mut h = Xxh3::new();
    h.update(b"ssc.target-context.v1");
    h.update(&engine_stamp.to_le_bytes());
    h.update(&config_fingerprint(config).to_le_bytes());
    fold_book_leaves(&mut h, target);
    h.digest()
}

/// Fold a corpus's ordered `(slug, book content hash)` leaves, count-prefixed
/// and per-leaf length-prefixed so no book boundary or slug can bleed into the
/// next. Reads the owned `BookLayout` — no verse-text walk.
fn fold_book_leaves(h: &mut Xxh3, corpus: &Corpus) {
    let layout = corpus.book_layout();
    h.update(&(layout.len() as u32).to_le_bytes());
    for book in layout {
        h.update(&(book.slug.len() as u32).to_le_bytes());
        h.update(book.slug.as_bytes());
        h.update(&book.hash.to_le_bytes());
    }
}

/// Deterministic fingerprint of the complete config (rules + every knob) via
/// its `Debug` form. `Config`'s ordered `BTreeMap` rule set and fixed float
/// formatting make this stable run-to-run.
fn config_fingerprint(config: &Config) -> u64 {
    xxhash_rust::xxh3::xxh3_64(format!("{config:?}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuleId;

    fn corpus(pairs: &[(&str, &str)]) -> Corpus {
        Corpus::try_from_parts(
            pairs.iter().map(|(k, _)| k.to_string()).collect(),
            pairs.iter().map(|(_, t)| t.to_string()).collect(),
        )
        .unwrap()
    }

    fn target() -> Corpus {
        corpus(&[("GEN 1:1", "in the beginning"), ("GEN 1:2", "and the earth")])
    }

    fn source() -> Corpus {
        corpus(&[("GEN 1:1", "beginning"), ("GEN 1:2", "earth")])
    }

    /// Deterministic: identical inputs → identical ids, across instances.
    #[test]
    fn ids_are_deterministic_across_instances() {
        let cfg = Config::v1_defaults();
        let a = AnalysisId::compute(&target(), Some(&source()), &cfg);
        let b = AnalysisId::compute(&target(), Some(&source()), &cfg);
        assert_eq!(a, b);
        assert_eq!(
            TargetContextId::compute(&target(), &cfg),
            TargetContextId::compute(&target(), &cfg)
        );
    }

    /// A semantic no-op (a distinct-but-equal target value) recurs the id —
    /// the property edit-then-undo relies on.
    #[test]
    fn equal_but_distinct_inputs_recur_the_id() {
        let cfg = Config::v1_defaults();
        let first = AnalysisId::compute(&target(), None, &cfg);
        let rebuilt = AnalysisId::compute(&target(), None, &cfg);
        assert_eq!(first, rebuilt);
    }

    /// `AnalysisId` changes for a changed target, reference, config, or engine
    /// stamp.
    #[test]
    fn analysis_id_is_sensitive_to_every_input() {
        let cfg = Config::v1_defaults();
        let base = AnalysisId::compute(&target(), Some(&source()), &cfg);

        let edited_target = corpus(&[("GEN 1:1", "changed"), ("GEN 1:2", "and the earth")]);
        assert_ne!(AnalysisId::compute(&edited_target, Some(&source()), &cfg), base);

        let edited_ref = corpus(&[("GEN 1:1", "beginning"), ("GEN 1:2", "changed")]);
        assert_ne!(AnalysisId::compute(&target(), Some(&edited_ref), &cfg), base);

        let mut cfg2 = Config::v1_defaults();
        cfg2.rules.insert(RuleId::DuplicateWord, true);
        assert_ne!(AnalysisId::compute(&target(), Some(&source()), &cfg2), base);
    }

    /// Reference presence is part of `AnalysisId`: absent ≠ present, even for
    /// the same target/config. The presence tag also stops an empty reference
    /// corpus from aliasing "no reference".
    #[test]
    fn analysis_id_distinguishes_reference_presence() {
        let cfg = Config::v1_defaults();
        let absent = AnalysisId::compute(&target(), None, &cfg);
        let present = AnalysisId::compute(&target(), Some(&source()), &cfg);
        assert_ne!(absent, present);

        let empty = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        let empty_ref = AnalysisId::compute(&target(), Some(&empty), &cfg);
        assert_ne!(empty_ref, absent, "empty reference is not 'no reference'");
    }

    /// `TargetContextId` is insensitive to the reference (presence or content)
    /// but sensitive to target, config, and engine stamp.
    #[test]
    fn target_context_id_ignores_reference_but_not_target_config_stamp() {
        let cfg = Config::v1_defaults();
        let base = TargetContextId::compute(&target(), &cfg);
        // A target-context id does not even take a reference argument, so
        // reference changes cannot affect it by construction; the AnalysisId
        // test above proves reference *does* move the full id.

        let edited_target = corpus(&[("GEN 1:1", "changed"), ("GEN 1:2", "and the earth")]);
        assert_ne!(TargetContextId::compute(&edited_target, &cfg), base);

        let mut cfg2 = Config::v1_defaults();
        cfg2.rules.insert(RuleId::DuplicateWord, true);
        assert_ne!(TargetContextId::compute(&target(), &cfg2), base);

        // Engine-stamp sensitivity, proven at the fold (the const cannot be
        // mutated in a test).
        let s1 = target_context_fold(&target(), &cfg, 1);
        let s2 = target_context_fold(&target(), &cfg, 2);
        assert_ne!(s1, s2, "a changed engine stamp changes the id");
    }
}
