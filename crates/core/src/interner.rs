//! The cache-owned word interner — one shared, append-only table of folded
//! word types, so a chapter's cached observation stores 4-byte symbols instead
//! of its own owned copy of every word type it contains.
//!
//! Why this exists, and why it is *only* this: measured, in
//! `documentation/calibration/2026-07-24-word-interner-spike.md`. That spike
//! rejected the tempting uniform shape — dense symbols everywhere, with a
//! string-sorted permutation rebuilt so an order-sensitive aggregate can still
//! be summed in sorted order (175–216 ns/key against 2.6–2.9 ns/key for a
//! natively-ordered map: 60–80x worse). What it found genuinely winning was
//! narrower and is exactly what this module serves:
//!
//! - a warm interner lookup is a borrow-only hash probe (~8.4 ns/word) where a
//!   `BTreeMap::entry()` must allocate a fresh owned key on every call
//!   (~155 ns/word) because `entry`'s signature demands an owned `K`; and
//! - a per-chapter list only ever needs to *reference* a word type, never own
//!   one, and a 4-byte symbol beats every owned-string representation measured
//!   at that job (6x against a 24-byte inline string, unconditionally, on both
//!   a highly repetitive and a hapax-heavy corpus).
//!
//! So: symbols in the per-chapter tables, natively-ordered owned keys in the
//! per-book aggregate the judge sums over. Anyone tempted to push the symbols
//! further — a dense `Vec`-by-symbol corpus aggregate, an incrementally
//! maintained sorted-symbol permutation — should read that calibration document
//! first; both were measured and both lost.
//!
//! ## Invariants
//!
//! - **Append-only.** A symbol's meaning is fixed the moment it is assigned and
//!   is never reused or renumbered. This is what makes a cached observation
//!   mapped at any earlier time comparable with one mapped now: symbol equality
//!   is string equality, permanently, so an observation's symbols stay valid
//!   evidence across any number of edits.
//! - **Interior mutability, because mapping fans out.** Chapter mapping runs
//!   under the ordered chapter-parallel seam, so the shared table is reached
//!   through `&self` from several threads. Interning is batched one lock per
//!   chapter (a chapter's whole first-sight key list at once), never one lock
//!   per word.
//! - **Symbol *numbers* are not part of any answer.** Which number a word gets
//!   depends on the order chapters happened to finish mapping, so it is not
//!   deterministic across thread counts — and nothing downstream may depend on
//!   it. Every order that reaches a finding is a string order (the per-book
//!   table is sorted by resolved key), so findings are byte-identical
//!   regardless of how symbols were numbered.
//! - **Owned resolution.** [`WordInterner::resolve_all`] hands back `Arc<str>`
//!   clones rather than borrows: the arena lives behind a lock, and a retained
//!   judge model must be able to outlive any borrow of it (it can even outlive
//!   a `clear()` — the `Arc`s keep their bytes alive, so there is no ordering
//!   hazard between dropping the table and dropping a model built from it).

use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;

/// A folded word type's symbol in a [`WordInterner`]. Meaningful only against
/// the interner that issued it (one per cache), and permanent within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WordSym(u32);

#[derive(Default)]
struct Inner {
    /// symbol → the word's bytes. Indexed by `WordSym.0`.
    arena: Vec<Arc<str>>,
    /// The word's bytes → symbol. Shares each allocation with `arena`, so a
    /// word's bytes are stored once, not once per lookup direction.
    index: FxHashMap<Arc<str>, u32>,
}

/// The shared, append-only folded-word table. Cache-owned: cleared only when
/// the whole substrate section is (a corpus replacement or the
/// last-consumer-disabled invalidation). `remove_book` deliberately does **not**
/// compact it — compacting would renumber live symbols in every other book's
/// cached observations, which is the one thing the append-only invariant forbids.
/// The cost of that choice is dead symbols: a removed book's unique word types
/// keep one arena slot and one small allocation each until the section is
/// cleared, so the table's size is bounded by the distinct folded word types
/// this cache has ever seen rather than the ones it currently holds.
#[derive(Default)]
pub(crate) struct WordInterner {
    inner: Mutex<Inner>,
}

impl WordInterner {
    /// Intern one chapter's first-sight key list, in order, under a single lock.
    /// Consumes the keys: a miss moves the bytes into the arena, a hit drops
    /// them. The returned symbols are positionally aligned with `keys`, so a
    /// chapter-local id remains an index into this vector.
    pub(crate) fn intern_all(&self, keys: Vec<String>) -> Vec<WordSym> {
        let mut inner = self.lock();
        // Sized from the batch itself rather than a corpus-level guess: the
        // worst case is "every key in this chapter is new", which is exactly
        // `keys.len()`, and a reserve that is already satisfied costs nothing.
        inner.index.reserve(keys.len());
        inner.arena.reserve(keys.len());
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let sym = match inner.index.get(key.as_str()) {
                Some(&sym) => sym,
                None => {
                    let sym = inner.arena.len() as u32;
                    let shared: Arc<str> = Arc::from(key);
                    inner.arena.push(Arc::clone(&shared));
                    inner.index.insert(shared, sym);
                    sym
                }
            };
            out.push(WordSym(sym));
        }
        out
    }

    /// Resolve symbols to their words under a single lock. Each resolution is a
    /// refcount bump, not a copy.
    pub(crate) fn resolve_all(
        &self,
        syms: impl ExactSizeIterator<Item = WordSym>,
    ) -> Vec<Arc<str>> {
        let inner = self.lock();
        syms.map(|WordSym(s)| Arc::clone(&inner.arena[s as usize]))
            .collect()
    }

    /// How many distinct word types this table holds — the growth bound above,
    /// observable for tests and probes.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn len(&self) -> usize {
        self.lock().arena.len()
    }

    /// A poisoned lock carries no half-written state: `intern_all` cannot unwind
    /// between pushing to the arena and inserting the index entry (neither the
    /// hash nor the allocation can panic there without aborting), so recovering
    /// the guard is safe and keeps a panicking consumer from cascading.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_are_stable_and_deduplicated() {
        let interner = WordInterner::default();
        let a = interner.intern_all(vec!["god".into(), "said".into(), "god".into()]);
        assert_eq!(a[0], a[2], "the same word interns to the same symbol");
        assert_ne!(a[0], a[1]);
        assert_eq!(interner.len(), 2, "only distinct types enter the arena");

        // A later batch sees the earlier symbols unchanged — the invariant a
        // cached observation's stored symbols depend on.
        let b = interner.intern_all(vec!["said".into(), "light".into()]);
        assert_eq!(b[0], a[1]);
        assert_eq!(interner.len(), 3);

        let words = interner.resolve_all(vec![a[0], a[1], b[1]].into_iter());
        assert_eq!(
            words.iter().map(|w| &**w).collect::<Vec<_>>(),
            vec!["god", "said", "light"]
        );
    }
}
