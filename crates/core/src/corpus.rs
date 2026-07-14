//! Ordered corpus and address newtypes (finding-address-representation
//! plan, Step 1b) — additive scaffolding alongside the existing
//! `Sid`/`VerseMap` path. This is temporary migration surface, not a
//! compatibility promise: Step 2 cuts the engine over to it and deletes
//! `sid.rs`/`VerseMap`.
//!
//! Three things this module is strict about, because they are correctness,
//! not style:
//!
//! 1. `Corpus` is an ordered structure-of-arrays (`keys`/`texts`), so it can
//!    represent duplicate key strings and caller order — a map cannot.
//! 2. Local ([`LocalKeyIdx`], book-relative) and global ([`KeyIdx`],
//!    corpus-relative) addresses are distinct newtypes. A cached per-book
//!    product holds a local index; a returned finding holds a global one.
//!    Rebase between them only through [`rebase`].
//! 3. Each book slug must occupy one contiguous block. `GEN, EXO, GEN` is
//!    rejected — accepting it would let a repeated slug collide in every
//!    slug-keyed stats/cache map and silently reorder the caller's seams.
//!
//! Not yet wired into execution — Step 2A cuts the engine over to it, hence
//! the interim `dead_code` allowance below.

#![allow(dead_code)]

use std::fmt;

use rustc_hash::FxHashSet;

use crate::key::{self, parse_key};
use crate::span::Span;

/// Position in the complete [`Corpus`] supplied for one call. Global;
/// `u32` because a corpus can exceed 65k entries and `Finding` (the type
/// that carries this) is the low-volume public output.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyIdx(u32);

impl KeyIdx {
    pub(crate) fn new(v: u32) -> Self {
        KeyIdx(v)
    }

    fn try_from_usize(v: usize) -> Result<Self, CorpusError> {
        u32::try_from(v)
            .map(KeyIdx)
            .map_err(|_| CorpusError::CorpusTooLarge { len: v })
    }
}

/// Position within one [`BookGroup`]. Stable for an unchanged book across
/// calls (the retained-cache invariant). `u16` is safe: the largest book
/// (Psalms, ~2.5k verses) is ~26x under the ceiling even with
/// duplicate/sub-verse inflation, and [`Corpus::try_from_parts`] rejects
/// any book block longer than `u16::MAX + 1`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalKeyIdx(u16);

impl LocalKeyIdx {
    pub(crate) fn new(v: u16) -> Self {
        LocalKeyIdx(v)
    }

    fn try_from_usize(v: usize, slug: &str) -> Result<Self, CorpusError> {
        u16::try_from(v)
            .map(LocalKeyIdx)
            .map_err(|_| CorpusError::BookTooLarge {
                slug: slug.to_string(),
                len: v,
            })
    }
}

/// Rebase a book-local index to its global position in the current call's
/// `Corpus`, given that book's [`BookGroup::base`]. The one checked helper —
/// retained products never widen a `LocalKeyIdx` any other way.
pub(crate) fn rebase(base: KeyIdx, local: LocalKeyIdx) -> KeyIdx {
    KeyIdx(
        base.0
            .checked_add(u32::from(local.0))
            .expect("validated corpus indices"),
    )
}

/// A packed, location-only site: 6 bytes (align 2, no padding). Verse-
/// relative byte offsets; the book is implicit in the owning [`BookGroup`].
/// For the high-volume pure-location site vecs only — richer site structs
/// that carry extra fields keep `LocalKeyIdx` + `Span` unpacked.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SiteAddr {
    pub(crate) local: u16,
    pub(crate) start: u16,
    pub(crate) end: u16,
}

impl SiteAddr {
    /// Pack a local index + byte range. Verse-relative offsets are a few
    /// hundred bytes in practice (Step 0's fleet scan: max ~13 KiB, well
    /// under the `u16` ceiling) — a single checked branch, never hit in
    /// practice, so it costs nothing yet rules out a silent release-mode
    /// wrap.
    pub(crate) fn pack(local: LocalKeyIdx, range: Span) -> Self {
        SiteAddr {
            local: local.0,
            start: u16::try_from(range.start).expect("verse offset fits u16"),
            end: u16::try_from(range.end).expect("verse offset fits u16"),
        }
    }

    pub(crate) fn unpack(self) -> (LocalKeyIdx, Span) {
        (
            LocalKeyIdx(self.local),
            Span {
                start: u32::from(self.start),
                end: u32::from(self.end),
            },
        )
    }
}

