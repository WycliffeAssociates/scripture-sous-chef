//! Scripture identifiers. `Sid` is `Copy` and 6 bytes — cheaper than a
//! `String` of `"GEN 1:1"` and equally meaningful.

use std::fmt;

/// Three-letter USFM book code (e.g. `GEN`, `JHN`, `REV`). Stored as raw
/// bytes so the type is `Copy` and equality is a single 24-bit compare.
/// Validation (membership in the 66-book canon) is the ingest layer's job.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BookId(pub [u8; 3]);

impl BookId {
    pub const fn from_bytes(b: [u8; 3]) -> Self {
        Self(b)
    }

    pub fn as_str(&self) -> &str {
        // Caller-side invariant: BookId always holds ASCII. Constructed via
        // `from_str` (validated) or `from_bytes` (caller asserts ASCII).
        std::str::from_utf8(&self.0).unwrap_or("???")
    }

    /// Parse a 3-character ASCII USFM book code. Returns `None` for any
    /// other length or non-ASCII content.
    // Deliberately named `from_str` to read as a parser, but returns `Option`
    // (an invalid book code is not an error worth a dedicated `Err` type), so
    // it does not implement `std::str::FromStr`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 3 || !b.iter().all(|c| c.is_ascii()) {
            return None;
        }
        Some(Self([b[0], b[1], b[2]]))
    }
}

impl fmt::Debug for BookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BookId({})", self.as_str())
    }
}

impl fmt::Display for BookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Scripture verse identifier. 6 bytes, `Copy`, hashable, totally ordered
/// by (book, chapter, verse).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Sid {
    pub book: BookId,
    pub chapter: u16,
    pub verse: u16,
}

impl Sid {
    pub const fn new(book: BookId, chapter: u16, verse: u16) -> Self {
        Self {
            book,
            chapter,
            verse,
        }
    }

    /// Parse `"GEN 1:1"` or `"GEN 1.1"`. Lenient on whitespace and the
    /// chapter/verse separator; strict on the book code length.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split(|c: char| c.is_whitespace() || c == '_');
        let book_s = parts.next()?;
        let cv = parts.next()?;
        let book = BookId::from_str(book_s)?;
        let mut cv_iter = cv.split([':', '.']);
        let ch: u16 = cv_iter.next()?.parse().ok()?;
        let vs: u16 = cv_iter.next()?.parse().ok()?;
        Some(Self::new(book, ch, vs))
    }
}

impl fmt::Debug for Sid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Sid({} {}:{})",
            self.book.as_str(),
            self.chapter,
            self.verse
        )
    }
}

impl fmt::Display for Sid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book.as_str(), self.chapter, self.verse)
    }
}

/// Serialize a [`Sid`] as its canonical `"GEN 1:1"` string and parse it back,
/// so a `Copy` `Sid` field in a cached-stats struct round-trips across the wire
/// as a string (matching `Finding`'s string sids) without the native side ever
/// paying a `String` allocation outside serialization. Shared by every stateful
/// rule that stores sites (`casing`, `zero_width_space`, …).
#[cfg(feature = "serde")]
pub(crate) mod sid_as_string {
    use super::Sid;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sid: &Sid, ser: S) -> Result<S::Ok, S::Error> {
        ser.collect_str(sid)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Sid, D::Error> {
        let s = String::deserialize(de)?;
        Sid::parse(&s).ok_or_else(|| serde::de::Error::custom("invalid sid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sid_stays_pointer_sized() {
        // 3-byte book + 1 byte padding + u16 chapter + u16 verse = 8 bytes
        // on every common target. The point of the test is to catch a
        // regression where someone swaps to a heap-backed representation
        // (String / Box / Arc), which would push it to 16+ and reintroduce
        // allocation on the hot path.
        assert!(std::mem::size_of::<Sid>() <= 8);
    }

    #[test]
    fn parse_roundtrip() {
        let s = Sid::parse("JHN 3:16").unwrap();
        assert_eq!(s.book.as_str(), "JHN");
        assert_eq!(s.chapter, 3);
        assert_eq!(s.verse, 16);
        assert_eq!(format!("{}", s), "JHN 3:16");
    }

    #[test]
    fn rejects_bad_book() {
        assert!(BookId::from_str("GENESIS").is_none());
        assert!(Sid::parse("FOO 1:1").is_some()); // length-3 only check
        assert!(Sid::parse("GEN").is_none());
    }
}
