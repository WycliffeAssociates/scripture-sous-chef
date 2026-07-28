//! Shared sequential book walking for consumers that need a verse stream.
//!
//! It builds only the tape, grapheme, and token products the caller requests,
//! then supplies one [`VerseInputs`] view per verse. Typed substrates own their
//! resident incremental caches; this module holds no execution registry.
//!
//! **The verse is not a boundary.** Listeners that carry stream-order state
//! (casing's pending terminal, rare-glyph's forced-position machine,
//! bracket-balance's LIFO stack, duplicate-word's tail, spacing's cross-seam
//! neighbour classes) own that state across verse seams inside one book and
//! reset it only at book boundaries — exactly as their per-rule walks did
//! (repo CLAUDE.md: verse markers are addressing, not discourse).
//!
use crate::corpus::{BookGroup, LocalKeyIdx};
#[cfg(feature = "bench-probes")]
use crate::corpus::Books;
use crate::grapheme::{self, GSpan};
use crate::tape::{self, TapeEntry};
use crate::token::{self, Token};

/// The shared per-verse view every listener reads. Slices are empty when the
/// walk plan didn't require that product — a listener only touches what its
/// [`Needs`] declared.
pub(crate) struct VerseInputs<'t, 'b> {
    /// This verse's raw key string, borrowed from the owning `BookGroup` — a
    /// listener that needs a structural fact (duplicate-word's chapter gate)
    /// parses it via `key::parse_key`; nothing here parses it eagerly.
    pub key: &'t str,
    /// This verse's position within its book — the address every retained
    /// per-book product stores. Rebased to a global `KeyIdx` only at
    /// emission (judge time), against the *current call's* `BookGroup::base`.
    pub local_idx: LocalKeyIdx,
    /// The verse text, borrowed from the caller's `Corpus` — it outlives
    /// the walk, so a listener may carry slices of it across verses
    /// (duplicate-word's tail).
    pub text: &'t str,
    pub tape: &'b [TapeEntry],
    pub graphemes: &'b [GSpan],
    pub tokens: &'b [Token],
}

/// Which per-verse products a walk must build. Graphemes are tape-driven, so
/// `graphemes` implies `tape`.
#[derive(Clone, Copy, Default)]
pub(crate) struct Needs {
    pub tape: bool,
    pub graphemes: bool,
    pub tokens: bool,
}

impl Needs {
    fn union(self, o: Needs) -> Needs {
        Needs {
            tape: self.tape || o.tape || self.graphemes || o.graphemes,
            graphemes: self.graphemes || o.graphemes,
            tokens: self.tokens || o.tokens,
        }
    }
}


/// Sample size for the per-book adaptive tokenize gate (ADR 0064): the
/// number of a book's leading verses whose non-ASCII codepoint density
/// decides whether the WHOLE book delegates to `unicode_word_indices()` or
/// runs `token`'s hand-rolled walker — decided once, not re-checked per
/// verse. Calibrated in the ADR's book-level sampling survey (fleet-wide,
/// this N already reaches near-total directional agreement against several
/// candidate crossover thresholds).
const ADAPTIVE_SAMPLE_N: usize = 5;

/// PLACEHOLDER threshold, not a measured crossover — see the ADR. Only
/// ~0% (delegating wins big), ~10% (delegating still wins), and ~50%+ (the
/// hand-rolled walker wins big) are pinned down by direct measurement; 30%
/// is simply the midpoint of the still-unmeasured 10-50% gap, picked so this
/// gate could ship at all. Revisit once the real crossover is measured.
const ADAPTIVE_THRESHOLD: f64 = 0.30;

