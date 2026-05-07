//! Shared corpus-derived analysis state.
//!
//! Keep expensive discourse/lexicon/bootstrap work here instead of
//! letting each positional rule rebuild its own private view.

use std::collections::BTreeMap;

use crate::analysis::compression::{
    BucketedTextureBaseline, CompressionTextureConfig, CompressionTextureModel,
};
use crate::analysis::lemma_cluster::{LemmaClusterConfig, LemmaClusterStats, LemmaClusters};
use crate::analysis::lexicon::{Lexicon, LexiconConfig};
use crate::diagnostics::RuleId;
use crate::discourse::{DEFAULT_MAX_SPAN_SIDS, Discourse, SpanIndex, SpanIndexConfig};
use crate::project::Project;
use crate::signals::positional::{
    G2_THRESHOLD, LearnedNonTerminals, Transition, TriggerStats, collect_transitions,
    learn_non_terminal_clusters,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BootstrapStats {
    pub intrinsic_min_obs: u32,
    pub intrinsic_upper_rate_min: f64,
    pub intrinsic_lower_rate_max: f64,
    pub non_terminal_upper_rate_max: f64,
    pub g2_threshold: f64,
    pub max_span_sids: usize,
    pub n_safe_clusters: usize,
    pub safe_clusters: Vec<TriggerStats>,
    pub evaluated_clusters: Vec<TriggerStats>,
    pub morphology: MorphologyStats,
    pub lemma_clusters: LemmaClusterStats,
}

#[derive(Debug, Clone)]
pub struct AnalysisContext {
    pub discourse: Discourse,
    pub transitions: Vec<Transition>,
    pub strict_lexicon: Lexicon,
    pub lexicon: Lexicon,
    pub span_index: SpanIndex,
    pub texture_model: CompressionTextureModel,
    pub texture_baseline: Option<BucketedTextureBaseline>,
    pub source_texture_model: Option<CompressionTextureModel>,
    pub source_texture_baseline: Option<BucketedTextureBaseline>,
    pub lemma_clusters: LemmaClusters,
    pub morphology: MorphologyStats,
    pub bootstrap_stats: BootstrapStats,
}

impl AnalysisContext {
    pub fn build(project: &Project<'_>) -> Self {
        let config = BootstrapConfig::from_project(project);
        let discourse = Discourse::build(&project.target);
        let transitions = collect_transitions(&discourse.text);
        let strict_lexicon = Lexicon::build(&discourse, config.lexicon);
        let LearnedNonTerminals {
            clusters,
            safe_clusters,
        } = learn_non_terminal_clusters(
            &discourse.text,
            &transitions,
            &strict_lexicon,
            config.non_terminal_upper_rate_max,
            config.g2_threshold,
        );
        let lexicon =
            Lexicon::build_with_counted_clusters(&discourse, config.lexicon, &safe_clusters);
        let span_index = discourse.span_index_with_config(SpanIndexConfig {
            max_span_sids: config.max_span_sids,
        });
        let texture_model = CompressionTextureModel::build(&project.target, config.texture);
        let texture_baseline = BucketedTextureBaseline::build(&texture_model, &project.target);
        let (source_texture_model, source_texture_baseline) = match project.source.as_ref() {
            Some(source) => {
                let model = CompressionTextureModel::build(source, config.texture);
                let baseline = BucketedTextureBaseline::build(&model, source);
                (Some(model), baseline)
            }
            None => (None, None),
        };
        let lemma_clusters = LemmaClusters::build(&project.target, config.lemma_clusters);
        let morphology = MorphologyStats::from_project(project);
        let lemma_cluster_stats = lemma_clusters.stats();

        let mut evaluated_clusters: Vec<_> = clusters.into_values().collect();
        evaluated_clusters.sort_by(|a, b| {
            b.is_trigger
                .cmp(&a.is_trigger)
                .then_with(|| b.g2.total_cmp(&a.g2))
                .then(a.predecessor.cmp(&b.predecessor))
        });
        let mut safe_cluster_stats: Vec<_> = evaluated_clusters
            .iter()
            .filter(|s| s.is_trigger)
            .cloned()
            .collect();
        safe_cluster_stats.sort_by(|a, b| a.predecessor.cmp(&b.predecessor));

        let bootstrap_stats = BootstrapStats {
            intrinsic_min_obs: config.lexicon.intrinsic_min_obs,
            intrinsic_upper_rate_min: config.lexicon.intrinsic_upper_rate_min,
            intrinsic_lower_rate_max: config.lexicon.intrinsic_lower_rate_max,
            non_terminal_upper_rate_max: config.non_terminal_upper_rate_max,
            g2_threshold: config.g2_threshold,
            max_span_sids: config.max_span_sids,
            n_safe_clusters: safe_cluster_stats.len(),
            safe_clusters: safe_cluster_stats,
            evaluated_clusters,
            morphology: morphology.clone(),
            lemma_clusters: lemma_cluster_stats,
        };

        Self {
            discourse,
            transitions,
            strict_lexicon,
            lexicon,
            span_index,
            texture_model,
            texture_baseline,
            source_texture_model,
            source_texture_baseline,
            lemma_clusters,
            morphology,
            bootstrap_stats,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MorphologyStats {
    pub n_word_tokens: usize,
    pub n_word_types: usize,
    pub n_hapax_types: usize,
    pub type_token_ratio: f64,
    pub hapax_ratio: f64,
    pub char_signal_weight: f64,
    pub word_signal_weight: f64,
}

impl MorphologyStats {
    /// Infer how much the engine should trust word-level vs character-level
    /// evidence for this corpus.
    ///
    /// In a highly inflected/agglutinative corpus, many perfectly valid word
    /// forms appear once. Word n-gram "rarity" becomes a property of the
    /// language, not a typo signal. Character-level rules (NCD, char-LM) keep
    /// more usable evidence because affixes and spelling habits repeat inside
    /// words even when whole surface forms do not.
    pub fn from_project(project: &Project<'_>) -> Self {
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for verse in project.target.verses.values() {
            for (_, token_text) in verse.tokens_of(crate::verse::TokenKind::Word) {
                let word: String = token_text
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .flat_map(char::to_lowercase)
                    .collect();
                if !word.is_empty() {
                    *counts.entry(word).or_default() += 1;
                }
            }
        }

        let n_word_tokens = counts.values().map(|count| *count as usize).sum();
        let n_word_types = counts.len();
        let n_hapax_types = counts.values().filter(|count| **count == 1).count();
        let type_token_ratio = ratio(n_word_types, n_word_tokens);
        let hapax_ratio = ratio(n_hapax_types, n_word_types);
        let morphologically_sparse = type_token_ratio > 0.10 && hapax_ratio > 0.60;
        let (char_signal_weight, word_signal_weight) = if morphologically_sparse {
            (1.25, 0.65)
        } else {
            (1.0, 1.0)
        };

        Self {
            n_word_tokens,
            n_word_types,
            n_hapax_types,
            type_token_ratio,
            hapax_ratio,
            char_signal_weight,
            word_signal_weight,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BootstrapConfig {
    lexicon: LexiconConfig,
    non_terminal_upper_rate_max: f64,
    g2_threshold: f64,
    max_span_sids: usize,
    texture: CompressionTextureConfig,
    lemma_clusters: LemmaClusterConfig,
}

impl BootstrapConfig {
    fn from_project(project: &Project<'_>) -> Self {
        let default_lexicon = LexiconConfig::default();
        Self {
            lexicon: LexiconConfig {
                intrinsic_min_obs: param(project, "intrinsic_min_obs")
                    .map(|v| v as u32)
                    .unwrap_or(default_lexicon.intrinsic_min_obs),
                intrinsic_upper_rate_min: param(project, "intrinsic_upper_rate_min")
                    .unwrap_or(default_lexicon.intrinsic_upper_rate_min),
                intrinsic_lower_rate_max: param(project, "intrinsic_lower_rate_max")
                    .unwrap_or(default_lexicon.intrinsic_lower_rate_max),
            },
            non_terminal_upper_rate_max: param(project, "non_terminal_upper_rate_max")
                .unwrap_or(0.15),
            g2_threshold: param(project, "g2_threshold").unwrap_or(G2_THRESHOLD),
            max_span_sids: param(project, "max_span_sids")
                .map(|v| v as usize)
                .unwrap_or(DEFAULT_MAX_SPAN_SIDS),
            texture: CompressionTextureConfig {
                dict_size: param(project, "compression_texture_dict_size")
                    .map(|v| v as usize)
                    .unwrap_or(crate::analysis::compression::DEFAULT_DICT_SIZE),
            },
            lemma_clusters: LemmaClusterConfig {
                min_family_size: param(project, "lemma_min_family_size")
                    .map(|v| v as usize)
                    .unwrap_or(crate::analysis::lemma_cluster::DEFAULT_MIN_FAMILY_SIZE),
                min_stem_chars: param(project, "lemma_min_stem_chars")
                    .map(|v| v as usize)
                    .unwrap_or(crate::analysis::lemma_cluster::DEFAULT_MIN_STEM_CHARS),
                min_token_count: param(project, "lemma_min_token_count")
                    .map(|v| v as u32)
                    .unwrap_or(crate::analysis::lemma_cluster::DEFAULT_MIN_TOKEN_COUNT),
            },
        }
    }
}

fn ratio(num: usize, denom: usize) -> f64 {
    if denom == 0 {
        0.0
    } else {
        num as f64 / denom as f64
    }
}

fn param(project: &Project<'_>, name: &str) -> Option<f64> {
    positional_rule_params(project).find_map(|(_, param_name, value)| {
        if param_name == name {
            Some(value)
        } else {
            None
        }
    })
}

fn positional_rule_params<'a>(
    project: &'a Project<'_>,
) -> impl Iterator<Item = (RuleId, &'static str, f64)> + 'a {
    project.config.rules.iter().flat_map(|rule| {
        rule.params
            .iter()
            .map(move |(name, value)| (rule.id, *name, *value))
    })
}
