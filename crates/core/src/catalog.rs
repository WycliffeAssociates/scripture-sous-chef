//! The rule catalog — the human-facing card for every rule (ADR 0038).
//!
//! `RuleId` is the typed config and localization surface (ADR 0012); this
//! module is the shipped **English reference text** for that surface: what a
//! finding is, why it might deserve an eyeball, the plain-language question
//! behind every language-dependent on/off toggle, and the one sensitivity
//! dial. Consumers render these directly or key translations off `code`.
//!
//! Wording principles (the product voice — hold the line in edits):
//!
//! - **The translation is the authority, never "the language".** The
//!   corpus-relative rules compare text to *this translation's own
//!   patterns*; the text never accuses anyone of writing their language
//!   wrong. Say "this translation almost never does X", not "X is wrong".
//! - **"Worth an eyeball", not "error".** These are tiny, in-progress
//!   corpora of minority languages; most findings are invitations to look,
//!   not verdicts. Reserve firm wording for genuine file damage (NUL runs,
//!   `???` runs, conflict markers) where the verdict really is mechanical.
//! - **No statistics vocabulary.** The dial is "how unusual before we show
//!   it", never "Wilson lower bound"; the advanced knobs stay documented in
//!   `config.md` for calibrators, not here.

use crate::diagnostics::RuleId;

/// How a rule's findings are decided — drives which caption a UI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum Verdict {
    /// Fires on a fixed, mechanical condition; no score, no dial.
    Deterministic,
    /// Judged against this translation's own patterns; carries a score in
    /// `[0, 1]` and honours the sensitivity dial (`emit_score_min`).
    CorpusRelative,
    /// Judged against the paired source text.
    SourceRelative,
}

/// One rule's human-facing card.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuleCard {
    pub code: RuleId,
    /// Short plain name, list-friendly. "Doubled word", not "lex.duplicate-word".
    pub title: &'static str,
    /// What one finding *is*, one sentence, plain words.
    pub what: &'static str,
    /// Why it might deserve an eyeball, one sentence.
    pub why: &'static str,
    /// For language-dependent rules (usually default-off): the question a
    /// translator answers to decide the toggle. `None` = the rule isn't a
    /// language question and the default is right for everyone.
    pub enable_question: Option<&'static str>,
    pub verdict: Verdict,
}

/// The sensitivity dial's labelled stops, shared by every corpus-relative
/// rule (they all emit the same score unit — anomaly evidence — so one set
/// of words serves all of them). Values are `emit_score_min` settings;
/// higher = fewer, surer findings.
pub const SENSITIVITY_STOPS: &[(f32, &str)] = &[
    (0.9, "Only what this translation almost never does"),
    (0.7, "Unusual for this translation"),
    (0.5, "Anything even moderately unusual"),
];

/// The full catalog, one card per rule, in `RuleId::ALL` order. Complete by
/// construction — the exhaustive match fails to compile when a rule is added
/// without a card.
pub fn rule_cards() -> Vec<RuleCard> {
    RuleId::ALL.iter().map(|&id| card(id)).collect()
}