/// Why [`Corpus::try_from_parts`] rejected its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusError {
    MismatchedLengths { keys: usize, texts: usize },
    CorpusTooLarge { len: usize },
    MalformedKey { key: String, source: key::KeyError },
    BookTooLarge { slug: String, len: usize },
    ReopenedBook { slug: String },
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CorpusError::MismatchedLengths { keys, texts } => write!(
                f,
                "corpus keys/texts length mismatch: {keys} keys vs {texts} texts"
            ),
            CorpusError::CorpusTooLarge { len } => {
                write!(
                    f,
                    "corpus has {len} entries, which exceeds the addressable KeyIdx range"
                )
            }
            CorpusError::MalformedKey { key, source } => {
                write!(f, "malformed corpus key {key:?}: {source}")
            }
            CorpusError::BookTooLarge { slug, len } => write!(
                f,
                "book {slug:?} has {len} entries, which exceeds the addressable LocalKeyIdx range"
            ),
            CorpusError::ReopenedBook { slug } => write!(
                f,
                "book {slug:?} reopens after another book started — book blocks must be contiguous"
            ),
        }
    }
}

impl std::error::Error for CorpusError {}

/// Core's ordered structure-of-arrays. Preserves every input entry,
/// including duplicate key strings, in the caller's presented order. See
/// the module docs for the invariants `try_from_parts` enforces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    keys: Vec<String>,
    texts: Vec<String>,
}

impl Corpus {
    /// Validate and construct. Rejects mismatched array lengths, an
    /// unaddressable overall length, a malformed key (see [`parse_key`]), a
    /// book block longer than `u16::MAX + 1`, and a noncontiguous repeated
    /// book block (`GEN, EXO, GEN`). Caller order and duplicate keys are
    /// preserved, not validated away.
    pub fn try_from_parts(keys: Vec<String>, texts: Vec<String>) -> Result<Self, CorpusError> {
        if keys.len() != texts.len() {
            return Err(CorpusError::MismatchedLengths {
                keys: keys.len(),
                texts: texts.len(),
            });
        }
        KeyIdx::try_from_usize(keys.len())?;

        let mut current: Option<&str> = None;
        let mut current_len = 0usize;
        let mut closed: FxHashSet<&str> = FxHashSet::default();
        for key in &keys {
            let parts = parse_key(key).map_err(|source| CorpusError::MalformedKey {
                key: key.clone(),
                source,
            })?;
            if Some(parts.book) == current {
                current_len += 1;
                continue;
            }
            if let Some(slug) = current {
                LocalKeyIdx::try_from_usize(current_len, slug)?;
                closed.insert(slug);
            }
            if closed.contains(parts.book) {
                return Err(CorpusError::ReopenedBook {
                    slug: parts.book.to_string(),
                });
            }
            current = Some(parts.book);
            current_len = 1;
        }
        if let Some(slug) = current {
            LocalKeyIdx::try_from_usize(current_len, slug)?;
        }

        Ok(Corpus { keys, texts })
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn key(&self, idx: KeyIdx) -> &str {
        &self.keys[idx.0 as usize]
    }

    pub fn text(&self, idx: KeyIdx) -> &str {
        &self.texts[idx.0 as usize]
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn texts(&self) -> &[String] {
        &self.texts
    }
}

/// One contiguous book block's borrowed slices, plus its global base
/// index. `rebase(group.base, LocalKeyIdx(i))` addresses `group.keys[i]`.
pub struct BookGroup<'a> {
    pub slug: &'a str,
    pub base: KeyIdx,
    pub keys: &'a [String],
    pub texts: &'a [String],
}

pub type Books<'a> = Vec<BookGroup<'a>>;

/// Group a validated [`Corpus`] into contiguous per-book slices, in the
/// corpus's presented order (not canonical book order). Trusts the
/// contiguity/grammar invariants `Corpus::try_from_parts` already enforced.
pub fn by_book(corpus: &Corpus) -> Books<'_> {
    let mut groups: Books<'_> = Vec::new();
    let mut start = 0usize;
    while start < corpus.keys.len() {
        let slug = parse_key(&corpus.keys[start])
            .expect("Corpus validated keys")
            .book;
        let mut end = start + 1;
        while end < corpus.keys.len()
            && parse_key(&corpus.keys[end])
                .expect("Corpus validated keys")
                .book
                == slug
        {
            end += 1;
        }
        groups.push(BookGroup {
            slug,
            base: KeyIdx::new(start as u32),
            keys: &corpus.keys[start..end],
            texts: &corpus.texts[start..end],
        });
        start = end;
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(ks: &[&str]) -> Vec<String> {
        ks.iter().map(|s| s.to_string()).collect()
    }

    fn texts(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("text {i}")).collect()
    }

