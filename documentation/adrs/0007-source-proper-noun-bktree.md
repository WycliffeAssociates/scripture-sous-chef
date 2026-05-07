# ADR 0007: Source proper-noun match uses Damerau-Levenshtein ≤ 2 within the same verse

- **Date:** 2026-05-07 (originally specified BK-tree; superseded same day by amendment below)
- **Status:** Accepted (amended)
- **Plan reference:** `research/proposed/2026-05-06_signal-architecture/plan.md` §3.2 amendment (flavour 1)

## Amendment 2026-05-07 (during Phase A #4 implementation)

The original ADR specified a BK-tree over rare uppercase source tokens
("the codebase already has `analysis/bktree.rs`"). Implementation review
revealed `analysis/bktree.rs` is a doc stub — "Not yet implemented." The
codebase actually does its existing edit-distance work (in
`analysis/candidate_families.rs:311`) via `strsim::damerau_levenshtein`
directly, no BK-tree.

At this scale, brute-force is the right tool: per-target-token query
runs against the *intersection* of (a) rare uppercase source tokens
and (b) tokens present in the corresponding source verse. That set is
typically 0–3 tokens; the BK-tree's sublinear advantage matters only
when querying against thousands of candidates per call.

**Revised decision:** use `strsim::damerau_levenshtein` directly,
brute-forcing over the per-source-verse rare-uppercase token set. No
BK-tree built. The original semantic threshold (≤ 2) is unchanged.

## Context

The `source_relative_co_rarity` factor's flavour 1 (saturated downweight,
suspicion factor `0.0`) is meant to catch transliterated proper nouns:
a target token like Bemba `Davidi` should not surface as suspicious
when the source verse contains `David`.

The plan's original framing — "source verse contains an `IntrinsicUpper`
token that's also rare in the source" — is too loose. Under that rule,
*any* rare target token in a verse that happens to contain a source-side
proper noun gets exonerated, even if the target token has no relation
to that proper noun. False negatives in the per-token suspicion lane
result.

## Decision

For each rare target token in a verse, emit the saturated downweight
(`0.0`) only when **both** of the following hold:

1. The target token is itself uppercase-shaped per the target's case
   profile (i.e., the target is a candidate proper noun, not just any
   rare word).
2. The target token has BK-distance ≤ 2 to a rare uppercase token in
   the source's same verse, where "rare uppercase source tokens" are
   indexed in a `BkTree` built once at startup.

Otherwise, fall through to the next flavour or the unremarkable case.

## Rationale

**Why edit-distance gating.** Transliteration of a proper noun
typically produces a target form that's a small edit-distance away
from the source form (consonant insertion, vowel substitution, suffix
addition for Bantu cases like `David` → `Davidi`). BK-tree of
distance ≤ 2 catches the common transliteration shapes without
admitting unrelated rare words.

**Why ≤ 2.** Distance 1 is too tight (`David` → `Davide` is fine but
`Yesu` → `Iesus` is 3+). Distance 3 admits enough unrelated words to
produce false negatives. Distance 2 is the empirical sweet spot for
proper-noun transliteration in Latin-script corpora; we'll revisit
if a non-Latin pair shows different edit-distance distributions.

**Why both target uppercase-shape AND source uppercase.** Restricting
to target-side proper-noun candidates gates the BK-tree query to
tokens where transliteration is even plausible. Without this, every
rare target token would query the BK-tree and we'd get spurious
matches against unrelated rare uppercase source tokens that happen
to be edit-close.

**Why same verse.** The BK-tree is over the *whole source corpus's*
rare uppercase tokens, but the match must be a token also present in
the *corresponding source verse*. Without verse co-location, a target
hapax that happens to be edit-close to any proper noun anywhere in
the source NT would get exonerated, regardless of whether that
proper noun is contextually present.

**Why the BK-tree.** The codebase already has `analysis/bktree.rs`.
Per-target query is sublinear; the index of rare uppercase source
tokens is small (a few thousand entries max for an NT). Performance
is not a concern.

## Consequences

**Enables:**
- `Davidi` ↔ `David`, `Iesu` ↔ `Yesu`, `Petelo` ↔ `Peter` and similar
  transliteration patterns get the saturated downweight cleanly.
- Unrelated rare target tokens in proper-noun-containing source
  verses don't get spuriously exonerated.

**Forecloses:**
- Cannot match transliterations with edit-distance > 2 (e.g., heavy
  morphological wrapping like `aBuDaviDi`). If this matters in
  practice, we'd add a stem-extraction step before the BK query.

**Costs:**
- Source `Lexicon` must be built (free; same code path as target).
- BK-tree index built once at project load. Memory cost negligible.
- Slight increase in per-rare-token cost during triage for the
  uppercase-shape-then-BK-query path.

## Alternatives considered

1. **No edit-distance check; just "source has rare uppercase + target
   is rare in same verse."** Rejected: too loose; produces false
   negatives. This was the plan's original framing.
2. **Damerau-Levenshtein instead of standard Levenshtein.** Considered;
   adds transposition handling. Worth evaluating if the BK-tree
   library supports it cleanly. Defer the choice to implementation.
3. **Stem-prefix matching instead of edit-distance.** E.g., compare
   first N characters. Cheaper but brittle: doesn't catch
   `Yesu`/`Jesus` (different first character).
4. **Distance ≤ 1 or distance ≤ 3.** See rationale; ≤ 2 chosen as
   empirical sweet spot. Revisit if data shows otherwise.
