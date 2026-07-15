# Plan — `uni.mixed-normalization` (within-corpus encoding-mixing finding)

Date: 2026-07-14. Depth: standard. Interview: quick. Testing: standard.
Supersedes the design in `documentation/ideas/2026-07-11-mixed-normalization-rule.md`
(the probabilistic/scored approach — see "Design provenance" below for why it
was dropped).

## Goal

Detect corpora that write **the same abstract character two different ways**
(e.g. `é` as precomposed `U+00E9` in some places and `e` + `U+0301` in others).
That's a real defect — it silently breaks exact-match search, de-duplication,
and cross-corpus tooling, and today it is either invisible or mislabeled (the
ADR 0053 rare-glyph residual). Surface it as **one deterministic finding per
corpus**; let the editor fix it with a one-click **NFC** normalization.

### Non-goals

- **Not** a scored / convention-learned rule. There is no threshold, no knee,
  no calibration — the condition is binary.
- **Not** a "conform everything to NFC" checker. A corpus that *consistently*
  uses a non-NFC form (e.g. always-decomposed, or always the precomposed
  Bengali `য়`) is internally fine and stays **silent**. We flag *mixing*, not
  non-NFC-ness.
- **Not** an automatic mutation. Normalization is a deliberate, opt-in editor
  action (like running a formatter), never a side effect of opening/saving.

## Evidence (the spike — throwaway `examples/nfc_spike.rs`, `examples/nfc_fleet.rs`)

Measured over the full 1,504-corpus vref fleet:

- **Cost of the check is negligible.** `is_nfc` over a full English Bible is
  3.3 ms; worst case (Assamese, full corpus) 94 ms — and the real rule
  piggybacks the walk `analyze` already does. The `unicode-normalization`
  crate is fine on perf; no vendored table needed. (It pulled only `tinyvec`;
  `no_std`+alloc, wasm-clean — confirm exact wasm byte delta before landing.)
- **NFC is not "compose everything."** Bengali `য়` (`U+09DF`) is a composition
  *exclusion*: NFC **decomposes** it to `য` + `়` and the text gets *longer*.
  So "normalize" is script-dependent, not a tidy-up-into-composed operation.
- **A scored *mixing* rule would be silent on the biggest real signal.** The
  heavily-non-NFC Indic corpora are *consistent* (100% `য়`), so a
  dominance×rarity rule sees no minority form and says nothing. Confirmed dead.
- **Mixing is rare and precise.** Only **69 / 1504 (4.6%)** corpora mix at all;
  **54 (3.6%)** have ≥5 minority-form occurrences. By contrast "any non-NFC"
  would fire on ~900 corpora (833 are "mostly composed" but still contain
  stray non-NFC clusters). Detecting *mixing* is the signal; detecting
  *non-NFC* is noise.
- **The mixers are real defects.** Top offenders: `WA-lee-reg` (47 characters
  written two ways, ~30k minority occurrences, 32%/68% composed/decomposed),
  several African-Latin `reg` corpora, `hboWLC`/`hebwlc` (Hebrew — combining
  marks in non-canonical order), and the mostly-composed-with-a-stray-decomposed
  case that is exactly the ADR 0053 residual.

Full per-corpus table: `scratchpad/nfc_fleet.tsv`.

## Design

### Detection — deterministic `ProjectRule`

A new `ProjectRule` (sees the whole corpus as `Books`, returns `Vec<Finding>`;
`rule.rs:67`). The algorithm, validated in `nfc_fleet.rs`:

1. Walk grapheme clusters (`unicode-segmentation`, already a dep — see the
   grapheme-iteration preference, not hand-rolled mark tables).
2. Skip clusters that carry no form signal: pure-ASCII, and any cluster where
   `is_nfc(g) && is_nfd(g)` (atomic, no decomposition).
3. Bucket the rest by their **NFC key** (`g.nfc().collect::<String>()`),
   tracking each distinct raw form and its first `(Sid, byte-Span)`.
4. A corpus **mixes** iff some NFC key has **≥2 distinct raw forms**. Emit
   **exactly one** `Finding`, anchored at the first occurrence (corpus order)
   of a cluster that is *not* its key's majority form.

This bucketing handles compose / decompose / composition-exclusion / mark-order
cases **uniformly** — consistent `য়` yields one raw form per key → silent;
mixed `য়` vs `য়` yields two → fires. That uniformity is why we don't special-
case scripts.

- `score`: `None` (deterministic).
- `severity`: low / hint — mechanical, non-semantic; match the `uni.*` family.
  (Confirm against the `Severity` enum at build time.)
- Cost: one linear grapheme pass with an ASCII fast-path; piggybacks the
  existing corpus walk. No incremental machinery needed (re-scan is cheap).

### Finding payload — minimal

New closed-union arm `FindingArgs::Normalization { affected: u32, example: char }`
(`diagnostics.rs:189`):

- `affected` — total minority-form occurrences across all mixed keys. A **scale
  signal only** ("142 places affected"); the editor may render or ignore it.
