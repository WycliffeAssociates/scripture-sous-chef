//! Lemma-family feedback: append-only JSONL events that label
//! candidate families as confirmed, rejected, or partially wrong.
//!
//! Lives in the same `<corpus>/.sous/events.jsonl` file as the
//! finding-level feedback in [`crate::analysis::posterior`]. The two
//! readers each parse only the kinds they understand and silently skip
//! the rest, so both event families coexist on disk without
//! coordination.
//!
//! ## Why these three kinds
//!
//! - `lemma_family_confirm` — the named forms belong together AND are
//!   real words. Surface forms in this set are added to a project-local
//!   "known good" pool: rare-word triage stops asking about them; rules
//!   that fire on rare types drop them.
//! - `lemma_family_reject` — the named forms are not real words (typos,
//!   transliteration noise, OCR garbage). Surface forms here go to a
//!   "known bad" pool: rules that surface suspected typos *elevate*
//!   them to actual findings on the next run.
//! - `lemma_member_split` — the named form does not belong to the
//!   named family. Used when a generator over-grouped (`John` /
//!   `Joan`) but the rest of the family is fine. Form is removed from
//!   that specific family without rejecting the whole family.
//!
//! Event records are content-addressed by `family_id` (FNV-1a of the
//! sorted member set, computed by
//! [`crate::analysis::candidate_families::compute_family_id`]). That
//! lets the same family proposed by multiple generators dedupe to a
//! single label.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "serde")]
use std::fs;
#[cfg(feature = "serde")]
use std::io::{self, BufRead};
#[cfg(feature = "serde")]
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LemmaFeedbackKind {
    LemmaFamilyConfirm,
    LemmaFamilyReject,
    LemmaMemberSplit,
}

/// One labelled family event. Wire format matches the finding-level
/// events: `kind`, `ts`, version, plus the family-specific payload.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LemmaFeedbackEvent {
    #[cfg_attr(feature = "serde", serde(default = "default_version"))]
    pub v: u8,
    pub ts: String,
    pub kind: LemmaFeedbackKind,
    /// Stable hash of the sorted member set; matches
    /// [`crate::analysis::candidate_families::CandidateFamily::family_id`].
    pub family_id: u64,
    /// Members, lowercased surface forms. For `LemmaMemberSplit` the
    /// vector contains exactly one form (the one being removed).
    pub forms: Vec<String>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub reason: Option<String>,
}

#[cfg(feature = "serde")]
fn default_version() -> u8 {
    1
}

/// Authoritative, label-driven view of the project's lemma families.
/// Built by replaying the `events.jsonl` log; falls back to empty when
/// the log is missing or has no family events yet.
///
/// Downstream rules consult this index — *not* any individual
/// candidate generator — so the user's labels override generator
/// proposals where they disagree.
#[derive(Debug, Clone, Default)]
pub struct LabelledLemmaIndex {
    /// Surface forms confirmed as real words (any family confirm
    /// touches them).
    pub known_good: BTreeSet<String>,
    /// Surface forms confirmed as not-real-words (any family reject
    /// touches them).
    pub known_bad: BTreeSet<String>,
    /// Confirmed families: `family_id → ordered member forms`. Multiple
    /// confirm events for the same `family_id` merge their member sets.
    pub confirmed_families: BTreeMap<u64, BTreeSet<String>>,
    /// Per-family member-splits. If `(family_id, form)` is present, that
    /// form does NOT belong to that family even if a generator
    /// proposes it.
    pub member_splits: BTreeSet<(u64, String)>,
}

