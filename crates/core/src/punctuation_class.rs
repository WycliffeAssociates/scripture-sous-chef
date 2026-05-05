//! Single source of truth for how punctuation behaves with respect to
//! enclosed spans (quotes, brackets) and surrounding whitespace.
//!
//! The taxonomy is borrowed from SIL's `silnlp/common/normalizer.py`
//! (see `research/sil_audit.md` §1.4): every tracked punctuation
//! character has a *clinging class* — left-clinging marks bind to the
//! token following them (and want whitespace before), right-clinging
//! marks bind to the token preceding them (and want whitespace after),
//! and a small set of symmetric marks (`"`, `'`) classify dynamically
//! by surrounding whitespace.
//!
//! Two design notes:
//!
//! 1. Pair information lives on the `LeftClinging` variant directly
//!    (`closers: &[char]`). This keeps the matching table and the
//!    classification table from drifting — there is only one table.
//!    Most openers have a single closer; `\u{301D}` (Japanese
//!    double-prime quote) is the rare case that legitimately closes
//!    with either `\u{301E}` or `\u{301F}`.
//!
//! 2. The table is curated, not auto-derived from Unicode General
//!    Category. The earlier `is_open_punctuation` / `is_close_punctuation`
//!    predicates were Ps/Pi / Pe/Pf approximations, which over-recognized
//!    exotic mathematical brackets that never had matching partners in
//!    our pair-matching table — every occurrence produced a spurious
//!    `MismatchedClose`. Only chars we can confidently pair are tracked.

/// Classification of a single punctuation character.
///
/// The variants encode both *spacing convention* (left-clinging marks
/// want whitespace on the left, etc.) and, where applicable, *span
/// role* (does this char open or close an enclosed span). The two
/// axes mostly align — span closers are right-clinging, span openers
/// are left-clinging — but sentence terminators (`,`, `.`, `;`, `:`,
/// `!`, `?`) are right-clinging without ever closing a span, so they
/// get their own variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClingingClass {
    /// Opener of an enclosed span. `closers` lists every char that
    /// can validly close it (usually just one).
    /// Left-clinging spacing-wise.
    LeftClinging { closers: &'static [char] },
    /// Closer of an enclosed span. Pair lookup is performed from the
    /// opener side (the resolver always has the opener on its stack
    /// when popping), so closers don't carry inverse pointers.
    /// Right-clinging spacing-wise.
    RightClinging,
    /// Right-clinging spacing-wise but never closes a span:
    /// `,`, `.`, `;`, `:`, `!`, `?`. The resolver ignores these for
    /// span tracking; they exist in the table so spacing-aware
    /// heuristics (boundary detection for ambiguous quotes, future
    /// space-around-punct rules) can read one source of truth.
    Terminal,
    /// Em / en dashes — spaces both sides, never affect span tracking.
    LeftRightClinging,
    /// Symmetric mark whose role (open vs close) is decided by
    /// surrounding whitespace at use site. Used for ASCII straight
    /// quotes `"` and apostrophe `'`.
    AmbiguousSymmetric,
}

/// Classify a single character. Returns `None` for characters that do
/// not participate in the span / spacing taxonomy at all.
pub fn clinging_class(c: char) -> Option<ClingingClass> {
    use ClingingClass::*;

    // Common ASCII pairs.
    Some(match c {
        '(' => LeftClinging { closers: &[')'] },
        '[' => LeftClinging { closers: &[']'] },
        '{' => LeftClinging { closers: &['}'] },
        ')' | ']' | '}' => RightClinging,

        // Curly double quotes. Both LEFT DOUBLE and DOUBLE-LOW-9
        // openers close with the same RIGHT DOUBLE — the German
        // „..." style and English "..." style share a closer.
        '\u{201C}' => LeftClinging { closers: &['\u{201D}'] },
        '\u{201E}' => LeftClinging { closers: &['\u{201D}'] },
        '\u{201D}' => RightClinging,

        // Guillemets.
        '\u{00AB}' => LeftClinging { closers: &['\u{00BB}'] },
        '\u{00BB}' => RightClinging,
        '\u{2039}' => LeftClinging { closers: &['\u{203A}'] },
        '\u{203A}' => RightClinging,

        // CJK brackets.
        '\u{300C}' => LeftClinging { closers: &['\u{300D}'] },
        '\u{300D}' => RightClinging,
        '\u{300E}' => LeftClinging { closers: &['\u{300F}'] },
        '\u{300F}' => RightClinging,
        '\u{3008}' => LeftClinging { closers: &['\u{3009}'] },
        '\u{3009}' => RightClinging,
        '\u{300A}' => LeftClinging { closers: &['\u{300B}'] },
        '\u{300B}' => RightClinging,
        '\u{3010}' => LeftClinging { closers: &['\u{3011}'] },
        '\u{3011}' => RightClinging,

        // Japanese double-prime quotes — opener closes with either
        // of two distinct codepoints in real-world data.
        '\u{301D}' => LeftClinging { closers: &['\u{301E}', '\u{301F}'] },
        '\u{301E}' | '\u{301F}' => RightClinging,

        // Halfwidth corner brackets.
        '\u{FF62}' => LeftClinging { closers: &['\u{FF63}'] },
        '\u{FF63}' => RightClinging,

        // Em / en dashes — spaces-both-sides convention.
        '\u{2014}' | '\u{2013}' => LeftRightClinging,

        // Sentence terminators / medials — right-clinging spacing,
        // but no span role.
        ',' | '.' | ';' | ':' | '!' | '?' => Terminal,

        // ASCII straight quote and ASCII apostrophe — symmetric;
        // role decided per-occurrence by `resolve_ambiguous` based
        // on surrounding whitespace. Apostrophe-as-possessive
        // (`John's`) falls out of the same rule because it has
        // non-whitespace on both sides ⇒ `Internal` ⇒ skipped.
        '"' | '\'' => AmbiguousSymmetric,

        _ => return None,
    })
}

