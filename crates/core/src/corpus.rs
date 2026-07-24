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
//!    slug-keyed stats/cache map and silently reorder the caller's seams. The
//!    same contiguity holds one level down: inside a book each opaque chapter
//!    token is one run (`GEN 1, GEN 2, GEN 1` is rejected). Chapter tokens are
//!    compared opaquely — never parsed or ordered as numbers.
//!
use std::fmt;
use std::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};
use xxhash_rust::xxh3::Xxh3;

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

    /// The raw global index. The mechanical accessor the packed-findings wire
    /// (`ssc-wire`, Phase A-W) writes into each record; ordinary callers
    /// resolve a `KeyIdx` through [`Corpus::key`]/[`Corpus::text`] instead.
    pub fn get(self) -> u32 {
        self.0
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
    /// An opaque chapter token that reappears inside its book after another
    /// token closed it (`GEN 1:1, GEN 2:1, GEN 1:2`). Like a book, a chapter is
    /// one contiguous run; tokens are compared opaquely (no numeric ordering).
    ReopenedChapter { slug: String, chapter: String },
    /// A [`BookBlock`] key whose parsed book slug is not the block's own slug.
    SlugMismatch { slug: String, key: String },
    /// A [`BookBlock`] with no verses. Removal is the explicit
    /// [`Corpus::remove_book`], never an empty block.
    EmptyBook { slug: String },
    /// Two [`BookBlock`]s in one [`Corpus::replace_books`] batch share a slug.
    DuplicateSlugInBatch { slug: String },
    /// A [`ChapterBlock`] with no verses. A zero-verse chapter is a removal and
    /// must go through a whole-book [`Corpus::replace_books`], never an empty
    /// chapter block.
    EmptyChapterBlock { slug: String, chapter: String },
    /// A [`ChapterBlock`] key whose parsed chapter token is not the block's own
    /// chapter (its book already matched, or `SlugMismatch` would have fired).
    ChapterTokenMismatch { chapter: String, key: String },
    /// [`Corpus::replace_chapter`] found no `(slug, chapter)` run to replace.
    /// Whole-chapter insertion uses [`Corpus::replace_books`].
    ChapterNotFound { slug: String, chapter: String },
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
            CorpusError::ReopenedChapter { slug, chapter } => write!(
                f,
                "chapter {chapter:?} in book {slug:?} reopens after another chapter started — \
                 chapter runs must be contiguous"
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
            CorpusError::EmptyChapterBlock { slug, chapter } => write!(
                f,
                "chapter block {slug:?} {chapter:?} is empty; remove a chapter with a whole-book update"
            ),
            CorpusError::ChapterTokenMismatch { chapter, key } => write!(
                f,
                "chapter block {chapter:?} contains key {key:?}, whose chapter is not {chapter:?}"
            ),
            CorpusError::ChapterNotFound { slug, chapter } => write!(
                f,
                "no chapter {chapter:?} in book {slug:?} to replace; insert a chapter with a whole-book update"
            ),
        }
    }
}

impl std::error::Error for CorpusError {}

/// The explicit result of a validated resident mutation. `Unchanged` when the
/// new ordered semantic input is identical to the current input (a proven
/// no-op that preserves cache and publication validity); `Changed` otherwise.
/// The wasm adapter uses this to stale its published lazy-args lookup without
/// re-deriving equality by rehashing JS inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationEffect {
    Unchanged,
    Changed,
}

/// Content hash of an ordered `(keys, texts)` slice: xxh3-128 over each
/// entry's length-prefixed key bytes then text bytes, so distinct sequences
/// cannot collapse through concatenation (and each key carries its own
/// book/chapter/verse token, so a chapter-boundary rearrangement changes the
/// hash). This is the one verse-level hashing primitive; [`Corpus`]'s owned
/// **chapter** hashes use it directly, and the owned **book** hash folds those
/// chapter hashes (see [`fold_book_hash`]).
pub(crate) fn content_hash(keys: &[String], texts: &[String]) -> u128 {
    let mut hasher = Xxh3::new();
    for (key, text) in keys.iter().zip(texts.iter()) {
        hasher.update(&(key.len() as u32).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&(text.len() as u32).to_le_bytes());
        hasher.update(text.as_bytes());
    }
    hasher.digest128()
}

/// Fold a book's ordered chapter layouts into its content hash: a
/// count prefix, then per chapter the length-prefixed opaque token bytes
/// followed by that chapter's 16-byte content hash, in presented order. It is
/// order-sensitive and length-delimited, so neither a chapter reorder nor a
/// token/hash concatenation can collide, and it **composes from the
/// already-owned chapter hashes** rather than re-reading every verse (Entry 4
/// adjudication). Two books with identical ordered chapter content therefore
/// fold to the identical hash, and any chapter-token, chapter-order, or
/// chapter-content change moves it — so provenance/cache equality behaves
/// exactly as the former flat hash did, only over a cheaper composition.
fn fold_book_hash(chapters: &[ChapterLayout]) -> u128 {
    let mut hasher = Xxh3::new();
    hasher.update(&(chapters.len() as u32).to_le_bytes());
    for c in chapters {
        hasher.update(&(c.chapter.len() as u32).to_le_bytes());
        hasher.update(c.chapter.as_bytes());
        hasher.update(&c.hash.to_le_bytes());
    }
    hasher.digest128()
}

/// One chapter's owned layout inside a [`BookLayout`]: its opaque token, the
/// global verse range it occupies, and a content hash of that range. The token
/// is compared opaquely; nothing here parses or orders it as a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChapterLayout {
    pub(crate) chapter: Box<str>,
    pub(crate) range: Range<usize>,
    pub(crate) hash: u128,
}

