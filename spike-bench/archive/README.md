# Archived spikes

One-off measurement spikes that produced a `documentation/calibration/`
write-up and were then set aside. Kept here — rather than deleted — so a
future "didn't we already measure this?" lands on real code to point at,
not a blank.

**These are not wired into the build.** They live outside `src/bin/`, so
`cargo build` never touches them; they are not guaranteed to compile
against current `ssc-core` and may need dependency/API fixups before they
run again. That's fine — they're reference, not a live harness. If you
need to re-run one, copy it into `src/bin/` and fix it up there.

- `2026-07-18-grapheme-interning-bench/` — grapheme-cluster interning cost
  (lasso / string-interner / FxHashMap). Write-up:
  `documentation/calibration/2026-07-18-grapheme-interning-survey.md`.
- `2026-07-18-wire-format-benches/` — findings wire-format marshal /
  postMessage cost. Write-up:
  `documentation/calibration/2026-07-18-findings-wire-format-survey.md`.
- `2026-07-21-warm-path-profile/` — raw samply captures behind the warm
  resident-path decomposition (v1 defaults + all-rules). The captures
  themselves are gitignored (`*.profile.json`); the live harness that
  regenerates them is `src/bin/warm_ladder_profile.rs`. Write-up:
  `documentation/calibration/2026-07-21-warm-path-profile.md`.
- `2026-07-21-wire-live-confirmation/` — Node worker bench confirming the
  packed findings buffer's live end-to-end win on the real wasm surface.
  Results feed the packed-findings wire spec (granularity-spine plan,
  Appendix A).
