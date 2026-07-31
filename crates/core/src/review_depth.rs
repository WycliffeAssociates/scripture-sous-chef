//! Review Depth policy resolution.
//!
//! Review policy is user intent at the boundary. The analyzer keeps only the
//! resolved native [`Config`], so content identity and resident-cache
//! fingerprints describe semantic behavior rather than slider positions.

use std::collections::BTreeMap;

use crate::catalog::ReviewControl;
use crate::config::Config;
use crate::diagnostics::RuleId;

const MIN_DEPTH: u8 = 0;
const MAX_DEPTH: u8 = 100;
const DEFAULT_DEPTH: u8 = 50;

/// A validated project-wide Review Depth position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReviewDepth(u8);

impl ReviewDepth {
    pub const MIN: Self = Self(MIN_DEPTH);
    pub const MAX: Self = Self(MAX_DEPTH);
    pub const DEFAULT: Self = Self(DEFAULT_DEPTH);

    pub fn new(value: u8) -> Result<Self, ReviewPolicyError> {
        (value <= MAX_DEPTH)
            .then_some(Self(value))
            .ok_or(ReviewPolicyError::InvalidDepth(value as i16))
    }

    pub fn from_i16(value: i16) -> Result<Self, ReviewPolicyError> {
        (i16::from(MIN_DEPTH)..=i16::from(MAX_DEPTH))
            .contains(&value)
            .then_some(Self(value as u8))
            .ok_or(ReviewPolicyError::InvalidDepth(value))
    }

    pub const fn value(self) -> u8 {
        self.0
    }
}

/// A validated relative adjustment for one mapped rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReviewAdjustment(i8);

impl ReviewAdjustment {
    pub const MIN: Self = Self(-100);
    pub const MAX: Self = Self(100);

    pub fn new(value: i16) -> Result<Self, ReviewPolicyError> {
        (value >= i16::from(Self::MIN.0) && value <= i16::from(Self::MAX.0))
            .then_some(Self(value as i8))
            .ok_or(ReviewPolicyError::InvalidAdjustment(value))
    }

    pub fn from_i16(value: i16) -> Result<Self, ReviewPolicyError> {
        Self::new(value)
    }

    pub const fn value(self) -> i8 {
        self.0
    }
}

/// The unresolved user policy. It is folded into [`Config`] at an API
/// boundary and is never stored in a resident Galley.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPolicy {
    pub depth: ReviewDepth,
    pub adjustments: BTreeMap<RuleId, ReviewAdjustment>,
}

impl Default for ReviewPolicy {
    fn default() -> Self {
        Self {
            depth: ReviewDepth::DEFAULT,
            adjustments: BTreeMap::new(),
        }
    }
}

impl ReviewPolicy {
    /// Resolve the master plus one rule's relative adjustment with widened
    /// arithmetic before the final `[0, 100]` clamp.
    pub fn effective_depth(&self, rule: RuleId) -> Result<ReviewDepth, ReviewPolicyError> {
        if review_control(rule) == ReviewControl::Fixed && self.adjustments.contains_key(&rule) {
            return Err(ReviewPolicyError::FixedRuleAdjustment(rule));
        }
        let adjustment = self
            .adjustments
            .get(&rule)
            .copied()
            .unwrap_or(ReviewAdjustment(0));
        let value = i16::from(self.depth.value()) + i16::from(adjustment.value());
        ReviewDepth::new(value.clamp(i16::from(MIN_DEPTH), i16::from(MAX_DEPTH)) as u8)
    }
}

/// Errors returned before a malformed policy can affect an effective config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewPolicyError {
    InvalidDepth(i16),
    InvalidAdjustment(i16),
    FixedRuleAdjustment(RuleId),
}

impl std::fmt::Display for ReviewPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDepth(value) => {
                write!(f, "review depth must be an integer in 0..=100, got {value}")
            }
            Self::InvalidAdjustment(value) => {
                write!(
                    f,
                    "review adjustment must be an integer in -100..=100, got {value}"
                )
            }
            Self::FixedRuleAdjustment(rule) => {
                write!(f, "rule {rule} does not support Review Depth adjustments")
            }
        }
    }
}

impl std::error::Error for ReviewPolicyError {}

/// The closed resolver's current mapped set. A rule enters this list only
/// after its calibration packet has supplied a complete judging path.
pub const fn review_control(rule: RuleId) -> ReviewControl {
    match rule {
        RuleId::PunctuationSpacingAnomaly
        | RuleId::SentenceInitialLowercase
        | RuleId::InconsistentWordCasing => ReviewControl::Mapped,
        RuleId::ExcessHWhitespace
        | RuleId::TabInBody
        | RuleId::ControlChars
        | RuleId::ZeroWidthMisuse
        | RuleId::EmptyVerse
        | RuleId::InvalidCodepoint
        | RuleId::ReplacementRun
        | RuleId::ProjectLengthRatio
        | RuleId::SourceMarkerLeftover
        | RuleId::MergeConflictMarker
        | RuleId::PunctuationAdjacencyAnomaly
        | RuleId::DuplicateWord
        | RuleId::PunctOnlyToken
        | RuleId::CombiningMarkWithoutBase
        | RuleId::RedundantZeroWidthSpace
        | RuleId::MixedScriptInToken
        | RuleId::RepeatedCharacterRun
        | RuleId::MixedNumeralSystems
        | RuleId::BracketBalance
        | RuleId::RareGlyph
        | RuleId::MixedCaseWord
        | RuleId::MixedNormalization
        | RuleId::UntranslatedWord => ReviewControl::Fixed,
    }
}

