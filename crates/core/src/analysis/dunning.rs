//! Dunning's −2 log λ likelihood-ratio test for 2×2 contingency tables.
//! Reference: Dunning (1993), *Accurate Methods for the Statistics of
//! Surprise and Coincidence*, Computational Linguistics 19(1).
//!
//! The point of using LLR over χ² or raw frequency is that it stays
//! well-calibrated for low expected counts, which is exactly the
//! situation for rare bigrams and rare positional features in a NT-sized
//! corpus.
//!
//! ## Math sketch
//!
//! For observed counts `(a, b, c, d)` arranged as
//!
//! ```text
//!                 word2 = X      word2 != X
//!   word1 = Y       a               b
//!   word1 != Y      c               d
//! ```
//!
//! `−2 log λ = 2 · [ a·ln(a) + b·ln(b) + c·ln(c) + d·ln(d)
//!                 − r1·ln(r1) − r2·ln(r2)
//!                 − c1·ln(c1) − c2·ln(c2)
//!                 + N·ln(N) ]`
//!
//! with row/column/total sums and the convention `0·ln(0) = 0`.
//!
//! Asymptotically χ² with 1 df, so a value ≥ 10.83 is the standard
//! "p < 0.001" cutoff.
//!
//! ## TODO
//! - [ ] `pub struct Table2 { a, b, c, d: u64 }` constructor + `g2()`.
//! - [ ] Numerically-stable variant for tiny counts (kahan-sum the xlogx
//!       terms, or factor common N·ln(N)).
//! - [ ] Unit test against Dunning's worked example: `(the, swiss)` vs.
//!       `(the, ¬swiss)` from the Reuters fixture in §5 of the paper.
//! - [ ] Property test: `g2(a, b, c, d) == g2(a, c, b, d)` (table is
//!       symmetric across the diagonal we care about).
//! - [ ] Decide return type: `f64` vs. `Result<f64>` for degenerate
//!       tables (any row or column sum = 0). Currently lean `f64` with
//!       documented `0.0` for degenerate cases, since signals filter
//!       those upstream anyway.
