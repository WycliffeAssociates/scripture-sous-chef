//! Orthographic signals. Operate on `Verse.nfc`; never on `Verse.raw`.

use crate::context::AnalysisContext;
use crate::diagnostics::{
    AnalyzeStats, ByteRange, ClusterKey, Finding, FindingId, RuleId, Severity,
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

pub const DEFAULT_TEXTURE_MAD_THRESHOLD: f64 = 8.0;
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
        // Default-on. The model trains a project-wide zstd dict once
        // in `AnalysisContext::build`; per-verse scoring is a single
        // dict-warmed compression and runs in parallel via rayon.
        let min_verses = param(project, "compression_texture_min_verses")
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_TEXTURE_MIN_VERSES);
        if project.target.verses.len() < min_verses {
            return Vec::new();
        }

        // The model self-disables on tiny corpora (returns 0.0 from every
        // score); skip cleanly here too so we don't compute a bogus median.
        if context.texture_model.dict_bytes() == 0 {
            return Vec::new();
        }

        use rayon::prelude::*;
        let scored: Vec<(crate::sid::Sid, f64)> = project
            .target
            .verses
            .par_iter()
            .map(|(sid, verse)| (*sid, context.texture_model.score(&verse.nfc)))
            .collect();

        let scores: Vec<f64> = scored.iter().map(|(_, score)| *score).collect();
        let texture_stats = context.texture_model.stats_for_scores(&scores);
        let median = texture_stats.median_score;
        let mad = texture_stats.mad_score;
        stats.compression_texture = Some(texture_stats);

        // If every verse has the same texture score, the corpus gives us no
        // contrastive baseline. Returning no findings is better than inventing
        // confidence from a flat distribution. NaN slips in only when the
        // score vector was empty, which our `dict_bytes()` gate above already
        // rules out — but treat it as "no baseline" anyway.
        if !mad.is_finite() || mad <= f64::EPSILON {
            return Vec::new();
        }

        let threshold = param(project, "compression_texture_mad_threshold")
            .unwrap_or(DEFAULT_TEXTURE_MAD_THRESHOLD);
        let cutoff = median + threshold * mad;
        let mut findings = Vec::new();
        for (sid, score) in scored {
            if score <= cutoff {
                continue;
            }
            let Some(verse) = project.target.verses.get(&sid) else {
                continue;
            };
            let excess = ((score - cutoff) / (threshold * mad)).clamp(0.0, 1.0);
            findings.push(Finding {
                rule_id: COMPRESSION_TEXTURE,
                sid,
                severity: Severity::Info,
                // Whole-verse texture finding. The UI can render this as a
                // verse-level badge; a later token-level rule should provide
                // the precise underline if one exists.
                byte_range: ByteRange { start: 0, end: 0 },
                span: &verse.nfc[0..0],
                cluster_key: ClusterKey("compression-texture".to_string()),
                finding_id: FindingId::default(),
                message: format!(
                    "verse texture is unusual for this corpus (ratio {:.3}, baseline {:.3})",
                    score, median
                ),
                // Morphologically sparse corpora get a small boost for
                // character-level evidence. The boost is capped by the finding
                // evidence contract; aggregation still decides whether the
                // whole cluster surfaces.
                evidence: ((0.5 + 0.5 * excess) * context.morphology.char_signal_weight)
                    .clamp(0.0, 1.0),
            });
        }

        findings
    }
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
