//! Direct per-verse rules and shared chapter-map helpers.
//!
//! Corpus-aware rules use the typed observation-substrate registry. A future
//! rule that cannot fit that model requires its own approved execution design;
//! this module deliberately keeps no latent fallback registry.

use crate::corpus::{BookGroup, Books};
use crate::diagnostics::{RuleId, Severity};
use crate::signals;
use crate::span::Span;
use crate::tape::{Mask, TapeEntry};

/// The hot, stateless majority. `check` reads the verse's prebuilt scalar tape
/// (ADR 0045) — one shared decode+classify pass the runner does per verse —
/// instead of each rule re-walking `text.char_indices()`. `text` rides along
/// for the handful of scans that are byte-level (tab, `?`-run, USFM/HTML
/// markers) or need `text.len()`. `pub(crate)`: the tape type is internal, and
/// no consumer outside the crate names this trait.
pub(crate) trait PerVerseRule: Sync {
    fn id(&self) -> RuleId;
    fn severity(&self) -> Severity;
    fn check(&self, text: &str, tape: &[TapeEntry]) -> Vec<Span>;
    /// The per-verse dirty-bits gate (ADR 0046): the runner skips `check` on a
    /// verse whose [`Mask`] does not open this gate. The gate must be a **safe
    /// superset** of the rule's fire set — set on every verse `check` could
    /// return a finding for. Defaults to all-pass (always run), so a rule with
    /// no cheap prefilter simply never gets skipped.
    fn gate(&self) -> Mask {
        Mask::ALL_PASS
    }
}

/// Run `f` over every book and collect the outputs **in `books`' presented
/// order** (index-aligned with `books`, which is caller order, not canonical
/// book order — see `Corpus`). Under the `parallel` feature the books fan out
/// over rayon (ADR 0042); the output is identical either way — an indexed
/// collect preserves input order, and books are disjoint — so the feature can
/// never change results, only wall-clock. This is the *one* place the
/// stateful phase's parallelism lives; rules call it and stay `cfg`-free.
pub(crate) fn map_books<T: Send>(
    books: &Books<'_>,
    f: impl Fn(&BookGroup<'_>) -> T + Sync,
) -> Vec<T> {
    #[cfg(any(test, feature = "test-probes"))]
    let _guard = fanout::Guard::enter("map_books");
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        books
            .par_iter()
            .map(|group| {
                #[cfg(any(test, feature = "test-probes"))]
                let _worker = fanout::Guard::worker();
                f(group)
            })
            .collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        books.iter().map(&f).collect()
    }
}

/// The minimum dirty-chapter map work — measured as bytes of verse text, the
/// cheapest honest proxy available before the work runs — below which fanning out
/// by chapter costs more in scheduling than it saves.
///
/// It is a **performance route only**: every route produces byte-identical
/// output, so this value can never affect a finding. Calibrated on one-book cold
/// maps (3JN / MAT / PSA and a ladder of PSA prefixes), 10 cores: the direct
/// lane's own crossover sits near 8–11 KB, but a whole default-config analyze
/// still loses up to 8% at that size — fanning out one lane while every other
/// phase is serial costs more than the lane saves — and only stops regressing
/// around 22–24 KB. 32 KB keeps a margin over that neutral point, so no config
/// regresses, while every book large enough for the fan-out to matter (MAT
/// ~121 KB, PSA ~217 KB) clears it comfortably.
pub const PARALLEL_MIN_CHAPTER_MAP_BYTES: usize = 32 * 1024;

/// The threshold in force. A `bench-probes` build lets the calibration harness
/// move it so serial and chapter-parallel routes can be timed against each other
/// in one alternating run; every other build reads the calibrated constant.
#[cfg(feature = "bench-probes")]
static CHAPTER_MAP_MIN_BYTES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(PARALLEL_MIN_CHAPTER_MAP_BYTES);

/// Override the chapter fan-out threshold (`bench-probes` builds only — the
/// calibration harness). Not public API.
#[cfg(feature = "bench-probes")]
pub fn set_chapter_map_min_bytes(bytes: usize) {
    CHAPTER_MAP_MIN_BYTES.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

fn chapter_map_min_bytes() -> usize {
    #[cfg(feature = "bench-probes")]
    {
        CHAPTER_MAP_MIN_BYTES.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(feature = "bench-probes"))]
    {
        PARALLEL_MIN_CHAPTER_MAP_BYTES
    }
}

