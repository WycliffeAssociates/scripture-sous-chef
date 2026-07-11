//! Shared case-shape classification for the casing family (ADR 0055).
//!
//! Three rules read a word's *case shape* and must agree on what the shapes
//! are, so the definition lives here once instead of in three private copies:
//!
//! - **`case.mixed-case-word`** (ADR 0055) consumes [`CaseShape::OtherMixed`] —
//!   a word with an *interior* capital (`wOrd`, `DIos`, `LORDs`): has both cases
//!   and is neither Titlecase nor ALLCAPS.
//! - **`case.sentence-initial-lowercase` / `case.inconsistent-word-casing`**
//!   (ADR 0051/0052) use [`OtherMixed`](CaseShape::OtherMixed) as a *skip*
//!   predicate: an OtherMixed token is mixed-case's to report (one phenomenon,
//!   one finding), so casing's lowercase-site rules step over it.
//! - **`uni.rare-glyph`** (ADR 0053) asks a looser "is this a name-shaped
//!   container?" via [`is_titlecase_name`] — upper first **plus** ≥1 lowercase,
//!   which is deliberately broader than [`CaseShape::Title`]: it also admits
//!   OtherMixed names like `McDonald`/`HaMelech`. Rare-glyph only needs "does
//!   the rare glyph sit in a capitalized name?" to excuse the glyph; mixed-case
//!   needs the finer "is the *interior* irregular?". The two are intentionally
//!   different, not accidentally divergent — that difference is the reason this
//!   helper exposes both the four-way shape and the `is_titlecase_name`
//!   predicate.

use crate::charclass::class_of;

/// A word's observed case shape over its **cased** letters (combining marks and
/// caseless letters ignored, so an intra-word caseless glyph cannot manufacture
/// a shape). `None` from [`case_shape`] means no cased letter at all (a caseless
/// script / marks-only token): no shape, not a candidate for anything here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseShape {
    /// All cased letters lowercase (incl. a lone `a`).
    Lower,
    /// First cased letter upper, all the rest lower (the strict Titlecase —
    /// `Word`). A lone `I`/`A` is [`AllCaps`](CaseShape::AllCaps), never this.
    Title,
    /// All cased letters upper (incl. a lone `I`; pure `LORD`).
    AllCaps,
    /// Has both cases and is neither `Title` nor `AllCaps` — so it necessarily
    /// carries an **interior** capital (an uppercase letter that is not the
    /// first cased letter). This is the `wOrd` phenomenon.
    OtherMixed,
}

/// Classify a word by the case sequence of its **cased** letters. `None` = no
/// cased letter (caseless / marks only). A single cased letter is `Lower` or
/// `AllCaps`, never `OtherMixed` — the single-letter guard, so a lone `I`/`A`
/// does not read as mixed.
pub fn case_shape(word: &str) -> Option<CaseShape> {
    // `first` = the case of the first cased letter; `up`/`n` count the rest.
    let mut first: Option<bool> = None;
    let mut up = 0usize;
    let mut n = 0usize;
    // `interior_upper` — an uppercase letter after the first cased letter.
    let mut interior_upper = false;
    for c in word.chars() {
        let cl = class_of(c);
        let this = if cl.is_uppercase() {
            true
        } else if cl.is_lowercase() {
            false
        } else {
            continue;
        };
        if first.is_none() {
            first = Some(this);
        } else if this {
            interior_upper = true;
        }
        n += 1;
        if this {
            up += 1;
        }
    }
    let first = first?;
    Some(if up == 0 {
        CaseShape::Lower
    } else if up == n {
        CaseShape::AllCaps
    } else if first && !interior_upper {
        CaseShape::Title
    } else {
        CaseShape::OtherMixed
    })
}

/// The rare-glyph "name-shaped container" predicate (ADR 0053): the word's first
/// character is uppercase **and** it has ≥1 lowercase letter. Equivalent to
/// "first char upper and shape is `Title` or `OtherMixed`" — broader than
/// [`CaseShape::Title`] on purpose (it admits `McDonald`, `HaMelech`), and it
/// correctly excludes lone capitals (`Q`) and all-caps forms (`YÖ`), which are
/// capital-initial but carry no lowercase.
pub fn is_titlecase_name(word: &str) -> bool {
    let starts_upper = word.chars().next().is_some_and(|c| class_of(c).is_uppercase());
    starts_upper && matches!(case_shape(word), Some(CaseShape::Title | CaseShape::OtherMixed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_shapes() {
        assert_eq!(case_shape("word"), Some(CaseShape::Lower));
        assert_eq!(case_shape("Word"), Some(CaseShape::Title));
        assert_eq!(case_shape("WORD"), Some(CaseShape::AllCaps));
        assert_eq!(case_shape("wOrd"), Some(CaseShape::OtherMixed));
    }

    #[test]
    fn single_letter_is_never_mixed() {
        assert_eq!(case_shape("I"), Some(CaseShape::AllCaps));
        assert_eq!(case_shape("a"), Some(CaseShape::Lower));
    }

    #[test]
    fn caseless_has_no_shape() {
        assert_eq!(case_shape("好"), None);
        assert_eq!(case_shape("123"), None);
    }

    #[test]
    fn convention_shapes_are_othermixed() {
        assert_eq!(case_shape("McDonald"), Some(CaseShape::OtherMixed));
        assert_eq!(case_shape("kiSwahili"), Some(CaseShape::OtherMixed));
        assert_eq!(case_shape("LORDs"), Some(CaseShape::OtherMixed));
        // Pure ALLCAPS YHWH stays AllCaps — not a mixed candidate at all.
        assert_eq!(case_shape("LORD"), Some(CaseShape::AllCaps));
    }

    #[test]
    fn combining_marks_and_caseless_do_not_manufacture_mixing() {
        // Base + combining acute (decomposed é): still Lower.
        assert_eq!(case_shape("cafe\u{0301}"), Some(CaseShape::Lower));
        // Title with a trailing combining mark stays Title.
        assert_eq!(case_shape("A\u{0301}bc"), Some(CaseShape::Title));
    }

    #[test]
    fn titlecase_name_is_looser_than_title() {
        // Strict Title and OtherMixed-with-upper-first are both name-shaped …
        assert!(is_titlecase_name("Word"));
        assert!(is_titlecase_name("McDonald"));
        assert!(is_titlecase_name("HaMelech"));
        // … but lower-first, all-caps, and lone capitals are not.
        assert!(!is_titlecase_name("wOrd"));
        assert!(!is_titlecase_name("word"));
        assert!(!is_titlecase_name("YÖ"));
        assert!(!is_titlecase_name("Q"));
    }
}