impl LabelledLemmaIndex {
    #[cfg(feature = "serde")]
    pub fn from_event_log(path: &Path) -> io::Result<Self> {
        let mut index = Self::default();
        if !path.exists() {
            return Ok(index);
        }
        let file = fs::File::open(path)?;
        for line in io::BufReader::new(file).lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Forward-compat: silently skip events that don't parse as
            // a lemma-family event. Finding-level events live in the
            // same file and `posterior::PosteriorStore` reads those.
            let Ok(event) = serde_json::from_str::<LemmaFeedbackEvent>(trimmed) else {
                continue;
            };
            index.apply(&event);
        }
        Ok(index)
    }

    pub fn apply(&mut self, event: &LemmaFeedbackEvent) {
        match event.kind {
            LemmaFeedbackKind::LemmaFamilyConfirm => {
                let entry = self
                    .confirmed_families
                    .entry(event.family_id)
                    .or_default();
                for form in &event.forms {
                    entry.insert(form.clone());
                    self.known_good.insert(form.clone());
                    // A confirm overrides a previous reject.
                    self.known_bad.remove(form);
                }
            }
            LemmaFeedbackKind::LemmaFamilyReject => {
                for form in &event.forms {
                    self.known_bad.insert(form.clone());
                    // A reject overrides a previous confirm.
                    self.known_good.remove(form);
                }
                // The family itself is no longer confirmed; drop any
                // confirmed-members under this id.
                self.confirmed_families.remove(&event.family_id);
            }
            LemmaFeedbackKind::LemmaMemberSplit => {
                for form in &event.forms {
                    self.member_splits
                        .insert((event.family_id, form.clone()));
                    if let Some(family) = self.confirmed_families.get_mut(&event.family_id) {
                        family.remove(form);
                    }
                }
            }
        }
    }

    #[cfg(feature = "serde")]
    pub fn append_event(path: &Path, event: &LemmaFeedbackEvent) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        use std::io::Write;
        serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
        file.write_all(b"\n")
    }

    /// Returns true if this form was ever rejected (and not later
    /// re-confirmed).
    pub fn is_known_bad(&self, form: &str) -> bool {
        self.known_bad.contains(form)
    }

    /// Returns true if this form was ever confirmed (and not later
    /// rejected).
    pub fn is_known_good(&self, form: &str) -> bool {
        self.known_good.contains(form)
    }

    /// Returns true if `form` was explicitly removed from the named
    /// family.
    pub fn is_member_split(&self, family_id: u64, form: &str) -> bool {
        self.member_splits.contains(&(family_id, form.to_string()))
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    fn ev(kind: LemmaFeedbackKind, family_id: u64, forms: &[&str]) -> LemmaFeedbackEvent {
        LemmaFeedbackEvent {
            v: 1,
            ts: "2026-05-05T00:00:00Z".to_string(),
            kind,
            family_id,
            forms: forms.iter().map(|s| s.to_string()).collect(),
            reason: None,
        }
    }

    #[test]
    fn confirm_then_reject_inverts_classification() {
        let mut idx = LabelledLemmaIndex::default();
        idx.apply(&ev(LemmaFeedbackKind::LemmaFamilyConfirm, 1, &["walk", "walks"]));
        assert!(idx.is_known_good("walk"));
        assert!(!idx.is_known_bad("walk"));

        idx.apply(&ev(LemmaFeedbackKind::LemmaFamilyReject, 1, &["walk"]));
        assert!(idx.is_known_bad("walk"));
        assert!(!idx.is_known_good("walk"));
    }

    #[test]
    fn member_split_removes_from_confirmed_family() {
        let mut idx = LabelledLemmaIndex::default();
        idx.apply(&ev(
            LemmaFeedbackKind::LemmaFamilyConfirm,
            42,
            &["john", "joan", "johnny"],
        ));
        assert!(idx.confirmed_families[&42].contains("joan"));

        idx.apply(&ev(LemmaFeedbackKind::LemmaMemberSplit, 42, &["joan"]));
        assert!(idx.is_member_split(42, "joan"));
        assert!(!idx.confirmed_families[&42].contains("joan"));
        // joan was previously confirmed-good via the family confirm;
        // member_split alone doesn't remove from known_good (the form
        // is still a real word, just doesn't belong to this family).
        assert!(idx.is_known_good("joan"));
    }

    #[test]
    fn jsonl_roundtrip_preserves_event_replay() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ssc-lemma-feedback-{}-{}.jsonl",
            std::process::id(),
            "roundtrip"
        ));
        let _ = std::fs::remove_file(&path);

        LabelledLemmaIndex::append_event(
            &path,
            &ev(LemmaFeedbackKind::LemmaFamilyConfirm, 7, &["pray", "prays"]),
        )
        .unwrap();
        LabelledLemmaIndex::append_event(
            &path,
            &ev(LemmaFeedbackKind::LemmaFamilyReject, 8, &["xyzqq"]),
        )
        .unwrap();

        let idx = LabelledLemmaIndex::from_event_log(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(idx.is_known_good("pray"));
        assert!(idx.is_known_good("prays"));
        assert!(idx.is_known_bad("xyzqq"));
        assert!(idx.confirmed_families.contains_key(&7));
    }

    #[test]
    fn unknown_event_kinds_are_skipped() {
        // Mixed file: a finding-level event and a family-level event.
        // The family-level reader should ignore the finding event and
        // still produce a valid index.
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ssc-lemma-feedback-{}-{}.jsonl",
            std::process::id(),
            "mixed"
        ));
        let _ = std::fs::remove_file(&path);

        use std::io::Write;
        {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            // Finding-level event with a kind the lemma reader doesn't
            // understand.
            writeln!(
                file,
                "{}",
                r#"{"v":1,"ts":"2026-05-05T00:00:00Z","kind":"dismissed","finding_id":42,"rule_id":"hyg.tab-in-body","cluster_key":"x","sid":"GEN 1:1","source":"explicit","weight":1.0}"#,
            )
            .unwrap();
        }
        LabelledLemmaIndex::append_event(
            &path,
            &ev(LemmaFeedbackKind::LemmaFamilyConfirm, 99, &["walk"]),
        )
        .unwrap();

        let idx = LabelledLemmaIndex::from_event_log(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(idx.is_known_good("walk"));
    }
}
