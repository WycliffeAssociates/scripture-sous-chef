//! 2×2 association tests for sparse language evidence.
//!
//! Dunning's `G²` on the fast path (every expected cell ≥ 5); sparse margins
//! also use `G²` since ADR 0059 (Fisher's exact two-sided surprise is kept as
//! the other [`ExactTest`] arm, not the shipped default). Both statistics
//! move the same direction: larger = "less likely coincidence." Casing's
//! `terminal_strength` word-reshuffle
//! witness (ADR 0052) aggregates a per-juror [`Table2::association_score`] over
//! a class's following-word distribution; the machinery is factored here so a
//! future positional rule or the planned inventory mode reads the same code.
//!
//! `ln_binomial` for Fisher comes from a self-contained Lanczos `ln_gamma`
//! (`ssc-core` takes no external stats dependency). The textbook fixtures in
//! `tests` pin it to hand-verified values.
//!
//! Dunning (1993), *Accurate Methods for the Statistics of Surprise and
//! Coincidence*, Computational Linguistics 19(1).

/// 2×2 contingency table. Cells are `u64` so callers populate from token /
/// word counters without casting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table2 {
    pub a: u64,
    pub b: u64,
    pub c: u64,
    pub d: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationTest {
    DunningG2,
    FisherExact,
}

/// Which statistic backs [`Table2::association_score`] on a **sparse** table
/// (`min_expected_cell < 5`). Internal seam, deliberately not user-facing (no
/// `Config`/wire surface): measurement (2026-07-11, WA-en-ulb everything-on)
/// showed the "fallback" is actually the dominant path — 51,629 of 53,844
/// association calls (95.9%) route to Fisher and account for 99.97% of
/// association time (~8.9 µs vs ~59 ns per call) — because the per-juror 2×2
/// tables of casing's word-reshuffle witness are intrinsically sparse (the
/// after-class cell is small for almost every juror), not because the Cochran
/// threshold is wrong. Adopted as `G2Only` — see
/// [ADR 0059](../../../../documentation/adrs/0059-association-g2-only.md) for
/// the fleet-drift and perf record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExactTest {
    /// Two-sided Fisher exact surprise on sparse tables (pre-ADR-0059
    /// default). Retained as the other arm; still directly exercised by the
    /// textbook fixtures below.
    #[allow(dead_code)]
    Fisher,
    /// Dunning G² everywhere, sparse included. Shipped default since ADR
    /// 0059 (fleet drift: 142 sixth-decimal jitters on
    /// `case.inconsistent-word-casing`, zero verdict flips, −34% on
    /// WA-en-ulb everything-on).
    G2Only,
}

/// The active sparse-table strategy. `G2Only` since ADR 0059.
const EXACT_TEST: ExactTest = ExactTest::G2Only;

impl Table2 {
    pub const fn new(a: u64, b: u64, c: u64, d: u64) -> Self {
        Self { a, b, c, d }
    }

