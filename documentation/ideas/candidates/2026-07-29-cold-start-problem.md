# Candidate (whiteboard, not spike) — the statistical cold-start problem

Extracted 2026-07-29 from the post-port roadmap's "heresy still worth
debating" section, verbatim in substance. NOT the perf cold-start (that is
ADR 0068); this is the statistics: corpus-relative purity means a
one-chapter corpus knows nothing, and `confidence_z` is the only dial.

**The heresy.** A reference-fleet-derived prior used strictly for
**presentation ranking** — never gating, never findings, never entering
scores — could soften cold-start without touching the philosophy.

**The counterargument** (strong, may win): any fleet prior smuggles
majority-language conventions into minority-language UX. The debate deserves
to happen on purpose rather than by default.