/// Apply the validated policy to a resolved native config.
pub fn apply_review_policy(
    config: &mut Config,
    policy: &ReviewPolicy,
) -> Result<(), ReviewPolicyError> {
    for &rule in RuleId::ALL {
        let depth = policy.effective_depth(rule)?;
        match rule {
            RuleId::PunctuationSpacingAnomaly => {
                config.punctuation_spacing =
                    crate::signals::punctuation::config_at_review_depth(depth);
            }
            RuleId::SentenceInitialLowercase => {
                config.casing.sentence_initial =
                    crate::signals::casing::sentence_initial_config_at_review_depth(depth);
            }
            RuleId::InconsistentWordCasing => {
                config.casing.inconsistent_word =
                    crate::signals::casing::inconsistent_word_config_at_review_depth(depth);
            }
            RuleId::ExcessHWhitespace
            | RuleId::TabInBody
            | RuleId::ControlChars
            | RuleId::ZeroWidthMisuse
            | RuleId::EmptyVerse
            | RuleId::InvalidCodepoint
            | RuleId::ReplacementRun
            | RuleId::ProjectLengthRatio
            | RuleId::SourceMarkerLeftover
            | RuleId::MergeConflictMarker
            | RuleId::PunctuationAdjacencyAnomaly
            | RuleId::DuplicateWord
            | RuleId::PunctOnlyToken
            | RuleId::CombiningMarkWithoutBase
            | RuleId::RedundantZeroWidthSpace
            | RuleId::MixedScriptInToken
            | RuleId::RepeatedCharacterRun
            | RuleId::MixedNumeralSystems
            | RuleId::BracketBalance
            | RuleId::RareGlyph
            | RuleId::MixedCaseWord
            | RuleId::MixedNormalization
            | RuleId::UntranslatedWord => {}
        }
    }

    Ok(())
}

/// Piecewise-linear interpolation for a profile's continuous f32 parameter.
pub(crate) fn interpolate_f32(depth: ReviewDepth, anchors: &[(u8, f32)]) -> f32 {
    debug_assert!(anchors.len() >= 2);
    let value = f32::from(depth.value());
    for pair in anchors.windows(2) {
        let [(left_depth, left), (right_depth, right)] = pair else {
            unreachable!();
        };
        if depth.value() <= *right_depth {
            let span = f32::from(right_depth - left_depth);
            let progress = (value - f32::from(*left_depth)) / span;
            return *left + (*right - *left) * progress;
        }
    }
    anchors.last().expect("profile has an endpoint").1
}

