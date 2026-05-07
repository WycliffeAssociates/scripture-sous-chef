//! Candidate families: cheap proposals of "these forms might be the
//! same word."
//!
//! A *family* is a set of surface forms that some generator thinks
//! belong together. None of the generators is authoritative — the
//! user's labels (in `<corpus>/.sous/events.jsonl`) are. This module
//! produces proposals for the triage UI to show; it does not decide.
//!
//! ## Why multiple generators
//!
//! Each generator has different failure modes, so we run several and
//! deduplicate the proposals.
//!
//! - **Surface identity** — every form is in a family of size one.
//!   Free. Lets a form be triaged on its own when no other generator
//!   has anything to say.
//! - **BK-distance** — Damerau–Levenshtein neighbours within a small
//!   radius. Catches typos and minor spelling variants. Wrong on close
//!   look-alikes (`John` / `Joan`); right on `walk` / `walks`.
//! - **Prefix overlap** — the existing `LemmaClusters` 4-char prefix
//!   heuristic. Cheap, often right on analytic / fusional corpora, often
//!   wrong on agglutinative ones (Turkish `gözl-` over-merges).
//! - **Morphological segmentation** *(Track 1, not in this module yet)*
//!   — drops in alongside the others. Different bias again.
//!
//! Each generator labels its proposals; consumers can filter or weight
//! per-generator. The `family_id` is a stable hash of the sorted member
//! set, so the same set proposed by two generators dedupes to one
//! record (with both generator tags attached).

use std::collections::BTreeMap;

use crate::analysis::lemma_cluster::{LemmaClusters, LemmaForm};
use crate::analysis::lexicon::Lexicon;
use crate::analysis::morphology::{SegmentedCorpus, SegmenterKind};

/// Default radius for the BK-distance proposer. DL ≤ 2 covers most
/// single-typo and short-paradigm relationships.
pub const DEFAULT_BK_RADIUS: u32 = 2;

/// Cap on how many forms a single BK-distance family can list. Beyond
/// this the proposal is unwieldy in a triage UI; the user should see a
/// representative subset, sorted by frequency.
pub const DEFAULT_FAMILY_SIZE_LIMIT: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct CandidateFamiliesConfig {
    pub bk_radius: u32,
    pub family_size_limit: usize,
}

