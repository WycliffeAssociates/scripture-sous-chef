# Idea — PO proofreading checklist triage (Larry's scripts + Greek Room)

Date: 2026-07-10. **Refreshed 2026-07-30** against the current engine
(26 live rule IDs): the triage's build candidates #1/#2/#4 shipped as
ADRs 0053/0054/0055, #5/#7 dissolved into #1 as predicted, #6 moved to
doubtful, and the census shipped (ADR 0058) — statuses below updated in
place. Input: the product owner's proofreading list (Larry's
scripts per everyone, Greek Room per Aaron B), triaged item-by-item into
owning subsystem, with status against the engine, and remarks on how
sous-chef implements the idea
differently — convention-learned and score-surfaced in the hot loop,
and/or exhaustively counted in **absolute mode** (the census report,
`census(map) → Inventory`, ADR 0058). Report sections, in user-facing
terms: **Letters**
(the glyph census — "glyphs" internally), **Punctuation** (sequences,
spacing profiles, brackets, invisible/format characters), **Numbers**
(digit-bearing tokens grouped by shape), **Words** (case shapes,
mixed-casing table), and later **Compared to source** (untranslated
words; grows into alignment-backed checks if that lands).

Legend — **DONE**: shipped rule covers it. **PARTIAL**: adjacent rule
covers part. **CANDIDATE**: worth building. **CENSUS**: absolute-mode
row only; no hot-loop rule warranted (usually because it's house style,
not error — the census shows it, a human judges). **POSTPONED** /
**REJECTED**: adjudicated elsewhere (cited). **ASK PO**: meaning unclear.

## EDITOR (scripture-editor-proto-2 — lints, source-of-truth checks, UI)

| Item | Remarks |
| --- | --- |
| Repeated verse markers | Monotonicity isn't a USFM error; editor-owned lint. |
| Missing verse between | Same. |
| Verse out of order | Same. |
| Inconsistent book titles | Consistency linter over `\h`/`\cl`-class metadata (the `\cl` examiner). |
| Uppercase book title | Same family. |
| Inconsistent chapter titling | Same. |
| Nonstandard chapter label | Same. |
| Some chapters lack chapter labels | Same. |
| Possible section title at end / on own line | Layout-shaped; editor. |
| Book title does not match PPC | Needs external source of truth. |
| Number of verses in chapter not usual | Needs versification source of truth; non-statistical. |
| Verses in target but not in source (Greek Room Notes) | Simple presence diff once source is loaded; editor. |
| Verse bridges / verse number anomalies (Owl) | Editor or onion depending on where detected. |

## ONION (USFM correctness)

| Item | Remarks |
| --- | --- |
| Empty verse markers | Marker-level validity. NOTE: content-side is already DONE in sous (`hyg.empty-verse`). |
| Unsupported USFM marker | Onion. |
| Verse number in text / likely verse reference / probable chapter | Onion (or editor). |
| Extra text / unmarked text | **ASK PO** — probably text outside any marker → onion. |
| Optional text or untagged footnote | **ASK PO**. |

## SOUS — already covered

