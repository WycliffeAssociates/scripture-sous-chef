//! Orthographic signals. Operate on `Verse.nfc`; never on `Verse.raw`.

use crate::analysis::length_buckets::GraphemeCount;
use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, Lane, RuleId, Severity,
};
use crate::project::Project;
use crate::rule::Rule;

/// Character-LM surprisal: a token whose character n-gram probability
/// under a corpus-trained KN model is far below expectation. Catches
/// misspelled tokens and accidental script switches. Not yet implemented.
pub const CHAR_LM_SURPRISAL: RuleId = RuleId("orth.char-lm-surprisal");

/// NFC sanity: any verse where `raw != nfc` reveals un-normalised input.
/// Almost always a paste-from-Word artefact. Not yet implemented.
pub const NFC_SANITY: RuleId = RuleId("orth.nfc-sanity");

/// Script mixing: a single word token containing characters from more
/// than one script (e.g. Latin `o` glued into a Cyrillic word). Almost
/// always a homoglyph confusion. Not yet implemented.
pub const SCRIPT_MIXING: RuleId = RuleId("orth.script-mixing");

/// Compression-texture outlier: a verse whose character-level shape costs
/// unusually many bytes after a compressor has seen this corpus.
///
/// Whole-verse on purpose: this is not a highlighter, it is a "this verse
/// does not fit the local texture" signal. Token-level rules (char-LM,
/// lemma-family) can point at individual spans; this rule contributes an
/// independent holistic cue to the aggregator.
///
/// The string `orth.ncd-texture` is preserved as a historical shorthand —
/// see `analysis::compression` for why this isn't classical NCD.
pub const COMPRESSION_TEXTURE: RuleId = RuleId("orth.ncd-texture");

/// Default robust-z threshold (in 1.4826·MAD units) above which a verse
/// is considered anomalous against its length-cohort baseline.
pub const DEFAULT_TEXTURE_Z_THRESHOLD: f64 = 3.0;
pub const DEFAULT_TEXTURE_MIN_VERSES: usize = 20;

#[derive(Debug, Clone, Copy)]
pub struct CompressionTexture;

impl Rule for CompressionTexture {
    fn id(&self) -> RuleId {
        COMPRESSION_TEXTURE
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        let min_verses = param(project, "compression_texture_min_verses")
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_TEXTURE_MIN_VERSES);
        if project.target.verses.len() < min_verses {
            return Vec::new();
        }

        let Some(target_baseline) = context.texture_baseline.as_ref() else {
            return Vec::new();
        };

        // Source mirror (§3.3 of the plan, ADR 0005): when both source model
        // and source baseline are available, subtract the source verse's
        // own-cohort z-score from the target's. A verse anomalous on both
        // sides was anomalous content, not a target-side problem.
        let source_mirror = context
            .source_texture_model
            .as_ref()
            .zip(context.source_texture_baseline.as_ref());

        // Populate the legacy stats slot with the global view so existing
        // diagnostics consumers still get something useful. Per-bucket
        // stats live on `texture_baseline` if a richer view is needed.
        stats.compression_texture = Some(
            context
                .texture_model
                .stats_for_scores(&collect_target_scores(project, &context.texture_model)),
        );

        let z_threshold = param(project, "compression_texture_z_threshold")
            .unwrap_or(DEFAULT_TEXTURE_Z_THRESHOLD);

        let mut findings = Vec::new();
        for (sid, verse) in &project.target.verses {
            let target_score = context.texture_model.score(&verse.nfc);
            let target_z =
                target_baseline.z_for(GraphemeCount::of(&verse.nfc), target_score);
            if !target_z.is_finite() {
                continue;
            }

            let source_z = source_mirror
                .and_then(|(model, baseline)| {
                    let source_verse = project.source.as_ref()?.verses.get(sid)?;
                    let z = baseline
                        .z_for(GraphemeCount::of(&source_verse.nfc), model.score(&source_verse.nfc));
                    z.is_finite().then_some(z)
                })
                .unwrap_or(0.0);

            let mirrored_z = target_z - source_z;
            if mirrored_z <= z_threshold {
                continue;
            }

            let excess = ((mirrored_z - z_threshold) / z_threshold).clamp(0.0, 1.0);
            findings.push(Finding {
                rule_id: COMPRESSION_TEXTURE,
                sid: *sid,
                severity: Severity::Info,
                lane: Lane::VerseAnomaly,
                // Whole-verse finding; UI renders as a verse-level badge.
                byte_range: ByteRange { start: 0, end: 0 },
                span: &verse.nfc[0..0],
                cluster_key: ClusterKey("compression-texture".to_string()),
                finding_id: FindingId::default(),
                message: format!(
                    "verse texture is unusual for its length cohort (z {:.2} vs threshold {:.2})",
                    mirrored_z, z_threshold
                ),
                evidence: ((0.5 + 0.5 * excess) * context.morphology.char_signal_weight)
                    .clamp(0.0, 1.0),
            });
        }