/// One book's owned layout: slug, its global verse range, its ordered chapter
/// layouts, and a content hash of the whole book. The book hash is the ordered,
/// length-delimited fold of its `(chapter token, chapter hash)` pairs (see
/// [`fold_book_hash`]) — the derived proof the analysis path reads
/// instead of re-hashing per call. It composes from the owned chapter hashes,
/// so distinguishing chapter boundaries and order costs no verse re-read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BookLayout {
    pub(crate) slug: Box<str>,
    pub(crate) range: Range<usize>,
    pub(crate) chapters: Vec<ChapterLayout>,
    pub(crate) hash: u128,
}

/// Build one book's layout from its contiguous run starting at global index
/// `start` in an already-validated `(keys, texts)` pair. Chapter and book
/// ranges are **global** (absolute indices into `keys`/`texts`), so the result
/// drops straight into [`Corpus::layout`]; `book.range.end` is the next book's
/// start. The run is assumed validated (every mutation validates grammar and
/// contiguity before building), so the parses cannot fail.
fn build_book_at(keys: &[String], texts: &[String], start: usize) -> BookLayout {
    let n = keys.len();
    let slug = parse_key(&keys[start]).expect("Corpus validated keys").book;
    let mut chapters: Vec<ChapterLayout> = Vec::new();
    let mut i = start;
    loop {
        let chap_start = i;
        let chapter = parse_key(&keys[i]).expect("Corpus validated keys").chapter;
        i += 1;
        while i < n {
            let p = parse_key(&keys[i]).expect("Corpus validated keys");
            if p.book != slug || p.chapter != chapter {
                break;
            }
            i += 1;
        }
        chapters.push(ChapterLayout {
            chapter: Box::from(chapter),
            range: chap_start..i,
            hash: content_hash(&keys[chap_start..i], &texts[chap_start..i]),
        });
        if i >= n || parse_key(&keys[i]).expect("Corpus validated keys").book != slug {
            break;
        }
    }
    BookLayout {
        slug: Box::from(slug),
        hash: fold_book_hash(&chapters),
        range: start..i,
        chapters,
    }
}

/// Rebase a book's global ranges (its own and every chapter's) so the book
/// starts at `new_start`, preserving every relative offset. Used to maintain
/// the layout locally after a length-changing splice: the books *after* the
/// changed one only shift position — their text, chapter boundaries, and
/// hashes are unchanged, so they are rebased rather than re-parsed/re-hashed.
/// All arithmetic is unsigned and relative (chapter ranges always sit at or
/// after their book's start), so it cannot wrap — signed deltas would, on
/// targets where `isize` is narrower than the address space the domain admits.
fn shift_book(book: &mut BookLayout, new_start: usize) {
    let old_start = book.range.start;
    let rebase = |r: &Range<usize>| {
        (new_start + (r.start - old_start))..(new_start + (r.end - old_start))
    };
    book.range = rebase(&book.range);
    for c in &mut book.chapters {
        c.range = rebase(&c.range);
    }
}

/// Build the derived book/chapter layout for an already-validated
/// `(keys, texts)` pair (`try_from_parts` and `replace_corpus` use this;
/// length-narrowing mutations maintain the layout locally instead). One pass in
/// presented order; each book's run is laid out by [`build_book_at`].
fn build_layout(keys: &[String], texts: &[String]) -> Vec<BookLayout> {
    let n = keys.len();
    let mut books: Vec<BookLayout> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let book = build_book_at(keys, texts, i);
        i = book.range.end;
        books.push(book);
    }
    books
}

/// Core's ordered structure-of-arrays. Preserves every input entry,
/// including duplicate key strings, in the caller's presented order. See
/// the module docs for the invariants `try_from_parts` enforces.
///
/// It also owns pure derived metadata — the per-book/per-chapter [`BookLayout`]
/// (ranges + content hashes) — rebuilt atomically by construction and every
/// mutation. This is *proof*, not a caller promise: because the `Corpus` alone
/// owns its vectors, its layout cannot silently drift from the text it
/// describes, so the analysis path reads these hashes instead of re-deriving
/// them each call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    keys: Vec<String>,
    texts: Vec<String>,
    layout: Vec<BookLayout>,
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

/// One validated whole-chapter-run block for [`Corpus::replace_chapter`]: the
/// complete replacement for exactly one existing `(slug, chapter)` run. Every
/// `keys[i]` must parse to `slug` and `chapter`; `keys`/`texts` must match in
/// length and be non-empty (a zero-verse chapter is a removal — use a
/// whole-book [`Corpus::replace_books`]). Caller order and duplicate keys are
/// preserved. Whole-chapter insertion/removal/reorder is a whole-book update.
#[derive(Debug, Clone)]
pub struct ChapterBlock {
    pub slug: Box<str>,
    pub chapter: Box<str>,
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