| Item | Status | Rule / remarks |
| --- | --- | --- |
| Verse lengths vs source | **DONE** (uncalibrated) | `prop.length-ratio` — median+MAD robust-z per book and project. Never calibrated (shortlist item 1: source-paired survey). |
| Space around punctuation | **DONE** | `punct.spacing-anomaly`, rewritten as joint attachment signatures (ADR 0054, superseding the before-only ADR 0050 model) — both sides of every mark observed. Convention-learned: French `« … »` spacing is the corpus's own majority, never flagged. Census lane: `punct.mark-spacing`. |
| Space before phrase-ending mark | **DONE** | Same rule — it's one spacing context. |
| Repeated words | **DONE** | `lex.duplicate-word` (toggle; auto-recommendation folded into config-recommender idea). |
| Unpaired delimiters (paren-like) | **DONE** | `punct.bracket-balance` + ADR 0049 inventory (corpus-learned pair set; CJK corner brackets excluded). Quotes deliberately out — parked with census data, ADR 0039. |
| Unmatched angle bracket | **DONE** | Bracket inventory; a lone `<` is unpaired by count. |
| Free-floating mark | **DONE** | `uni.combining-mark-without-base` (+ `uni.redundant-zero-width-space`, `hyg.zero-width-misuse`). |
| Orphaned punctuation | **DONE** | `lex.punct-only-token`. |
| Stranded backslash at end of line | **DONE** | `struct.source-marker-leftover`; marker validity itself is onion. |
| Unresolved translation conflict | **DONE** | `struct.merge-conflict-marker`. |
| Unexpected characters (universally-wrong subset) | **DONE** | `hyg.invalid-codepoint`, `hyg.control-chars`, `hyg.replacement-run`, `hyg.tab-in-body`. The *corpus-relative* subset (legit-somewhere glyphs, rare here) is the rare-glyph candidate below. |
| Repeated / doubled punctuation (Amharic `፡፡`-class) | **DONE** (separators) / **PARTIAL** (quotes) | `punct.adjacency-anomaly` already *is* "repeated-character-run for punct": it catches `wait,, what` and `what?!?` by counting how often this corpus writes that exact run — a run that's frequent (Amharic `፡፡`) or spread across many books is learned as a convention and goes silent; a run the corpus almost never writes surfaces, scored higher the longer it is. The gap: **quote marks are excluded from its candidate set** (`""`, `''` never enter stats), so literal doubled quotes are unjudged — that stays parked with the quote-balance work (ADR 0039). Rare-but-valid *cross*-mark pairings in quote-heavy corpora (`"!`) are a non-issue for the same reason: quotes never enter, so they can't be flagged. |
| Punctuation after quote mark | **PARTIAL** (by design, updated 2026-07-30) | The signature rewrite (ADR 0054) now observes both sides of every mark, but a quote as *context* reads generic `punct` and quote-adjacent sites abstain on the quote side — so `."` vs `".` ordering is still unjudged as a specific quote question. Quote-specific attachment stays parked with the quote work (ADR 0039). The census `punct.mark-spacing` lane shows quote-adjacent combos with counts. |
| Text begins with phrase-ending punctuation | **DONE** (via ADR 0054) | A verse-leading `?And` is now an ordinary spacing opportunity: the seam reads as whitespace (no `edge` class — verses are addressing), so the mark signs `space\|letter`, and if that signature is rare for `?` in this corpus it surfaces. `lex.punct-only-token` still catches the free-standing form (`. And`). |
| Word-medial punctuation | **DONE** (via ADR 0054 — was the gap this triage found) | `word,word` signs `letter\|letter`, judged like any other signature against the mark's own corpus conventions. Genuinely medial `-`/`'` conventions are learned as majority signatures and stay silent, as intended. |
| Divergent verse lengths (Owl) | **DONE** | = `prop.length-ratio`. |
| Verse fragment / verse may be untranslated (length reading) | **DONE**-ish | Short-vs-source is `prop.length-ratio`. The copy-from-source reading is the untranslated-words candidate below. |

## SOUS — candidates (status refreshed 2026-07-30: 3 shipped, 2 dissolved into #1, 1 doubtful, 1 open)

All seven were built or dispositioned via the
[rare-glyph/signatures/mixed-case plan](../plans/completed/2026-07-10-rare-glyph-signatures-mixedcase-plan.md)
and the backlog reorganization:

1. **Rare-glyph / rare-letter rule** — **SHIPPED** as `uni.rare-glyph`
   (ADR 0053; reduce page table ADR 0056). The Hawaiian case (Latn
   keyboard, 13-letter alphabet, a stray `q`): corpus-learned letter
   frequency, established-inventory × minority-recurrence two-factor
   shape, no hardcoded bad-character list. Shares its walk with the
   census `letters.glyphs` lane as planned.
2. **Mark attachment signatures** — **SHIPPED** as the rewrite of
   `punct.spacing-anomaly` (ADR 0054), superseding the before-only ADR
   0050 model with no compat shim. Joint (before, after) signatures over
   {letter, digit, space, punct}; **no `edge` class** — the verse/book
   seam reads as whitespace (verses are addressing, per CLAUDE.md), so a
   verse-leading `.word` is ordinary `space|letter` coverage. Quotes stay
   out of the candidate mark set and read generic `punct` as context;
   quote-specific attachment remains parked (ADR 0039). This closed the
   `word,word` and `?And` gaps this triage found.
3. **Untranslated words / source-copy** (Owl) — **STILL OPEN**, the one
   surviving build candidate:
   [`2026-07-30-untranslated-word-calibration.md`](../calibration/2026-07-30-untranslated-word-calibration.md).
   Membership test against the source verse's tokens, run-length bonus,
   recurrence knee for loan words. Blocked on source loading — the
   source-paired tier, alongside `prop.length-ratio` calibration
   ([`2026-07-30-length-ratio-paired-survey.md`](../calibration/2026-07-30-length-ratio-paired-survey.md)).
   (Greek Room's spelling report stays out of scope: it's alignment-gated
   machinery.)
4. **Mixed-case word** (`wOrd`) — **SHIPPED** as `case.mixed-case-word`
   (ADR 0055). Letter-run token unit (so `Hyphenated-Name` is two
   ordinary tokens); recurrence knee excuses `LORD` / `McX`-class
   conventions; census `words.case-shapes` lane shows all counts.
5. **Quotation-mark anomalies / straight-vs-curly** — **RESOLVED by #1
   shipping**, as predicted: quote-type mixing (straight `"` 2× in a
   corpus of 4,500 curly) is the rare-glyph rule doing its job; no
   separate rule. Quote *balance* still parked (ADR 0039); the census
   glyph lane shows the mix unconditionally.
