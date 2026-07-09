//! The unit sous analyzes: a map of `{ sid -> text }`.
//!
//! `text` is the **lossless** plain text of a verse, exactly as onion's
//! vref projection produced it (markup stripped, no collapse, no trim).
//! sous does NOT normalise (no NFC) or re-derive the **source → verse**
//! segmentation — onion is the single segmenter of record for that, and
//! re-deriving it here would silently diverge from the editor's snapshot.
//! (Core does segment this verse text into grapheme clusters internally for
//! its grapheme-level rules — ADR 0021 — but that is downstream of the text
//! onion handed it, not a re-segmentation of the source.) The wrappers (CLI,
//! wasm) obtain this map from onion; core never reads files or calls onion.
//! See ADR 0010.

use std::collections::BTreeMap;

use crate::sid::{BookId, Sid};

/// `{ sid -> verse text }`. Equivalent to onion's `VrefMap`. Hand
/// `analyze` one of these for a verse, a book, or a whole project.
pub type VerseMap = BTreeMap<Sid, String>;

/// A `VerseMap` grouped by book — the unit the stateful phase fans out over
/// (ADR 0042). Books iterate in `BookId` order; each book's verses stay in
/// canonical `(chapter, verse)` order.
pub type Books<'a> = BTreeMap<BookId, Vec<(Sid, &'a str)>>;

/// Group a `VerseMap`'s verses by book, each book's verses in canonical
/// `(chapter, verse)` order. The grouping is free: `VerseMap` is a
/// `BTreeMap<Sid, _>` already ordered by `(book, chapter, verse)`, so this
/// just chunks on book boundaries — it never re-sorts. The shared shape
/// every book-scoped rule walks (bracket-balance, duplicate-word). See
/// ADR 0016. `analyze_stateful` computes it **once** and hands it to every
/// stateful rule (ADR 0042) — rules never rebuild it per phase.
pub fn by_book(map: &VerseMap) -> Books<'_> {
    let mut books: Books<'_> = BTreeMap::new();
    for (sid, text) in map {
        books.entry(sid.book).or_default().push((*sid, text.as_str()));
    }
    books
}
