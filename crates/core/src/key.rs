//! Corpus key grammar (finding-address-representation plan, Step 1b).
//!
//! The engine does not parse chapter or verse numbers, but it does need
//! enough shape to find the book and chapter boundary existing rules key
//! off (duplicate-word's chapter gate, proportionality's book grouping,
//! `Corpus`'s contiguous-book-block invariant). An accepted key has this
//! exact shape:
//!
//! ```text
//! <nonempty book slug><ASCII space><nonempty chapter token>:<nonempty verse token>
//! ```
//!
//! Split at the **last** ASCII space (permits a spaced slug such as
//! `1 corinthians 1:1`), then split the remaining address at the **first**
//! `:`. Neither token is trimmed, normalized, case-folded, or parsed as a
//! number — the verse token is opaque, so `1a`, `2-3`, and duplicates are
//! all valid. `.` is not an alternate chapter separator in this pass;
//! broadening the grammar is unrelated to this change.

use std::fmt;

/// The three slash-free slices of an accepted key, borrowed from the
/// original `&str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyParts<'a> {
    pub book: &'a str,
    pub chapter: &'a str,
    pub verse: &'a str,
}

/// Why a key failed the grammar in [`parse_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    MissingBookSeparator,
    EmptyBook,
    MissingChapterSeparator,
    EmptyChapter,
    EmptyVerse,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            KeyError::MissingBookSeparator => "key has no ASCII space separating book from address",
            KeyError::EmptyBook => "key's book slug is empty",
            KeyError::MissingChapterSeparator => {
                "key's address has no ':' separating chapter from verse"
            }
            KeyError::EmptyChapter => "key's chapter token is empty",
            KeyError::EmptyVerse => "key's verse token is empty",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for KeyError {}

/// Parse a corpus key into its book/chapter/verse parts. See the module
/// docs for the exact grammar; this does not validate book-code membership
/// in a canon or parse chapter/verse as numbers.
pub fn parse_key(key: &str) -> Result<KeyParts<'_>, KeyError> {
    let space = key.rfind(' ').ok_or(KeyError::MissingBookSeparator)?;
    let (book, address_with_space) = key.split_at(space);
    let address = &address_with_space[1..];
    if book.is_empty() {
        return Err(KeyError::EmptyBook);
    }
    let (chapter, verse) = address
        .split_once(':')
        .ok_or(KeyError::MissingChapterSeparator)?;
    if chapter.is_empty() {
        return Err(KeyError::EmptyChapter);
    }
    if verse.is_empty() {
        return Err(KeyError::EmptyVerse);
    }
    Ok(KeyParts {
        book,
        chapter,
        verse,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_key() {
        let p = parse_key("GEN 1:1").unwrap();
        assert_eq!(
            p,
            KeyParts {
                book: "GEN",
                chapter: "1",
                verse: "1"
            }
        );
    }

    #[test]
    fn opaque_sub_verse_token() {
        let p = parse_key("GEN 1:1a").unwrap();
        assert_eq!(p.verse, "1a");
    }

    #[test]
    fn spaced_book_slug_splits_at_last_space() {
        let p = parse_key("1 corinthians 3:8").unwrap();
        assert_eq!(
            p,
            KeyParts {
                book: "1 corinthians",
                chapter: "3",
                verse: "8"
            }
        );
    }

    #[test]
    fn duplicate_input_parses_identically_each_time() {
        assert_eq!(parse_key("GEN 1:1"), parse_key("GEN 1:1"));
    }

    #[test]
    fn missing_space_is_an_error() {
        assert_eq!(parse_key("GEN1:1"), Err(KeyError::MissingBookSeparator));
    }

    #[test]
    fn empty_book_is_an_error() {
        assert_eq!(parse_key(" 1:1"), Err(KeyError::EmptyBook));
    }

    #[test]
    fn missing_colon_is_an_error() {
        assert_eq!(parse_key("GEN 11"), Err(KeyError::MissingChapterSeparator));
    }

    #[test]
    fn empty_chapter_is_an_error() {
        assert_eq!(parse_key("GEN :1"), Err(KeyError::EmptyChapter));
    }

    #[test]
    fn empty_verse_is_an_error() {
        assert_eq!(parse_key("GEN 1:"), Err(KeyError::EmptyVerse));
    }
}