6. **Punctuation missing at end of chapter** — **DOUBTFUL** (moved
   2026-07-29 to
   [`ideas/doubtful/2026-07-29-doubtful-rules.md`](../ideas/doubtful/2026-07-29-doubtful-rules.md)):
   doubtful for need, not cost — no user or corpus has asked for it; a
   rule in search of a need. The Wilson self-gating story remains sound
   if a need ever appears.
7. **Superscript digits / odd numerals** — **RESOLVED by #1 shipping**,
   as written here: the rare-glyph rule flags the first `¹` in a corpus
   that never uses them at full score, with no false universal assertion
   against the corpus that legitimately writes `½`. Never built
   separately; the census `numbers.token-shapes` lane shows them.

## SOUS — census-only (absolute mode dissolves the house-style fight)

> **SHIPPED** (ADR 0058, plan completed:
> [absolute-mode census plan](../plans/completed/2026-07-10-absolute-mode-census-plan.md)):
> `census(map) → Inventory`, knob-free, eight lanes over the rules' own
> walks/extractors. Everything in the table below landed in the
> `numbers.token-shapes` lane (number shapes), `letters.glyphs`
> (wildebeest letter counts, quote counts), and `punct.*` lanes.
> Open census follow-ons live in
> [`ideas/discussing/2026-07-29-census-workstream.md`](../ideas/discussing/2026-07-29-census-workstream.md)
> and [`ideas/committed/2026-07-14-census-both-forms-mark-examples.md`](../ideas/committed/2026-07-14-census-both-forms-mark-examples.md).

These are the naive-Latin-convention items. A hot-loop rule would need a
house-style config war; a census row just shows what's there, ranked by
corpus-relative rarity, and a human with knowledge the engine lacks
decides. All land in the **number-token census** (every digit-bearing
token grouped by shape) or the glyph/punct-sequence tables:

| Item | Census home |
| --- | --- |
| Possible fraction in text | number-token shape `\d/\d` |
| Invalid leading zero | shape `0\d+` |
| Invalid number prefix or suffix (`1st`, `2nd`) | shape `\d+letter` / `letter\d+` |
| Space between digits | shape `\d \d` |
| Unsegmented number (long digit runs, no separators — best guess) | shape row by run length |
| Embedded number in word | shape `letter\d letter` (dup of prefix/suffix) |
| Wildebeest letter counts | glyph census (letters section, ascending count) |
| Wildebeest punctuation-combination counts | punct-sequence census |
| Left/right quotation mark counts (Owl) | glyph census + ADR 0039 census data |

