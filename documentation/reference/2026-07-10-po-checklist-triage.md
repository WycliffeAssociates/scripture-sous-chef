# Idea — PO proofreading checklist triage (Larry's scripts + Greek Room)

Date: 2026-07-10. **Refreshed 2026-07-30**, and again **2026-08-04** for
ADR 0071. Against the current engine (24 live rule IDs): the triage's build
candidates #1/#2/#4 shipped as ADRs 0053/0054/0055; #5/#7 were routed to #1 and
have since been **re-routed** (see below — they are the new non-letter rule's,
not rare-glyph's); #6 moved to doubtful; the census shipped (ADR 0058); and on
2026-08-04 three punctuation rules were replaced by one convention-learned rule
over visible non-letters, `uni.nonletter-usage-anomaly` (ADR 0071), which now
owns most of this list's punctuation rows. Statuses below updated in place. Input: the product owner's proofreading list (Larry's
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
| Space around punctuation | **DONE** | `uni.nonletter-usage-anomaly`'s **placement** channel (ADR 0071, absorbing `punct.spacing-anomaly` and its ADR 0050 knee + ADR 0054 class conditioning). Both sides of every visible non-letter are observed as logical start/end marginals plus a four-state topology. Convention-learned: French `« … »` spacing is the corpus's own majority, never flagged. Census lane: `punct.mark-spacing` (unchanged — the extractor survives the rule). |
| Space before phrase-ending mark | **DONE** | Same rule, same channel — it's one placement context. |
| Repeated words | **DONE** | `lex.duplicate-word` (toggle; auto-recommendation folded into config-recommender idea). |
| Unpaired delimiters (paren-like) | **DONE**, with corrected wording | These are **corpus-relative** bracket findings, not a universally deterministic pairing check: `punct.bracket-balance` scores a delimiter family's *own* learned pairing dominance over the ADR 0049 inventory (CJK corner brackets excluded), so a never-paired glyph self-suppresses. Where pairing **abstains** — no learned convention for that family — `uni.nonletter-usage-anomaly` still provides a generic rarity/placement fallback on the same glyph. Quotes deliberately out of bracket balance — parked with census data, ADR 0039. |
| Unmatched angle bracket | **DONE**, same wording correction | Same two-layer answer: the bracket inventory judges it if the corpus pairs `<` at all; otherwise it is an ordinary visible non-letter and reads through the generic rule. Not "unpaired by count" in the absolute sense. |
| Free-floating mark | **DONE**, split by domain | Two different findings, and the split is by Unicode category, not by appearance: a **combining** mark with no base (U+0301 COMBINING ACUTE) is deterministic **hygiene** — `uni.combining-mark-without-base` — while a **spacing** clone of the same shape (U+00B4 ACUTE ACCENT `´`, category `Sk`) is a visible non-letter candidate and reads through `uni.nonletter-usage-anomaly`. That is why typing `´` in an editor produces no combining-mark finding: it never was one. (+ `uni.redundant-zero-width-space`, `hyg.zero-width-misuse` for the invisible cases.) |
| Orphaned punctuation | **DONE** | `uni.nonletter-usage-anomaly`. A detached mark is a **placement** answer — its outer topology is `Neither`, judged against how this translation otherwise places that glyph — and a barely-used glyph is a **rarity** answer. The retired `lex.punct-only-token` asked a narrower whitespace-chunk question; its domain is a strict subset of the new candidate domain (`lost = 0`). |
| Stranded backslash at end of line | **DONE** | `struct.source-marker-leftover`; marker validity itself is onion. |
| Unresolved translation conflict | **DONE** | `struct.merge-conflict-marker`. |
| Unexpected characters | **DONE**, split three ways | The item has three distinct domains and no single rule owns it: (1) **universally wrong** — `hyg.invalid-codepoint`, `hyg.control-chars`, `hyg.replacement-run`, `hyg.tab-in-body`, plus a combining mark with no base; (2) a **rare Letter** for this translation — `uni.rare-glyph`, the Letter lane only; (3) an **unusual visible non-letter** — `uni.nonletter-usage-anomaly`'s rarity channel, which covers punctuation, quotes, symbols, digits and emoji. A glyph is in exactly one of these domains, so the three never double-report the same span. |
| Repeated / doubled punctuation (Amharic `፡፡`-class) | **DONE**, quote gap closed (ADR 0071) | `uni.nonletter-usage-anomaly`'s **sequence** channel: directed grapheme pairs (`lead → follower`) over the lead's run-leading opportunities, plus a bounded same-glyph continuation histogram for the `::`-vs-`:::` case pairs cannot reach. A pairing the translation writes often (Amharic `፡፡`, Ethiopic `፡ → ፤`) is its convention and goes silent; `,;` `.;;` `,......` surface. **Quotes now participate** — the retired adjacency rule excluded them from its candidate set, this rule does not — so literal `""` is judged as visible usage. What stays parked is quote *balance* and open/close role assignment (ADR 0039), which is a different question. |
| Punctuation after quote mark | **DONE as visible usage** (ADR 0071), still parked as a quote-role question | `."` vs `".` is now judged: both graphemes are candidates, the ordering is a directed pair, and each mark's attachment to the other is placement evidence. What the rule deliberately does **not** do is assign opening/closing **roles**, match, nest or balance quotes — so it answers "does this translation write `."` elsewhere?" and never "is this the right kind of quote here?". Quote balance stays parked (ADR 0039). The census `punct.mark-spacing` lane still shows quote-adjacent combos with counts. |
| Text begins with phrase-ending punctuation | **DONE** as **logical placement** (ADR 0071) | A verse-leading `?And` is judged on its logical **start** side, which reads `Spaced` at a verse seam (a verse boundary is addressing, never a sentence boundary — the domain invariant), and on its four-state topology. If that placement is rare for `?` in this translation it surfaces; the free-standing form (`. And`) is the detached `Neither` topology. Logical start/end, never visual left/right, so the finding does not move with text direction. |
| Word-medial punctuation | **DONE**, and widened (ADR 0071) | Not just punctuation: `wo.rd`, `wo"rd`, `th3e` and a medial symbol are all the same question — a visible non-letter attached to content at both ends. `wo"rd` is the case the four-state topology exists for: it surfaces even when `"word` and `word"` are both ordinary one-sided forms. Genuinely medial conventions (`-`, and the apostrophe that is a **glottal stop letter** in Mayan/Tupí–Guaraní, 57–97% `Both`-dominant) are learned and stay silent, with no allow-list. |
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
   `punct.spacing-anomaly` (ADR 0054), then **absorbed** into
   `uni.nonletter-usage-anomaly` (ADR 0071, 2026-08-04). Two things
   survived the absorption and are now the replacement's placement channel:
   ADR 0050's opportunity-proportional recurrence knee and ADR 0054's
   class-conditioned pooling. **No `edge` class** then or now — the verse/book
   seam reads as whitespace (verses are addressing, per CLAUDE.md). Quotes are
   no longer excluded from candidacy, which is what closed the doubled-quote and
   `."`-ordering gaps this table used to carry as PARTIAL.
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
5. **Quotation-mark anomalies / straight-vs-curly** — **RESOLVED, and
   RE-ROUTED 2026-08-04.** The 2026-07-30 refresh credited this to
   `uni.rare-glyph`, which is wrong: that rule is the **Letter lane only** (ADR
   0053), and a quote mark is not a letter. Straight-vs-curly mixing (a straight
   `"` twice in a corpus of 4,500 curly) is `uni.nonletter-usage-anomaly`'s
   **rarity** channel — same shape of answer, correct owner. Still no separate
   rule. Quote *balance* remains parked (ADR 0039); the census glyph lane shows
   the mix unconditionally.
6. **Punctuation missing at end of chapter** — **DOUBTFUL** (moved
   2026-07-29 to
   [`ideas/doubtful/2026-07-29-doubtful-rules.md`](../ideas/doubtful/2026-07-29-doubtful-rules.md)):
   doubtful for need, not cost — no user or corpus has asked for it; a
   rule in search of a need. The Wilson self-gating story remains sound
   if a need ever appears.
7. **Superscript digits / odd numerals** — **RESOLVED, and RE-ROUTED
   2026-08-04**, for the same reason as #5: a superscript digit is not a letter,
   so this is `uni.nonletter-usage-anomaly`'s rarity channel, not
   `uni.rare-glyph`'s. The first `¹` in a corpus that never uses them scores at
   full strength, with no false universal assertion against a corpus that
   legitimately writes `½` — and deliberately so: **No**/**Nl** numerals keep
   their own identity and are never pooled into the Nd digit class, which is what
   preserves their ability to fire. Never built separately; the census
   `numbers.token-shapes` lane shows them.

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
token grouped by shape) or the glyph/punct-sequence tables.

**The number-shape census stays exhaustive and descriptive**, and that is
unchanged by ADR 0071. `uni.nonletter-usage-anomaly` may surface an
unusual digit *placement* (`th3e`, a digit attached inside a word) as a scored
finding, but the census makes **no semantic validity claim** about any number
shape — it counts `1st`, `0.5`, `3/4` and `1,000` and lets a human judge. The
two surfaces answer different questions over the same walk, and the census
remains knob-free.

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
   ADR 0054, since absorbed into `uni.nonletter-usage-anomaly` by ADR
   0071 — which is where most of this list's punctuation rows now resolve, as one
   rule instead of three). With the census (ADR 0058) also landed, the PO list's
   only remaining engine work is the source-paired tier (#3, untranslated
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

---

## Owner's reading notes on this triage — answered by ADR 0071

Raw jottings made while reading the tables above, preserved with the answer the
non-letter epic produced for each. They are the questions that shaped the epic,
so they are kept rather than deleted.

| Owner's note | Answer as shipped |
| --- | --- |
| *"space around punct — spacing-anomaly"* | Now `uni.nonletter-usage-anomaly`'s placement channel; `punct.spacing-anomaly` is deleted (ADR 0071). |
| *"unpaired delimiters — learn what actually gets paired first and look for pairing. What's the 'pairing' threshold is the question I suppose?"* | Exactly the shipped design: `punct.bracket-balance` learns each family's own pairing dominance, so the "threshold" is corpus-relative rather than absolute, and a never-paired glyph self-suppresses. Where no pairing convention exists the glyph still reads through the generic non-letter rule. |
| *"unmatched angle bracket — above same. Does it pair, if not, other rule"* | Confirmed and now literally true: bracket balance if the corpus pairs it, otherwise the generic rule. |
| *"free-floating-mark — combining-mark-without-base etc; though I did type a `´` in my editor and got no results"* | Correct behavior, and the reason is the category split: `´` is U+00B4 ACUTE ACCENT (`Sk`), a **spacing** character that was never a combining mark, so the hygiene rule cannot see it. It is a visible non-letter candidate and now reads through `uni.nonletter-usage-anomaly`. U+0301 COMBINING ACUTE with no base remains hygiene's. |
| *"orphaned punct — punct-only token (i.e. should this one actually roll into our new rule as well), i.e. likely returned as the product of '$glyph never appears detached on both sides'?"* | Yes to both halves. It rolled in — `lex.punct-only-token` is deleted — and the mechanism is the one guessed here: the four-state outer topology, where a detached mark is `Neither` and is judged against how the translation otherwise places that glyph. |
| *"universally wrong — hygiene"* | Unchanged: hygiene owns the universally-wrong domain, and it wins at an exact overlapping span. |
| *"repeated/doubled → learned via proposed rule?"* | Yes: the sequence channel's directed pairs plus the bounded same-glyph continuation histogram. |
| *"punctuation after quote mark"* | Judged as visible usage now (quotes are candidates), but never as a quote *role* — no matching, nesting or balance. Balance stays parked (ADR 0039). |
| *"phrase-ending (product of clinging side + capitalization?)"* | The clinging side, yes — the logical start/end marginals. **Not** capitalization: a verse seam is not a sentence boundary, so nothing here reads casing, and the casing rules stay independent. |
| *"word-medial"* | Shipped and widened past punctuation: `wo.rd`, `wo"rd`, `th3e`, medial symbols. `wo"rd` is why the four-state topology exists. |
