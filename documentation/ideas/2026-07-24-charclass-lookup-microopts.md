# Idea — charclass lookup micro-opts (fixed walk-cost diet, round 2)

Date: 2026-07-24. Status: **spiked same day and declined** — see
`../calibration/2026-07-24-charclass-lookup-spike.md`. The classifier is
already a direct 65,536-entry array index (no hash in the per-scalar path);
alternative lookup shapes lose or win only script-conditionally (SWAR ~3.2× on
Latin, a net loss on Devanagari/Han), and classification is only ~1.3 ms of
the warm PSA path, below the spine's multi-millisecond terms. Revisit only if
a post-spine profile shows the walk floor dominating again, and then start
from the spike's tables, not from scratch. Original sketches kept below for
that reader:

- **Pre-map script → table, linear-scan lookups.** The fused `Class` table is
  consulted per scalar; if a book/chapter's scripts are known up front (we
  already compute script sets elsewhere), a per-script dense table or a small
  linear-scan hot table may beat the general lookup for the dominant script.
- **Used-scalar fast path.** Letters/scalars actually used by a corpus are a
  small set; a preallocated vec (or minimal perfect probe) checked before the
  general table could short-circuit the common case.

Both are walk-floor levers (`floor.rs` tiers are the baseline to beat), so any
attempt is oracle-gated + §13-measured like everything else. Do not pick these
up during the spine epic; they compete with, and would muddy, the phase
gates' attribution.

Relates to: `../calibration/2026-07-21-warm-path-profile.md` (fixed-floor
decomposition), ADR 0022 (fused static classification table).
