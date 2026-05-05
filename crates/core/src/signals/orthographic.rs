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

/// Compression texture outlier: a verse whose character-level shape costs
/// unusually many bytes after a compressor has seen this corpus.
///
/// This is Phase H's first engine-visible rule. It is intentionally whole-verse:
/// NCD is not a highlighter, it is a "this verse does not fit the local texture"
/// signal. Later char-LM and lemma-family rules can point at individual tokens;
/// this rule gives the aggregator an independent holistic cue.
pub const NCD_TEXTURE: RuleId = RuleId("orth.ncd-texture");

pub const DEFAULT_NCD_MAD_THRESHOLD: f64 = 8.0;
pub const DEFAULT_NCD_MIN_VERSES: usize = 20;

#[derive(Debug, Clone, Copy)]
pub struct NcdTexture;

impl Rule for NcdTexture {
    fn id(&self) -> RuleId {
        NCD_TEXTURE
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        // First-pass NCD is a real compressor loop, not yet an incremental
        // language model. Keep it opt-in so normal `sous check` runs do not
        // pay whole-corpus compression cost until a project deliberately tests
        // the Phase H signal.
        if !project
            .config
            .rules
            .iter()
            .any(|rule| rule.id == NCD_TEXTURE)
        {
            return Vec::new();
        }

        let min_verses = param(project, "ncd_min_verses")
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_NCD_MIN_VERSES);
        if project.target.verses.len() < min_verses {
            return Vec::new();
        }

        let mut scored = Vec::with_capacity(project.target.verses.len());
        for (sid, verse) in &project.target.verses {
            scored.push((*sid, context.ncd_model.score(&verse.nfc)));
        }

        let scores: Vec<f64> = scored.iter().map(|(_, score)| *score).collect();
        let ncd_stats = context.ncd_model.stats_for_scores(&scores);
        let median = ncd_stats.median_score;
        let mad = ncd_stats.mad_score;
        stats.ncd_texture = Some(ncd_stats);

        // If every verse has the same texture score, the corpus gives us no
        // contrastive baseline. Returning no findings is better than inventing
        // confidence from a flat distribution.
        if mad <= f64::EPSILON {
            return Vec::new();
        }

        let threshold = param(project, "ncd_mad_threshold").unwrap_or(DEFAULT_NCD_MAD_THRESHOLD);
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
                rule_id: NCD_TEXTURE,
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
                    "verse texture is unusual for this corpus (ncd {:.3}, baseline {:.3})",
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
        if rule.id != NCD_TEXTURE {
            return None;
        }
        rule.params
            .iter()
            .find_map(|(param_name, value)| (*param_name == name).then_some(*value))
    })
}