/// Resolution of an `AmbiguousSymmetric` occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbiguousResolution {
    /// Leading whitespace, trailing non-whitespace ⇒ acts as opener.
    OpensSpan,
    /// Leading non-whitespace, trailing whitespace ⇒ acts as closer.
    ClosesSpan,
    /// Both sides non-whitespace. Word-internal — apostrophe in a
    /// contraction or possessive, or a stray quote glued to a word.
    /// Never affects the span stack.
    Internal,
    /// Both sides whitespace (or boundary). Genuinely ambiguous;
    /// caller may fall back to a stack toggle.
    Unresolved,
}

/// Classify an `AmbiguousSymmetric` occurrence by its surrounding
/// characters. `prev` and `next` are the immediately adjacent chars
/// (or `None` at the start / end of input).
///
/// The decision turns on whether each side is a *content character*
/// — alphanumeric, i.e. a letter or digit. If a quote is glued to
/// the start of a word, it opens; if glued to the end of a word, it
/// closes; if glued on both sides (like the apostrophe in `John's`),
/// it's word-internal and skipped; if neither side is a content
/// character, the call is genuinely ambiguous and the resolver falls
/// back to a stack toggle.
///
/// Why content rather than whitespace: nested quotation makes
/// adjacent symmetric chars common (`die.'"'"` closes three levels).
/// A whitespace-only test would mark the inner `'` as Internal and
/// skip it. Content-vs-non-content correctly routes those cases to
/// `Unresolved`, where the stack toggle resolves them via LIFO order.
pub fn resolve_ambiguous(prev: Option<char>, next: Option<char>) -> AmbiguousResolution {
    let prev_content = prev.is_some_and(|c| c.is_alphanumeric());
    let next_content = next.is_some_and(|c| c.is_alphanumeric());
    match (prev_content, next_content) {
        (true, true) => AmbiguousResolution::Internal,
        (false, true) => AmbiguousResolution::OpensSpan,
        (true, false) => AmbiguousResolution::ClosesSpan,
        (false, false) => AmbiguousResolution::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every closer named in some `LeftClinging { closers }` entry
    /// must itself classify as `RightClinging`. Catches table-edit
    /// drift the moment tests run.
    #[test]
    fn closers_classify_as_right_clinging() {
        // Walk every codepoint we explicitly handle. Cheap — the
        // table is small. A range scan is overkill but proves the
        // invariant for any future additions without a registry.
        for c in '\u{0000}'..='\u{FFFF}' {
            if let Some(ClingingClass::LeftClinging { closers }) = clinging_class(c) {
                for &closer in closers {
                    assert_eq!(
                        clinging_class(closer),
                        Some(ClingingClass::RightClinging),
                        "closer U+{:04X} of opener U+{:04X} is not RightClinging",
                        closer as u32,
                        c as u32,
                    );
                }
            }
        }
    }

    #[test]
    fn ascii_brackets_pair() {
        assert!(matches!(
            clinging_class('('),
            Some(ClingingClass::LeftClinging { closers: &[')'] })
        ));
        assert_eq!(clinging_class(')'), Some(ClingingClass::RightClinging));
    }

    #[test]
    fn ambiguous_resolves_by_content_neighborhood() {
        // Opener: non-content before, content after.
        assert_eq!(
            resolve_ambiguous(Some(' '), Some('h')),
            AmbiguousResolution::OpensSpan
        );
        // Quote after `(`: still glued to start of content.
        assert_eq!(
            resolve_ambiguous(Some('('), Some('h')),
            AmbiguousResolution::OpensSpan
        );
        // Closer: content before, non-content after.
        assert_eq!(
            resolve_ambiguous(Some('o'), Some(' ')),
            AmbiguousResolution::ClosesSpan
        );
        // Quote before `?`: glued to end of content.
        assert_eq!(
            resolve_ambiguous(Some('o'), Some('?')),
            AmbiguousResolution::ClosesSpan
        );
        // Apostrophe as possessive: letter both sides.
        assert_eq!(
            resolve_ambiguous(Some('n'), Some('s')),
            AmbiguousResolution::Internal
        );
        // Nested closer like `."'"`: punct on both sides — caller
        // falls back to the stack toggle, which has the LIFO state
        // needed to resolve the nesting.
        assert_eq!(
            resolve_ambiguous(Some('.'), Some('"')),
            AmbiguousResolution::Unresolved
        );
        // Both boundaries (start and end of input).
        assert_eq!(
            resolve_ambiguous(None, None),
            AmbiguousResolution::Unresolved
        );
    }

    #[test]
    fn straight_quote_and_apostrophe_are_ambiguous() {
        assert_eq!(clinging_class('"'), Some(ClingingClass::AmbiguousSymmetric));
        assert_eq!(clinging_class('\''), Some(ClingingClass::AmbiguousSymmetric));
    }

    #[test]
    fn em_and_en_dash_are_left_right_clinging() {
        assert_eq!(clinging_class('\u{2014}'), Some(ClingingClass::LeftRightClinging));
        assert_eq!(clinging_class('\u{2013}'), Some(ClingingClass::LeftRightClinging));
    }
}