        findings
    }
}

fn collect_target_scores(
    project: &Project<'_>,
    model: &crate::analysis::compression::CompressionTextureModel,
) -> Vec<f64> {
    use rayon::prelude::*;
    project
        .target
        .verses
        .par_iter()
        .map(|(_, verse)| model.score(&verse.nfc))
        .collect()
}

fn param(project: &Project<'_>, name: &str) -> Option<f64> {
    project.config.rules.iter().find_map(|rule| {
        if rule.id != COMPRESSION_TEXTURE {
            return None;
        }
        rule.params
            .iter()
            .find_map(|(param_name, value)| (*param_name == name).then_some(*value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ExceptionSet};
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    fn sid(v: u16) -> Sid {
        let chapter = (v / 30 + 1).max(1);
        let verse = (v % 30) + 1;
        Sid::new(BookId::from_str("GEN").unwrap(), chapter, verse)
    }

    fn corpus(name: &str, verses: Vec<(Sid, String)>) -> NamedCorpus<'static> {
        let mut map: BTreeMap<Sid, _> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t));
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

    /// Mixed corpus of long-prose verses and very short verses sharing a
    /// common short-cohort baseline. Under the old global median+MAD,
    /// the short verses' elevated compression ratios looked anomalous
    /// (the "Jesus wept" false positive). Under length-conditioned
    /// scoring, the short cohort has its own baseline and no individual
    /// short verse stands out within it.
    #[test]
    fn short_verse_does_not_surface_against_short_cohort_baseline() {
        let mut verses = Vec::new();
        for v in 0..100u16 {
            verses.push((
                sid(v + 1),
                format!(
                    "the quick brown fox jumps over the lazy dog and the moon and the stars verse {v}"
                ),
            ));
        }
        for v in 0..40u16 {
            verses.push((sid(v + 200), format!("X{v}")));
        }
        let project = project_of(corpus("t", verses), None);
        let context = AnalysisContext::build(&project);
        let mut stats = AnalyzeStats::default();
        let findings = CompressionTexture.check(&project, &context, &mut stats);

        for f in &findings {
            let verse = project.target.verses.get(&f.sid).unwrap();
            let len = verse.nfc.chars().count();
            assert!(
                len > 10,
                "short verse '{}' (len={len}) surfaced under length-conditioned rule: {:?}",
                verse.nfc,
                f
            );
        }
    }

    /// When the corresponding source verse is also anomalous within its own
    /// length cohort, the target verse's anomaly is partially exonerated by
    /// the source mirror (ADR 0005). A verse that surfaces in target-only
    /// mode should drop out when the source verse is also anomalous.
    #[test]
    fn source_anomaly_subtracts_from_target_to_exonerate() {
        let outlier = sid(255);
        // Vocabulary of common English fragments — enough variation across
        // verses for MAD > 0, while keeping the corpus stylistically uniform
        // so the outlier's jumbled texture is genuinely anomalous.
        let stems = [
            "the quick brown fox jumps over",
            "and the moon shone bright",
            "behold the children of light",
            "let your works be seen",
            "for great is the day",
            "by the rivers of water",
            "in the time of harvest",
            "all his ways are righteous",
        ];
        let make_corpus = |name: &str| -> NamedCorpus<'static> {
            let mut verses = Vec::new();
            for v in 0..120u16 {
                let stem = stems[(v as usize) % stems.len()];
                verses.push((
                    sid(v + 1),
                    format!("{stem} verse number {v} of the chapter"),
                ));
            }
            verses.push((
                outlier,
                "qzxqzxqzx ##!@#$%^&*()_+ qzxqzx zzzqq Æ⌘ƒ©˙∆ jumbled novel xxx vvv yyy"
                    .to_string(),
            ));
            corpus(name, verses)
        };
        let target_only = project_of(make_corpus("t"), None);
        let target_with_source = project_of(make_corpus("t"), Some(make_corpus("s")));

        let context_no_source = AnalysisContext::build(&target_only);
        let mut stats = AnalyzeStats::default();
        let findings_no_source =
            CompressionTexture.check(&target_only, &context_no_source, &mut stats);
        assert!(
            findings_no_source.iter().any(|f| f.sid == outlier),
            "outlier should surface without source mirror; got {:?}",
            findings_no_source
        );

        let context_with_source = AnalysisContext::build(&target_with_source);
        let mut stats = AnalyzeStats::default();
        let findings_with_source = CompressionTexture.check(
            &target_with_source,
            &context_with_source,
            &mut stats,
        );
        assert!(
            !findings_with_source.iter().any(|f| f.sid == outlier),
            "outlier should be exonerated when source is equally anomalous; got {:?}",
            findings_with_source
        );
    }
}
