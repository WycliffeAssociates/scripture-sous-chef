# SIL Audit — Implementation Log

Companion to `sil_audit.md`. Tracks which audit items have actually
landed, what got changed during implementation, and what we learned
that wasn't in the original write-up. Append-only — when an item
ships, add an entry below; do not edit the audit document itself.

---

## §1.4 Punctuation taxonomy (clinging classes) — Batch A

**Status:** Shipped.

**What landed:**

- New module `crates/core/src/punctuation_class.rs` with a single
  curated classification table (`ClingingClass` enum) replacing four
  drifting tables that had grown up across the codebase:
  - `unicode::is_open_punctuation` (Ps/Pi approximation, ~50 codepoints)
  - `unicode::is_close_punctuation` (Pe/Pf approximation, ~50 codepoints)
  - `unicode::is_symmetric_quote` (single codepoint)
  - `discourse::punctuation_matches` (11-pair lookup)
- Pair info lives on the `LeftClinging { closers: &'static [char] }`
  variant; matching is checked from the opener side. Single source of
  truth, structurally drift-free, with a debug-time invariant test
  asserting every named closer itself classifies as `RightClinging`.
- `resolve_ambiguous(prev, next)` decides per-occurrence whether `"` /
  `'` opens, closes, is word-internal, or remains ambiguous, based on
  whether neighbors are content characters (alphanumeric).
- `discourse.rs::SpanIndex::build_inner` rewritten to dispatch on
  `clinging_class(c)` with peeked `next` and tracked `prev`. The
  30-Sid corruption guard and book-boundary flush are unchanged —
  both are independently load-bearing.
- The three deleted predicates have **no** wrapper / shim layer.
  Call sites read the new API directly. (Pre-alpha; clean redesign
  preferred over compat.)

**Deviations from the audit's recommendation:**

1. **Added a `Terminal` variant** for `,.;:!?`. The audit's SIL
   reference puts these under `RIGHT_CLINGING`, but our `RightClinging`
   means "closes a span". Two distinct behaviors needed two variants.
   Spacing-class consumers (future hygiene rules) read `Terminal` and
   `RightClinging` together; the resolver only acts on `RightClinging`.
2. **`closers` is a slice, not a single char.** Driven by the
   Japanese double-prime quote `\u{301D}` legitimately closing with
   either `\u{301E}` or `\u{301F}` in real-world data.
3. **Apostrophe `'` lifted into `AmbiguousSymmetric`** (the audit
   left it as a TODO). The same content-vs-non-content rule that
   handles `"` also handles `'` correctly: `John's` is letter/letter
   ⇒ `Internal` ⇒ skip, with no apostrophe-specific code.
4. **Asymmetric resolver policy for `'` vs `"`.** Discovered while
   running on en_ulb: `'` is far more often a contraction or possessive
   than a paired quote, so:
   - `'` only opens spans when there's an enclosing `"` already on
     the stack (nested-quote context).
   - `'` only closes spans when there's a matching `'` on top of
     the stack.
   - `'` ClosesSpan with no stack support is silently dropped
     (plural possessives like `fathers'` would otherwise produce
     constant `UnexpectedClose` anomalies).
   - `"` keeps the strict policy — orphan close → `UnexpectedClose`,
     which is the original false-positive we set out to fix.

**Empirical impact (en_ulb, 30,389 verses):**

| Metric | Before | After §1.4 toggle fix | After `'`-policy fix |
| --- | --- | --- | --- |
| `punct.paired-balance` findings | 3,099 | 1,954 | 1,661 |
| Surfaced clusters | 1,381 | 624 | 374 |

The remaining 374 surfaced clusters are spot-checking as legitimate
translation imbalances — quotes that genuinely don't close within the
30-Sid corruption window. (E.g. `1KI 17:14` is missing a `'` before
its closing `"`.)

**Bonus refinement** (not part of the audit): paired-balance findings
now embed a one-word-per-side context snippet so nested-quote
findings name the actual quote in question. `2SA 7:8` reads
`unclosed punctuation ''' opened in 2SA 7:8 near …David, 'This…`
rather than just `'`.

**Tests:** 104 core-crate tests passing. Regression coverage for:
- nested LIFO (`("go")`)
- multi-level nested closures (`."'"'"`)
- orphan straight quote not desyncing later legitimate pairs
- plural possessive (`fathers'`, `Moses'`) producing no anomalies
- standalone `'twas`-style apostrophes producing no false opens
- corruption guard / book boundary still active

