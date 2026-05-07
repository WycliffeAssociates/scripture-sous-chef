//! Verse-length cohort bucketing for length-conditioned anomaly scoring.
//!
//! A short verse like "Jesus wept" has unusual compression-texture by virtue
//! of being short, not because anything is wrong with it. Rules that compare
//! a per-verse metric against a corpus-wide baseline produce length-driven
//! false positives unless they bucket verses by length first and compare
//! against the bucket-specific baseline.
//!
//! This module owns the bucketing primitives:
//!
//! - [`GraphemeCount`]: a verse's length in grapheme clusters. Distinct from
//!   `usize` so we don't accidentally bucket by token count or byte count.
//! - [`LengthBucket`]: which quintile a verse falls in (Q1..Q5).
//! - [`LengthBucketBoundaries`]: the four grapheme-count thresholds, computed
//!   empirically from the corpus's verse-length distribution.
//!
//! Why graphemes (not tokens, not bytes)?
//! - Compression-texture is a character-level signal; bucket at the level of
//!   the underlying measurement.
//! - Token counts vary wildly across morphological regimes; comparing 10
//!   tokens of Bemba to 10 tokens of English isn't a like-for-like cohort.
//! - Bytes inflate asymmetrically across scripts (Devanagari vs. Latin).
//!
//! Why quintiles (not Gaussian, not fixed boundaries)?
//! Bible verse lengths are right-skewed: many short verses, long tail of
//! long ones. Quantile bucketing is distribution-free and guarantees every
//! bucket has enough verses for a stable median+MAD. Five buckets is the
//! sweet spot for NT-scale data — coarse enough to keep ~1500 verses per
//! bucket, fine enough to separate "Jesus wept" from a 40-grapheme prose
//! verse.
//!
//! See `documentation/adrs/0006-verse-length-quintiles.md` for the
//! decision rationale.

use unicode_segmentation::UnicodeSegmentation;

/// Number of grapheme clusters in a verse. Newtype to keep length units
/// straight: byte count, char count, and grapheme count are easy to confuse
/// at the call site, and this rule depends on graphemes specifically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphemeCount(pub usize);

impl GraphemeCount {
    pub fn of(text: &str) -> Self {
        Self(text.graphemes(true).count())
    }

    pub fn get(self) -> usize {
        self.0
    }
}

/// Which quintile a verse's length falls into. Quintile 1 is shortest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LengthBucket {
    Q1,
    Q2,
    Q3,
    Q4,
    Q5,
}

impl LengthBucket {
    /// All buckets in order. Useful for iteration over per-bucket arrays.
    pub const ALL: [LengthBucket; 5] = [Self::Q1, Self::Q2, Self::Q3, Self::Q4, Self::Q5];

    /// Index into a fixed-size 5-element array. Q1 → 0, ..., Q5 → 4.
    pub fn index(self) -> usize {
        match self {
            Self::Q1 => 0,
            Self::Q2 => 1,
            Self::Q3 => 2,
            Self::Q4 => 3,
            Self::Q5 => 4,
        }
    }
}

/// The four grapheme-count thresholds that split a corpus into quintiles.
/// `thresholds[0]` separates Q1 from Q2, ..., `thresholds[3]` separates Q4
/// from Q5. A count strictly less than `thresholds[i]` lands in bucket `i`;
/// a count `≥ thresholds[3]` lands in Q5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LengthBucketBoundaries {
    thresholds: [usize; 4],
}

impl LengthBucketBoundaries {
    /// Compute boundaries from a slice of observed verse lengths. Empirical
    /// quintiles via index-based percentile picking — distribution-free, no
    /// Gaussian assumption.
    ///
    /// Edge cases:
    /// - empty input: returns boundaries `[0, 0, 0, 0]`. Every count lands
    ///   in Q5; the caller should detect the empty corpus separately and
    ///   skip the rule.
    /// - constant input: returns four copies of that constant. The
    ///   `bucket_for` rule (`<` for thresholds 0..3, `≥` for Q5) means
    ///   every verse lands in Q5. Per-bucket MAD on Q5 will then behave
    ///   like a single-bucket baseline, which is the right degenerate
    ///   behavior.
    pub fn compute(lengths: &[GraphemeCount]) -> Self {
        if lengths.is_empty() {
            return Self {
                thresholds: [0; 4],
            };
        }
        let mut sorted: Vec<usize> = lengths.iter().map(|g| g.get()).collect();
        sorted.sort_unstable();
        let n = sorted.len();
        let pick = |q: usize| -> usize {
            let idx = (q * n) / 5;
            sorted[idx.min(n - 1)]
        };
        Self {
            thresholds: [pick(1), pick(2), pick(3), pick(4)],
        }
    }

    /// Assign a grapheme count to its bucket.
    pub fn bucket_for(&self, count: GraphemeCount) -> LengthBucket {
        let n = count.get();
        if n < self.thresholds[0] {
            LengthBucket::Q1
        } else if n < self.thresholds[1] {
            LengthBucket::Q2
        } else if n < self.thresholds[2] {
            LengthBucket::Q3
        } else if n < self.thresholds[3] {
            LengthBucket::Q4
        } else {
            LengthBucket::Q5
        }
    }

    pub fn thresholds(&self) -> [usize; 4] {
        self.thresholds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lengths(values: &[usize]) -> Vec<GraphemeCount> {
        values.iter().copied().map(GraphemeCount).collect()
    }

    #[test]
    fn boundaries_split_uniform_distribution_into_quintiles() {
        let xs: Vec<usize> = (1..=100).collect();
        let boundaries = LengthBucketBoundaries::compute(&lengths(&xs));
        let [t1, t2, t3, t4] = boundaries.thresholds();
        // Index-based picking on n=100: q*n/5 = 20, 40, 60, 80; sorted[20]=21,
        // sorted[40]=41, sorted[60]=61, sorted[80]=81.
        assert_eq!([t1, t2, t3, t4], [21, 41, 61, 81]);
    }

    #[test]
    fn bucket_for_assigns_each_quintile_correctly() {
        let xs: Vec<usize> = (1..=100).collect();
        let boundaries = LengthBucketBoundaries::compute(&lengths(&xs));
        // thresholds [21, 41, 61, 81]; below 21 = Q1, [21,41) = Q2, ...
        assert_eq!(boundaries.bucket_for(GraphemeCount(1)), LengthBucket::Q1);
        assert_eq!(boundaries.bucket_for(GraphemeCount(20)), LengthBucket::Q1);
        assert_eq!(boundaries.bucket_for(GraphemeCount(21)), LengthBucket::Q2);
        assert_eq!(boundaries.bucket_for(GraphemeCount(40)), LengthBucket::Q2);
        assert_eq!(boundaries.bucket_for(GraphemeCount(60)), LengthBucket::Q3);
        assert_eq!(boundaries.bucket_for(GraphemeCount(80)), LengthBucket::Q4);
        assert_eq!(boundaries.bucket_for(GraphemeCount(81)), LengthBucket::Q5);
        assert_eq!(boundaries.bucket_for(GraphemeCount(1000)), LengthBucket::Q5);
    }

    #[test]
    fn graphemes_count_combining_marks_as_one_unit() {
        // "é" composed (e + combining acute) is one grapheme cluster.
        let composed = "e\u{0301}";
        assert_eq!(GraphemeCount::of(composed), GraphemeCount(1));
        // Devanagari conjunct: ka + virama + sa — one cluster per UAX 29.
        let conjunct = "क्स";
        assert_eq!(GraphemeCount::of(conjunct), GraphemeCount(1));
    }
}
