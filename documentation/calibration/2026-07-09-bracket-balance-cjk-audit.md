# Calibration — bracket-balance CJK corner-bracket audit (ADR 0049)

- **Date:** 2026-07-09
- **Harness:** `cargo run --release -p ssc-core --example calibrate --
  --fleet corpora/vref`, **1,504 corpora**, `Config::all()`, all floors
  zeroed for the histograms; per-corpus audit via the new `--bracket <corpus>`
  mode (floor-0 score distribution, per-family tallies, ~20 sample findings
  with their `DelimObservation` inventories).

## The storm

The 2026-07-09 fleet survey found `punct.bracket-balance` surfacing **4,578**
findings fleet-wide, of which three Chinese editions carried **58%**:

| corpus | surfaced | edition |
|---|---|---|
| cmncbt | 1,556 | Chinese Contemporary Bible (Traditional) |
| cmn-cu89s | 543 | Chinese Union Version 1919 (Simplified) |
| cmn-cu89t | 539 | Chinese Union Version 1919 (Traditional) |
| latVUC | 129 | Latin Clementine Vulgate (unrelated — see below) |

The prior bracket calibration (2026-07-06, ADR 0037) surveyed **106** corpora
and explicitly recorded "CJK corners … none stormed — their corpora either
pair them or don't use them." That survey **did not include the Chinese or
Japanese editions**; the fleet has since grown to 1,504 corpora, and the
storm was invisible until it did.

## Root cause: corner brackets are quotation marks, not brackets

`--bracket cmncbt` per-family tally (family key = the pair's open glyph):

| pair | events | orphans | long | pairing rate |
|---|---|---|---|---|
| 「…」 U+300C corner | 12,548 | 1,450 | 4 | 88.4% |
| 『…』 U+300E white corner | 2,438 | 102 | 0 | 95.8% |
| （…）U+FF08 fullwidth paren | 148 | 0 | 0 | 100.0% |
| 《…》U+300A title mark | 108 | 0 | 0 | 100.0% |

**100% of cmncbt's 1,556 findings come from the corner-bracket families
「」 and 『』.** The genuine CJK text brackets in the same corpus — fullwidth
parens （）, title marks 《》 — pair 100% and produce zero orphans.

The corner brackets 「」 (U+300C/D), 『』 (U+300E/F), and halfwidth ｢｣
(U+FF62/63) are `Ps`/`Pe` in the UCD and so appear in `BidiBrackets.txt`, which
feeds `BRACKET_PAIRS`. But in Chinese/Japanese typography **they are quotation
marks**: 「」 is the primary quote, 『』 the nested quote. Feeding them to a
LIFO bracket matcher makes `punct.bracket-balance` a de-facto quote-balance
rule — and quote balance is deliberately deferred (**ADR 0039**), precisely
because dialogue quoting nests deeply and *re-opens across verse/paragraph
boundaries without closing*, so a stack cannot separate the continuation
convention from real unmatched-opener errors.

The sample inventories show exactly this convention. cmn-cu89s DEU 5:6–5:20
(the Ten Commandments) renders as long runs of unmatched nested openers
`「o! 『o! 「o! 『o! …` — each commandment verse re-opens the speaker quote 「
and the divine-speech quote 『 without closing them, verse after verse. 2KI
19:20–21 and the EZK 16 oracle show the same signature. This is textbook
quote continuation, character-for-character the phenomenon ADR 0039's census
described.

## Not the secondary hypotheses

- **Fullwidth （） / title 《》 / lenticular 【】**: genuine text brackets,
  pair at 99.7–100%, produced **0 orphans** in cmncbt (2 legitimate （）
  orphans in cmn-cu89s at DEU 10:5 / NUM 11:9 — real one-sided brackets, kept).
- **Genuine editorial damage in the 1919 CUV**: not the driver — the orphan
  volume is entirely corner-bracket quote continuation, not stray text
  brackets.

## latVUC is a different phenomenon (out of scope)

latVUC's 129 findings are **all ASCII `[` long-span pairs** (0 orphans, 129
long-span; `[` pairs 100% but the corpus brackets whole canticles/psalms with
`[…]` spanning many verses — `[Confitemini Domino…`). This is the short-span
verdict firing on a genuine long editorial convention, unrelated to CJK
quotes, and unaffected by this change. Flagged here for a future look; not
addressed by ADR 0049.

## Decision (ADR 0049): exclude the corner-bracket family

Option (a) from the audit brief: exclude the quote-role glyphs. The
generator (`xtask gen_charclass_table.rs`) now drops **「」 (U+300C/D), 『』
(U+300E/F), ｢｣ (U+FF62/63)** from `BRACKET_PAIRS` with a documented reason,
mirroring the existing FD3E/FD3F supplement pattern. All other CJK brackets
(《》 title, 〈〉 angle, 【】 lenticular, （）［］ fullwidth) stay in. They
return to bracket scope only when a purpose-built quote engine ships (ADR
0039 revisit criteria).

This was **not ambiguous** between (a) and (b): the matcher is behaving
correctly (it pairs a LIFO stack over declared pairs); the defect is the
*inventory* claiming quotation marks are brackets. The fix belongs at the
inventory boundary, not in the matcher.

## Before / after (fleet, 1,504 corpora)

| metric | before | after | delta |
|---|---|---|---|
| surfaced (≥ 0.5) | 4,578 | 1,920 | −2,658 (−58%) |
| scored sites (floor 0) | 5,385 | 2,727 | −2,658 |
| corpora hit | 371 | 369 | −2 |

Per-corpus — **only four corpora changed, no corpus rose**:

| corpus | before | after |
|---|---|---|
| cmncbt | 1,556 | 0 |
| cmn-cu89s | 543 | 2 |
| cmn-cu89t | 539 | 1 |
| jpn1965 (Japanese) | 23 | 0 |

Controls: **WA-en-ulb** 0 → 0 (clean throughout); **pon2006** (mid-volume
non-CJK, Pohnpeian) 53 → 53 — all ASCII `(` orphans (93.1% pairing), a
genuine bracket signal the change leaves untouched. The fix is surgical: every
delta is a corner-bracket-quoting edition, and jpn1965 confirms it generalizes
beyond Chinese.

## The "85% of floor-0 candidates surface" observation

Partly resolved, largely inherent. Fleet-wide, floor-0 candidates clearing the
0.5 floor: **85.0% → 70.4%**. The corner-bracket volume was the bulk of the
high-scoring mass, so removing it drops the ratio. The residual is *inherent*
to the corpus-relative design: an orphan in a family the corpus pairs at 90%+
scores ~0.9 by construction (Wilson lower bound of the pairing rate), so any
surviving orphan in a high-pairing family clears 0.5. The floor ranks rather
than gates — that is the intended ADR 0037 behavior, not a defect. It is not
fully "resolved" and cannot be without changing what the score means.
