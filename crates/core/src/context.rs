//! Shared corpus-derived analysis state.
//!
//! Keep expensive discourse/lexicon/bootstrap work here instead of
//! letting each positional rule rebuild its own private view.

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
}

#[derive(Debug, Clone)]
pub struct AnalysisContext {
    pub discourse: Discourse,
    pub transitions: Vec<Transition>,
    pub strict_lexicon: Lexicon,
    pub lexicon: Lexicon,
    pub span_index: SpanIndex,
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
        };

        Self {
            discourse,
            transitions,
            strict_lexicon,
            lexicon,
            span_index,
            bootstrap_stats,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BootstrapConfig {
    lexicon: LexiconConfig,
    non_terminal_upper_rate_max: f64,
    g2_threshold: f64,
    max_span_sids: usize,
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
        }
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
