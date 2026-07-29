# Macro rejections — category-wide "no"s and why

Big rejections that keep resurfacing in per-idea docs; recorded once.

## wasm parallelism (2026-07-29, consolidating the 2026-07 record)

The browser pays serial, permanently for now. A gated threaded-rayon spike
was tried and rejected (`galley-resident-handle` branch tip). wasm threads
(SharedArrayBuffer + COOP/COEP headers) remain a known technical path, but
the deployment complexity (cross-origin isolation on every embedding host)
is a real product cost, and every measured need so far dissolved elsewhere:
the resident warm path is sub-millisecond serial, and the perf cold-start is
adjudicated as acceptable with the persisted-findings warm start covering
perceived latency (ADR 0068, `handoffs/2026-07-21-persist-packed-findings-recipe.md`).
Revive condition: a measured, user-visible browser need that survives those
two mitigations — not a benchmark.
