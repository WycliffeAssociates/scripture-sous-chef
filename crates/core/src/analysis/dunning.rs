//! Dunning's −2 log λ likelihood-ratio test for 2×2 contingency tables.
//! Reference: Dunning (1993), *Accurate Methods for the Statistics of
//! Surprise and Coincidence*, Computational Linguistics 19(1).
//!
//! Why LLR over χ² or raw frequency: it stays well-calibrated for low
//! expected counts. That matters a lot for NT-sized corpora where rare
//! bigrams and rare positional features are exactly the cells we care
//! about.
//!
//! ## The math
//!
//! For observed counts arranged as
//!
//! ```text
//!                 col 1      col 2
//!   row 1           a          b
//!   row 2           c          d
//! ```
//!
//! the test statistic is
//!
//! `−2 log λ = 2 · [ Σᵢⱼ Oᵢⱼ·ln(Oᵢⱼ)
//!                 − Σᵢ  Rᵢ·ln(Rᵢ)
//!                 − Σⱼ  Cⱼ·ln(Cⱼ)
//!                 + N·ln(N) ]`
//!
//! with `Rᵢ` row sums, `Cⱼ` column sums, `N` the grand total, and the
//! convention `0·ln(0) = 0`. Asymptotically χ² with 1 df, so 6.63 is
//! roughly the p < 0.01 cutoff and 10.83 the p < 0.001 cutoff.
//!
//! Equivalent to the G-test form `2·Σ Oᵢⱼ·ln(Oᵢⱼ/Eᵢⱼ)` with
//! `Eᵢⱼ = Rᵢ·Cⱼ/N` — the xlogx expansion is just a numerically tidier
//! way to evaluate it.

/// 2×2 contingency table for the LLR test. Cells are `u64` so callers
/// can populate from token / bigram counters without casting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table2 {
    pub a: u64,
    pub b: u64,
    pub c: u64,
    pub d: u64,
}

impl Table2 {
    pub const fn new(a: u64, b: u64, c: u64, d: u64) -> Self {
        Self { a, b, c, d }
    }

    /// −2 log λ. Returns `0.0` for the all-zero table and for any
    /// degenerate table where a row or column sum is zero (the test
    /// is undefined in those cases; signals upstream typically filter
    /// them out before calling).
    pub fn g2(&self) -> f64 {
        let a = self.a as f64;
        let b = self.b as f64;
        let c = self.c as f64;
        let d = self.d as f64;
        let r1 = a + b;
        let r2 = c + d;
        let c1 = a + c;
        let c2 = b + d;
        let n = r1 + r2;
        if n == 0.0 || r1 == 0.0 || r2 == 0.0 || c1 == 0.0 || c2 == 0.0 {
            return 0.0;
        }
        2.0 * (xlogx(a) + xlogx(b) + xlogx(c) + xlogx(d)
            - xlogx(r1)
            - xlogx(r2)
            - xlogx(c1)
            - xlogx(c2)
            + xlogx(n))
    }
}

/// `x · ln(x)` with the convention `0 · ln(0) = 0`. The xlogx form
/// keeps the LLR computation numerically tidy by collapsing what would
/// otherwise be a sum of `O·ln(O/E)` terms.
fn xlogx(x: f64) -> f64 {
    if x <= 0.0 { 0.0 } else { x * x.ln() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independence: every cell equal ⇒ G² = 0.
    #[test]
    fn independence_is_zero() {
        let g2 = Table2::new(10, 10, 10, 10).g2();
        assert!(g2.abs() < 1e-9, "expected ~0, got {}", g2);
    }

    /// Hand-computed fixture: `{10, 20, 20, 10}`.
    /// Expected E_ij = 15 in every cell (all marginals equal 30).
    /// G² = 2·[2·10·ln(10/15) + 2·20·ln(20/15)]
    ///    = 2·[20·(-0.40546511) + 40·0.28768207]
    ///    = 2·3.39798072
    ///    ≈ 6.79596145
    #[test]
    fn balanced_off_diagonal_matches_hand_computation() {
        let g2 = Table2::new(10, 20, 20, 10).g2();
        assert!((g2 - 6.79596145).abs() < 1e-6, "got {}", g2);
    }

    /// Hand-computed fixture: `{5, 1, 1, 10}`. Strong row→column
    /// association.
    ///
    /// `xlogx(5)+xlogx(1)+xlogx(1)+xlogx(10)−xlogx(6)−xlogx(11)−xlogx(6)−xlogx(11)+xlogx(17)`
    /// = 8.04718956 + 0 + 0 + 23.02585093
    ///   − 10.75055681 − 26.37684800 − 10.75055681 − 26.37684800
    ///   + 48.16462685
    /// = 4.98285772
    /// G² = 9.96571544
    #[test]
    fn strong_association_matches_hand_computation() {
        let g2 = Table2::new(5, 1, 1, 10).g2();
        assert!((g2 - 9.96571544).abs() < 1e-6, "got {}", g2);
    }

    /// G² is symmetric under row swap, column swap, and table
    /// transpose. (Dunning's table indexes a 2×2 categorical
    /// distribution; swapping which category is "row 1" vs "row 2"
    /// can't change the test statistic.)
    #[test]
    fn symmetric_under_row_col_swap() {
        let base = Table2::new(7, 11, 23, 5).g2();
        let row_swap = Table2::new(23, 5, 7, 11).g2();
        let col_swap = Table2::new(11, 7, 5, 23).g2();
        let transpose = Table2::new(7, 23, 11, 5).g2();
        assert!((base - row_swap).abs() < 1e-9);
        assert!((base - col_swap).abs() < 1e-9);
        assert!((base - transpose).abs() < 1e-9);
    }

    #[test]
    fn zero_marginal_returns_zero() {
        // Whole row zero (no observations of "row 1" event).
        assert_eq!(Table2::new(0, 0, 5, 5).g2(), 0.0);
        // Whole column zero.
        assert_eq!(Table2::new(0, 5, 0, 5).g2(), 0.0);
        // All zero.
        assert_eq!(Table2::new(0, 0, 0, 0).g2(), 0.0);
    }

    #[test]
    fn zero_cell_does_not_panic() {
        // Single zero cell: 0·ln(0) convention keeps the formula
        // finite. Worth a sanity test because naive `x.ln()` would
        // produce −∞.
        let g2 = Table2::new(0, 5, 5, 100).g2();
        assert!(g2.is_finite());
        assert!(g2 > 0.0);
    }
}
