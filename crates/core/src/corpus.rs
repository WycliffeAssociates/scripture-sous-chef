//! Ordered corpus and address newtypes (ADR 0061): the addressing substrate
//! every rule, cache, and the wasm boundary builds on.
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
use std::fmt;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::key::{self, parse_key};
use crate::span::Span;

/// Position in the complete [`Corpus`] supplied for one call. Global;
/// `u32` because a corpus can exceed 65k entries and `Finding` (the type
/// that carries this) is the low-volume public output.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct KeyIdx(u32);

impl KeyIdx {
    /// Narrow a corpus-loop index already bounded by `Corpus::try_from_parts`'s
    /// own addressable-length check. Panics rather than silently truncating
    /// if that invariant is ever violated — the checked constructor the
    /// "never a truncating `as` cast" contract requires.
    pub(crate) fn from_usize(v: usize) -> Self {
        KeyIdx(u32::try_from(v).expect("index bounded by Corpus's validated addressable length"))
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
/// any book block longer than `u16::MAX`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct LocalKeyIdx(u16);

impl LocalKeyIdx {
    pub(crate) fn get(self) -> u16 {
        self.0
    }

    /// Narrow a verse-loop index already bounded by this book's own
    /// `Corpus`-validated `LocalKeyIdx` capacity. Panics rather than silently
    /// truncating if that invariant is ever violated — the checked
    /// constructor the "never a truncating `as` cast" contract requires.
    pub(crate) fn from_usize(v: usize) -> Self {
        LocalKeyIdx(
            u16::try_from(v)
                .expect("verse index bounded by Corpus's validated book-block capacity"),
        )
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

/// The inverse of [`rebase`]: narrow a global index freshly computed this
/// call back to book-local, for storage in a retained cache product. Only
/// ever applied to a `KeyIdx` this same call derived from `base`, so the
/// subtraction and narrowing cannot fail.
pub(crate) fn unrebase(base: KeyIdx, global: KeyIdx) -> LocalKeyIdx {
    LocalKeyIdx(u16::try_from(global.0 - base.0).expect("global was rebased from this call's base"))
}

/// A packed, location-only site: 6 bytes (align 2, no padding). Verse-
/// relative byte offsets; the book is implicit in the owning [`BookGroup`].
/// For the high-volume pure-location site vecs only — richer site structs
/// that carry extra fields keep `LocalKeyIdx` + `Span` unpacked.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteAddr {
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
    /// A [`BookBlock`] key whose parsed book slug is not the block's own slug.
    SlugMismatch { slug: String, key: String },
    /// A [`BookBlock`] with no verses. Removal is the explicit
    /// [`Corpus::remove_book`], never an empty block.
    EmptyBook { slug: String },
    /// Two [`BookBlock`]s in one [`Corpus::replace_books`] batch share a slug.
    DuplicateSlugInBatch { slug: String },
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
            CorpusError::SlugMismatch { slug, key } => write!(
                f,
                "book block {slug:?} contains key {key:?}, whose book is not {slug:?}"
            ),
            CorpusError::EmptyBook { slug } => {
                write!(f, "book block {slug:?} is empty; use remove_book to delete a book")
            }
            CorpusError::DuplicateSlugInBatch { slug } => {
                write!(f, "book {slug:?} appears more than once in one replace_books batch")
            }
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

/// One validated whole-book block for [`Corpus::replace_books`]. Shared by
/// core and the shell crate — the shell does not define its own update type.
/// Every `keys[i]` must parse to `slug`; `keys`/`texts` must match in length
/// and be non-empty (removal is [`Corpus::remove_book`], never an empty block).
#[derive(Debug, Clone)]
pub struct BookBlock {
    pub slug: Box<str>,
    pub keys: Vec<String>,
    pub texts: Vec<String>,
}

impl Corpus {
    /// Validate and construct. Rejects mismatched array lengths, an
    /// unaddressable overall length, a malformed key (see [`parse_key`]), a
    /// book block longer than `u16::MAX`, and a noncontiguous repeated
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

    /// Atomically replace/insert whole books. Every block is validated first
    /// (key grammar with book == slug; matched, non-empty `keys`/`texts`; the
    /// `LocalKeyIdx` u16 ceiling; no duplicate slug in the batch) and the
    /// resulting corpus length is checked addressable; only then does it splice,
    /// so a rejected batch leaves the corpus untouched. An existing slug is
    /// replaced in place (its presented position is kept); a new slug is
    /// appended at the end in batch order. Splices move `String`s — no text
    /// copies; validation only borrows.
    pub fn replace_books(&mut self, batch: Vec<BookBlock>) -> Result<(), CorpusError> {
        // 1. Validate every block, and reject a slug repeated within the batch.
        let mut batch_slugs: FxHashSet<&str> = FxHashSet::default();
        for block in &batch {
            if block.keys.len() != block.texts.len() {
                return Err(CorpusError::MismatchedLengths {
                    keys: block.keys.len(),
                    texts: block.texts.len(),
                });
            }
            if block.keys.is_empty() {
                return Err(CorpusError::EmptyBook {
                    slug: block.slug.to_string(),
                });
            }
            LocalKeyIdx::try_from_usize(block.keys.len(), &block.slug)?;
            for k in &block.keys {
                let parts = parse_key(k).map_err(|source| CorpusError::MalformedKey {
                    key: k.clone(),
                    source,
                })?;
                if parts.book != &*block.slug {
                    return Err(CorpusError::SlugMismatch {
                        slug: block.slug.to_string(),
                        key: k.clone(),
                    });
                }
            }
            if !batch_slugs.insert(&block.slug) {
                return Err(CorpusError::DuplicateSlugInBatch {
                    slug: block.slug.to_string(),
                });
            }
        }

        // 2. Map each batch slug to its slot, and lay out the existing books so
        //    replacements stay in place and the untouched books carry by move.
        let mut slots: Vec<Option<BookBlock>> = batch.into_iter().map(Some).collect();
        let slug_to_slot: FxHashMap<Box<str>, usize> = slots
            .iter()
            .enumerate()
            .map(|(i, b)| (b.as_ref().expect("slot just filled").slug.clone(), i))
            .collect();

        // (start, end, Some(slot) if this existing book is being replaced).
        let mut layout: Vec<(usize, usize, Option<usize>)> = Vec::new();
        let mut start = 0usize;
        while start < self.keys.len() {
            let slug = parse_key(&self.keys[start]).expect("Corpus validated keys").book;
            let mut end = start + 1;
            while end < self.keys.len()
                && parse_key(&self.keys[end]).expect("Corpus validated keys").book == slug
            {
                end += 1;
            }
            layout.push((start, end, slug_to_slot.get(slug).copied()));
            start = end;
        }

        // 3. The resulting corpus must stay addressable (Corpus's own invariant,
        //    checked before any mutation so the batch is still all-or-nothing).
        let appended: usize = slots
            .iter()
            .enumerate()
            .filter(|(i, _)| !layout.iter().any(|&(_, _, r)| r == Some(*i)))
            .map(|(_, slot)| slot.as_ref().map_or(0, |b| b.keys.len()))
            .sum();
        let final_len: usize = layout
            .iter()
            .map(|&(s, e, replaced)| match replaced {
                Some(i) => slots[i].as_ref().expect("replacement slot present").keys.len(),
                None => e - s,
            })
            .sum::<usize>()
            + appended;
        KeyIdx::try_from_usize(final_len)?;

        // 4. Splice: existing books (replaced or carried) in order, then the
        //    new-slug blocks appended in batch order. Everything moves.
        let mut old_keys = std::mem::take(&mut self.keys).into_iter();
        let mut old_texts = std::mem::take(&mut self.texts).into_iter();
        let mut new_keys: Vec<String> = Vec::with_capacity(final_len);
        let mut new_texts: Vec<String> = Vec::with_capacity(final_len);
        for (s, e, replaced) in layout {
            let n = e - s;
            if let Some(i) = replaced {
                for _ in 0..n {
                    old_keys.next();
                    old_texts.next();
                }
                let block = slots[i].take().expect("replacement slot present");
                new_keys.extend(block.keys);
                new_texts.extend(block.texts);
            } else {
                for _ in 0..n {
                    new_keys.push(old_keys.next().expect("layout bounded by corpus length"));
                    new_texts.push(old_texts.next().expect("layout bounded by corpus length"));
                }
            }
        }
        for slot in &mut slots {
            if let Some(block) = slot.take() {
                new_keys.extend(block.keys);
                new_texts.extend(block.texts);
            }
        }
        self.keys = new_keys;
        self.texts = new_texts;
        Ok(())
    }

    /// Remove `slug`'s contiguous block entirely. Returns `false` when the slug
    /// is absent (a no-op). Removing the last book leaves a valid empty corpus.
    pub fn remove_book(&mut self, slug: &str) -> bool {
        let mut start = 0usize;
        while start < self.keys.len() {
            let book = parse_key(&self.keys[start]).expect("Corpus validated keys").book;
            let mut end = start + 1;
            while end < self.keys.len()
                && parse_key(&self.keys[end]).expect("Corpus validated keys").book == book
            {
                end += 1;
            }
            if book == slug {
                self.keys.drain(start..end);
                self.texts.drain(start..end);
                return true;
            }
            start = end;
        }
        false
    }
}

/// One contiguous book block's borrowed slices, plus its global base
/// index. `rebase(group.base, LocalKeyIdx(i))` addresses `group.keys[i]`.
#[derive(Clone, Copy)]
pub struct BookGroup<'a> {
    pub slug: &'a str,
    pub base: KeyIdx,
    pub keys: &'a [String],
    pub texts: &'a [String],
}

impl<'a> BookGroup<'a> {
    pub(crate) fn key(&self, local: LocalKeyIdx) -> &'a str {
        &self.keys[usize::from(local.0)]
    }

    pub(crate) fn text(&self, local: LocalKeyIdx) -> &'a str {
        &self.texts[usize::from(local.0)]
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }
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
            base: KeyIdx::from_usize(start),
            keys: &corpus.keys[start..end],
            texts: &corpus.texts[start..end],
        });
        start = end;
    }
    groups
}