/// The per-book adaptive tokenize decision (ADR 0064): samples the non-ASCII
/// codepoint density of the first `ADAPTIVE_SAMPLE_N` verses (or fewer, for
/// a short book) and decides ONCE whether the whole book should delegate to
/// `unicode_word_indices()` (`true`) or run `token`'s hand-rolled walker
/// (`false`). A plain per-book-local decision — no atomic, no mutex: each
/// book's own walk is already strictly sequential even under the parallel
/// per-book fan-out (ADR 0018), so there is nothing to synchronize, and a
/// counter shared *across* books would need real synchronization for no
/// benefit (each book's own density is what determines whether ITS walk
/// should delegate). Applies uniformly regardless of whether the book is in
/// the `counted` or anchor (uncounted) set — this is a pure
/// tokenization-performance detail, not a counting concern.
fn book_prefers_delegation(texts: &[String]) -> bool {
    let sample_n = ADAPTIVE_SAMPLE_N.min(texts.len());
    let (mut non_ascii, mut total) = (0u64, 0u64);
    for t in &texts[..sample_n] {
        for c in t.chars() {
            total += 1;
            if !c.is_ascii() {
                non_ascii += 1;
            }
        }
    }
    let density = if total > 0 {
        non_ascii as f64 / total as f64
    } else {
        0.0
    };
    density < ADAPTIVE_THRESHOLD
}

/// Drive one consumer over one book. It is shared by census and focused
/// rule-level tests, not by a hidden analyzer fallback.
pub(crate) fn drive_book<'g, A, T>(
    group: &BookGroup<'g>,
    needs: Needs,
    mut acc: A,
    mut feed: impl FnMut(&mut A, &VerseInputs<'g, '_>),
    finish: impl FnOnce(A) -> T,
) -> T {
    let needs = needs.union(Needs::default()); // normalize graphemes ⇒ tape
    // Same per-book adaptive gate as `walk_book` (ADR 0064) — computed
    // identically (a pure function of `group.texts`) so this trait-driven
    // path and the fused walk can never diverge on which one a given book
    // takes.
    let delegate_tokens = needs.tokens && book_prefers_delegation(group.texts);
    let mut tape_buf: Vec<TapeEntry> = Vec::new();
    let mut graphemes_buf: Vec<GSpan> = Vec::new();
    let mut tokens_buf: Vec<Token> = Vec::new();
    for (vi, (key, text)) in group.keys.iter().zip(group.texts.iter()).enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        let text = text.as_str();
        if needs.tape {
            tape::build(text, &mut tape_buf);
        }
        if needs.graphemes {
            grapheme::segment_tape(text, &tape_buf, &mut graphemes_buf);
        }
        if needs.tokens {
            if delegate_tokens {
                token::tokenize_oracle_into(text, &mut tokens_buf);
            } else {
                token::tokenize_hand_rolled_into(text, &mut tokens_buf);
            }
        }
        let v = VerseInputs {
            key,
            local_idx,
            text,
            tape: if needs.tape { &tape_buf } else { &[] },
            graphemes: if needs.graphemes { &graphemes_buf } else { &[] },
            tokens: if needs.tokens { &tokens_buf } else { &[] },
        };
        feed(&mut acc, &v);
    }
    finish(acc)
}

/// Bench-only mirror of [`Needs`] (`bench-probes` feature): which per-verse
/// products to force on, with zero rule listener attached. Kept as its own
/// type rather than exposing `Needs` itself, so the production type's
/// crate-private visibility never has to move for a dev tool.
#[cfg(feature = "bench-probes")]
#[derive(Clone, Copy, Default)]
pub struct FloorNeeds {
    pub tape: bool,
    pub graphemes: bool,
    pub tokens: bool,
}

/// The fused walk's substrate cost over `books`, for the requested
/// [`FloorNeeds`], with no rule listener attached — the floor any rule
/// combination pays into before its own logic runs. Drives the exact same
/// per-verse build every real listener shares ([`drive_book`]), so this can
/// never silently drift from what `analyze`'s benches measure. Returns the
/// summed product counts (tape entries + graphemes + tokens) so the compiler
/// can't fold the walk away.
#[cfg(feature = "bench-probes")]
pub fn walk_floor(books: &Books<'_>, needs: FloorNeeds) -> usize {
    let needs = Needs {
        tape: needs.tape,
        graphemes: needs.graphemes,
        tokens: needs.tokens,
    };
    books
        .iter()
        .map(|group| {
            drive_book(
                group,
                needs,
                0usize,
                |acc, v| *acc += v.tape.len() + v.graphemes.len() + v.tokens.len(),
                |acc| acc,
            )
        })
        .sum()
}
