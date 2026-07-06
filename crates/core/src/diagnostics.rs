//! Findings.
//!
//! A `Finding` carries only what makes it *addressable* and *routable*:
//! where it is (`sid` + `range`), what it is (`code`), how loud
//! (`severity`), and an optional confidence (`score`). No rendered
//! message — the consumer localises from `code` (the editor already does
//! this for onion via lingui). No DOM, no token ids, no source spans:
//! mapping a range back to a DOM node or source offset is the
//! orchestrator's job via onion's `segments`. See ADR 0010.

use crate::sid::Sid;
use crate::span::Span;

/// Defines the closed `RuleId` set from a single list, so a rule's
/// variant, its `serde` rename (the wire / localisation code), its
/// `RuleId::ALL` membership, and its `code()` arm **cannot drift**.
///
/// Adding a rule is exactly one line here; the enum, `ALL`, and `code()`
/// all derive from it (and the `Tsify` string union with them). See
/// ADR 0012. The compiler still forces every rule's `PerVerseRule` /
/// `ProjectRule` impl; this macro only owns the identity↔code mapping.
macro_rules! define_rule_ids {
    ($( $(#[$vmeta:meta])* $variant:ident => $code:literal ),+ $(,)?) => {
        /// Stable, machine-readable rule identity — a **closed set**.
        /// Internally a cheap enum discriminant (zero per-finding
        /// allocation); each variant serialises to its dotted code string
        /// (e.g. `"lex.excess-h-whitespace"`) only at the wasm/IPC
        /// boundary. The closed set is the typed surface consumers key
        /// config and localisation off: Rust via [`RuleId::ALL`] +
        /// exhaustive `match`; TS via the `Tsify` string union.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
        pub enum RuleId {
            $(
                $(#[$vmeta])*
                #[cfg_attr(feature = "serde", serde(rename = $code))]
                $variant,
            )+
        }

        impl RuleId {
            /// Every rule identity — the full closed set, for exhaustive
            /// iteration when a consumer builds a config or localisation
            /// map. Stays complete by construction (macro-generated).
            pub const ALL: &'static [RuleId] = &[ $( RuleId::$variant ),+ ];

            /// The stable code string — the wasm/IPC wire form and the
            /// localisation key. Macro-generated from the same list as the
            /// `serde` rename, so the two are identical by construction.
            pub fn code(self) -> &'static str {
                match self {
                    $( RuleId::$variant => $code, )+
                }
            }
        }
    };
}

define_rule_ids! {
    ExcessHWhitespace        => "lex.excess-h-whitespace",
    TabInBody                => "hyg.tab-in-body",
    ControlChars             => "hyg.control-chars",
    ZeroWidthMisuse          => "hyg.zero-width-misuse",
    EmptyVerse               => "hyg.empty-verse",
    InvalidCodepoint         => "hyg.invalid-codepoint",
    ProjectLengthRatio       => "prop.length-ratio",
    SourceMarkerLeftover     => "struct.source-marker-leftover",
    MergeConflictMarker      => "struct.merge-conflict-marker",
    PunctuationAdjacencyAnomaly => "punct.adjacency-anomaly",
    DuplicateWord            => "lex.duplicate-word",
    PunctOnlyToken           => "lex.punct-only-token",
    CombiningMarkWithoutBase => "uni.combining-mark-without-base",
    RedundantZeroWidthSpace  => "uni.redundant-zero-width-space",
    MixedScriptInToken       => "uni.mixed-script-in-token",
    RepeatedCharacterRun     => "lex.repeated-character-run",
    MixedNumeralSystems      => "uni.mixed-numeral-systems",
    PlaceholderLeftover      => "punct.placeholder-leftover",
    BracketBalance           => "punct.bracket-balance",
    PunctuationSpacingAnomaly => "punct.spacing-anomaly",
    SentenceInitialLowercase => "case.sentence-initial-lowercase",
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// How loud a finding is. Maps 1:1 to the editor's annotation severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Whether an observed delimiter opens or closes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum DelimRole {
    Open,
    Close,
}

/// One delimiter seen inside a `punct.bracket-balance` window: which verse
/// (`sid` as the canonical `"GEN 1:1"` string), its glyph, whether it opens
/// or closes, and whether the matcher paired it. The whole list lets a
/// reviewer see the full bracket context of the window and decide what is
/// actually missing — not just stare at the lone orphan. `sid` is a string
/// (not the byte-offset `Span` other findings use) because each observation
/// lives in a *different* verse; the orphan's own precise range is carried
/// on the `Finding`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct DelimObservation {
    pub sid: String,
    pub glyph: String,
    pub role: DelimRole,
    pub matched: bool,
}

/// Which distribution flagged a `prop.length-ratio` verse, with the robust
/// z-score(s) that did. Modelled so a scope cannot exist without its
/// score(s): `Both` carries both, the single scopes carry one. The sign of
/// `z` is informative (negative = shorter than the median).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum LengthRatioScope {
    Book { z: f32 },
    Project { z: f32 },
    Both { book_z: f32, project_z: f32 },
}

/// Structured message arguments — the additive payload ADR 0010 §6
/// anticipated. A **closed** discriminated union, like `RuleId`: rules
/// whose localised message interpolates values add a variant here, and
/// the consumer's ICU layer renders from it. Never a rendered string.
/// Deterministic no-interpolation rules carry `None` on the finding.
///
/// Not `Copy`: the `BracketWindow` payload owns a `Vec`. Findings are
/// collected into `Vec`s and never copied on a hot path, so this costs
/// nothing real (ADR 0016).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum FindingArgs {
    /// `prop.length-ratio`: the verse's length relative to the reference
    /// (`ratio_pct`, e.g. `312.0` = 312% of the reference length) plus the
    /// robust z-score(s) that flagged it — within its book, the whole
    /// project, or both (ADR 0017 §8). The scope variant carries exactly the
    /// z-scores it has, so a finding can't claim a scope without its score.
    #[cfg_attr(feature = "serde", serde(rename = "length-ratio"))]
    LengthRatio {
        ratio_pct: f32,
        scope: LengthRatioScope,
    },
    /// `punct.bracket-balance`: every delimiter the matcher saw within the
    /// window around the orphan, so the consumer can render the full
    /// bracket context. The finding's `range` anchors the orphan itself.
    #[cfg_attr(feature = "serde", serde(rename = "bracket-window"))]
    BracketWindow { window: Vec<DelimObservation> },
    /// `lex.duplicate-word`, **cross-verse** case only: the doubled word
    /// straddles a verse boundary, so the finding anchors the deletable
    /// *second* occurrence (its `sid`/`range`) and carries the *first*
    /// occurrence's verse here (`first_sid` as the canonical `"GEN 1:1"`
    /// string, like `DelimObservation.sid`, because it lives in a different
    /// verse). The within-verse case carries `None`: its `range` already
    /// spans both words. See ADR 0016 (amendment).
    #[cfg_attr(feature = "serde", serde(rename = "duplicate-word"))]
    DuplicateWord { first_sid: String },
}

/// One addressable content finding in one verse.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    pub sid: Sid,
    pub code: RuleId,
    pub severity: Severity,
    /// Byte offsets into the verse text. Project with `range.to_utf16` /
    /// `to_graphemes` at the consumer boundary.
    pub range: Span,
    /// Confidence, for rules that have one (the editor's confidence
    /// chip). `None` for deterministic rules; corpus/statistical rules
    /// fill it when they graduate from `labs`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub score: Option<f32>,
    /// Structured args for the consumer's interpolated message. `None`
    /// for rules whose message needs no interpolation.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub args: Option<FindingArgs>,
}