/// Resolve a finding's global address back to its key string. Checked
/// (panics on an out-of-range `idx`, exactly like a slice index) — every
/// `KeyIdx` on a `Finding` this call returned is valid against this same
/// `Corpus`.
pub fn resolve_key(corpus: &Corpus, idx: KeyIdx) -> &str {
    corpus.key(idx)
}

pub fn resolve_text(corpus: &Corpus, idx: KeyIdx) -> &str {
    corpus.text(idx)
}

/// A [`Finding`] with its `key_idx` resolved to the owned key string —
/// native reporting facade for dev tools and non-wasm callers, so they
/// don't each invent their own projection logic.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFinding {
    pub sid: String,
    pub code: RuleId,
    pub severity: Severity,
    pub range: Span,
    pub score: Option<f32>,
    pub args: Option<FindingArgs>,
}

pub fn resolve_findings(corpus: &Corpus, findings: &[Finding]) -> Vec<ResolvedFinding> {
    findings
        .iter()
        .map(|f| ResolvedFinding {
            sid: corpus.key(f.key_idx).to_string(),
            code: f.code,
            severity: f.severity,
            range: f.range,
            score: f.score,
            args: f.args.clone(),
        })
        .collect()
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
    fn rejects_corpus_past_key_idx() {
        // `u32::MAX + 1` entries is infeasible to allocate in a unit test
        // (~4.3 billion strings); exercise the checked boundary
        // `Corpus::try_from_parts` itself calls instead of the whole
        // constructor.
        assert!(KeyIdx::try_from_usize(u32::MAX as usize).is_ok());
        let err = KeyIdx::try_from_usize(u32::MAX as usize + 1).unwrap_err();
        assert!(matches!(err, CorpusError::CorpusTooLarge { .. }));
    }

    #[test]
    fn preserves_duplicate_keys() {
        let c = Corpus::try_from_parts(keys(&["GEN 1:1", "GEN 1:1"]), texts(2)).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c.key(KeyIdx::from_usize(0)), c.key(KeyIdx::from_usize(1)));
    }

    #[test]
    fn preserves_sub_verse_tokens() {
        let c = Corpus::try_from_parts(keys(&["GEN 1:1a"]), texts(1)).unwrap();
        assert_eq!(c.key(KeyIdx::from_usize(0)), "GEN 1:1a");
    }

    #[test]
    fn preserves_caller_order() {
        let c = Corpus::try_from_parts(keys(&["REV 1:1", "GEN 1:1"]), texts(2)).unwrap();
        assert_eq!(c.key(KeyIdx::from_usize(0)), "REV 1:1");
        assert_eq!(c.key(KeyIdx::from_usize(1)), "GEN 1:1");
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
        let base = KeyIdx::from_usize(10);
        let local = LocalKeyIdx::from_usize(5);
        assert_eq!(rebase(base, local), KeyIdx::from_usize(15));
    }

    #[test]
    fn site_addr_pack_unpack_round_trips() {
        let local = LocalKeyIdx::from_usize(3);
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
        SiteAddr::pack(LocalKeyIdx::from_usize(0), range);
    }

    #[test]
    fn by_book_bases_and_borrowed_slices_are_correct() {
        let c = Corpus::try_from_parts(keys(&["REV 1:1", "REV 1:2", "GEN 1:1"]), texts(3)).unwrap();
        let groups = by_book(&c);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].slug, "REV");
        assert_eq!(groups[0].base, KeyIdx::from_usize(0));
        assert_eq!(groups[0].keys, &c.keys()[0..2]);
        assert_eq!(groups[1].slug, "GEN");
        assert_eq!(groups[1].base, KeyIdx::from_usize(2));
        assert_eq!(groups[1].keys, &c.keys()[2..3]);
    }

    fn block(slug: &str, ks: &[&str], txt: &[&str]) -> BookBlock {
        BookBlock {
            slug: slug.into(),
            keys: keys(ks),
            texts: keys(txt),
        }
    }

    /// B-1: replacing a book in place (same slug, new/longer text) splices at
    /// the book's presented position and leaves later books byte-for-byte.
    #[test]
    fn replace_books_in_place_keeps_siblings_untouched() {
        let mut c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "EXO 1:1", "EXO 1:2"]),
            keys(&["g1", "g2", "e1", "e2"]),
        )
        .unwrap();
        c.replace_books(vec![block(
            "GEN",
            &["GEN 1:1", "GEN 1:2", "GEN 1:3"],
            &["G1", "G2", "G3"],
        )])
        .unwrap();
        assert_eq!(
            c.keys(),
            keys(&["GEN 1:1", "GEN 1:2", "GEN 1:3", "EXO 1:1", "EXO 1:2"]).as_slice()
        );
        assert_eq!(c.texts(), keys(&["G1", "G2", "G3", "e1", "e2"]).as_slice());
    }

    /// B-2: a new slug appends at the end; a mixed batch (replace + insert)
    /// keeps existing presented order and appends the new book last.
    #[test]
    fn replace_books_appends_new_slug_and_handles_mixed_batch() {
        let mut c =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();
        c.replace_books(vec![
            block("EXO", &["EXO 1:1", "EXO 1:2"], &["E1", "E2"]),
            block("LEV", &["LEV 1:1"], &["l"]),
        ])
        .unwrap();
        assert_eq!(
            c.keys(),
            keys(&["GEN 1:1", "EXO 1:1", "EXO 1:2", "LEV 1:1"]).as_slice()
        );
        assert_eq!(c.texts(), keys(&["g", "E1", "E2", "l"]).as_slice());
    }

    /// B-3: a batch failing on its LAST block leaves the corpus untouched —
    /// validation is complete before any splice (all-or-nothing).
    #[test]
    fn replace_books_is_atomic_on_a_late_failure() {
        let original =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();

        // SlugMismatch on the second block.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![
                block("GEN", &["GEN 1:1"], &["G"]),
                block("EXO", &["GEN 1:9"], &["x"]), // key's book != slug
            ])
            .unwrap_err();
        assert!(matches!(err, CorpusError::SlugMismatch { .. }));
        assert_eq!(c, original, "a rejected batch leaves the corpus untouched");

        // Length mismatch on the last block.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![BookBlock {
                slug: "EXO".into(),
                keys: keys(&["EXO 1:1", "EXO 1:2"]),
                texts: keys(&["only-one"]),
            }])
            .unwrap_err();
        assert!(matches!(err, CorpusError::MismatchedLengths { .. }));
        assert_eq!(c, original);

        // Duplicate slug within one batch.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![
                block("GEN", &["GEN 1:1"], &["a"]),
                block("GEN", &["GEN 1:2"], &["b"]),
            ])
            .unwrap_err();
        assert!(matches!(err, CorpusError::DuplicateSlugInBatch { .. }));
        assert_eq!(c, original);

        // An empty block is an error, never a removal.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![BookBlock {
                slug: "EXO".into(),
                keys: Vec::new(),
                texts: Vec::new(),
            }])
            .unwrap_err();
        assert!(matches!(err, CorpusError::EmptyBook { .. }));
        assert_eq!(c, original);
    }

    /// B-4 (corpus half): `remove_book` returns true/false and removing the
    /// last book leaves a valid empty corpus.
    #[test]
    fn remove_book_reports_presence_and_empties_cleanly() {
        let mut c =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();
        assert!(!c.remove_book("LEV"), "absent slug is a no-op");
        assert!(c.remove_book("GEN"));
        assert_eq!(c.keys(), keys(&["EXO 1:1"]).as_slice());
        assert!(c.remove_book("EXO"));
        assert!(c.is_empty(), "removing the last book leaves a valid empty corpus");
    }
}