impl Default for CandidateFamiliesConfig {
    fn default() -> Self {
        Self {
            bk_radius: DEFAULT_BK_RADIUS,
            family_size_limit: DEFAULT_FAMILY_SIZE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GeneratorKind {
    /// Trivial family-of-one: the form itself.
    SurfaceIdentity,
    /// Forms within Damerau–Levenshtein `radius` of the seed form.
    BkDistance { radius: u32 },
    /// Forms sharing a leading-character prefix (existing
    /// `LemmaClusters` 4-char heuristic).
    PrefixOverlap,
    /// Forms sharing the same morphological stem (per
    /// `analysis::morphology::SegmentedCorpus`).
    SegmenterStem,
}

/// One proposal: a set of surface forms that *some generator* thinks
/// belong together. Stable across runs as long as the member set is
/// stable; the `family_id` is a content hash.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CandidateFamily {
    /// Content-addressed identifier; FNV-1a of the sorted member
    /// surface forms. Two generators that propose the same member set
    /// produce the same `family_id` and dedupe to one record.
    pub family_id: u64,
    /// Which generator(s) proposed this family. When two generators
    /// propose the same set, both tags appear.
    pub proposed_by: Vec<GeneratorKind>,
    /// Member surface forms with their corpus counts, sorted by count
    /// descending so the highest-frequency form leads.
    pub forms: Vec<LemmaForm>,
    /// Representative form for UI display — the highest-frequency
    /// member of `forms`.
    pub representative: String,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CandidateFamilies {
    /// Distinct families, keyed by `family_id`.
    pub families: BTreeMap<u64, CandidateFamily>,
    /// Reverse lookup: each form → all `family_id`s that include it.
    pub by_form: BTreeMap<String, Vec<u64>>,
}

impl CandidateFamilies {
    /// Run every cheap generator over the corpus and collect proposals
    /// for `seed_forms` (typically the rare-word triage queue).
    ///
    /// The lexicon supplies global counts; `clusters` supplies the
    /// existing prefix-overlap families; BK-distance is computed on
    /// the fly against `lexicon.words.keys()`.
    /// Build the candidate-family map without consulting morphology.
    /// Equivalent to `build_with_morphology(.., None, ..)`.
    pub fn build(
        lexicon: &Lexicon,
        clusters: &LemmaClusters,
        seed_forms: &[String],
        config: CandidateFamiliesConfig,
    ) -> Self {
        Self::build_with_morphology(lexicon, clusters, None, seed_forms, config)
    }

    /// Like [`build`], but adds the segmenter as a fourth proposer
    /// (when supplied and not `Disabled`). Each seed form's stem
    /// (per `SegmentedCorpus::stem_for`) anchors a family of all
    /// forms sharing that stem in the project.
    pub fn build_with_morphology(
        lexicon: &Lexicon,
        clusters: &LemmaClusters,
        morphology: Option<&SegmentedCorpus>,
        seed_forms: &[String],
        config: CandidateFamiliesConfig,
    ) -> Self {
        let mut families: BTreeMap<u64, CandidateFamily> = BTreeMap::new();
        let mut by_form: BTreeMap<String, Vec<u64>> = BTreeMap::new();

        // If morphology is supplied and active, build a stem → forms
        // reverse index up front so each seed's stem-family proposal
        // is an O(1) lookup. Forms are paired with their corpus counts
        // for sorting later.
        let stem_to_forms: BTreeMap<String, Vec<(String, u32)>> = match morphology {
            Some(m) if m.stats.segmenter != SegmenterKind::Disabled => {
                let mut idx: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
                for (form, _morphs) in &m.by_form {
                    if let Some(stem) = m.stem_for(form) {
                        let count = lexicon
                            .words
                            .get(form)
                            .map(|profile| profile.n_total())
                            .unwrap_or(0);
                        idx.entry(stem.to_string())
                            .or_default()
                            .push((form.clone(), count));
                    }
                }
                idx
            }
            _ => BTreeMap::new(),
        };

        // Length-bucket every form once so BK-distance only scans the
        // few buckets within `bk_radius` of the seed length, instead of
        // every form in the corpus. On agglutinative corpora (~80k
        // types) this turns each seed lookup from "scan 80k" into
        // "scan ~5 buckets averaging ~10k each, with the same length-
        // diff prune still applying inside".
        let mut forms_by_len: BTreeMap<usize, Vec<(String, u32)>> = BTreeMap::new();
        for (form, profile) in &lexicon.words {
            forms_by_len
                .entry(form.len())
                .or_default()
                .push((form.clone(), profile.n_total()));
        }

        // Pre-snapshot lexicon entries by frequency, so we can sort
        // proposed members by count without re-querying the lexicon
        // for every family.
        let count_of = |form: &str| -> u32 {
            lexicon
                .words
                .get(form)
                .map(|profile| profile.n_total())
                .unwrap_or(0)
        };

        for seed in seed_forms {
            // Surface identity: family of one. Always proposed so the
            // triage UI has something to show even when nothing else
            // matched. Cheap to dedupe later.
            insert_proposal(
                &mut families,
                &mut by_form,
                vec![LemmaForm {
                    form: seed.clone(),
                    count: count_of(seed),
                }],
                GeneratorKind::SurfaceIdentity,
            );

            // BK-distance: scan only forms whose length is within
            // `bk_radius` of the seed (using the pre-built bucket map).
            let neighbours = bk_neighbours_bucketed(seed, &forms_by_len, config.bk_radius);
            if !neighbours.is_empty() {
                let mut members: Vec<LemmaForm> = std::iter::once(LemmaForm {
                    form: seed.clone(),
                    count: count_of(seed),
                })
                .chain(neighbours.iter().map(|n| LemmaForm {
                    form: n.form.clone(),
                    count: n.count,
                }))
                .collect();
                members.sort_by(|a, b| b.count.cmp(&a.count).then(a.form.cmp(&b.form)));
                members.truncate(config.family_size_limit);
                insert_proposal(
                    &mut families,
                    &mut by_form,
                    members,
                    GeneratorKind::BkDistance {
                        radius: config.bk_radius,
                    },
                );
            }

            // Segmenter stem: forms sharing the same morphological
            // stem form a family. Skips when morphology is disabled or
            // the seed has no stem assignment.
            if let Some(m) = morphology {
                if let Some(stem) = m.stem_for(seed) {
                    if let Some(siblings) = stem_to_forms.get(stem) {
                        let mut members: Vec<LemmaForm> = siblings
                            .iter()
                            .map(|(f, c)| LemmaForm {
                                form: f.clone(),
                                count: *c,
                            })
                            .collect();
                        if members.len() >= 2 {
                            members.sort_by(|a, b| {
                                b.count.cmp(&a.count).then(a.form.cmp(&b.form))
                            });
                            members.truncate(config.family_size_limit);
                            insert_proposal(
                                &mut families,
                                &mut by_form,
                                members,
                                GeneratorKind::SegmenterStem,
                            );
                        }
                    }
                }
            }

            // Prefix overlap: hand off to the existing LemmaClusters.
            if let Some(family) = clusters.family_for_form(seed) {
                let mut members: Vec<LemmaForm> = family
                    .forms
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                members.sort_by(|a, b| b.count.cmp(&a.count).then(a.form.cmp(&b.form)));
                members.truncate(config.family_size_limit);
                insert_proposal(
                    &mut families,
                    &mut by_form,
                    members,
                    GeneratorKind::PrefixOverlap,
                );
            }
        }

        Self { families, by_form }
    }

    /// Distinct families that contain `form`, in insertion order.
    pub fn families_for(&self, form: &str) -> Vec<&CandidateFamily> {
        self.by_form
            .get(form)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.families.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct BkNeighbour {
    form: String,
    count: u32,
}

fn bk_neighbours_bucketed(
    seed: &str,
    forms_by_len: &BTreeMap<usize, Vec<(String, u32)>>,
    radius: u32,
) -> Vec<BkNeighbour> {
    let radius_usize = radius as usize;
    let seed_len = seed.len();
    let lo = seed_len.saturating_sub(radius_usize);
    let hi = seed_len.saturating_add(radius_usize);
    let mut out: Vec<BkNeighbour> = Vec::new();
    for (_len, bucket) in forms_by_len.range(lo..=hi) {
        for (form, count) in bucket {
            if form == seed {
                continue;
            }
            let d = strsim::damerau_levenshtein(seed, form);
            if d == 0 || d > radius_usize {
                continue;
            }
            out.push(BkNeighbour {
                form: form.clone(),
                count: *count,
            });
        }
    }
    out
}

fn insert_proposal(
    families: &mut BTreeMap<u64, CandidateFamily>,
    by_form: &mut BTreeMap<String, Vec<u64>>,
    members: Vec<LemmaForm>,
    generator: GeneratorKind,
) {
    let id = compute_family_id(&members);
    let existing = families.entry(id).or_insert_with(|| {
        let representative = members
            .iter()
            .max_by_key(|f| f.count)
            .map(|f| f.form.clone())
            .unwrap_or_default();
        CandidateFamily {
            family_id: id,
            proposed_by: Vec::new(),
            forms: members.clone(),
            representative,
        }
    });
    if !existing.proposed_by.contains(&generator) {
        existing.proposed_by.push(generator);
    }
    for member in &members {
        let entry = by_form.entry(member.form.clone()).or_default();
        if !entry.contains(&id) {
            entry.push(id);
        }
    }
}

/// FNV-1a over the sorted member surface forms. The order
/// independence is what makes "same set proposed twice" dedupe
/// cleanly; the FNV constants match `diagnostics::finding_id_for`.
pub fn compute_family_id(members: &[LemmaForm]) -> u64 {
    let mut sorted: Vec<&str> = members.iter().map(|m| m.form.as_str()).collect();
    sorted.sort();
    sorted.dedup();
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;
    let mut h = FNV_OFFSET;
    for form in sorted {
        for byte in form.as_bytes() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Member separator so {"ab", "c"} and {"a", "bc"} hash distinctly.
        h ^= 0xff;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::lemma_cluster::{LemmaClusterConfig, LemmaClusters};
    use crate::analysis::lexicon::{Lexicon, LexiconConfig};
    use crate::discourse::Discourse;
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    fn sid(v: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, v)
    }

    fn corpus<S: Into<String>>(verses: Vec<(Sid, S)>) -> NamedCorpus<'static> {
        let mut map: BTreeMap<Sid, _> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.into()));
        }
        NamedCorpus {
            name: "t".into(),
            verses: map,
            _src: PhantomData,
        }
    }

    #[test]
    fn family_id_order_independent() {
        let a = vec![
            LemmaForm {
                form: "walk".into(),
                count: 5,
            },
            LemmaForm {
                form: "walks".into(),
                count: 3,
            },
        ];
        let b = vec![
            LemmaForm {
                form: "walks".into(),
                count: 99,
            },
            LemmaForm {
                form: "walk".into(),
                count: 1,
            },
        ];
        // Counts don't affect identity; only the set of forms.
        assert_eq!(compute_family_id(&a), compute_family_id(&b));
    }

    #[test]
    fn family_id_changes_with_member_set() {
        let a = vec![
            LemmaForm {
                form: "walk".into(),
                count: 5,
            },
            LemmaForm {
                form: "walks".into(),
                count: 3,
            },
        ];
        let b = vec![
            LemmaForm {
                form: "walk".into(),
                count: 5,
            },
            LemmaForm {
                form: "walked".into(),
                count: 3,
            },
        ];
        assert_ne!(compute_family_id(&a), compute_family_id(&b));
    }

    #[test]
    fn family_id_distinguishes_concatenations() {
        // {"ab", "c"} should not collide with {"a", "bc"}, even though
        // the bytes concat the same. Verifies the per-member separator.
        let a = vec![
            LemmaForm {
                form: "ab".into(),
                count: 1,
            },
            LemmaForm {
                form: "c".into(),
                count: 1,
            },
        ];
        let b = vec![
            LemmaForm {
                form: "a".into(),
                count: 1,
            },
            LemmaForm {
                form: "bc".into(),
                count: 1,
            },
        ];
        assert_ne!(compute_family_id(&a), compute_family_id(&b));
    }

    #[test]
    fn surface_identity_proposed_for_every_seed() {
        let c = corpus(vec![
            (sid(1), "alpha beta gamma"),
            (sid(2), "alpha alpha"),
            (sid(3), "beta beta beta"),
        ]);
        let d = Discourse::build(&c);
        let lex = Lexicon::build(&d, LexiconConfig::default());
        let clusters = LemmaClusters::build(&c, LemmaClusterConfig::default());
        let seeds = vec!["gamma".to_string()];
        let cf = CandidateFamilies::build(&lex, &clusters, &seeds, Default::default());
        // Surface-identity family of one for `gamma`.
        let f = cf.families_for("gamma");
        assert!(!f.is_empty(), "expected at least the surface-identity family");
        assert!(
            f.iter()
                .any(|fam| fam.proposed_by.contains(&GeneratorKind::SurfaceIdentity)),
        );
    }

    #[test]
    fn bk_distance_proposes_for_typo_and_paradigm() {
        // Build a corpus where `markket` is a hapax close to `market`,
        // and `walked`/`walks` are paradigm members close to `walk`.
        let c = corpus(vec![
            (sid(1), "the market sells fish"),
            (sid(2), "they walk to the market"),
            (sid(3), "she walks slowly"),
            (sid(4), "he walked home"),
            (sid(5), "behold the markket"),
            (sid(6), "market market market"),
            (sid(7), "walk walk walk"),
        ]);
        let d = Discourse::build(&c);
        let lex = Lexicon::build(&d, LexiconConfig::default());
        let clusters = LemmaClusters::build(&c, LemmaClusterConfig::default());
        let seeds = vec!["markket".to_string()];
        let cf = CandidateFamilies::build(&lex, &clusters, &seeds, Default::default());

        let families = cf.families_for("markket");
        // Should have at least surface-identity + a BK-distance family
        // that includes `market`.
        let bk = families.iter().find(|fam| {
            fam.proposed_by
                .iter()
                .any(|g| matches!(g, GeneratorKind::BkDistance { .. }))
        });
        let bk = bk.expect("BK-distance family should be proposed");
        assert!(
            bk.forms.iter().any(|f| f.form == "market"),
            "BK family should include the high-frequency neighbour `market`",
        );
        assert_eq!(bk.representative, "market");
    }

    #[test]
    fn segmenter_stem_proposes_family_when_morphology_is_enabled() {
        use crate::analysis::morphology::{
            MorphemePosition, MorphemeToken, SegmentationStats, SegmentedCorpus, SegmenterKind,
        };

        let c = corpus(vec![
            (sid(1), "walk walks walked"),
            (sid(2), "walking walks"),
            (sid(3), "talk talked"),
        ]);
        let d = Discourse::build(&c);
        let lex = Lexicon::build(&d, LexiconConfig::default());
        let clusters = LemmaClusters::build(&c, LemmaClusterConfig::default());

        // Synthesise a segmentation where walk*-forms share stem
        // `walk` and talk*-forms share stem `talk`.
        let mut by_form: std::collections::BTreeMap<String, Vec<MorphemeToken>> =
            std::collections::BTreeMap::new();
        for (form, stem, suffix) in [
            ("walk", "walk", None),
            ("walks", "walk", Some("s")),
            ("walked", "walk", Some("ed")),
            ("walking", "walk", Some("ing")),
            ("talk", "talk", None),
            ("talked", "talk", Some("ed")),
        ] {
            let mut tokens = vec![MorphemeToken {
                morpheme: stem.to_string(),
                position: MorphemePosition::Stem,
            }];
            if let Some(suf) = suffix {
                tokens.push(MorphemeToken {
                    morpheme: suf.to_string(),
                    position: MorphemePosition::Suffix,
                });
            }
            by_form.insert(form.to_string(), tokens);
        }
        let morph = SegmentedCorpus {
            by_form,
            stats: SegmentationStats {
                segmenter: SegmenterKind::Morfessor20,
                ..Default::default()
            },
        };
        let cf = CandidateFamilies::build_with_morphology(
            &lex,
            &clusters,
            Some(&morph),
            &["walks".to_string()],
            Default::default(),
        );
        let stem_family = cf
            .families_for("walks")
            .into_iter()
            .find(|f| f.proposed_by.contains(&GeneratorKind::SegmenterStem))
            .expect("stem family should be proposed");
        let names: Vec<&str> = stem_family.forms.iter().map(|f| f.form.as_str()).collect();
        assert!(names.contains(&"walk"));
        assert!(names.contains(&"walks"));
        assert!(names.contains(&"walked"));
        assert!(names.contains(&"walking"));
        // talk* forms should NOT join because they have a different
        // stem.
        assert!(!names.contains(&"talk"));
    }

    #[test]
    fn dedupes_when_two_generators_propose_same_set() {
        // Construct a corpus where the prefix-overlap and BK-distance
        // proposers happen to land on the same member set: `pray` and
        // `prays` are within DL=1 *and* share the 4-char prefix `pray`.
        let c = corpus(vec![
            (sid(1), "pray pray pray"),
            (sid(2), "prays prays"),
            (sid(3), "alpha beta"),
        ]);
        let d = Discourse::build(&c);
        let lex = Lexicon::build(&d, LexiconConfig::default());
        let clusters = LemmaClusters::build(&c, LemmaClusterConfig::default());
        let seeds = vec!["prays".to_string()];
        let cf = CandidateFamilies::build(&lex, &clusters, &seeds, Default::default());

        // Find the family containing exactly {pray, prays}.
        let target_id = compute_family_id(&[
            LemmaForm {
                form: "pray".into(),
                count: 0,
            },
            LemmaForm {
                form: "prays".into(),
                count: 0,
            },
        ]);
        let family = cf
            .families
            .get(&target_id)
            .expect("the {pray, prays} family should exist");
        // Both generators tagged on the same record.
        assert!(family.proposed_by.contains(&GeneratorKind::PrefixOverlap));
        assert!(
            family
                .proposed_by
                .iter()
                .any(|g| matches!(g, GeneratorKind::BkDistance { .. })),
        );
    }
}
