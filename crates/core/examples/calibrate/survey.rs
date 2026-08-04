//! Calibration survey tools, split by theme out of the original monolithic
//! `main.rs` (pure code movement — no logic changed). `shared` holds the
//! handful of constants/helpers genuinely used by more than one theme; `misc`
//! holds the smaller assorted early spike tools; the rest are one file per
//! big calibration-spike cluster. Each file keeps its own top-of-block doc
//! comment explaining what it measures and why.

pub(crate) mod casing;
pub(crate) mod glyphs;
pub(crate) mod misc;
pub(crate) mod mixedcase;
pub(crate) mod nonletter;
pub(crate) mod paired;
pub(crate) mod pooled;
pub(crate) mod review_depth_candidates;
pub(crate) mod shared;
pub(crate) mod signatures;
pub(crate) mod terminal;