        // One pass validating both contiguity invariants: a book is one run
        // that may not reopen, and inside a book each opaque chapter token is
        // one run that may not reopen. Chapter tokens are only unique within a
        // book, so `closed_chapters` resets at every book seam. Both closed sets
        // borrow `&str` from `keys` (opaque comparison; never numeric).
        let mut current: Option<&str> = None;
        let mut current_chapter: Option<&str> = None;
        let mut current_len = 0usize;
        let mut closed: FxHashSet<&str> = FxHashSet::default();
        let mut closed_chapters: FxHashSet<&str> = FxHashSet::default();
        for key in &keys {
            let parts = parse_key(key).map_err(|source| CorpusError::MalformedKey {
                key: key.clone(),
                source,
            })?;
            if Some(parts.book) == current {
                current_len += 1;
                if Some(parts.chapter) != current_chapter {
                    if closed_chapters.contains(parts.chapter) {
                        return Err(CorpusError::ReopenedChapter {
                            slug: parts.book.to_string(),
                            chapter: parts.chapter.to_string(),
                        });
                    }
                    if let Some(ch) = current_chapter {
                        closed_chapters.insert(ch);
                    }
                    current_chapter = Some(parts.chapter);
                }
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
            current_chapter = Some(parts.chapter);
            current_len = 1;
            closed_chapters.clear();
        }
        if let Some(slug) = current {
            LocalKeyIdx::try_from_usize(current_len, slug)?;
        }

        let layout = build_layout(&keys, &texts);
        Ok(Corpus {
            keys,
            texts,
            layout,
        })
    }

    /// The owned per-book layout (ranges + content hashes), in presented order.
    /// Crate-internal proof for the analysis path and cache keying; the storage
    /// types are not part of the public API.
    pub(crate) fn book_layout(&self) -> &[BookLayout] {
        &self.layout
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
    ///
    /// Returns [`MutationEffect::Unchanged`] — skipping the splice and the
    /// layout rebuild — when every block is byte-identical to an existing
    /// same-slug book (a proven no-op), else [`MutationEffect::Changed`].
    pub fn replace_books(&mut self, batch: Vec<BookBlock>) -> Result<MutationEffect, CorpusError> {
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
            // A block is one whole book, so every key's book must be `slug`; its
            // chapter tokens must also each be one contiguous non-reopening run
            // (the same within-book invariant `try_from_parts` enforces).
            let mut current_chapter: Option<&str> = None;
            let mut closed_chapters: FxHashSet<&str> = FxHashSet::default();
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
                if Some(parts.chapter) != current_chapter {
                    if closed_chapters.contains(parts.chapter) {
                        return Err(CorpusError::ReopenedChapter {
                            slug: block.slug.to_string(),
                            chapter: parts.chapter.to_string(),
                        });
                    }
                    if let Some(ch) = current_chapter {
                        closed_chapters.insert(ch);
                    }
                    current_chapter = Some(parts.chapter);
                }
            }
            if !batch_slugs.insert(&block.slug) {
                return Err(CorpusError::DuplicateSlugInBatch {
                    slug: block.slug.to_string(),
                });
            }
        }

        // Proven no-op: every block byte-equals an existing same-slug book (so
        // no new slug and no content change). Skip the splice and layout rebuild
        // entirely, preserving the corpus and its derived metadata exactly.
        if batch.iter().all(|block| self.book_matches(block)) {
            return Ok(MutationEffect::Unchanged);
        }

        // 2. Map each batch slug to its slot, and lay out the existing books so
        //    replacements stay in place and the untouched books carry by move.
        let mut slots: Vec<Option<BookBlock>> = batch.into_iter().map(Some).collect();
        // The lookup map clones each slug (a short book code) — never a key or
        // text `String`, so the "no text copies" contract holds.
        let slug_to_slot: FxHashMap<Box<str>, usize> = slots
            .iter()
            .enumerate()
            .map(|(i, b)| (b.as_ref().expect("slot just filled").slug.clone(), i))
            .collect();

