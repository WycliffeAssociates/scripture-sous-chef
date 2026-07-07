# ADR 0039: Quote / discourse-marker balance stays deferred — now with census data

- **Date:** 2026-07-07
- **Status:** Deferred (reaffirms the ADR 0011/0016 deferral, with evidence)
- **Relates to:** [ADR 0037](0037-bracket-balance-corpus-relative.md) (the
  bracket machinery a quote engine would reuse).

## Context

With brackets solved corpus-relatively (ADR 0037), quotes are the obvious
next balance family — and the obviously harder one: `"`/`'` are
ambidextrous, curly-quote direction is a per-language convention (German
`„…“` reuses English's opener as a closer), the apostrophe is a full letter
in many minority-language orthographies, and English-style typography
re-opens a multi-paragraph quotation per paragraph without closing it —
putting a pervasive *convention* directly on top of the unmatched-opener
*error class*. Rather than park on intuition, a read-only census of all 106
corpora measured who marks dialogue, with what, and what a naive balance
pass would find (2026-07-07; per-corpus×glyph aggregates committed at
[`../calibration/data/2026-07-07-quote-census.tsv`](../calibration/data/2026-07-07-quote-census.tsv);
method: a naive `\v`-line extraction + per-occurrence clinging
classification + per-book stack simulation over directional pairs).

## What the census found

- **99/106 corpora mark dialogue with quote glyphs**; 7 don't (my_juds,
  hac, ta_ulb, bji, tel-x-gusavu, bez, kmr-IQ-badini ≈ zero density).
- **Systems:** ASCII straight `"` **67**, curly `“”` **24**, guillemets
  `«»` **4**, curly single **2**, ASCII `<<`/`>>` **2**. Zero corpora use
  CJK corners, `„`, or fullwidth forms.
- **The apostrophe is a letter in 28 corpora** (≥50% of `'`/`’`
  occurrences mid-word; gey 99.7%, nyn 99.0%, hke 97.1%, plt 96.1% — plus
  French/Assamese elision). Any rule touching the single-quote family
  misfires on ordinary words in ~¼ of the audience without a per-corpus
  letter gate.
- **No role reversals**: every directional glyph runs in its
  Unicode-default direction across all 106. (Measurement note for any
  future attempt: closers classify as ISOLATED, not CLOSE-like, because
  they cling to sentence punctuation — direction must be read off the OPEN
  axis.)
- **Balance simulation** (44 corpora with a clearly directional pair):
  86.6k opens / 86.8k closes; **~4,200 real unmatched closers** (after
  removing 4,611 artifacts from one apostrophe-letter corpus), **8,598
  unmatched openers at book end**, **7,948 continuation-signature opens**
  (opener at a verse start while the same quote is open). 43 of 44 corpora
  have unmatched closers.
- The unmatched-closer volume is **convention-driven, not error-driven**:
  gu_ulb's 1,082 come from a house style that systematically *drops the
  opener* (1,522 closers vs 440 openers); mji's 1,397 from count asymmetry
  across verse splits. Naive nesting depth reaches 150–225 in
  continuation-style corpora — a stack cannot separate real two-level
  nesting from re-opens.

## Decision

The family stays **deferred**, now for cited reasons rather than caution:

1. Balance logic is inapplicable to the majority system (67 corpora use
   ambidextrous `"` — nothing to stack without per-occurrence role
   inference of unproven reliability).
2. The pervasive continuation/re-open convention sits on the
   unmatched-opener class; conventions and errors share the observable.
3. Even the hypothesized "clean slice" — stray closers, which no
   convention protects in principle — is dominated in practice by
   dropped-opener house styles and cross-verse span accounting, not
   defects. The genuinely quiet subset is ~5 low-density curly-quote
   corpora with single-digit counts: not a rule's worth of signal.
4. Existing rules already catch quote *wreckage* through other doors
   (punct-only's stranded `` ` ``/`("` chunks, adjacency's doubled-quote
   anomalies), so the marginal value is a sliver.

Also rejected: exploring via English-source sample alignment — that is
source-relative typography comparison, which asks "does the target diverge
from English quoting?", a question this architecture refuses on principle.

## Revisit criteria

Unpark only if one of these changes the picture: (a) a consumer asks for a
per-project, opt-in, curly-pair-only diagnostic and accepts manual triage;
(b) the source-relative tier exists and quote structure is wanted as a
*weak suspicion signal* there, never a verdict; (c) a future census shows a
corpus population whose quoting is directional and continuation-free. The
committed census TSV documents the baseline to re-measure against.