/// The card for one rule.
pub fn card(id: RuleId) -> RuleCard {
    use Verdict::*;
    let (title, what, why, enable_question, verdict) = match id {
        RuleId::ExcessHWhitespace => (
            "Doubled spaces",
            "Two or more spaces in a row inside a sentence — including invisible no-break spaces.",
            "Usually a stray keystroke or a paste artifact. Double spacing after a sentence-ending mark is a real convention and is left alone.",
            None,
            Deterministic,
        ),
        RuleId::TabInBody => (
            "Tab character in verse text",
            "A tab character inside the verse.",
            "Tabs are file formatting, not text — they usually leak in from a spreadsheet or an export.",
            None,
            Deterministic,
        ),
        RuleId::ControlChars => (
            "Invisible control characters",
            "A run of invisible control characters (such as NUL bytes) in the verse.",
            "File-damage artifacts: invisible to readers but able to break other tools. One finding covers each run.",
            None,
            Deterministic,
        ),
        RuleId::ZeroWidthMisuse => (
            "Stray invisible formatting character",
            "A byte-order mark, directional mark, or similar invisible control inside verse text.",
            "Almost always pasted in by accident; these can silently reorder or glue together what readers see.",
            None,
            Deterministic,
        ),
        RuleId::EmptyVerse => (
            "Empty verse",
            "A verse with no text in it.",
            "Sometimes intentional (verse bridges are real); worth confirming it isn't lost text.",
            None,
            Deterministic,
        ),
        RuleId::InvalidCodepoint => (
            "Broken character",
            "A character that cannot appear in valid text — like the \u{FFFD} replacement mark.",
            "A sure sign the file was damaged in a conversion; the original character is gone.",
            None,
            Deterministic,
        ),
        RuleId::ReplacementRun => (
            "Destroyed text (??? runs)",
            "A run of three or more question marks.",
            "This is what a failed encoding conversion leaves where words used to be. The original text is likely lost and needs re-importing from a good copy.",
            None,
            Deterministic,
        ),
        RuleId::ProjectLengthRatio => (
            "Verse length far from its source",
            "A verse much longer or shorter than the same verse in the source translation.",
            "Can point to an omission, an addition, or verses merged differently — or just a free rendering; worth a look either way.",
            None,
            SourceRelative,
        ),
        RuleId::SourceMarkerLeftover => (
            "Leftover markup",
            "A piece of USFM or HTML markup sitting inside the verse text.",
            "Markup belongs to the file format, not the translation; readers would see it verbatim.",
            None,
            Deterministic,
        ),
        RuleId::MergeConflictMarker => (
            "Merge-conflict leftovers",
            "Version-control conflict markers (like <<<<<<<) committed into the text.",
            "A merge was saved half-finished; both versions of the passage are probably still in the file.",
            None,
            Deterministic,
        ),
        RuleId::PunctuationAdjacencyAnomaly => (
            "Unusual punctuation combination",
            "Punctuation doubled or combined in a way this translation almost never writes — a one-off \u{201C}?.\u{201D} or \u{201C},,\u{201D}.",
            "Your own text defines what's normal: patterns it uses throughout (doubled marks, local conventions) are respected; the flagged ones stand alone against that habit.",
            None,
            CorpusRelative,
        ),
        RuleId::DuplicateWord => (
            "Doubled word",
            "The same word twice in a row with only space between (\u{201C}the the\u{201D}).",
            "In languages that don't repeat words as grammar, this is a near-certain typo.",
            Some(
                "Does your language repeat words on purpose — for emphasis, plurals, or meanings like \u{201C}very\u{201D}? If yes, leave this off; if doubling would always be a mistake, turn it on.",
            ),
            Deterministic,
        ),
        RuleId::PunctOnlyToken => (
            "Stranded punctuation",
            "Punctuation standing alone between words, in a pattern this translation doesn't otherwise use.",
            "Detached marks the text uses everywhere (a spaced danda, quotation styles) are respected as house style; a lone stray is usually debris from editing.",
            None,
            CorpusRelative,
        ),
        RuleId::CombiningMarkWithoutBase => (
            "Accent with nothing to attach to",
            "A combining accent or vowel sign with no letter in front of it — after a space or punctuation.",
            "Accents ride on letters; a detached one usually means a letter was deleted or a space crept in.",
            None,
            Deterministic,
        ),
        RuleId::RedundantZeroWidthSpace => (
            "Doubled invisible word-break",
            "The same invisible word-break character (ZWSP) typed twice in a row.",
            "One is all any writing system ever needs; doubles are typing or paste artifacts.",
            None,
            Deterministic,
        ),
        RuleId::MixedScriptInToken => (
            "Mixed alphabets in one word",
            "A word mixing letters from two writing systems — like a Latin \u{201C}o\u{201D} inside a word in another script.",
            "Usually a look-alike character from the wrong keyboard layout; it reads fine on screen and breaks searching and sorting.",
            None,
            Deterministic,
        ),
        RuleId::RepeatedCharacterRun => (
            "Repeated letter",
            "A letter repeated three or more times (\u{201C}joyfullly\u{201D}) where neither that repetition nor the word is something this translation does elsewhere.",
            "Long vowels, ideophones, and stretched words are real in many languages — and they recur; this only surfaces repetitions your own text doesn't back up.",
            None,
            CorpusRelative,
        ),
        RuleId::MixedNumeralSystems => (
            "Mixed number systems",
            "Digits from two different numbering systems inside one verse.",
            "Usually one number was typed with the wrong keyboard; the odd one out is flagged.",
            None,
            Deterministic,
        ),
        RuleId::BracketBalance => (
            "Unmatched bracket",
            "An opening or closing bracket with no partner — in a bracket family this translation reliably pairs.",
            "Judged against your own pairing habits: a symbol your text never pairs (some orthographies use bracket shapes as letters) is left alone.",
            None,
            CorpusRelative,
        ),
        RuleId::PunctuationSpacingAnomaly => (
            "Inconsistent spacing around punctuation",
            "A mark spaced away from (or attached to) its word, the opposite way from how this translation usually writes that mark.",
            "Pure consistency review against your own dominant habit — expect many results if the text genuinely mixes both forms.",
            Some(
                "Turn this on when you want a spacing-consistency pass; it lists every occurrence written against your majority style, which can be a long list in a mixed text.",
            ),
            CorpusRelative,
        ),
        RuleId::SentenceInitialLowercase => (
            "Lowercase sentence start",
            "A lowercase word right after a mark this translation reliably follows with a capital.",
            "Only speaks up where your own text has established the capital-after-this-mark habit; caseless scripts and mixed-habit texts stay silent.",
            Some(
                "Does your writing system use capital letters, and does your translation capitalise after sentence breaks? Turn this on only if both are yes.",
            ),
            CorpusRelative,
        ),
    };
    RuleCard {
        code: id,
        title,
        what,
        why,
        enable_question,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_has_a_card_with_real_text() {
        let cards = rule_cards();
        assert_eq!(cards.len(), RuleId::ALL.len());
        for c in &cards {
            assert!(!c.title.is_empty() && !c.what.is_empty() && !c.why.is_empty());
            // The one-liners stay one-liners: list-renderable, no headings.
            assert!(!c.what.contains('\n') && !c.why.contains('\n'), "{}", c.code);
        }
    }

    #[test]
    fn corpus_relative_cards_match_the_scored_rules() {
        // The verdict tag is UI-load-bearing (it selects the sensitivity
        // dial); pin the set so a recast updates the card too.
        let scored: Vec<RuleId> = rule_cards()
            .iter()
            .filter(|c| c.verdict == Verdict::CorpusRelative)
            .map(|c| c.code)
            .collect();
        assert_eq!(
            scored,
            vec![
                RuleId::PunctuationAdjacencyAnomaly,
                RuleId::PunctOnlyToken,
                RuleId::RepeatedCharacterRun,
                RuleId::BracketBalance,
                RuleId::PunctuationSpacingAnomaly,
                RuleId::SentenceInitialLowercase,
            ]
        );
    }

    #[test]
    fn sensitivity_stops_are_descending_and_in_range() {
        let mut prev = 1.0f32;
        for &(v, label) in SENSITIVITY_STOPS {
            assert!(v < prev && v > 0.0 && !label.is_empty());
            prev = v;
        }
    }
}