**Notes for future work in this area:**

- The `Terminal` variant currently has only one consumer (the
  resolver's "no-op for span tracking" branch). When we eventually
  add hygiene rules like `space-before-right-clinging`,
  `consecutive-punctuation`, etc., they should read directly from
  `clinging_class(c)` and not maintain their own char lists.
- The asymmetric `'` / `"` policy in the resolver is currently
  policy-as-code in `discourse.rs`. If a third symmetric mark ever
  joins (e.g. some script's symmetric quote), we should revisit
  whether to encode this policy on the classification variant
  itself rather than branching on `c == '\''`.
- Cross-verse quote-span findings (`unclosed `'` opened in X, not
  closed by Y`) are noisy when the gap is large. The 30-Sid window
  catches the majority. If we ever find this still too aggressive,
  the next step is to teach the rule to suppress findings when the
  open and close are both within the same paragraph break.

---

## What's next — recommendation

The audit's §14 batches still on the table:

| Item | Section | Adds rule IDs? | Cost |
| ---- | ------- | -------------- | ---- |
| **Lemma-cluster induction** | §3.1 | No — foundational, sharpens existing rules | Multi-day |
| Beta-Binomial conjugate updates | §6.1 | No — calibration layer | Small core, but needs UI plumbing for labels |
| JSD per-verse vocab drift | §5.3 | Yes — `SSC-PROP-004` | Small |
| Mixed-script-in-token | §7.2 | Yes — `SSC-UNI-002` | Small |
| Charset-divergence-per-verse | §7.3 | Yes — `SSC-UNI-003` | Small |
| Extended edit metric (transpose / expand / compress) | §4.1 | No — sharpens `SSC-CONS-001` | Medium |
| UPGMA / DBSCAN clustering | §4.2 / §4.3 | Adds `SSC-CONS-004` for canonical-form output | Medium |

**My pick: lemma-cluster induction (`SSC-LEMMA-001`, audit §3.1).**

Why this and not one of the cheaper Batch A items:

1. **It matches your stated philosophy.** You said you're keeping rule
   count down and refining the analysis phase. Lemma-clustering adds
   *zero* new rule IDs — it's a foundational data-structure change
   that makes existing rules (hapax-suspicion, IntrinsicUpper voting,
   source-relative co-occurrence, position-conditional Dunning) more
   accurate by aggregating evidence at the lemma level instead of
   the surface-form level. Same surface area, sharper signal.

2. **It unblocks the most downstream value.** The audit lists four
   modules that improve the moment lemma clusters exist:
   `analysis/lexicon.rs`, `analysis/dunning.rs` (positional),
   `signals/lexical.rs` (hapax), `signals/source_relative.rs`. None
   of the other Batch A items have that kind of leverage.

3. **It uses what's already built.** The algorithm sketch in §3.1 is
   "BK-tree neighborhoods + source-anchored Dunning LLR + LCS-fraction
   guard" — `bktree.rs` and `source_relative.rs` already exist. The
   new code is the LCS guard and the cluster-induction loop. No new
   external dependencies.

4. **It's the prerequisite for several Batch B/C items.** PoorMans
   stemming (§3.2) is conditional on lemma-clustering being insufficient
   first; UPGMA canonical forms (§4.2) become more meaningful when the
   clusters they structure are lemma clusters, not surface clusters.
   Skipping straight to lemma-clustering shortens the dependency chain.

**Rough shape of the work:**

- New module `crates/core/src/analysis/lemma.rs`.
- Two-branch induction (source-anchored for tokens with strong source
  correlates; target-only frequency-greedy for the rest).
- The `lcs_fraction ≥ 0.6` guard is the load-bearing detail — without
  it, Bantu prefix paradigms collapse incorrectly. Worth lifting from
  the audit verbatim.
- Counter-example test (the `John` / `Joan` case from §3.1) goes in
  alongside.
- Wire into `lexicon.rs` as the first consumer (vote IntrinsicUpper at
  lemma level). Measure delta on en_ulb / Bemba / Rai before extending
  to the other consumers.

**The case against doing JSD or mixed-script-in-token first** is
mainly that they each add a *new* rule ID that you'll then have to
calibrate and weight in `AggregationPolicy`, with no compounding
effect on the rest of the engine. They're cheap to land but the
return is bounded. Lemma-clustering is the opposite shape — more
upfront work, but it raises the quality ceiling for everything else
already in the engine.

What's your read?
