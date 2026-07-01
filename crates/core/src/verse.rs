//! The unit sous analyzes: a map of `{ sid -> text }`.
//!
//! `text` is the **lossless** plain text of a verse, exactly as onion's
//! vref projection produced it (markup stripped, no collapse, no trim).
//! sous does NOT normalise (no NFC) or segment it — onion is the single
//! segmenter of record, and re-deriving here would silently diverge from
//! the editor's snapshot. The wrappers (CLI, wasm) obtain this map from
//! onion; core never reads files or calls onion. See ADR 0010.

use std::collections::BTreeMap;

use crate::sid::{BookId, Sid};

/// `{ sid -> verse text }`. Equivalent to onion's `VrefMap`. Hand
/// `analyze` one of these for a verse, a book, or a whole project.
pub type VerseMap = BTreeMap<Sid, String>;

/// Group a `VerseMap`'s verses by book, each book's verses in canonical
/// `(chapter, verse)` order. The grouping is free: `VerseMap` is a
/// `BTreeMap<Sid, _>` already ordered by `(book, chapter, verse)`, so this
/// just chunks on book boundaries — it never re-sorts. The shared shape
/// every book-scoped rule walks (bracket-balance, duplicate-word). See
/// ADR 0016.
pub fn by_book(map: &VerseMap) -> BTreeMap<BookId, Vec<(Sid, &str)>> {
    let mut books: BTreeMap<BookId, Vec<(Sid, &str)>> = BTreeMap::new();
    for (sid, text) in map {
        books.entry(sid.book).or_default().push((*sid, text.as_str()));
    }
    books
}
