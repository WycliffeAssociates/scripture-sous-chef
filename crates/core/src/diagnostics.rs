//! Findings.
//!
//! A `Finding` carries only what makes it *addressable* and *routable*:
//! where it is (`sid` + `range`), what it is (`code`), how loud
//! (`severity`), and an optional confidence (`score`). No rendered
//! message — the consumer localises from `code` (the editor already does
//! this for onion via lingui). No DOM, no token ids, no source spans:
//! mapping a range back to a DOM node or source offset is the
//! orchestrator's job via onion's `segments`. See ADR 0010.

use crate::corpus::KeyIdx;
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
    ReplacementRun           => "hyg.replacement-run",
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
    BracketBalance           => "punct.bracket-balance",
    PunctuationSpacingAnomaly => "punct.spacing-anomaly",
    SentenceInitialLowercase => "case.sentence-initial-lowercase",
    InconsistentWordCasing   => "case.inconsistent-word-casing",
    RareGlyph                => "uni.rare-glyph",
    MixedCaseWord            => "case.mixed-case-word",
    MixedNormalization       => "uni.mixed-normalization",
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// The closed, output-level classification of a rule's semantic inputs, used
/// by content identity and persisted-findings validation. It
/// describes which inputs may affect a rule's *findings* — never its substrate
/// or cache implementation. Rules never inspect it; the closed registry and
/// the generated wire schema do. It is an enum, not a bool, so a future
/// non-silent absence behavior or new input kind forces an explicit
/// exhaustive decision rather than silently entering the reference-removal
/// salvage path.
///
/// The generated JS schema spells these `"target-only"` and
/// `"target-and-reference-silent-when-absent"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum InputDependency {
    TargetOnly,
    TargetAndReferenceSilentWhenAbsent,
}

impl RuleId {
    /// This rule's output-level [`InputDependency`]. An exhaustive match (no
    /// `_` arm), so a new rule cannot land without an explicit classification.
    /// Only `prop.length-ratio` reads the reference (source) corpus, and it
    /// emits nothing at all when no reference is present.
    pub fn input_dependency(self) -> InputDependency {
        match self {
            RuleId::ProjectLengthRatio => InputDependency::TargetAndReferenceSilentWhenAbsent,
            RuleId::ExcessHWhitespace
            | RuleId::TabInBody
            | RuleId::ControlChars
            | RuleId::ZeroWidthMisuse
            | RuleId::EmptyVerse
            | RuleId::InvalidCodepoint
            | RuleId::ReplacementRun
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
            | RuleId::PunctuationSpacingAnomaly
            | RuleId::SentenceInitialLowercase
            | RuleId::InconsistentWordCasing
            | RuleId::RareGlyph
            | RuleId::MixedCaseWord
            | RuleId::MixedNormalization => InputDependency::TargetOnly,
        }
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

/// A separator mark's *judged* form on one side of an occurrence (ADR 0054 2nd
/// amendment — the binary bit inside a class pool):
///
/// - `Attached` — the mark clings directly to the neighbour (no whitespace).
/// - `Spaced` — horizontal whitespace was crossed to reach the neighbour, **or**
///   the verse/book seam was reached (the seam reads as whitespace, never its
///   own category — repo `CLAUDE.md`; a terminal is never attached across a
///   seam). The neighbour's *class* is still read across the seam, in book order.
///
/// The form is orthogonal to the neighbour's [`SpacingClass`]: a `Number`-pool
/// `.` can be `Attached` (`7.8`, a decimal) or `Spaced` (`verse. 3`, a
/// cross-reference), and the pool learns which is the convention.
///
/// This is the shipped wire/args vocabulary as well as the rule's internal one —
/// deliberately one type, so the published JSON string and the counter the rule
/// incremented cannot drift. Every variant carries an **explicit** `serde`
/// rename: these strings are a published surface and must never depend on an
/// inferred naming convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum SpacingForm {
    #[cfg_attr(feature = "serde", serde(rename = "attached"))]
    Attached,
    #[cfg_attr(feature = "serde", serde(rename = "spaced"))]
    Spaced,
}

/// The content class of a mark's first non-whitespace neighbour — the **pool**
/// its attached-vs-spaced binary is conditioned on (ADR 0054 2nd amendment).
/// Quote is merged into `Punct` (user ruling). A `Number` neighbour is a
/// (non-quote) numeric scalar; a `Letter` neighbour is any cluster containing an
/// alphabetic scalar (a decomposed base + combining letter still counts);
/// everything else — another mark, a quote, a bracket, a symbol — is `Punct`.
///
/// Explicitly renamed per variant for the same reason as [`SpacingForm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum SpacingClass {
    #[cfg_attr(feature = "serde", serde(rename = "letter"))]
    Letter,
    #[cfg_attr(feature = "serde", serde(rename = "number"))]
    Number,
    #[cfg_attr(feature = "serde", serde(rename = "punct"))]
    Punct,
}

