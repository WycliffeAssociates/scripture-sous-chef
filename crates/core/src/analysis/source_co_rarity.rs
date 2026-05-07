//! Source-relative co-rarity factor for rare-word triage.
//!
//! Per ADR 0003 / 0007 and §3.2 of the plan: when a project has a
//! source corpus loaded, ask "is this rare target token *explained*
//! by something on the source side?" Bibles especially have their
//! false-positive mass dominated by transliterated proper nouns
//! (`Davidi` ↔ `David`), place names, and theological loanwords.
//! Catching them here prevents the engine from drowning the user in
//! proper-noun candidates the very first time triage runs, before
//! any labelled-snowball loop has had a chance to learn.
//!
//! # Per-verse states (placeholders, see plan §8 #1)
//!
//! For each rare target token's verse occurrence we map the
//! corresponding source verse to one of three states:
//!
//! | Source verse state                                           | Suspicion factor |
//! | ------------------------------------------------------------ | ---------------- |
//! | Target uppercase-shaped + capitalized source token at BK ≤ 2 | 0.0              |
//! | Source verse has any rare source token (no BK match above)   | 0.3              |
//! | Source verse unremarkable (no rare tokens, no BK match)      | 0.7              |
//!
//! "Uppercase-shaped" means the surface form starts with an uppercase
//! character in the verse text — NOT the lexicon's
//! `CaseClass::IntrinsicUpper`, which requires ≥5 observations
//! (`intrinsic_min_obs`) to fire and would never classify a hapax as
//! a proper noun. Surface uppercase is what we actually want here:
//! `Davidi` is a hapax but it's still capitalized in the verse text,
//! and `David` is well-attested in source but still capitalized.
//!
//! Aggregation across multiple verse occurrences of the same target
//! form: **min** (most-exonerating wins). A single `Davidi` ↔ `David`
//! match in any verse is enough to exonerate the target form across
//! the project. Real proper nouns appear consistently; this is the
//! right policy.
//!
//! # Abstain semantics (ADR 0003)
//!
//! No source corpus loaded → factor is dropped from the per-token
//! Noisy-OR product entirely (returns 0.0, the Noisy-OR identity).
//! It does NOT return 0.7. Returning 0.7 unconditionally for every
//! token in non-source projects would floor every score at ≥0.7 once
//! Noisy-OR'd against the other factors.
//!
//! # Edit-distance match (ADR 0007 amendment)
//!
//! `strsim::damerau_levenshtein` against the per-source-verse set of
//! capitalized tokens. No BK-tree built; the per-verse candidate set
//! is too small for the BK-tree's sublinear-query advantage to
//! matter.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::analysis::lexicon::Lexicon;
use crate::project::{NamedCorpus, Project};
use crate::sid::Sid;
use crate::verse::TokenKind;

#[derive(Debug, Clone, Default)]
struct SourceVerseSnapshot {
    /// Lowercased source tokens that are rare per `rare_count_max`.
    rare_lower: BTreeSet<String>,
    /// Source tokens whose surface form starts with an uppercase
    /// character (BK-match candidates for proper-noun pairing). Stored
    /// lowercased because BK comparison is case-insensitive.
    capitalized_lower: BTreeSet<String>,
}

/// Compute per-form source co-rarity factors. Returns a map keyed by
/// lowercased target form. Forms not in the map are the caller's
/// responsibility to handle as "abstain" (factor 0.0) — that's the
/// case both for "no source loaded" and for "this form has no
/// verse-level evidence yet."
pub fn compute_factors_per_form(
    project: &Project<'_>,
    rare_target_forms: &BTreeSet<String>,
    rare_count_max: u32,
) -> HashMap<String, f64> {
    let Some(source) = project.source.as_ref() else {
        return HashMap::new();
    };
    if rare_target_forms.is_empty() {
        return HashMap::new();
    }

    let source_lexicon = Lexicon::build(
        &crate::discourse::Discourse::build(source),
        Default::default(),
    );
    let per_source_verse = snapshot_source_verses(source, &source_lexicon, rare_count_max);

    let mut per_form_min: HashMap<String, f64> = HashMap::new();
    for (sid, target_verse) in &project.target.verses {
        // Only verses that have a corresponding source verse contribute.
        // Target verses with no source pair leave the form's factor
        // untouched; if no other verse contributes, the form ends up
        // with no entry, which the caller treats as abstain.
        if !source.verses.contains_key(sid) {
            continue;
        }
        let source_snapshot = per_source_verse.get(sid).cloned().unwrap_or_default();
        for (_, token_text) in target_verse.tokens_of(TokenKind::Word) {
            let form_lower = lowercase_normalize(token_text);
            if !rare_target_forms.contains(&form_lower) {
                continue;
            }
            let target_uppercase_shaped = first_grapheme_is_uppercase(token_text);
            let factor = factor_for_verse(
                &form_lower,
                target_uppercase_shaped,
                &source_snapshot,
            );
            per_form_min
                .entry(form_lower)
                .and_modify(|v| {
                    if factor < *v {
                        *v = factor;
                    }
                })
                .or_insert(factor);
        }
    }
    per_form_min
}

fn factor_for_verse(
    target_form_lower: &str,
    target_uppercase_shaped: bool,
    source: &SourceVerseSnapshot,
) -> f64 {
    if target_uppercase_shaped {
        for source_capitalized in &source.capitalized_lower {
            if strsim::damerau_levenshtein(target_form_lower, source_capitalized) <= 2 {
                return 0.0;
            }
        }
    }
    if source.rare_lower.is_empty() {
        0.7
    } else {
        0.3
    }
}

