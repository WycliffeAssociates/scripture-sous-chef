# ADR 0025: Drop ZWNJ/ZWJ flagging from hygiene — flagging nothing beats flagging wrong

- **Date:** 2026-07-06
- **Status:** Accepted
- **Builds on:** [ADR 0014](0014-deterministic-rule-batch.md) (the deterministic
  batch that introduced the joiner allow-list),
  [ADR 0023](0023-zero-width-space-corpus-relative-anomaly.md) (the corpus-relative
  treatment of the *other* orthography-dependent zero-width char, U+200B).
- **Amends:** `hyg.zero-width-misuse` — removes its treatment of U+200C ZWNJ and
  U+200D ZWJ. Supersedes the parenthetical in ADR 0023 Decision 1 that had
  hygiene keep "the script-aware ZWNJ/ZWJ."

## Context

`hyg.zero-width-misuse` flagged the joiners ZWNJ (`U+200C`) and ZWJ (`U+200D`)
unless the verse's **majority script** was on a hardcoded allow-list of
joiner-using families (Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil,
Telugu, Kannada, Malayalam, Sinhala, Arabic, Myanmar, Thaana).

That allow-list is Latin-centric and wrong in both directions:

- **False positives.** Khmer uses ZWNJ legitimately but is *not* on the list
  (Khmer is a scriptio-continua script whose joiner behaviour the list never
  modelled). A single Khmer corpus (km_ulb) carries **22,648** ZWNJ, every one
  of which the rule flagged — a false-positive storm as large as the U+200B
  storm ADR 0023 removed. The list would need per-script curation to be right,
  and even then majority-script gating mishandles a joiner-script word embedded
  in a Latin-majority verse.
- **Category error.** Joiner legitimacy is not a script on/off fact. It depends
  on `Joining_Type` and the effective shaping context (a cursive-joining
  neighbour, a virama), plus emoji ZWJ sequences that are script-agnostic
  entirely. A majority-script allow-list can't express any of that.

This is the same lesson as ADR 0023 (U+200B): a fixed predicate can't tell a
convention from a slip for an orthography-dependent character. ADR 0023 replaced
the ZWSP predicate with a corpus-relative rule. The joiners deserve the same
treatment — but the correct rule is spec-first (UAX #31 §2.3 allowed-joiner
contexts; `ArabicShaping.txt` `Joining_Type`; Core Spec §23.2 / Ch.9), not
something to ship under time pressure.

## Decision

1. **`hyg.zero-width-misuse` no longer judges ZWNJ or ZWJ at all.** The scan
   skips `U+200C` and `U+200D` alongside the already-skipped `U+200B`, and the
   `majority_script` / `script_allows_joiners` allow-list machinery is deleted
   outright (no shim — this is pre-alpha).
2. **The rule is now purely universal-wrong hygiene.** After this change it
   flags only characters that are invalid *regardless of script* — BOM, RLM/LRM,
   the bidi embeddings/overrides, the word joiner, and the rest of the
   format-control range. Both script-dependent controls (ZWSP via ADR 0023, and
   now the joiners) have left the rule. This is a cleaner philosophical boundary:
   hygiene asserts only the never-legitimate.
3. **No replacement ships now.** A property-driven, corpus-relative joiner rule
   — the shape of `uni.zero-width-space-anomaly`, keyed on `Joining_Type` /
   effective shaping context rather than a curated script list — is the
   sanctioned successor, and is deferred as future work.
   (**Note, added later:** `uni.zero-width-space-anomaly` was itself retired by
   [ADR 0027](0027-redundant-zwsp-deterministic-retire-corpus-relative.md); the
   "shape" meant here is the *corpus-relative learning* approach, not that live
   rule, which no longer exists.)

## Rationale

- **Flagging nothing beats flagging wrong.** The choice is between (a) keep the
  allow-list, accept ~22.6k false positives per Khmer-scale corpus and unknown
  miscalibration on every other non-listed joiner script, or (b) stop flagging
  joiners and lose detection of a genuinely-wrong joiner in a non-joining script
  (a Latin `fo<ZWNJ>o` typo). A wrong joiner in Latin text is rare; the FP storm
  is guaranteed and large. (b) is the strictly better net until a real rule
  exists.
- **Consistent with ADR 0023.** We already accepted, for U+200B, that an
  orthography-dependent zero-width character does not belong in a fixed hygiene
  predicate. Applying the same reasoning to the joiners removes the last
  script-dependent judgement from hygiene rather than leaving one inconsistent
  survivor.
- **Delete, don't shim.** The allow-list, `majority_script`, and the lazy
  joiner-scan bookkeeping are removed rather than disabled behind a flag —
  pre-alpha, no back-compat surface to preserve, and a future joiner rule will
  be built fresh from character properties, not by patching this list.

## Consequences

- The 22,648-per-corpus Khmer ZWNJ false-positive storm goes to zero. Combined
  with ADR 0023, `hyg.zero-width-misuse` now produces **zero** findings on a
  clean Khmer corpus (both its ZWSP and its ZWNJ are left to corpus-relative
  judgement — one implemented, one future).
- **Capability lost until the successor lands:** a wrong joiner in a
  non-joining-script word is now unflagged. Accepted, documented tradeoff.
- The rule's code shrinks: `majority_script`, `script_allows_joiners`, and the
  `HashMap`/`ScriptTag` bookkeeping they needed are gone; the scan is a single
  linear pass with a three-character skip set (`ZWSP`, `ZWNJ`, `ZWJ`).
- ADR 0023 Decision 1's parenthetical ("and the script-aware ZWNJ/ZWJ") is
  superseded; hygiene keeps every *other* zero-width/bidi/format control exactly
  as before.