    #[test]
    fn rejects_mismatched_array_lengths() {
        let err = Corpus::try_from_parts(keys(&["GEN 1:1"]), texts(2)).unwrap_err();
        assert_eq!(err, CorpusError::MismatchedLengths { keys: 1, texts: 2 });
    }

    #[test]
    fn rejects_malformed_key() {
        let err = Corpus::try_from_parts(keys(&["GEN1:1"]), texts(1)).unwrap_err();
        assert!(matches!(err, CorpusError::MalformedKey { .. }));
    }

    #[test]
    fn rejects_book_block_past_local_key_idx() {
        let ks: Vec<String> = (0..=u16::MAX as u32 + 1)
            .map(|v| format!("GEN 1:{v}"))
            .collect();
        let n = ks.len();
        let err = Corpus::try_from_parts(ks, texts(n)).unwrap_err();
        assert!(matches!(err, CorpusError::BookTooLarge { .. }));
    }

    #[test]
    fn preserves_duplicate_keys() {
        let c = Corpus::try_from_parts(keys(&["GEN 1:1", "GEN 1:1"]), texts(2)).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.key(KeyIdx::new(0)), c.key(KeyIdx::new(1)));
    }

    #[test]
    fn preserves_sub_verse_tokens() {
        let c = Corpus::try_from_parts(keys(&["GEN 1:1a"]), texts(1)).unwrap();
        assert_eq!(c.key(KeyIdx::new(0)), "GEN 1:1a");
    }

    #[test]
    fn preserves_caller_order() {
        let c = Corpus::try_from_parts(keys(&["REV 1:1", "GEN 1:1"]), texts(2)).unwrap();
        assert_eq!(c.key(KeyIdx::new(0)), "REV 1:1");
        assert_eq!(c.key(KeyIdx::new(1)), "GEN 1:1");
    }

    #[test]
    fn accepts_noncanonical_book_block_order() {
        let c = Corpus::try_from_parts(keys(&["EXO 1:1", "GEN 1:1"]), texts(2)).unwrap();
        let groups = by_book(&c);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].slug, "EXO");
        assert_eq!(groups[1].slug, "GEN");
    }

    #[test]
    fn rejects_reopened_book_block() {
        let err =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1", "GEN 1:2"]), texts(3)).unwrap_err();
        assert_eq!(
            err,
            CorpusError::ReopenedBook {
                slug: "GEN".to_string()
            }
        );
    }

    #[test]
    fn checked_index_conversion_and_rebase() {
        let base = KeyIdx::new(10);
        let local = LocalKeyIdx::new(5);
        assert_eq!(rebase(base, local), KeyIdx::new(15));
    }

    #[test]
    fn site_addr_pack_unpack_round_trips() {
        let local = LocalKeyIdx::new(3);
        let range = Span { start: 12, end: 34 };
        let packed = SiteAddr::pack(local, range);
        let (unpacked_local, unpacked_range) = packed.unpack();
        assert_eq!(unpacked_local, local);
        assert_eq!(unpacked_range, range);
    }

    #[test]
    #[should_panic(expected = "verse offset fits u16")]
    fn site_addr_pack_guards_the_u16_offset() {
        let range = Span {
            start: 0,
            end: u32::from(u16::MAX) + 1,
        };
        SiteAddr::pack(LocalKeyIdx::new(0), range);
    }

    #[test]
    fn by_book_bases_and_borrowed_slices_are_correct() {
        let c = Corpus::try_from_parts(keys(&["REV 1:1", "REV 1:2", "GEN 1:1"]), texts(3)).unwrap();
        let groups = by_book(&c);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].slug, "REV");
        assert_eq!(groups[0].base, KeyIdx::new(0));
        assert_eq!(groups[0].keys, &c.keys()[0..2]);
        assert_eq!(groups[1].slug, "GEN");
        assert_eq!(groups[1].base, KeyIdx::new(2));
        assert_eq!(groups[1].keys, &c.keys()[2..3]);
    }
}