/// One violated side of a `punct.spacing-anomaly` finding (ADR 0054 2nd
/// amendment — the pooled class-conditioned model): the observed minority `form`
/// against the neighbour-content pool `class` that judged it, how many of the
/// mark's occurrences **in that pool** take this form (`count`), and the pool's
/// judged occupancy `N_pool` (`total`). `count / total` is the descriptive rate
/// the Wilson-bound `score` deliberately isn't (ADR 0048).
///
/// `form`/`class` are closed vocabularies and are typed as such: they were
/// `String` until 2026-07-27, which cost 48 bytes of heap-pointer weight per
/// side and made `FindingArgs` the widest thing in a `Finding`. Field order is
/// unchanged, so the serialized object's key order is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct SpacingSide {
    pub form: SpacingForm,
    pub class: SpacingClass,
    pub count: u32,
    pub total: u32,
}

/// Which of `punct.bracket-balance`'s two corpus conventions a finding
/// broke — so the consumer knows which descriptive sentence the counts in
/// [`FindingArgs::BracketWindow`] belong to. `Pairing`: the family is closed
/// at all (`majority` = matched delimiter events); `ShortSpan`: the family's
/// pairs close within the window (`majority` = pairs closing in-window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum BracketMeasure {
    Pairing,
    ShortSpan,
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
    /// window around the orphan, so the consumer can render the full bracket
    /// context (the finding's `range` anchors the orphan itself), plus the
    /// corpus convention the finding broke as a raw share. `measure` says
    /// which convention; `majority / total` is that convention's descriptive
    /// rate — the plain "this family is closed 99% of the time (n = 2043)"
    /// the Wilson-bound `score` deliberately isn't (ADR 0048).
    #[cfg_attr(feature = "serde", serde(rename = "bracket-window"))]
    BracketWindow {
        window: Vec<DelimObservation>,
        measure: BracketMeasure,
        majority: u32,
        total: u32,
    },
    /// `punct.spacing-anomaly`: the mark's **per-side spacing** here against its
    /// corpus convention (ADR 0054 amendment), so the consumer can render "`,` is
    /// attached on the right in only 1 of 1053 places" — the descriptive rate
    /// behind the Wilson-bound `score` (ADR 0048). Each side present in the args
    /// is one this occurrence violated (its form is the rare minority for that
    /// side); a side whose neighbour is punct/digit abstains and is absent. An
    /// occurrence can violate one or both sides — a single finding carries both.
    #[cfg_attr(feature = "serde", serde(rename = "spacing-convention"))]
    SpacingConvention {
        mark: char,
        left: Option<SpacingSide>,
        right: Option<SpacingSide>,
    },
    /// `case.sentence-initial-lowercase`: the forced-position habit's
    /// corpus-wide uppercase-vs-total counts among words the lexicon calls
    /// intrinsically lowercase, so the consumer can render "after `.` this
    /// translation capitalizes in 512 of 520 places" — the descriptive rate
    /// behind the Wilson-bound `score` (ADR 0048, 0051). The flagged token is
    /// the lowercase minority; `upper / total` is the majority uppercase
    /// share. `glyph` is the terminal that forced the position, or `None` for
    /// the book-initial word (which has no terminal glyph). `quoted` marks the
    /// boundary *class* (ADR 0052): a close-quote intervened between the
    /// terminal and the flagged word (`."`, `said: "`), a distinct learned
    /// class from the bare terminal — so the consumer can render "after `.\"`"
    /// vs "after `.`".
    #[cfg_attr(feature = "serde", serde(rename = "casing-convention"))]
    CasingConvention {
        glyph: Option<char>,
        quoted: bool,
        upper: u32,
        total: u32,
    },
    /// `case.inconsistent-word-casing`: the flagged word's corpus-wide
    /// capitalized-vs-total counts, so the consumer can render "this
    /// translation writes ‘jesus’ capitalized in 1315 of 1316 places; here it
    /// is lowercase" — the descriptive rate behind the Wilson-bound `score`
    /// (ADR 0048, 0051). The flagged occurrence is the lowercase minority;
    /// `upper / total` is the majority capitalized share. `word` is the
    /// case-folded form (the lexicon key).
    #[cfg_attr(feature = "serde", serde(rename = "word-casing"))]
    WordCasing {
        word: String,
        upper: u32,
        total: u32,
    },
    /// `lex.punct-only-token`: how rare this stranded-punctuation pattern is —
    /// `count` occurrences across `units` lexical units (ADR 0048). The plain
    /// rarity behind the score; the flagged mark is in the finding's `range`.
    #[cfg_attr(feature = "serde", serde(rename = "punct-only-rate"))]
    PunctOnlyRate { count: u32, units: u32 },
    /// `punct.adjacency-anomaly`: the two independent convention axes behind
    /// the score, as raw counts (ADR 0048). `pattern` is the flagged run;
    /// `k / lead_n` is how often it occurs among that lead glyph's runs;
    /// `books / corpus` is how many books use it. No single %, so both ship.
    #[cfg_attr(feature = "serde", serde(rename = "adjacency-evidence"))]
    AdjacencyEvidence {
        pattern: String,
        k: u32,
        lead_n: u32,
        books: u32,
        corpus: u32,
    },
    /// `uni.mixed-script-in-token`: the convention axes behind the score, as
    /// raw counts (ADR 0048). `k / n` is this script mix's share of its
    /// dominant script's tokens; `books / corpus` is how many books use it.
    #[cfg_attr(feature = "serde", serde(rename = "script-mix-evidence"))]
    ScriptMixEvidence {
        k: u32,
        n: u32,
        books: u32,
        corpus: u32,
    },
    /// `lex.repeated-character-run`: the repeated character and how many times
    /// it repeats in the flagged run (ADR 0048) — the plain fact behind the
    /// score, in words a reviewer reads rather than convention strengths.
    #[cfg_attr(feature = "serde", serde(rename = "repeat-evidence"))]
    RepeatEvidence { ch: char, run: u32 },
    /// `lex.duplicate-word`, **cross-verse** case only: the doubled word
    /// straddles a verse boundary, so the finding anchors the deletable
    /// *second* occurrence (its `sid`/`range`) and carries the *first*
    /// occurrence's verse here (`first_sid` as the canonical `"GEN 1:1"`
    /// string, like `DelimObservation.sid`, because it lives in a different
    /// verse). The within-verse case carries `None`: its `range` already
    /// spans both words. See ADR 0016 (amendment).
    #[cfg_attr(feature = "serde", serde(rename = "duplicate-word"))]
    DuplicateWord { first_sid: String },
    /// `uni.rare-glyph`: the flagged letter and how many times it occurs in the
    /// whole translation (ADR 0053) — the plain rarity fact behind the score.
    /// The flagged occurrence is in the finding's `range`; `count` is the
    /// corpus-wide eligible (single-script letter-token) occurrence count.
    #[cfg_attr(feature = "serde", serde(rename = "rare-glyph"))]
    RareGlyph { glyph: char, count: u32 },
    /// `case.mixed-case-word`: the flagged word's corpus-wide OtherMixed-vs-total
    /// counts, so the consumer can render "this translation writes ‘dios’ with an
    /// interior capital 1 of 41 places" — the descriptive rate behind the
    /// Wilson-bound `score` (ADR 0048, 0055). The flagged occurrence (the mixed
    /// form) is in the finding's `range`; `word` is the case-folded key, `other`
    /// the mixed-shape count, `total` all cased occurrences of the word.
    #[cfg_attr(feature = "serde", serde(rename = "mixed-case-word"))]
    MixedCaseWord {
        word: String,
        other: u32,
        total: u32,
    },
    /// `uni.mixed-normalization`: the translation writes canonically
    /// equivalent grapheme clusters in more than one raw Unicode form (ADR
    /// 0063) — e.g. precomposed `é` in one place, `e` + COMBINING ACUTE in
    /// another. `example` is the anchor's NFC key: a `String`, not `char`,
    /// because composition exclusions and multi-mark clusters can make it
    /// more than one scalar. `affected` sums the minority-form occurrence
    /// count across every mixed key in the corpus, not just the anchor's.
    #[cfg_attr(feature = "serde", serde(rename = "normalization"))]
    Normalization { affected: u32, example: String },
}

