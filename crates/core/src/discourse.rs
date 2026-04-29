//! Discourse-level view of a corpus. Design notes only — no impl yet.
//!
//! ## Why a separate view
//!
//! VREF (Sid → verse text) is the right shape for two things:
//! 1. Parallel-data signals (source-vs-target alignment, MAD/Z over
//!    per-Sid metrics).
//! 2. Downstream consumer ergonomics — emitting findings as
//!    `(Sid, span)` lets editors jump straight to the location.
//!
//! It is the *wrong* shape for everything else. Sentences cross verse
//! boundaries. Whitespace conventions only make sense over flowing
//! text. Sentence-start capitalisation depends on the previous
//! sentence's terminator, which may be in the previous verse. A
//! per-verse view forces every cross-verse rule to reinvent
//! verse-stitching.
//!
//! ## Shape
//!
//! ```ignore
//! pub struct Discourse<'p> {
//!     /// Verses concatenated in canonical order (book → chapter → verse),
//!     /// joined by a single ASCII space (configurable).
//!     pub text: String,
//!     /// Byte offsets into `text` where each Sid begins. Sorted.
//!     pub sid_starts: Vec<(Sid, usize)>,
//!     /// Backref to the source NamedCorpus for `Verse` lookup.
//!     pub corpus: &'p NamedCorpus<'p>,
//! }
//!
//! impl<'p> Discourse<'p> {
//!     /// Map a byte offset (or range) in `text` back to a single Sid
//!     /// or Sid range.
//!     pub fn locate(&self, byte_offset: usize) -> Sid;
//!     pub fn locate_range(&self, range: Range<usize>) -> (Sid, Sid);
//!
//!     /// Slice `text` for a finding — returns the matched substring
//!     /// for embedding into a `Finding`.
//!     pub fn slice(&self, range: Range<usize>) -> &str;
//! }
//! ```
//!
//! ## Built once, used by many
//!
//! Built at `Project` construction time alongside the per-`Verse`
//! views (matches the all-upfront-work decision in `verse.rs`).
//! Memory cost is roughly one extra copy of the corpus text plus a
//! `(Sid, usize)` per verse — small compared to per-verse token
//! arrays.
//!
//! ## Which signals use Discourse vs. Verse
//!
//! Discourse-level (operate on `Discourse`):
//! - `signals::positional::*`
//! - `signals::punctuation::*`
//!
//! Verse-level (operate on `BTreeMap<Sid, Verse>`):
//! - `signals::source_relative::*`
//! - `signals::glossary::*`
//! - `signals::hygiene::*`
//! - `signals::edit_distance::*`
//!
//! Both (use Verse for token iteration, may consult Discourse for
//! sentence boundaries):
//! - `signals::orthographic::*`
//! - `signals::lexical::*`
//!
//! ## TODO
//! - [ ] Decide configurable inter-verse joiner. Default `' '` is fine
//!       for whitespace-segmented scripts; for scriptio-continua we
//!       still want a separator so ICU4X doesn't accidentally merge
//!       verse-final and verse-initial tokens. Use a single space; if
//!       segmenter quirks emerge, switch to a sentinel (NBSP? U+2063
//!       INVISIBLE SEPARATOR?).
//! - [ ] Sentence-boundary detection: ICU4X has a SentenceSegmenter;
//!       use it on `text`, expose sentence boundaries to positional /
//!       punctuation rules.
//! - [ ] How `Finding<'a>` borrows: when a discourse-level rule
//!       produces a span, the `&str` slice is into `Discourse.text`,
//!       not `Verse.nfc`. Either (a) accept that `Finding` can borrow
//!       from either source — same lifetime — or (b) materialise the
//!       Sid-mapped Verse-relative offset and re-slice from
//!       `Verse.nfc`. (a) is simpler; (b) makes downstream consumers'
//!       lives easier. Lean (b).