fn snapshot_source_verses(
    source: &NamedCorpus<'_>,
    source_lexicon: &Lexicon,
    rare_count_max: u32,
) -> BTreeMap<Sid, SourceVerseSnapshot> {
    let mut out: BTreeMap<Sid, SourceVerseSnapshot> = BTreeMap::new();
    for (sid, verse) in &source.verses {
        let mut snapshot = SourceVerseSnapshot::default();
        for (_, token_text) in verse.tokens_of(TokenKind::Word) {
            let form_lower = lowercase_normalize(token_text);
            if form_lower.is_empty() {
                continue;
            }
            if first_grapheme_is_uppercase(token_text) {
                snapshot.capitalized_lower.insert(form_lower.clone());
            }
            let count = source_lexicon
                .words
                .get(&form_lower)
                .map(|p| p.n_total())
                .unwrap_or(0);
            if count > 0 && count <= rare_count_max {
                snapshot.rare_lower.insert(form_lower);
            }
        }
        if !snapshot.rare_lower.is_empty() || !snapshot.capitalized_lower.is_empty() {
            out.insert(*sid, snapshot);
        }
    }
    out
}

fn first_grapheme_is_uppercase(text: &str) -> bool {
    use unicode_segmentation::UnicodeSegmentation;
    text.graphemes(true)
        .next()
        .and_then(|g| g.chars().next())
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

fn lowercase_normalize(text: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    text.graphemes(true)
        .filter(|g| {
            g.chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        })
        .flat_map(|g| g.chars().flat_map(char::to_lowercase))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ExceptionSet};
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::marker::PhantomData;

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn corpus(name: &str, verses: Vec<(Sid, &str)>) -> NamedCorpus<'static> {
        let mut map: BTreeMap<Sid, _> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: name.to_string(),
            verses: map,
            _src: PhantomData,
        }
    }

    fn project_of(
        target: NamedCorpus<'static>,
        source: Option<NamedCorpus<'static>>,
    ) -> Project<'static> {
        Project {
            target,
            source,
            config: Config::default(),
            exceptions: ExceptionSet::default(),
            lemma_labels: Default::default(),
        }
    }

    /// Body verses use a fixed phrase so non-outlier tokens are
    /// well-attested in BOTH corpora. The outlier verse pair contains
    /// the rare target token on one side and the proper-noun match on
    /// the other.
    fn body(name: &str, count: u16, body_text: &str, outlier_sid: Sid, outlier_text: &str)
        -> NamedCorpus<'static>
    {
        let mut verses = Vec::new();
        for v in 1..=count {
            verses.push((sid("GEN", 1, v), body_text));
        }
        verses.push((outlier_sid, outlier_text));
        corpus(name, verses)
    }

    /// Surface-form uppercase target token + capitalized source token
    /// at BK distance 1 (`Davidi` ↔ `David`). Factor saturates to 0.0.
    #[test]
    fn proper_noun_bk_match_emits_zero() {
        let outlier = sid("GEN", 5, 1);
        let target = body("t", 200, "the lord said unto the people", outlier, "Davidi went forth in the morning");
        let source = body("s", 200, "the lord said unto the people", outlier, "David went forth in the morning");
        let project = project_of(target, Some(source));
        let mut rare = BTreeSet::new();
        rare.insert("davidi".to_string());
        let factors = compute_factors_per_form(&project, &rare, 2);
        assert_eq!(factors.get("davidi"), Some(&0.0));
    }

    /// Target rare token has no proper-noun shape (lowercase) in target
    /// verse text. Source verse has rare tokens but no BK match. Factor
    /// = 0.3 — co-rarity present, no proper-noun exoneration.
    #[test]
    fn co_rare_no_proper_noun_match_emits_three_tenths() {
        let outlier = sid("GEN", 5, 1);
        let target = body("t", 200, "the lord said unto the people", outlier, "qzxqzx walked at dawn");
        // Source outlier: "kabsheel" is rare and lowercase (no proper-
        // noun shape, no BK candidate); the rest are well-attested.
        let source = body("s", 200, "the lord said unto the people", outlier, "kabsheel the lord said unto");
        let project = project_of(target, Some(source));
        let mut rare = BTreeSet::new();
        rare.insert("qzxqzx".to_string());
        let factors = compute_factors_per_form(&project, &rare, 2);
        assert_eq!(factors.get("qzxqzx"), Some(&0.3));
    }

    /// Target rare; source verse is plain prose with no rare tokens.
    /// Factor = 0.7 — no exonerating evidence on the source side.
    #[test]
    fn unremarkable_source_verse_emits_seven_tenths() {
        let outlier = sid("GEN", 5, 1);
        let target = body("t", 200, "the lord said unto the people", outlier, "qzxqzx the lord said unto");
        let source = body("s", 200, "the lord said unto the people", outlier, "the lord said unto the people");
        let project = project_of(target, Some(source));
        let mut rare = BTreeSet::new();
        rare.insert("qzxqzx".to_string());
        let factors = compute_factors_per_form(&project, &rare, 2);
        assert_eq!(factors.get("qzxqzx"), Some(&0.7));
    }

    /// No source loaded → returns empty map. Caller treats absent
    /// entries as abstain (factor 0.0 / dropped from Noisy-OR).
    #[test]
    fn empty_when_no_source_loaded() {
        let target = corpus("t", vec![(sid("GEN", 1, 1), "Davidi went forth")]);
        let project = project_of(target, None);
        let mut rare = BTreeSet::new();
        rare.insert("davidi".to_string());
        let factors = compute_factors_per_form(&project, &rare, 2);
        assert!(factors.is_empty());
    }
}