/// Piecewise-linear interpolation for an integer parameter. Rounding is
/// explicitly half-up so calibration TSVs can reproduce the shipped value.
pub(crate) fn interpolate_u32(depth: ReviewDepth, anchors: &[(u8, u32)]) -> u32 {
    interpolate_f32(
        depth,
        &anchors
            .iter()
            .map(|&(position, value)| (position, value as f32))
            .collect::<Vec<_>>(),
    )
    .round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_policy_values() {
        assert_eq!(ReviewDepth::new(100).unwrap().value(), 100);
        assert!(ReviewDepth::new(101).is_err());
        assert_eq!(ReviewAdjustment::new(-100).unwrap().value(), -100);
        assert!(ReviewAdjustment::new(101).is_err());
    }

    #[test]
    fn additive_depth_clamps_after_widened_arithmetic() {
        let mut policy = ReviewPolicy::default();
        policy.adjustments.insert(
            RuleId::PunctuationSpacingAnomaly,
            ReviewAdjustment::new(20).unwrap(),
        );
        assert_eq!(
            policy
                .effective_depth(RuleId::PunctuationSpacingAnomaly)
                .unwrap()
                .value(),
            70
        );

        policy.depth = ReviewDepth::new(95).unwrap();
        assert_eq!(
            policy
                .effective_depth(RuleId::PunctuationSpacingAnomaly)
                .unwrap()
                .value(),
            100
        );
        policy.adjustments.insert(
            RuleId::PunctuationSpacingAnomaly,
            ReviewAdjustment::new(-100).unwrap(),
        );
        assert_eq!(
            policy
                .effective_depth(RuleId::PunctuationSpacingAnomaly)
                .unwrap()
                .value(),
            0
        );
    }

    #[test]
    fn fixed_rule_adjustments_are_rejected() {
        let mut policy = ReviewPolicy::default();
        policy
            .adjustments
            .insert(RuleId::DuplicateWord, ReviewAdjustment::new(10).unwrap());
        assert_eq!(
            policy.effective_depth(RuleId::DuplicateWord),
            Err(ReviewPolicyError::FixedRuleAdjustment(
                RuleId::DuplicateWord
            ))
        );
        let mut config = Config::v1_defaults();
        assert_eq!(
            apply_review_policy(&mut config, &policy),
            Err(ReviewPolicyError::FixedRuleAdjustment(
                RuleId::DuplicateWord
            ))
        );
    }

    #[test]
    fn interpolation_is_piecewise_and_half_up() {
        let anchors = [(0, 0.0), (50, 10.0), (100, 20.0)];
        assert_eq!(
            interpolate_f32(ReviewDepth::new(25).unwrap(), &anchors),
            5.0
        );
        assert_eq!(
            interpolate_f32(ReviewDepth::new(75).unwrap(), &anchors),
            15.0
        );
        assert_eq!(
            interpolate_u32(ReviewDepth::new(25).unwrap(), &[(0, 0), (50, 11)]),
            6
        );
    }

    #[test]
    fn default_policy_resolves_to_default_judging_configs() {
        let mut config = Config::v1_defaults();
        let expected = config.clone();
        apply_review_policy(&mut config, &ReviewPolicy::default()).unwrap();
        assert_eq!(config.punctuation_spacing, expected.punctuation_spacing);
        assert_eq!(config.casing, expected.casing);
    }

    #[test]
    fn mapped_registry_matches_catalog_control() {
        for &rule in RuleId::ALL {
            assert_eq!(review_control(rule), crate::catalog::review_control(rule));
        }
    }

    #[test]
    fn production_profiles_have_ordered_safe_endpoints_and_default_midpoints() {
        let depths = [0, 25, 50, 75, 100].map(|value| ReviewDepth::new(value).unwrap());
        let spacing: Vec<_> = depths
            .iter()
            .map(|&depth| crate::signals::punctuation::config_at_review_depth(depth))
            .collect();
        let positional: Vec<_> = depths
            .iter()
            .map(|&depth| crate::signals::casing::sentence_initial_config_at_review_depth(depth))
            .collect();
        let intrinsic: Vec<_> = depths
            .iter()
            .map(|&depth| crate::signals::casing::inconsistent_word_config_at_review_depth(depth))
            .collect();

        assert_eq!(spacing[2], Config::v1_defaults().punctuation_spacing);
        assert_eq!(positional[2], Config::v1_defaults().casing.sentence_initial);
        assert_eq!(intrinsic[2], Config::v1_defaults().casing.inconsistent_word);

        let close = |actual: f32, expected: f32| {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        };
        close(spacing[1].emit_score_min, 0.65);
        close(spacing[1].confidence_z, 2.27);
        assert_eq!(spacing[1].minority_recurrence_k, 24.0);
        assert_eq!(spacing[1].minority_rate_per_10k, 30.0);
        close(spacing[3].emit_score_min, 0.40);
        close(spacing[3].confidence_z, 1.62);
        assert_eq!(spacing[3].minority_recurrence_k, 48.0);
        assert_eq!(spacing[3].minority_rate_per_10k, 53.0);

        for profile in [&positional[1].evidence, &intrinsic[1].evidence] {
            close(profile.emit_score_min, 0.97);
            close(profile.confidence_z, 2.27);
            assert_eq!(profile.recurrence_k, 24.0);
        }
        close(positional[1].trust_gate, 0.925);
        for profile in [&positional[3].evidence, &intrinsic[3].evidence] {
            close(profile.emit_score_min, 0.875);
            close(profile.confidence_z, 1.62);
            assert_eq!(profile.recurrence_k, 48.0);
        }
        close(positional[3].trust_gate, 0.825);

        for pair in spacing.windows(2) {
            assert!(pair[0].emit_score_min >= pair[1].emit_score_min);
            assert!(pair[0].confidence_z >= pair[1].confidence_z);
            assert!(pair[0].minority_recurrence_k <= pair[1].minority_recurrence_k);
            assert!(pair[0].minority_rate_per_10k <= pair[1].minority_rate_per_10k);
            for value in [
                pair[0].emit_score_min,
                pair[0].confidence_z,
                pair[0].minority_recurrence_k,
                pair[0].minority_rate_per_10k,
            ] {
                assert!(value.is_finite() && value >= 0.0);
            }
        }
        for pair in positional.windows(2) {
            assert!(pair[0].evidence.emit_score_min >= pair[1].evidence.emit_score_min);
            assert!(pair[0].evidence.confidence_z >= pair[1].evidence.confidence_z);
            assert!(pair[0].evidence.recurrence_k <= pair[1].evidence.recurrence_k);
            assert!(pair[0].trust_gate >= pair[1].trust_gate);
            assert!((0.0..=1.0).contains(&pair[0].trust_gate));
        }
        for pair in intrinsic.windows(2) {
            assert!(pair[0].evidence.emit_score_min >= pair[1].evidence.emit_score_min);
            assert!(pair[0].evidence.confidence_z >= pair[1].evidence.confidence_z);
            assert!(pair[0].evidence.recurrence_k <= pair[1].evidence.recurrence_k);
        }
    }
}