## Adjudicated elsewhere (pointers refreshed 2026-07-30 — the shortlist these referenced was dissolved into the ideas lifecycle)

- **Spelling variants as site findings** — dead as a blanket rule;
  strictly scoped variants are a candidate:
  [`ideas/candidates/2026-07-29-edit-distance-typo-scoped.md`](../ideas/candidates/2026-07-29-edit-distance-typo-scoped.md).
  Greek Room escapes it only via alignment, which is out of scope.
- **Sentence-end/-start positional site rules** — end-side dead as site
  rule (2026-07-09 ruling); start-side doubtful pending base-rate
  scrutiny. Both recorded in
  [`ideas/doubtful/2026-07-29-doubtful-rules.md`](../ideas/doubtful/2026-07-29-doubtful-rules.md).
- **Verse-boundary anything** — verses are reference plumbing; no rule
  may treat verse-initial as sentence-initial (CLAUDE.md, methods §0.1).

## Cross-cutting conclusions

1. **Absolute mode = same walks, second accumulator.** The census
   entrypoint (`census(map) → Inventory`, cold path, ADR 0052 candidate)
   must reuse the exact walkers the rules use so report and squiggles
   never disagree about tokenization/terminals. Rows: category, type key,
   exact count, learned-rarity sort key, capped example sites as packed
   `(u8,u8,u8)` SIDs. Rows are never filtered; only example lists cap.
2. **Two genuinely new hot-loop items fell out of the whole PO list**,
   and both shipped: the rare-glyph rule (#1 → ADR 0053, doubling as the
   glyph-census accumulator) and the attachment-signature rewrite (#2 →
   ADR 0054). With the census (ADR 0058) also landed, the PO list's only
   remaining engine work is the source-paired tier (#3, untranslated
   words + length-ratio calibration) — everything else is editor, onion,
   census follow-ons, or adjudicated dead/doubtful.
3. **The census is not regex-matching.** The "number shapes" and glyph
   tallies are charclass lanes emitted during the same grapheme walk the
   rules use — classification during the walk, never pattern-matching
   over raw text. And because the census is pure counting (no scoring,
   no cross-token state beyond small windows, cold path), it is the
   friendliest first customer for the previously-deferred single-pass
   streaming/SIMD char automaton: count everything + where it occurred
   is exactly the shape that architecture wants.
4. **Greek Room's presentation lesson**: grouping by type + SID list is
   right (their duplicate-check report); static HTML with no text
   click-through is wrong. Absolute mode renders in the findings UI
   shell so site navigation and ignore-plumbing come free; PO reports
   are an export view of that page.

---

## Still owed to the PO (carried from the dissolved shortlist, 2026-07-29)

From the triage's **ASK PO** rows: "Extra text / unmarked text" and
"Optional text or untagged footnote" — meaning unclear; probably
text-outside-any-marker → onion territory, but confirm with the PO before
routing anywhere.



space around punct - spacing-anomaly
space aroud punct
repeated words
unpaired delimiters - learn what actually gets paired first and look for pairing. What's the "pairing" threshold is the question I suppose?
unmatched angle bracket - above same. Does it pair, if not, other rule
free-floating-mark - combining-mark-without base etc; Though I did type a ´ in my editor and got no results
orphaned punct - punc only token (i.e. should this one actually roll into our new rule as well), i.e. likely returned as the product of "$glyph" never appears detached on both sides?
Universally wrong - hygiene
Repeated/doubled -> Learned via proposed rule? 
Punctuation after quote mark
phrase-ending (product of clinging side + capitalizaiton?)
word-medial - 



Reading:Rare-glyph / rare-letter rule — SHIPPED as uni.rare-glyph (ADR 0053; reduce page table ADR 0056). The Hawaiian case (Latn keyboard, 13-letter alphabet, a stray q): corpus-learned letter frequency, established-inventory × minority-recurrence two-factor shape, no hardcoded bad-character list. Shares its walk with the census letters.glyphs lane as planned.