    /// −2 log λ. `0.0` for the all-zero table and any degenerate table with a
    /// zero row or column sum (the test is undefined there).
    pub fn g2(&self) -> f64 {
        let (a, b, c, d) = (self.a as f64, self.b as f64, self.c as f64, self.d as f64);
        let (r1, r2, c1, c2) = (a + b, c + d, a + c, b + d);
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

    /// Dunning on well-populated tables; on sparse tables, whatever
    /// [`EXACT_TEST`] selects (G² everywhere by default, ADR 0059).
    pub fn association_score(&self) -> f64 {
        match (EXACT_TEST, self.association_test()) {
            (_, AssociationTest::DunningG2) | (ExactTest::G2Only, _) => self.g2(),
            (ExactTest::Fisher, AssociationTest::FisherExact) => {
                let p = self.fisher_two_sided_p();
                if p <= 0.0 { f64::INFINITY } else { -2.0 * p.ln() }
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
        let (a, b, c, d) = (self.a as f64, self.b as f64, self.c as f64, self.d as f64);
        let (r1, r2, c1, c2) = (a + b, c + d, a + c, b + d);
        let n = r1 + r2;
        if n == 0.0 || r1 == 0.0 || r2 == 0.0 || c1 == 0.0 || c2 == 0.0 {
            return 0.0;
        }
        ((r1 * c1) / n).min((r1 * c2) / n).min((r2 * c1) / n).min((r2 * c2) / n)
    }

    /// Two-sided Fisher exact p with fixed margins: sum of all same-margin
    /// tables whose hypergeometric probability is ≤ the observed table's.
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

/// `x · ln(x)` with `0 · ln(0) = 0`.
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
    ln_factorial(n) - ln_factorial(k) - ln_factorial(n - k)
}

fn ln_factorial(n: u64) -> f64 {
    // ln(n!) = ln_gamma(n + 1).
    ln_gamma((n as f64) + 1.0)
}

/// Lanczos approximation to `ln Γ(x)` for `x > 0` (g = 7, 9 coefficients).
/// Accurate to ~1e-13 over the range this touches — replaces an external
/// `ln_binomial` with no dependency.
fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    // Published Lanczos g=7 coefficients, kept at full source precision (the
    // `ln_choose` fixture pins the accuracy this buys).
    #[allow(clippy::excessive_precision)]
    const COEF: [f64; 9] = [
        0.999_999_999_999_809_93,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        // Reflection: Γ(x)Γ(1−x) = π / sin(πx).
        (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x)
    } else {
        let x = x - 1.0;
        let mut a = COEF[0];
        let t = x + G + 0.5;
        for (i, &c) in COEF.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independence_is_zero() {
        assert!(Table2::new(10, 10, 10, 10).g2().abs() < 1e-9);
    }

    /// Hand fixture `{10,20,20,10}`, all marginals 30 ⇒ E = 15, G² ≈ 6.79596145.
    #[test]
    fn balanced_off_diagonal_matches_hand_computation() {
        let g2 = Table2::new(10, 20, 20, 10).g2();
        assert!((g2 - 6.795_961_45).abs() < 1e-6, "got {g2}");
    }

    /// Hand fixture `{5,1,1,10}` ⇒ G² ≈ 9.96571544.
    #[test]
    fn strong_association_matches_hand_computation() {
        let g2 = Table2::new(5, 1, 1, 10).g2();
        assert!((g2 - 9.965_715_44).abs() < 1e-6, "got {g2}");
    }

    #[test]
    fn symmetric_under_row_col_swap() {
        let base = Table2::new(7, 11, 23, 5).g2();
        assert!((base - Table2::new(23, 5, 7, 11).g2()).abs() < 1e-9);
        assert!((base - Table2::new(11, 7, 5, 23).g2()).abs() < 1e-9);
        assert!((base - Table2::new(7, 23, 11, 5).g2()).abs() < 1e-9);
    }

    #[test]
    fn zero_marginal_returns_zero() {
        assert_eq!(Table2::new(0, 0, 5, 5).g2(), 0.0);
        assert_eq!(Table2::new(0, 5, 0, 5).g2(), 0.0);
        assert_eq!(Table2::new(0, 0, 0, 0).g2(), 0.0);
    }

    #[test]
    fn zero_cell_does_not_panic() {
        let g2 = Table2::new(0, 5, 5, 100).g2();
        assert!(g2.is_finite() && g2 > 0.0);
    }

    #[test]
    fn well_populated_table_uses_dunning_fast_path() {
        let t = Table2::new(10, 20, 20, 10);
        assert_eq!(t.association_test(), AssociationTest::DunningG2);
        assert!((t.association_score() - t.g2()).abs() < 1e-9);
    }

    #[test]
    fn sparse_table_uses_fisher_exact() {
        let t = Table2::new(1, 9, 8, 6);
        assert_eq!(t.association_test(), AssociationTest::FisherExact);
        assert!(t.min_expected_cell() < 5.0);
    }

    /// Classic Fisher exact fixture: two-sided p ≈ 0.002759456.
    #[test]
    fn fisher_two_sided_matches_textbook_fixture() {
        let p = Table2::new(1, 9, 11, 3).fisher_two_sided_p();
        assert!((p - 0.002_759_456).abs() < 1e-8, "got {p}");
    }

    /// Since ADR 0059, `EXACT_TEST` is `G2Only`, so even a sparse table
    /// (`association_test() == FisherExact`) scores via `g2()`, not Fisher
    /// surprise. Fisher itself is still directly tested above
    /// (`fisher_two_sided_matches_textbook_fixture`) — only the switch
    /// changed, not the fn.
    #[test]
    fn sparse_association_score_uses_g2_since_adr_0059() {
        let t = Table2::new(1, 9, 8, 6);
        assert_eq!(t.association_test(), AssociationTest::FisherExact);
        assert!((t.association_score() - t.g2()).abs() < 1e-9);
    }

    /// The Lanczos `ln_gamma` reproduces small factorials exactly enough that
    /// `ln_choose(n,k)` matches direct computation (guards the statrs swap).
    #[test]
    fn ln_choose_matches_direct_small_values() {
        // C(10,3) = 120, C(20,10) = 184756, C(52,5) = 2598960.
        for &(n, k, want) in &[(10u64, 3u64, 120.0f64), (20, 10, 184_756.0), (52, 5, 2_598_960.0)] {
            let got = super::ln_choose(n, k).exp();
            assert!((got - want).abs() / want < 1e-9, "C({n},{k}) got {got} want {want}");
        }
    }
}
