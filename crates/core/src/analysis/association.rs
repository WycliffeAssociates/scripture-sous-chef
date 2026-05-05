//! 2×2 association tests for sparse language evidence.
//!
//! The signal code wants one monotone score: larger means "less likely to be
//! coincidence." Dunning's likelihood-ratio `G²` is fast and fine once every
//! expected cell is at least 5. Below that, the synthesis plan calls for
//! Fisher's exact test because singleton-heavy corpora are exactly where the
//! approximation is easiest to fool.
//!
//! `association_score()` hides that choice. It returns Dunning `G²` on the
//! fast path and `-2 ln(p)` from two-sided Fisher on sparse tables. Both move
//! in the same direction and keep existing threshold/evidence code usable
//! while we migrate toward explicit p-value reporting.
//!
//! Dunning reference: Dunning (1993), *Accurate Methods for the Statistics of
//! Surprise and Coincidence*, Computational Linguistics 19(1).
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

/// 2×2 contingency table for association tests. Cells are `u64` so callers
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

    /// Main score used by signals.
    ///
    /// Dunning stays as the fast path for well-populated tables. Fisher's
    /// exact test handles sparse margins without pretending the asymptotic
    /// chi-square approximation is calibrated.
    pub fn association_score(&self) -> f64 {
        match self.association_test() {
            AssociationTest::DunningG2 => self.g2(),
            AssociationTest::FisherExact => {
                let p = self.fisher_two_sided_p();
                if p <= 0.0 {
                    f64::INFINITY
                } else {
                    -2.0 * p.ln()
                }
            }
        }
    }

    pub fn association_test(&self) -> AssociationTest {
        if self.min_expected_cell() >= 5.0 {
            AssociationTest::DunningG2
        } else {
            AssociationTest::FisherExact
        }
    }

    pub fn min_expected_cell(&self) -> f64 {
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
        ((r1 * c1) / n)
            .min((r1 * c2) / n)
            .min((r2 * c1) / n)
            .min((r2 * c2) / n)
    }

    /// Two-sided Fisher exact p-value with fixed margins.
    ///
    /// The two-sided definition sums all tables with the same margins whose
    /// hypergeometric probability is no greater than the observed table. This
    /// is the conventional "as or more extreme" exact test used when we do
    /// not know the direction of the association in advance.
    pub fn fisher_two_sided_p(&self) -> f64 {
        let r1 = self.a + self.b;
        let r2 = self.c + self.d;
        let c1 = self.a + self.c;
        let n = r1 + r2;
        if n == 0 || r1 == 0 || r2 == 0 || c1 == 0 || c1 == n {
            return 1.0;
        }

        let min_a = c1.saturating_sub(r2);
        let max_a = r1.min(c1);
        let observed = hypergeom_ln_p(self.a, r1, c1, n).exp();
        let epsilon = observed * 1e-12 + 1e-15;
        let mut total = 0.0;
        for a in min_a..=max_a {
            let p = hypergeom_ln_p(a, r1, c1, n).exp();
            if p <= observed + epsilon {
                total += p;
            }
        }
        total.min(1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationTest {
    DunningG2,
    FisherExact,
}

/// `x · ln(x)` with the convention `0 · ln(0) = 0`. The xlogx form
/// keeps the LLR computation numerically tidy by collapsing what would
/// otherwise be a sum of `O·ln(O/E)` terms.
fn xlogx(x: f64) -> f64 {
    if x <= 0.0 { 0.0 } else { x * x.ln() }
}

fn hypergeom_ln_p(a: u64, row1: u64, col1: u64, n: u64) -> f64 {
    ln_choose(col1, a) + ln_choose(n - col1, row1 - a) - ln_choose(n, row1)
}

fn ln_choose(n: u64, k: u64) -> f64 {
    if k > n {
        return f64::NEG_INFINITY;
    }
    statrs::function::factorial::ln_binomial(n, k)
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

    #[test]
    fn well_populated_table_uses_dunning_fast_path() {
        let table = Table2::new(10, 20, 20, 10);
        assert_eq!(table.association_test(), AssociationTest::DunningG2);
        assert!((table.association_score() - table.g2()).abs() < 1e-9);
    }

    #[test]
    fn sparse_table_uses_fisher_exact() {
        let table = Table2::new(1, 9, 8, 6);
        assert_eq!(table.association_test(), AssociationTest::FisherExact);
        assert!(table.min_expected_cell() < 5.0);
    }

    #[test]
    fn fisher_two_sided_matches_textbook_fixture() {
        // Classic Fisher exact example. With fixed margins, the two-sided
        // p-value is about 0.002759456.
        let p = Table2::new(1, 9, 11, 3).fisher_two_sided_p();
        assert!((p - 0.002759456).abs() < 1e-8, "got {}", p);
    }

    #[test]
    fn sparse_association_score_is_fisher_surprise() {
        let table = Table2::new(1, 9, 8, 6);
        let expected = -2.0 * table.fisher_two_sided_p().ln();
        assert!((table.association_score() - expected).abs() < 1e-9);
    }
}
