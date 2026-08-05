# Candidate — `uni.nonletter-usage-anomaly` materialize-time segmentation trade

- **Date:** 2026-08-04
- **Status:** candidate, **deliberately not implemented**
- **Origin:** the nonletter-usage epic's checkpoint-5 warm-path packet
  (progress log Entries 19–20). It was remedy (2) of the two the warm regression
  offered; the mediator ruled remedy (1) (the dirty-chapter restriction) and ruled
  this one recorded-not-taken.

`NonletterBookContribution::materialize` re-derives per site what its map already
knew: it calls `grapheme::segment` on the run and then scans every member against
every channel. With remedy (1) landed, that work is now paid only for the chapters
a call actually owes — but it is still the dominant per-chapter materialization
cost, and it is paid in full on every whole-partition rebuild (any edit that moves
a nonletter count, every judging-knob change, every cold analyze).

The trade would be to retain the run's member spans (or member count + each
member's coarse class) on the site instead of recomputing them, or to memoize
segmentation per distinct run string within a materialize pass.

**Why it was not taken now.** It is a retained-layout change against a measured
budget that is already the tight one: the substrate retains **4.01 MB** on
WA-en-ulb (≈3.4 KB/chapter), which is **32% of the whole resident footprint at
shipped defaults**. Plan §7.5 chose retained compact sites on a measurement that
predates both the class-conditioned topology table and the deferred-edge identity
strings, so the honest starting point for this idea is a fresh retained-bytes
measurement, not the packet's superseded 1.1 KB/chapter estimate.

**What would justify it.** A measurement showing that whole-partition rebuilds are
a real interaction cost after remedy (1) — i.e. that punctuation edits (which
legitimately move the corpus-global denominators and so legitimately owe every
chapter) are common enough in the editor to matter — *and* that the retained-byte
cost of the memo lands well under the segmentation saved. A per-pass memo keyed by
run string is the cheaper half of the idea and carries no retained cost at all; it
should be measured first and separately.

Emission-side either way: neither variant can change a finding, so both are gated
by byte-identity against the pinned full-fleet dumps.
