//! Scripture identifiers. `Sid` is `Copy` and 6 bytes — cheaper than a
//! `String` of `"GEN 1:1"` and equally meaningful.

use std::fmt;

/// Three-letter USFM book code (e.g. `GEN`, `JHN`, `REV`). Stored as raw
/// bytes so the type is `Copy` and equality is a single 24-bit compare.
/// Validation (membership in the 66-book canon) is the ingest layer's job.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

// Wire format: both types cross serde as their canonical strings — `BookId`
// as `"GEN"`, `Sid` as `"GEN 1:1"` — reusing `as_str`/`Display` out and
// `from_str`/`parse` back in. Map keys and cached observations stay
// byte-identical to the earlier String-keyed stats while the in-memory
// types stay `Copy`: nothing allocates until the wire itself.
#[cfg(feature = "serde")]
mod serde_impls {
    use super::{BookId, Sid};

    impl serde::Serialize for BookId {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(self.as_str())
        }
    }

    impl<'de> serde::Deserialize<'de> for BookId {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl serde::de::Visitor<'_> for V {
                type Value = BookId;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a 3-character ASCII USFM book code")
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<BookId, E> {
                    BookId::from_str(v)
                        .ok_or_else(|| E::custom(format_args!("invalid book code {v:?}")))
                }
            }
            d.deserialize_str(V)
        }
    }

    impl serde::Serialize for Sid {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.collect_str(self)
        }
    }

    impl<'de> serde::Deserialize<'de> for Sid {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl serde::de::Visitor<'_> for V {
                type Value = Sid;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a canonical sid like \"GEN 1:1\"")
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Sid, E> {
                    Sid::parse(v).ok_or_else(|| E::custom(format_args!("invalid sid {v:?}")))
                }
            }
            d.deserialize_str(V)
        }
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

    /// The wire format is pinned: canonical strings both ways, so the
    /// `BookId`-keyed stats maps serialize byte-identically to the earlier
    /// `String`-keyed ones and round-trip through the shell.
    #[cfg(feature = "serde")]
    #[test]
    fn serde_wire_is_canonical_strings() {
        let sid = Sid::parse("JHN 3:16").unwrap();
        assert_eq!(serde_json::to_string(&sid).unwrap(), "\"JHN 3:16\"");
        assert_eq!(serde_json::to_string(&sid.book).unwrap(), "\"JHN\"");
        assert_eq!(serde_json::from_str::<Sid>("\"JHN 3:16\"").unwrap(), sid);
        assert_eq!(serde_json::from_str::<BookId>("\"JHN\"").unwrap(), sid.book);
        // Map keys — the actual stats shape — work under serde_json, which
        // requires string keys.
        let map: std::collections::BTreeMap<BookId, u32> = [(sid.book, 1)].into();
        assert_eq!(serde_json::to_string(&map).unwrap(), r#"{"JHN":1}"#);
        assert!(serde_json::from_str::<BookId>("\"GENESIS\"").is_err());
        assert!(serde_json::from_str::<Sid>("\"JHN\"").is_err());
    }
}