/// The single Rayon grain one chapter-map call uses. Exactly one is selected per
/// call from the dirty map scope, and the two parallel grains are never nested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapRoute {
    /// One map task worth doing, or too little work to schedule: map in place.
    Serial,
    /// More than one dirty book: fan out by book, each worker mapping its own
    /// book's dirty chapters serially. The established grain (ADR 0042).
    Books,
    /// Exactly one dirty book with several dirty chapters and enough work: fan
    /// out by chapter within that book.
    Chapters,
}

impl MapRoute {
    /// A stable label for the work probes.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn label(self) -> &'static str {
        match self {
            MapRoute::Serial => "serial",
            MapRoute::Books => "books",
            MapRoute::Chapters => "chapters",
        }
    }
}

/// Choose the grain from the dirty map scope alone — called once per map call by
/// the caller, which records it and hands it to the seam, so exactly one decision
/// exists. Serial builds have no grain to choose. Note the ordering of the tests:
/// a multi-book scope takes the book grain *before* the chapter threshold is
/// consulted, so the two can never both apply.
pub(crate) fn map_route(
    book_runs: &[std::ops::Range<usize>],
    work_len: usize,
    work_bytes: usize,
) -> MapRoute {
    if !cfg!(feature = "parallel") {
        return MapRoute::Serial;
    }
    if book_runs.len() > 1 {
        MapRoute::Books
    } else if work_len > 1 && work_bytes >= chapter_map_min_bytes() {
        MapRoute::Chapters
    } else {
        MapRoute::Serial
    }
}

/// Guards against nesting the two fan-out grains: a seam entered while this
/// thread is already inside one is a nested fan-out. Rayon runs part of a
/// `par_iter` on the calling thread, so an inner seam reached from a fanned-out
/// closure lands here. Test/probe builds only.
#[cfg(any(test, feature = "test-probes"))]
mod fanout {
    use std::cell::Cell;

    thread_local! {
        static IN_FANOUT: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) struct Guard(bool);

    impl Guard {
        pub(super) fn enter(seam: &str) -> Self {
            let nested = IN_FANOUT.with(Cell::get);
            assert!(
                !nested,
                "nested fan-out: {seam} entered from inside another map fan-out — exactly one \
                 Rayon grain is selected per map call"
            );
            IN_FANOUT.with(|c| c.set(true));
            Guard(nested)
        }

        /// Mark a fanned-out worker's thread as inside the fan-out. Rayon injects
        /// work from a non-pool caller and does not run it on the calling thread,
        /// so the flag has to travel with the task or a nested seam on a worker
        /// would look like a fresh top-level call.
        #[cfg(feature = "parallel")]
        pub(super) fn worker() -> Self {
            let previous = IN_FANOUT.with(Cell::get);
            IN_FANOUT.with(|c| c.set(true));
            Guard(previous)
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            IN_FANOUT.with(|c| c.set(self.0));
        }
    }
}

/// Run `f` over every dirty **chapter** work item and collect the outputs **in
/// caller order** (index-aligned with `work`, which the caller built by walking
/// the corpus layout). `book_runs` are `work`'s contiguous per-book runs, in
/// caller order. `route` is the single grain decision ([`map_route`]).
///
/// The corpus layout supplies every ordinal here: nothing derives an order from a
/// slug or an opaque chapter token, and each route writes its results back into
/// the caller-order slot they came from (an indexed `collect` preserves input
/// order regardless of completion order). So serial and parallel builds — and any
/// thread count — produce byte-identical output; the route is a wall-clock
/// decision only.
///
/// One grain per call, never nested: the book route's workers map their chapters
/// serially and the chapter route's workers map exactly one chapter each, and in
/// both cases the closure they call is `f` itself.
pub(crate) fn map_chapter_work<W: Sync, T: Send>(
    work: &[W],
    book_runs: &[std::ops::Range<usize>],
    route: MapRoute,
    f: impl Fn(&W) -> T + Sync,
) -> Vec<T> {
    #[cfg(any(test, feature = "test-probes"))]
    let _guard = fanout::Guard::enter("map_chapter_work");
    // Only the book route reads the runs; a serial build has no book route.
    #[cfg(not(feature = "parallel"))]
    let _ = book_runs;
    match route {
        MapRoute::Serial => work.iter().map(&f).collect(),
        #[cfg(feature = "parallel")]
        MapRoute::Books => {
            use rayon::prelude::*;
            let per_book: Vec<Vec<T>> = book_runs
                .par_iter()
                .map(|run| {
                    #[cfg(any(test, feature = "test-probes"))]
                    let _worker = fanout::Guard::worker();
                    work[run.clone()].iter().map(&f).collect()
                })
                .collect();
            per_book.into_iter().flatten().collect()
        }
        #[cfg(feature = "parallel")]
        MapRoute::Chapters => {
            use rayon::prelude::*;
            work.par_iter()
                .map(|w| {
                    #[cfg(any(test, feature = "test-probes"))]
                    let _worker = fanout::Guard::worker();
                    f(w)
                })
                .collect()
        }
        // A serial build never selects a parallel grain.
        #[cfg(not(feature = "parallel"))]
        MapRoute::Books | MapRoute::Chapters => unreachable!("serial builds route Serial"),
    }
}