- `example` — the mixed abstract character in NFC form (the anchor's key), so
  the message can say *"e.g. `é`"*.
- **Deliberately no** dominant-lean / composed-vs-decomposed breakdown. That
  distributional detail is census/fleet-report territory, not an editor finding
  (end users experience it as noise).

Message (consumer-side ICU, plain): *"This text encodes some characters two
different ways, which can break search and consistency. Normalize to the
standard form?"*

### Fix — opt-in NFC in the editor's format path

The core never mutates (it is report-only). The fix lives in the JS consumer's
**existing opt-in "format text" path** (the one that already strips linebreaks
for typesetting): `verses.map(v => v.normalize('NFC'))`. Properties:

- **Idempotent** — after one pass the corpus is stable; opening/saving never
  re-dirties it. The one-time byte diff is the cost of a stable baseline
  (git-visible, committed once — the `rustfmt`/`prettier` model).
- **Target = NFC, uniform.** It is the interchange standard every downstream
  tokenizer/search index expects, works for every case (including the Hebrew
  ordering and Indic exclusion cases where "dominant form" is undefined), and
  matches the fleet majority (833 composed vs 39 decomposed). The one-time diff
  is larger for the ~39 decomposed-leaning corpora and decomposes `য়` — the ADR
  records this; the `affected` count gives the human the blast radius up front.

### Dependency

Promote `unicode-normalization` from the throwaway `[dev-dependencies]` line to
a real workspace dependency used by `crates/core`. Consistent with the three
Unicode crates core already ships (`unicode-segmentation`, `unicode-script`,
`unicode-properties`); a vendored decomposition table is *not* warranted —
correct NFC needs canonical-ordering + composition-exclusions (unlike the flat
`BidiBrackets` table), and the table's only prior justification (feeding a
scored rule) is gone.

### Design provenance (why the idea doc's shape was dropped)

The 2026-07-11 idea proposed a scored, convention-learned `StatefulRule` with a
vendored decomposition table. The spike killed both halves: (a) a scored mixing
rule is silent on the dominant real case (consistent-but-non-NFC Indic corpora);
(b) hand-rolling NFC from a "trimmed table" is far more than the flat
`BidiBrackets` precedent it invoked, and unjustified once the score is gone.
The condition is deterministic and binary, so it is a deterministic finding.

## ADR (next free number at write time)

Record, because a future reader will wonder and it is hard-ish to reverse:

1. **Deterministic mixing finding, not a scored rule** — with the spike numbers
   (4.6% / 3.6% prevalence) as the calibration record.
2. **One finding per corpus** — the first finding in the codebase whose
   cardinality is "one per corpus" rather than "one per occurrence." Establishes
   the pattern for future mechanical hygiene fixes (`str.replace(badcp, "")`
   et al.): a corpus-wide mechanical condition → one anchored finding + bulk fix.
3. **NFC as the fix target** — the values call (interchange standard vs
   minimal per-corpus churn) and the `য়`-decomposition caveat.
4. Closes the ADR 0053 M-exclusion residual note (this finding now owns the
   mislabeled signal).

## Testing (standard — synthetic `VerseMap`s, never corpus fixtures)

Behavior, tied to intent:

- **Fires once** on a corpus mixing composed `é` and decomposed `e`+`◌́`;
  `affected` = the minority count; anchor span = first deviant cluster.
- **Silent** on a consistently-composed corpus.
- **Silent** on a consistently-decomposed corpus. *(This is the load-bearing
  test: mixing ≠ non-NFC.)*
- **Silent** on a corpus consistently using precomposed-excluded `য়`.
- **Fires** on a corpus mixing `য়` (`U+09DF`) with `য`+`়` — proves the
  NFC-key bucketing catches the exclusion case.
- **Silent** on pure ASCII.
- **One finding, not N** — a corpus mixing several distinct characters still
  emits exactly one finding.
- JS side: the fix is idempotent (`nfc(nfc(x)) == nfc(x)`) — a property test in
  the consumer.

## Execution steps (verify each before the next)

1. **Promote the dep** (`unicode-normalization` → real core dependency) and
   confirm `no_std`/wasm build is clean; note the wasm byte delta. *Verify:*
   `cargo build -p ssc-core` + the wasm build succeed.
2. **`FindingArgs::Normalization` arm** + `RuleId::uni.mixed-normalization` +
   Tsify/serde wiring. *Verify:* TS types regenerate; serde round-trips.
3. **The `ProjectRule`** (detection algorithm above), registered in
   `project_rules`, default-off/labs. *Verify:* the synthetic tests above.
4. **Consumer message + editor fix** in the existing format path. *Verify:*
   drive the editor path on a hand-built mixed sample; confirm one finding, and
   that applying the fix clears it and is idempotent.
5. **ADR** + docs page under `uni.md`.
6. **Oracle check:** default-off keeps the `v1_defaults` finding dump
   byte-identical; the everything-on dump gains this finding — intentional,
   recorded in the ADR (not perf drift).
7. **Remove spike scaffolding** (`examples/nfc_spike.rs`, `examples/nfc_fleet.rs`,
   the dev-dep line, `scratchpad/nfc_fleet.tsv`).

## Follow-ups (explicitly deferred, not v1)

- **Census lane** — knob-free `normalization` count carrying the
  composed/decomposed/neither breakdown for the offline fleet report.
- **ADR 0053 rare-glyph coordination** — teach the rare-glyph lane to skip a
  scalar that is a normalization variant of a common glyph (the ADR 0034
  "one phenomenon, one finding" carve-out), now that the crate makes the
  equivalence test available. Can land after the finding.
- **NFD-alternative button** — if respecting per-corpus convention over the
  interchange standard ever matters, offer NFD in the editor with NFC as the
  recommended default. Not auto-picked from the dominant count.

## Open decisions / risks

- **Severity** of the finding — confirm the exact `Severity` variant.
- **Default-off vs labs vs on** — proposed default-off initially (consistent
  with new `uni.*` rules), graduate after review.
- **Anchor choice** — "first deviant occurrence" is a defensible example;
  revisit only if the editor wants a different jump target.