        // (start, end, Some(slot) if this existing book is being replaced),
        // read from the owned layout — no whole-corpus key re-parse (Phase A
        // step 8); the layout already knows every book's boundaries and slug.
        let layout: Vec<(usize, usize, Option<usize>)> = self
            .layout
            .iter()
            .map(|b| (b.range.start, b.range.end, slug_to_slot.get(&*b.slug).copied()))
            .collect();

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
        //    new-slug blocks appended in batch order. Everything moves. The
        //    owned layout is maintained LOCALLY: a replaced or
        //    appended book is rebuilt from its spliced text; a carried book
        //    reuses its existing `BookLayout` — unchanged chapter boundaries and
        //    hashes — integer-rebased to its new global position. No
        //    whole-corpus re-parse; `build_layout` stays for construction.
        //    `layout` walks existing books in order, matching `old_layout`.
        let old_layout = std::mem::take(&mut self.layout);
        let mut old_keys = std::mem::take(&mut self.keys).into_iter();
        let mut old_texts = std::mem::take(&mut self.texts).into_iter();
        let mut new_keys: Vec<String> = Vec::with_capacity(final_len);
        let mut new_texts: Vec<String> = Vec::with_capacity(final_len);
        let mut new_layout: Vec<BookLayout> = Vec::with_capacity(old_layout.len() + slots.len());
        for ((s, e, replaced), old_bl) in layout.into_iter().zip(old_layout) {
            let n = e - s;
            let cursor = new_keys.len();
            if let Some(i) = replaced {
                for _ in 0..n {
                    old_keys.next();
                    old_texts.next();
                }
                let block = slots[i].take().expect("replacement slot present");
                new_keys.extend(block.keys);
                new_texts.extend(block.texts);
                new_layout.push(build_book_at(&new_keys, &new_texts, cursor));
            } else {
                for _ in 0..n {
                    new_keys.push(old_keys.next().expect("layout bounded by corpus length"));
                    new_texts.push(old_texts.next().expect("layout bounded by corpus length"));
                }
                // Carried unchanged book: reuse its layout, rebased to `cursor`.
                let mut bl = old_bl;
                shift_book(&mut bl, cursor);
                new_layout.push(bl);
            }
        }
        for slot in &mut slots {
            if let Some(block) = slot.take() {
                let cursor = new_keys.len();
                new_keys.extend(block.keys);
                new_texts.extend(block.texts);
                new_layout.push(build_book_at(&new_keys, &new_texts, cursor));
            }
        }
        self.keys = new_keys;
        self.texts = new_texts;
        self.layout = new_layout;
        Ok(MutationEffect::Changed)
    }

    /// Does an existing same-slug book byte-equal `block`? The owned book hash
    /// is now a chapter-hash fold ([`fold_book_hash`]), not a flat hash of the
    /// block's verses, so it is not a cheap pre-filter against raw block bytes;
    /// the real proof — and it always was, a hash match alone never proves it
    /// for untrusted replacement bytes — is the ordered length + semantic
    /// comparison, which early-exits on the first difference.
    fn book_matches(&self, block: &BookBlock) -> bool {
        let Some(book) = self.layout.iter().find(|b| *b.slug == *block.slug) else {
            return false;
        };
        book.range.len() == block.keys.len()
            && self.keys[book.range.clone()] == block.keys[..]
            && self.texts[book.range.clone()] == block.texts[..]
    }

    /// Atomically replace exactly one existing `(slug, chapter)` run with a
    /// complete [`ChapterBlock`]. Validates fully (matched non-empty
    /// `keys`/`texts`; every key parses to `slug` and `chapter`; the run
    /// exists; the resulting book/corpus stay addressable) before any mutation,
    /// so a rejected block leaves the corpus untouched. Caller order and
    /// duplicate keys inside the chapter are preserved. Whole-chapter
    /// insertion/removal/reorder is a whole-book [`replace_books`] instead.
    ///
    /// Returns [`MutationEffect::Unchanged`] — skipping the splice — when the
    /// block byte-equals the current run (a proven no-op), else
    /// [`MutationEffect::Changed`].
    pub fn replace_chapter(&mut self, block: ChapterBlock) -> Result<MutationEffect, CorpusError> {
        // 1. Shape validation.
        if block.keys.len() != block.texts.len() {
            return Err(CorpusError::MismatchedLengths {
                keys: block.keys.len(),
                texts: block.texts.len(),
            });
        }
        if block.keys.is_empty() {
            return Err(CorpusError::EmptyChapterBlock {
                slug: block.slug.to_string(),
                chapter: block.chapter.to_string(),
            });
        }
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
            if parts.chapter != &*block.chapter {
                return Err(CorpusError::ChapterTokenMismatch {
                    chapter: block.chapter.to_string(),
                    key: k.clone(),
                });
            }
        }

        // 2. Locate the unique matching run. Corpus already guarantees a
        //    chapter is one contiguous non-reopening run, so there is at most
        //    one; absent means insertion, which is a whole-book update.
        let range = self
            .layout
            .iter()
            .find(|b| *b.slug == *block.slug)
            .and_then(|b| b.chapters.iter().find(|c| *c.chapter == *block.chapter))
            .map(|c| c.range.clone())
            .ok_or_else(|| CorpusError::ChapterNotFound {
                slug: block.slug.to_string(),
                chapter: block.chapter.to_string(),
            })?;

        // 3. The resulting book and corpus must stay addressable, checked
        //    before any mutation so the operation is all-or-nothing.
        let book_range = self
            .layout
            .iter()
            .find(|b| *b.slug == *block.slug)
            .expect("book located above")
            .range
            .clone();
        let new_book_len = book_range.len() - range.len() + block.keys.len();
        LocalKeyIdx::try_from_usize(new_book_len, &block.slug)?;
        let new_corpus_len = self.keys.len() - range.len() + block.keys.len();
        KeyIdx::try_from_usize(new_corpus_len)?;

        // 4. Proven no-op: the block byte-equals the current run. Fast-path on
        //    the chapter hash, then confirm ordered equality (§16 footgun).
        let chapter_hash = self
            .layout
            .iter()
            .find(|b| *b.slug == *block.slug)
            .and_then(|b| b.chapters.iter().find(|c| *c.chapter == *block.chapter))
            .map(|c| c.hash)
            .expect("chapter located above");
        if range.len() == block.keys.len()
            && chapter_hash == content_hash(&block.keys, &block.texts)
            && self.keys[range.clone()] == block.keys[..]
            && self.texts[range.clone()] == block.texts[..]
        {
            return Ok(MutationEffect::Unchanged);
        }

        // 5. Splice the run in place (preserves surrounding chapters, order,
        //    and duplicates) and maintain the owned layout LOCALLY: rebuild
        //    only the affected book's chapter layouts/hashes and rebase the
        //    later books' global ranges. The book's own start is unchanged
        //    (in-place chapter splice), so the later books simply re-tile from
        //    its new end — books are contiguous, so each starts exactly where
        //    the previous one ends.
        let book_idx = self
            .layout
            .iter()
            .position(|b| *b.slug == *block.slug)
            .expect("book located above");
        let book_start = self.layout[book_idx].range.start;
        self.keys.splice(range.clone(), block.keys);
        self.texts.splice(range, block.texts);
        self.layout[book_idx] = build_book_at(&self.keys, &self.texts, book_start);
        let mut cursor = self.layout[book_idx].range.end;
        for b in &mut self.layout[book_idx + 1..] {
            shift_book(b, cursor);
            cursor = b.range.end;
        }
        Ok(MutationEffect::Changed)
    }

    /// Decompose a global index into its chapter-local address — book slug,
    /// opaque chapter token, and the verse's index within that chapter's
    /// contiguous run — from the owned layout. Books and chapters are
    /// range-contiguous and ascending, so each level is a binary search. Every
    /// `KeyIdx` a finding carries this call is valid against this same `Corpus`,
    /// so it always resolves. This is the decompose half of the resident
    /// finding partition's chapter-local addressing: a partition stores this
    /// address, never a global `KeyIdx`, so an earlier insertion cannot
    /// invalidate a later record (the rebase happens once at assembly).
    pub(crate) fn locate(&self, idx: KeyIdx) -> ChapterAddr<'_> {
        let g = idx.0 as usize;
        let bi = self.layout.partition_point(|b| b.range.end <= g);
        let book = &self.layout[bi];
        let ci = book.chapters.partition_point(|c| c.range.end <= g);
        let chapter = &book.chapters[ci];
        ChapterAddr {
            slug: &book.slug,
            chapter: &chapter.chapter,
            local: LocalKeyIdx::from_usize(g - chapter.range.start),
        }
    }

    /// The global base index of a chapter run (its start), by book slug + opaque
    /// chapter token — the rebase half of chapter-local addressing: a partition
    /// record's `(slug, chapter, local)` resolves to `chapter_base + local`.
    /// `None` when the chapter is absent (e.g. after a book/chapter removal), so
    /// a stale cross-call record is dropped rather than mis-rebased.
    /// The current global verse range of `(slug, chapter)`, or `None` when that
    /// chapter no longer exists. Existence alone is NOT containment proof for a
    /// retained chapter-local record: a shrunk chapter leaves a stale local
    /// index globally in-bounds but pointing into the next chapter/book, so the
    /// caller must also check the local index against this range's length
    /// before rebasing.
    pub(crate) fn chapter_range(&self, slug: &str, chapter: &str) -> Option<Range<usize>> {
        self.layout
            .iter()
            .find(|b| *b.slug == *slug)
            .and_then(|b| b.chapters.iter().find(|c| *c.chapter == *chapter))
            .map(|c| c.range.clone())
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
                // Maintain the owned layout LOCALLY: drop this book's layout
                // and re-tile every later book from the removed run's start —
                // no whole-corpus re-parse. Books are contiguous, so each
                // starts exactly where the previous one ends.
                let book_idx = self
                    .layout
                    .iter()
                    .position(|b| *b.slug == *slug)
                    .expect("book located in the same walk above");
                self.layout.remove(book_idx);
                let mut cursor = start;
                for b in &mut self.layout[book_idx..] {
                    shift_book(b, cursor);
                    cursor = b.range.end;
                }
                return true;
            }
            start = end;
        }
        false
    }
}