/// Every per-verse rule wired in. The registry is complete — including
/// rules `Config::v1_defaults` disables by default — so an explicit
/// enable in config is all it takes to run one.
pub(crate) fn per_verse_rules() -> Vec<Box<dyn PerVerseRule>> {
    vec![
        Box::new(signals::whitespace::ExcessHWhitespace),
        Box::new(signals::hygiene::TabInBody),
        Box::new(signals::hygiene::ControlChars),
        Box::new(signals::hygiene::ZeroWidthMisuse),
        Box::new(signals::hygiene::EmptyVerse),
        Box::new(signals::hygiene::InvalidCodepoint),
        Box::new(signals::hygiene::ReplacementRun),
        Box::new(signals::hygiene::CombiningMarkWithoutBase),
        Box::new(signals::hygiene::MixedNumeralSystems),
        Box::new(signals::zero_width_space::RedundantZeroWidthSpace),
        Box::new(signals::structural::SourceMarkerLeftover),
        Box::new(signals::structural::MergeConflictMarker),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enough bytes to clear the chapter-fan-out threshold.
    fn over_threshold() -> usize {
        PARALLEL_MIN_CHAPTER_MAP_BYTES
    }

    /// The routing table, exhaustively: one grain per dirty map scope, and the
    /// two parallel grains are mutually exclusive by construction — a multi-book
    /// scope takes the book grain before the chapter threshold is even consulted.
    // A one-element run array is exactly the one-dirty-book case under test, not a
    // mistyped slice index.
    #[allow(clippy::single_range_in_vec_init)]
    #[test]
    fn one_grain_is_selected_per_dirty_map_scope() {
        let expect_parallel = |route: MapRoute| {
            if cfg!(feature = "parallel") {
                route
            } else {
                MapRoute::Serial
            }
        };

        // Several dirty books: fan out by book, whatever the byte count.
        assert_eq!(
            map_route(&[0..3, 3..7], 7, over_threshold()),
            expect_parallel(MapRoute::Books)
        );
        assert_eq!(
            map_route(&[0..1, 1..2], 2, 1),
            expect_parallel(MapRoute::Books)
        );
        // One dirty book, several dirty chapters, enough work: fan out by chapter.
        assert_eq!(
            map_route(&[0..12], 12, over_threshold()),
            expect_parallel(MapRoute::Chapters)
        );
        // One dirty chapter: there is only one useful map task.
        assert_eq!(map_route(&[0..1], 1, over_threshold()), MapRoute::Serial);
        // Several dirty chapters but too little work to schedule.
        assert_eq!(
            map_route(&[0..12], 12, over_threshold() - 1),
            MapRoute::Serial
        );
        // Nothing dirty at all.
        assert_eq!(map_route(&[], 0, 0), MapRoute::Serial);
    }

    /// Every route writes its results back into the caller-order slot they came
    /// from, so the mapped output is index-aligned with `work` regardless of the
    /// route, the feature, or the thread count. The whole reason a route may be
    /// chosen for wall-clock alone.
    // A one-element run array is exactly the one-dirty-book case under test, not a
    // mistyped slice index.
    #[allow(clippy::single_range_in_vec_init)]
    #[test]
    fn every_route_collects_into_caller_order_slots() {
        // Work items are just their own index; `f` is order-revealing.
        let work: Vec<usize> = (0..12).collect();
        let expected: Vec<usize> = work.iter().map(|i| i * 7).collect();

        // one dirty chapter (serial), one book many chapters (chapter route),
        // several books (book route) — and a below-threshold multi-chapter scope.
        let scopes: Vec<(Vec<std::ops::Range<usize>>, usize)> = vec![
            (vec![0..12], over_threshold()),
            (vec![0..12], 0),
            (vec![0..3, 3..5, 5..12], over_threshold()),
            (vec![0..4, 4..8, 8..9, 9..10, 10..11, 11..12], 1),
        ];
        for (runs, bytes) in scopes {
            let route = map_route(&runs, work.len(), bytes);
            let got = map_chapter_work(&work, &runs, route, |w| w * 7);
            assert_eq!(got, expected, "route {route:?} reordered results");
        }
        // The single-item scope, too.
        assert_eq!(
            map_chapter_work(&work[0..1], &[0..1], MapRoute::Serial, |w| w * 7),
            vec![0]
        );
    }

    /// Neither route nests the two Rayon grains: a fan-out entered from inside
    /// another one trips the thread-local guard. Rayon runs part of a `par_iter`
    /// on the calling thread, so the inner seam is reached here even in a
    /// parallel build.
    // A one-element run array is exactly the one-dirty-book case under test, not a
    // mistyped slice index.
    #[allow(clippy::single_range_in_vec_init)]
    #[test]
    #[should_panic(expected = "nested fan-out")]
    fn nesting_a_fan_out_inside_the_chapter_seam_is_rejected() {
        let work: Vec<usize> = (0..12).collect();
        let inner: Vec<usize> = (0..4).collect();
        let outer = map_route(&[0..12], work.len(), over_threshold());
        let _ = map_chapter_work(&work, &[0..12], outer, |w| {
            map_chapter_work(&inner, &[0..4], MapRoute::Serial, |x| x + w)
                .into_iter()
                .sum::<usize>()
        });
    }

    /// The safety property the whole prefilter rests on (ADR 0046, ported from
    /// the spike's corpus-wide assertion): every per-verse rule's gate is a
    /// **safe superset** of its fire set — on any verse where `check` returns a
    /// finding, the verse's dirty-bits mask opens that rule's gate. If this
    /// held only on clean corpora the prefilter could silently drop findings;
    /// these synthetic verses fire *every* rule at least once (asserted below).
    #[test]
    fn every_gate_is_a_safe_superset_of_its_fire_set() {
        // A battery that fires all twelve rules plus clean / adjacent cases.
        let verses = [
            "",
            "   ",
            "In the beginning God created the heavens.",
            "मन ने कहा। हाँ भई हाँ।",
            "a  b",                                  // excess whitespace
            "End.  Next",                            // protected (no fire) but EXCESS_WS set
            "a\u{00A0}\u{00A0}b",                    // NBSP run
            "foo\tbar",                              // tab
            "foo\u{0007}bar\u{0085}baz",             // C0 + C1 controls
            "a\u{FEFF}b\u{2060}c\u{202E}d",          // zero-width / format
            "god\u{FFFD}\u{FDD0}\u{FFFE}x",          // invalid codepoints
            "a\u{1FFFE}b",                           // astral noncharacter
            "word ????? end",                        // ?×5
            "\u{0301}abc word.\u{0301} x",           // baseless marks
            "12 men and ४५ women",                   // mixed numerals
            "a\u{200B}\u{200B}b c\u{200B}\u{200B}d", // doubled ZWSP runs
            r"In the \v 2 \add beginning\add*",
            "a <b>bold</b> <br/> word",
            "<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> z",
            "||||||| base",
            "5 < 7 and 7 > 5", // lone comparisons (no conflict fire)
            "what?? really",   // ?×2 (no replacement fire)
        ];
        let rules = per_verse_rules();
        let mut tape = Vec::new();
        // Which rules actually fired somewhere — to prove the battery is real.
        let mut fired_any = vec![false; rules.len()];
        for text in verses {
            let mask = crate::tape::build_masked(text, &mut tape);
            for (i, r) in rules.iter().enumerate() {
                let fires = !r.check(text, &tape).is_empty();
                if fires {
                    fired_any[i] = true;
                    assert!(
                        mask.opens(r.gate()),
                        "{:?} fired on {text:?} but its gate stayed closed",
                        r.id()
                    );
                }
            }
        }
        for (i, r) in rules.iter().enumerate() {
            assert!(
                fired_any[i],
                "battery never fired {:?} — test is vacuous for it",
                r.id()
            );
        }
    }
}
