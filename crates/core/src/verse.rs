//! The unit sous analyzes: a map of `{ sid -> text }`.
//!
//! `text` is the **lossless** plain text of a verse, exactly as onion's
//! vref projection produced it (markup stripped, no collapse, no trim).
//! sous does NOT normalise (no NFC) or segment it — onion is the single
//! segmenter of record, and re-deriving here would silently diverge from
//! the editor's snapshot. The wrappers (CLI, wasm) obtain this map from
//! onion; core never reads files or calls onion. See ADR 0010.

use std::collections::BTreeMap;

use crate::sid::Sid;

/// `{ sid -> verse text }`. Equivalent to onion's `VrefMap`. Hand
/// `analyze` one of these for a verse, a book, or a whole project.
pub type VerseMap = BTreeMap<Sid, String>;