/// A verse's chapter-local address: book slug, opaque chapter token, and the
/// verse's index within that chapter run. Produced by [`Corpus::locate`] and
/// stored (as owned slug/token) in a resident finding partition; rebased back
/// to a global [`KeyIdx`] via [`Corpus::chapter_base`] + [`rebase`].
pub(crate) struct ChapterAddr<'a> {
    pub(crate) slug: &'a str,
    pub(crate) chapter: &'a str,
    pub(crate) local: LocalKeyIdx,
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
/// corpus's presented order (not canonical book order). Reads the corpus's
/// owned [`BookLayout`] instead of re-parsing every key — the layout is the
/// authoritative record of the same book boundaries, kept current by
/// construction and every mutation.
pub fn by_book(corpus: &Corpus) -> Books<'_> {
    corpus
        .layout
        .iter()
        .map(|book| BookGroup {
            slug: &book.slug,
            base: KeyIdx::from_usize(book.range.start),
            keys: &corpus.keys[book.range.clone()],
            texts: &corpus.texts[book.range.clone()],
        })
        .collect()
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
    fn rejects_reopened_chapter_in_construction() {
        // GEN chapter 1 closes when chapter 2 starts; 1 then reappearing is a
        // reopened chapter (distinct from an out-of-order verse within one
        // chapter, which is legal — see below).
        let err = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 2:1", "GEN 1:2"]),
            texts(3),
        )
        .unwrap_err();
        assert_eq!(
            err,
            CorpusError::ReopenedChapter {
                slug: "GEN".to_string(),
                chapter: "1".to_string(),
            }
        );
    }

    #[test]
    fn accepts_out_of_order_verses_within_one_chapter() {
        // Out-of-order verse tokens in the SAME chapter run are legal — the
        // chapter did not reopen, only the verse order is noncanonical.
        let c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:3", "GEN 1:2"]),
            texts(3),
        )
        .unwrap();
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn accepts_noncanonical_but_contiguous_chapter_runs() {
        // Chapters need not be in numeric order, only contiguous: `3` then `1`
        // is fine as long as neither reopens.
        let c = Corpus::try_from_parts(
            keys(&["GEN 3:1", "GEN 3:2", "GEN 1:1"]),
            texts(3),
        )
        .unwrap();
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn a_chapter_token_may_recur_in_a_different_book() {
        // Chapter tokens are book-local: GEN 1 and EXO 1 do not collide.
        let c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "EXO 1:1"]),
            texts(2),
        )
        .unwrap();
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn replace_books_rejects_a_reopened_chapter_in_a_block() {
        let mut c = Corpus::try_from_parts(keys(&["GEN 1:1"]), keys(&["g"])).unwrap();
        let err = c
            .replace_books(vec![block(
                "GEN",
                &["GEN 1:1", "GEN 2:1", "GEN 1:2"],
                &["a", "b", "c"],
            )])
            .unwrap_err();
        assert_eq!(
            err,
            CorpusError::ReopenedChapter {
                slug: "GEN".to_string(),
                chapter: "1".to_string(),
            }
        );
        assert_eq!(c.keys(), keys(&["GEN 1:1"]).as_slice(), "rejected block is a no-op");
    }

    /// The owned layout records presented-order books and their contiguous
    /// chapter runs with correct global ranges; each chapter hash is the flat
    /// content hash of its own verse slice, and the book hash is the ordered
    /// fold of its `(chapter token, chapter hash)` pairs.
    #[test]
    fn owned_layout_ranges_and_hashes_are_correct() {
        let c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "GEN 2:1", "EXO 1:1"]),
            keys(&["g11", "g12", "g21", "e11"]),
        )
        .unwrap();
        let layout = c.book_layout();
        assert_eq!(layout.len(), 2);

        assert_eq!(&*layout[0].slug, "GEN");
        assert_eq!(layout[0].range, 0..3);
        assert_eq!(layout[0].chapters.len(), 2);
        assert_eq!(&*layout[0].chapters[0].chapter, "1");
        assert_eq!(layout[0].chapters[0].range, 0..2);
        assert_eq!(&*layout[0].chapters[1].chapter, "2");
        assert_eq!(layout[0].chapters[1].range, 2..3);

        assert_eq!(&*layout[1].slug, "EXO");
        assert_eq!(layout[1].range, 3..4);

        // Book hash == the ordered fold of its (chapter token, chapter hash)
        // pairs, NOT the flat content hash of its verse slice.
        let groups = by_book(&c);
        for (book, group) in layout.iter().zip(groups.iter()) {
            assert_eq!(
                book.hash,
                fold_book_hash(&book.chapters),
                "book hash is the chapter-hash fold"
            );
            assert_ne!(
                book.hash,
                content_hash(group.keys, group.texts),
                "book hash is no longer the flat verse hash"
            );
        }
        // Chapter hash == the content hash of just that chapter's verses.
        let gen_book = &layout[0];
        assert_eq!(
            gen_book.chapters[0].hash,
            content_hash(&c.keys()[0..2], &c.texts()[0..2])
        );
    }

    /// Every mutation rebuilds the owned layout atomically: after a
    /// replace-in-place, an append-new, and a remove, the layout equals a
    /// freshly-constructed corpus's layout over the same final vectors.
    #[test]
    fn mutations_keep_owned_layout_current() {
        let mut c =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();

        // replace-in-place GEN (grows a chapter) + append-new LEV.
        c.replace_books(vec![
            block("GEN", &["GEN 1:1", "GEN 1:2"], &["G1", "G2"]),
            block("LEV", &["LEV 1:1"], &["l"]),
        ])
        .unwrap();
        let expect = Corpus::try_from_parts(c.keys().to_vec(), c.texts().to_vec()).unwrap();
        assert_eq!(c.layout, expect.layout, "layout current after replace_books");

        // remove EXO.
        c.remove_book("EXO");
        let expect = Corpus::try_from_parts(c.keys().to_vec(), c.texts().to_vec()).unwrap();
        assert_eq!(c.layout, expect.layout, "layout current after remove_book");
    }

    // ── Book-hash fold ───────────────────────────────────────────────────────

    /// The folded book hash is sensitive to chapter ORDER: two books with the
    /// identical set of chapters presented in a different order fold to
    /// different hashes (the fold hashes chapters in presented order).
    #[test]
    fn folded_book_hash_is_chapter_order_sensitive() {
        // Chapter tokens presented "1","2" vs "2","1" — each contiguous and
        // non-reopening, so both are legal Corpora (Corpus never numerically
        // orders tokens); only their order differs.
        let forward =
            Corpus::try_from_parts(keys(&["GEN 1:1", "GEN 2:1"]), keys(&["a", "b"])).unwrap();
        let reversed =
            Corpus::try_from_parts(keys(&["GEN 2:1", "GEN 1:1"]), keys(&["b", "a"])).unwrap();
        assert_ne!(
            forward.book_layout()[0].hash,
            reversed.book_layout()[0].hash,
            "reordering chapters moves the folded book hash"
        );
    }

    /// The folded book hash is sensitive to the chapter TOKEN: the same verse
    /// content under a different chapter token folds to a different hash.
    #[test]
    fn folded_book_hash_is_chapter_token_sensitive() {
        let a = Corpus::try_from_parts(keys(&["GEN 1:1"]), keys(&["x"])).unwrap();
        let b = Corpus::try_from_parts(keys(&["GEN 2:1"]), keys(&["x"])).unwrap();
        assert_ne!(
            a.book_layout()[0].hash,
            b.book_layout()[0].hash,
            "a different chapter token moves the folded book hash"
        );
    }

    /// Identical final content folds to the identical book hash regardless of
    /// how it was reached — direct construction, `replace_books`, or
    /// `replace_chapter` — so provenance/cache equality is path-independent.
    #[test]
    fn folded_book_hash_identical_across_construction_and_mutation() {
        let direct = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "GEN 2:1"]),
            keys(&["a", "b", "c"]),
        )
        .unwrap();
        let target = direct.book_layout()[0].hash;

        // Reach the same content via a whole-book replacement.
        let mut m1 = Corpus::try_from_parts(keys(&["GEN 1:1"]), keys(&["z"])).unwrap();
        m1.replace_books(vec![block(
            "GEN",
            &["GEN 1:1", "GEN 1:2", "GEN 2:1"],
            &["a", "b", "c"],
        )])
        .unwrap();
        assert_eq!(m1.book_layout()[0].hash, target, "replace_books folds identically");

        // Reach the same content via a single-chapter replacement.
        let mut m2 = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "GEN 2:1"]),
            keys(&["A", "B", "c"]),
        )
        .unwrap();
        m2.replace_chapter(chapter_block("GEN", "1", &["GEN 1:1", "GEN 1:2"], &["a", "b"]))
            .unwrap();
        assert_eq!(m2.book_layout()[0].hash, target, "replace_chapter folds identically");
    }

    fn chapter_block(slug: &str, chapter: &str, ks: &[&str], txt: &[&str]) -> ChapterBlock {
        ChapterBlock {
            slug: slug.into(),
            chapter: chapter.into(),
            keys: keys(ks),
            texts: keys(txt),
        }
    }

    /// Every length-narrowing mutation maintains the owned
    /// layout LOCALLY (rebuild the changed book, rebase later books) instead of
    /// re-parsing the whole corpus. This pins the equivalence the perf win rests
    /// on: after every mutation shape in §12.1's menu, the locally-maintained
    /// layout must be byte-for-byte what a from-scratch `build_layout` produces.
    #[test]
    fn local_layout_maintenance_equals_from_scratch_every_mutation_shape() {
        let assert_layout = |c: &Corpus, shape: &str| {
            assert_eq!(
                c.layout,
                build_layout(c.keys(), c.texts()),
                "spliced layout diverged from a from-scratch build after {shape}"
            );
        };

        // A multi-book, multi-chapter corpus with a duplicate and out-of-order
        // verse tokens (all legal), so the rebased suffix is non-trivial.
        let mut c = Corpus::try_from_parts(
            keys(&[
                "GEN 1:1", "GEN 1:3", "GEN 1:2", "GEN 2:1", "EXO 1:1", "EXO 1:1", "LEV 1:1",
                "LEV 1:2",
            ]),
            texts(8),
        )
        .unwrap();
        assert_layout(&c, "construction");

        // replace-in-place, changing the book's length (2 GEN ch1 verses -> 3),
        // so every later book must rebase.
        c.replace_books(vec![block(
            "GEN",
            &["GEN 1:1", "GEN 1:2", "GEN 1:3", "GEN 2:1"],
            &["a", "b", "c", "d"],
        )])
        .unwrap();
        assert_layout(&c, "replace-in-place (length change)");

        // append-new book at the end.
        c.replace_books(vec![block("NUM", &["NUM 1:1", "NUM 1:2"], &["n1", "n2"])])
            .unwrap();
        assert_layout(&c, "append-new");

        // chapter replace, changing the run length (EXO ch1: 1 verse -> 2).
        c.replace_chapter(chapter_block("EXO", "1", &["EXO 1:1", "EXO 1:2"], &["x", "y"]))
            .unwrap();
        assert_layout(&c, "chapter replace (length change)");

        // no-op chapter replace (byte-identical) leaves the layout untouched.
        c.replace_chapter(chapter_block("EXO", "1", &["EXO 1:1", "EXO 1:2"], &["x", "y"]))
            .unwrap();
        assert_layout(&c, "chapter replace no-op");

        // no-op book replace (byte-identical) leaves the layout untouched.
        c.replace_books(vec![block("NUM", &["NUM 1:1", "NUM 1:2"], &["n1", "n2"])])
            .unwrap();
        assert_layout(&c, "book replace no-op");

        // remove a middle book, rebasing the books after it back.
        c.remove_book("EXO");
        assert_layout(&c, "remove middle book");

        // remove the first book, rebasing everything back to 0.
        c.remove_book("GEN");
        assert_layout(&c, "remove first book");

        // remove the last remaining books down to empty.
        c.remove_book("LEV");
        c.remove_book("NUM");
        assert_layout(&c, "remove to empty");
    }

    /// Replacing an existing chapter run splices it in place (surrounding
    /// chapters and books untouched), preserves duplicate keys, updates the
    /// owned layout, and reports `Changed`. Insert/delete/reorder of verses
    /// inside the replacement is allowed (the run may change length).
    #[test]
    fn replace_chapter_splices_in_place_and_reports_changed() {
        let mut c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "GEN 2:1", "EXO 1:1"]),
            keys(&["g11", "g12", "g21", "e11"]),
        )
        .unwrap();
        // Replace GEN chapter 1 (2 verses) with 3 verses, one a duplicate key
        // and one reordered — a legal within-chapter reshape.
        let effect = c
            .replace_chapter(chapter_block(
                "GEN",
                "1",
                &["GEN 1:3", "GEN 1:3", "GEN 1:1"],
                &["n3", "n3b", "n1"],
            ))
            .unwrap();
        assert_eq!(effect, MutationEffect::Changed);
        assert_eq!(
            c.keys(),
            keys(&["GEN 1:3", "GEN 1:3", "GEN 1:1", "GEN 2:1", "EXO 1:1"]).as_slice()
        );
        assert_eq!(c.texts(), keys(&["n3", "n3b", "n1", "g21", "e11"]).as_slice());
        // Layout equals a freshly-built corpus over the spliced vectors.
        let expect = Corpus::try_from_parts(c.keys().to_vec(), c.texts().to_vec()).unwrap();
        assert_eq!(c.layout, expect.layout);
    }

    /// A byte-identical chapter re-supply is a proven no-op: `Unchanged`, and
    /// the corpus is untouched.
    #[test]
    fn replace_chapter_byte_identical_is_a_no_op() {
        let mut c = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 1:2", "GEN 2:1"]),
            keys(&["g11", "g12", "g21"]),
        )
        .unwrap();
        let before = c.clone();
        let effect = c
            .replace_chapter(chapter_block("GEN", "1", &["GEN 1:1", "GEN 1:2"], &["g11", "g12"]))
            .unwrap();
        assert_eq!(effect, MutationEffect::Unchanged);
        assert_eq!(c, before, "a no-op chapter replace leaves the corpus untouched");
    }

    /// Every `replace_chapter` rejection is atomic — the corpus is untouched —
    /// and each validation case fires its own error.
    #[test]
    fn replace_chapter_rejections_are_atomic() {
        let original = Corpus::try_from_parts(
            keys(&["GEN 1:1", "GEN 2:1"]),
            keys(&["g11", "g21"]),
        )
        .unwrap();

        // empty block
        let mut c = original.clone();
        let err = c
            .replace_chapter(ChapterBlock {
                slug: "GEN".into(),
                chapter: "1".into(),
                keys: Vec::new(),
                texts: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, CorpusError::EmptyChapterBlock { .. }));
        assert_eq!(c, original);

        // mismatched lengths
        let mut c = original.clone();
        let err = c
            .replace_chapter(ChapterBlock {
                slug: "GEN".into(),
                chapter: "1".into(),
                keys: keys(&["GEN 1:1", "GEN 1:2"]),
                texts: keys(&["only-one"]),
            })
            .unwrap_err();
        assert!(matches!(err, CorpusError::MismatchedLengths { .. }));
        assert_eq!(c, original);

        // key's book is not the block slug
        let mut c = original.clone();
        let err = c
            .replace_chapter(chapter_block("GEN", "1", &["EXO 1:1"], &["x"]))
            .unwrap_err();
        assert!(matches!(err, CorpusError::SlugMismatch { .. }));
        assert_eq!(c, original);

        // key's chapter is not the block chapter
        let mut c = original.clone();
        let err = c
            .replace_chapter(chapter_block("GEN", "1", &["GEN 2:9"], &["x"]))
            .unwrap_err();
        assert!(matches!(err, CorpusError::ChapterTokenMismatch { .. }));
        assert_eq!(c, original);

        // no such chapter run exists (insertion is a whole-book update)
        let mut c = original.clone();
        let err = c
            .replace_chapter(chapter_block("GEN", "9", &["GEN 9:1"], &["x"]))
            .unwrap_err();
        assert!(matches!(err, CorpusError::ChapterNotFound { .. }));
        assert_eq!(c, original);
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

    /// Replacing a book in place (same slug, new/longer text) splices at
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

    /// A mixed batch replaces in place and appends new books in batch
    /// order. Two new slugs (NUM before LEV) straddle the EXO replacement, so
    /// the append order genuinely reflects batch order — not slug order or the
    /// replacement's position.
    #[test]
    fn replace_books_appends_new_slugs_in_batch_order() {
        let mut c =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();
        c.replace_books(vec![
            block("NUM", &["NUM 1:1"], &["n"]),                    // new (batch idx 0)
            block("EXO", &["EXO 1:1", "EXO 1:2"], &["E1", "E2"]), // replacement
            block("LEV", &["LEV 1:1"], &["l"]),                    // new (batch idx 2)
        ])
        .unwrap();
        // GEN carried, EXO replaced in place, then new books in batch order
        // (NUM before LEV), regardless of the replacement sitting between them.
        assert_eq!(
            c.keys(),
            keys(&["GEN 1:1", "EXO 1:1", "EXO 1:2", "NUM 1:1", "LEV 1:1"]).as_slice()
        );
        assert_eq!(c.texts(), keys(&["g", "E1", "E2", "n", "l"]).as_slice());
    }

    /// A batch failing on its LAST block leaves the corpus untouched —
    /// validation is complete before any splice (all-or-nothing). Each case
    /// puts a valid block first so the failure is genuinely late.
    #[test]
    fn replace_books_is_atomic_on_a_late_failure() {
        let original =
            Corpus::try_from_parts(keys(&["GEN 1:1", "EXO 1:1"]), keys(&["g", "e"])).unwrap();
        let good = || block("GEN", &["GEN 1:1"], &["G"]);

        // SlugMismatch: last block's key parses to a different book.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![good(), block("EXO", &["GEN 1:9"], &["x"])])
            .unwrap_err();
        assert!(matches!(err, CorpusError::SlugMismatch { .. }));
        assert_eq!(c, original, "a rejected batch leaves the corpus untouched");

        // MismatchedLengths on the last block.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![
                good(),
                BookBlock {
                    slug: "EXO".into(),
                    keys: keys(&["EXO 1:1", "EXO 1:2"]),
                    texts: keys(&["only-one"]),
                },
            ])
            .unwrap_err();
        assert!(matches!(err, CorpusError::MismatchedLengths { .. }));
        assert_eq!(c, original);

        // BookTooLarge: last block past the LocalKeyIdx u16 ceiling.
        let big_keys: Vec<String> = (0..=u32::from(u16::MAX) + 1)
            .map(|v| format!("EXO 1:{v}"))
            .collect();
        let n = big_keys.len();
        let mut c = original.clone();
        let err = c
            .replace_books(vec![
                good(),
                BookBlock {
                    slug: "EXO".into(),
                    keys: big_keys,
                    texts: texts(n),
                },
            ])
            .unwrap_err();
        assert!(matches!(err, CorpusError::BookTooLarge { .. }));
        assert_eq!(c, original);

        // DuplicateSlugInBatch: the duplicate is the last block.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![good(), block("GEN", &["GEN 1:2"], &["b"])])
            .unwrap_err();
        assert!(matches!(err, CorpusError::DuplicateSlugInBatch { .. }));
        assert_eq!(c, original);

        // EmptyBook (last block): an empty block is an error, never a removal.
        let mut c = original.clone();
        let err = c
            .replace_books(vec![
                good(),
                BookBlock {
                    slug: "EXO".into(),
                    keys: Vec::new(),
                    texts: Vec::new(),
                },
            ])
            .unwrap_err();
        assert!(matches!(err, CorpusError::EmptyBook { .. }));
        assert_eq!(c, original);
    }

    /// `remove_book` returns true/false and removing the
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