/// One addressable content finding in one verse.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Finding {
    pub key_idx: KeyIdx,
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

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;

    /// `SpacingSide`'s `form`/`class` were `String` until 2026-07-27, so their
    /// JSON strings are a **shipped** surface with consumers already keying off
    /// them (the editor's message layer, the packed-wire vectors, every pinned
    /// oracle dump). Pin all six form × class combinations as exact bytes,
    /// including field order: the fleet happens to exercise all six today, but
    /// that is a property of the corpora, not a guarantee, and an inferred
    /// rename convention must never be what keeps this surface stable.
    #[test]
    fn spacing_side_pins_all_six_form_class_combinations_as_exact_bytes() {
        let cases = [
            (
                SpacingForm::Attached,
                SpacingClass::Letter,
                r#"{"form":"attached","class":"letter","count":1,"total":1053}"#,
            ),
            (
                SpacingForm::Attached,
                SpacingClass::Number,
                r#"{"form":"attached","class":"number","count":1,"total":1053}"#,
            ),
            (
                SpacingForm::Attached,
                SpacingClass::Punct,
                r#"{"form":"attached","class":"punct","count":1,"total":1053}"#,
            ),
            (
                SpacingForm::Spaced,
                SpacingClass::Letter,
                r#"{"form":"spaced","class":"letter","count":1,"total":1053}"#,
            ),
            (
                SpacingForm::Spaced,
                SpacingClass::Number,
                r#"{"form":"spaced","class":"number","count":1,"total":1053}"#,
            ),
            (
                SpacingForm::Spaced,
                SpacingClass::Punct,
                r#"{"form":"spaced","class":"punct","count":1,"total":1053}"#,
            ),
        ];
        for (form, class, want) in cases {
            let side = SpacingSide {
                form,
                class,
                count: 1,
                total: 1053,
            };
            assert_eq!(
                serde_json::to_string(&side).unwrap(),
                want,
                "{form:?}/{class:?} must serialize byte-for-byte as the shipped strings"
            );
            // Round-trip too: the args are `Deserialize` for the oracle/wire
            // vectors, so a rename that only went one way would still be a bug.
            assert_eq!(
                serde_json::from_str::<SpacingSide>(want).unwrap(),
                side,
                "{form:?}/{class:?} must deserialize from the shipped strings"
            );
        }
    }

    /// The whole `spacing-convention` args object, both sides present, pinned as
    /// exact bytes — the tagged-union `kind`, the `mark` scalar, and both nested
    /// sides in field order. This is the shape the oracle dumps carry.
    #[test]
    fn spacing_convention_args_pin_the_full_object_bytes() {
        let args = FindingArgs::SpacingConvention {
            mark: ',',
            left: Some(SpacingSide {
                form: SpacingForm::Attached,
                class: SpacingClass::Punct,
                count: 3,
                total: 97,
            }),
            right: Some(SpacingSide {
                form: SpacingForm::Spaced,
                class: SpacingClass::Number,
                count: 2,
                total: 41,
            }),
        };
        assert_eq!(
            serde_json::to_string(&args).unwrap(),
            r#"{"kind":"spacing-convention","mark":",","left":{"form":"attached","class":"punct","count":3,"total":97},"right":{"form":"spaced","class":"number","count":2,"total":41}}"#
        );
    }
}
