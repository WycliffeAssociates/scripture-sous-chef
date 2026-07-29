# Idea — PO proofreading checklist triage (Larry's scripts + Greek Room)

Date: 2026-07-10. Input: the product owner's proofreading list (Larry's
scripts per everyone, Greek Room per Aaron B), triaged item-by-item into
owning subsystem, with status against the engine as it exists today
(21 live rule IDs) and remarks on how sous-chef implements the idea
differently — convention-learned and score-surfaced in the hot loop,
and/or exhaustively counted in **absolute mode** (the census report,
ADR 0052 candidate). Report sections, in user-facing terms: **Letters**
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
| Space around punctuation | **DONE** | `punct.spacing-anomaly`, ADR 0050. Convention-learned: French `« … »` spacing is the corpus's own majority, never flagged. Also a census table. |
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
| Punctuation after quote mark | **PARTIAL** (mostly unjudged, by design) | Both punct rules deliberately skip quote-adjacent sites: `spacing-anomaly` drops a mark whose left neighbour is a quote (`word" ,` is not an opportunity), and `adjacency-anomaly` keeps quotes out of its run class. So today nothing judges `."` vs `".` ordering. The planned fix is boundary-class refinement (shortlist item 7: learn which boundary contexts the corpus itself is consistent about). Until then the census punct-sequence table shows every quote-adjacent combo with counts. |
| Text begins with phrase-ending punctuation | **PARTIAL** | `lex.punct-only-token` would catch it only when the mark stands alone as its own token (`. And` — a dot surrounded by spaces). A verse-initial mark glued to a word (`?And`) is not caught: verse-leading marks are excluded from spacing opportunities, and the casing walk treats a verse-initial terminal as belonging to the *previous* verse's flow (verses are addressing, not discourse). Census punct-sequence table shows verse-initial-mark contexts with counts. |
| Word-medial punctuation | **NOT COVERED — see candidate #2** | Previously misstated as covered. `spacing-anomaly` only observes the space *before* a mark; in `word,word` the comma is attached-on-the-left, which is the majority form (silent), and the missing space *after* is never looked at. Genuinely medial `-`/`'` conventions stay census either way. |
| Divergent verse lengths (Owl) | **DONE** | = `prop.length-ratio`. |
| Verse fragment / verse may be untranslated (length reading) | **DONE**-ish | Short-vs-source is `prop.length-ratio`. The copy-from-source reading is the untranslated-words candidate below. |

## SOUS — candidates (build or expand)

Ordered by conviction:

1. **Rare-glyph / rare-letter rule** (Greek Room wildebeest's real value;
   the Hawaiian case: Latn keyboard, 13-letter alphabet, a stray `q`).
   **CANDIDATE — top of list.** `uni.mixed-script-in-token` only catches
   *cross-script* intruders; a same-script letter the corpus never uses is
   invisible today. Corpus-learned letter/glyph frequency, two-factor shape
   (established inventory × minority recurrence — a glyph seen 1–2× in 300k
   is the hapax knee again). No hardcoded "bad character" list ever — the
   corpus votes. Absolute-mode glyph census is the same tally unfiltered,
   so the census accumulator and this rule share a walk.
2. **Mark attachment signatures** (generalizes the after-side gap).
   **CANDIDATE — found by this triage.** `punct.spacing-anomaly` observes
   only the mark's *left* side, so `word,word` (comma attached-left =
   majority, missing space *after* never looked at) is invisible today.
   Rather than bolt on one mirror channel, learn each mark's **joint
   left/right context signature**: (before, after) ∈ {letter, space,
   punct, edge}. `?` in English signs letter|space; Spanish `¿` signs
   space|letter — so a swapped `¿`/`?`, a `word,word`, an `away!Why?`,
   and a wrong-order quote+mark all surface as the same thing: a mark
   occurring in a signature that is rare *for that mark in this corpus*.
   Scoring is categorical, not binary majority/minority — a `.`
   legitimately holds several frequent signatures (letter|space,
   letter|verse-edge), so this is descriptive-share territory (ADR 0048):
   rare-signature share × minority recurrence. Supersedes the current
   before-only stats (pre-alpha, redesign cleanly, no compat shim).
3. **Untranslated words / source-copy** (Owl). **CANDIDATE.** The tier
   above proportionality: anything with a reference text. Walk target
   tokens, membership test against the source verse's tokens, run-length
   bonus (consecutive shared words look like paste). Recurrence knee
   handles loan words: a source-identical word recurring corpus-wide is a
   convention, not a miss. Needs source loading — joins the source-paired
   work (shortlist item 1), not absolute-mode v1. (Greek Room's spelling
   report is different machinery: uroman romanization + weighted edit
   distance, gated on shared *alignment* to a reference translation —
   alignment is declared out of scope; revisit spelling only if/when
   alignment research happens.)
4. **Mixed-case word** (`wOrd`). **CANDIDATE — rides ADR 0051.** Not
   checked today. Tokenization already helps: the letter-run token unit
   splits at hyphens, so `Hyphenated-Name` is two ordinarily-cased tokens,
   not a mixed-case one. Recurring legitimate shapes — `LORD` (the
   all-caps YHWH convention, hundreds of times per corpus), `McX`-style
   names — are exactly what the recurrence knee excuses: a case shape
   that recurs is a convention, a hapax `wOrd` is a slip. Hot loop flags
   the anomaly; census word-shape table shows all counts.
5. **Quotation-mark anomalies / straight-vs-curly.** **CANDIDATE (parked
   with data).** Quote *balance* is parked (ADR 0039). Quote-type mixing
   (straight `"` 2× in a corpus of 4,500 curly) is really the rare-glyph
   rule (#1) doing its job — no separate rule needed. Census glyph table
   makes the mix visible unconditionally.
6. **Punctuation missing at end of chapter.** **CANDIDATE — low.**
   Chapter is addressing, like verses — but "what fraction of this
   corpus's chapters end with a terminal mark" is an honest learned habit
   (really "paragraph-final punctuation" observed at a convenient
   boundary). The it's-not-always-true worry is handled by the machinery
   itself: Wilson dominance means the rule only speaks when the corpus is
   near-categorical about it — a corpus that's 80/20 never establishes
   the habit and the rule stays silent everywhere. Cheap once census
   walks exist.
7. **Superscript digits / odd numerals.** **CANDIDATE — small.**
   Superscript digits ARE codepoints (U+00B9, U+2070–2079, category No) —
   not *invalid*, so not `hyg.invalid-codepoint`. Tempting to call them
   always-wrong in scripture bodies, but the hygiene bar is "universally
   illegal," and No-class glyphs are occasionally legitimate (a
   modern-language translation writing measures with `½`). The rare-glyph
   rule (#1) delivers the always-wrong behaviour in practice — a corpus
   that never uses them flags the first one at full score — without a
   universal assertion that's false for the corpus that does. Prefer #1;
   don't build separately.

## SOUS — census-only (absolute mode dissolves the house-style fight)

> **Promoted 2026-07-10** to a committed plan:
> [absolute-mode census plan](../plans/2026-07-10-absolute-mode-census-plan.md)
> (queued after the rare-glyph plan, before the preset-table freeze).

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

## Adjudicated elsewhere (unchanged by this list)

- **Spelling variants as site findings** — REJECTED/POSTPONED (shortlist
  demotion: edit-distance typo pairs; the/then/thin problem). Greek Room
  escapes it only via alignment, which is out of scope.
- **Sentence-end/-start positional site rules** — end-side dead as site
  rule (2026-07-09 ruling); start-side pending base-rate scrutiny.
- **Verse-boundary anything** — verses are reference plumbing; no rule
  may treat verse-initial as sentence-initial (CLAUDE.md, methods §0.1).

## Cross-cutting conclusions

1. **Absolute mode = same walks, second accumulator.** The census
   entrypoint (`census(map) → Inventory`, cold path, ADR 0052 candidate)
   must reuse the exact walkers the rules use so report and squiggles
   never disagree about tokenization/terminals. Rows: category, type key,
   exact count, learned-rarity sort key, capped example sites as packed
   `(u8,u8,u8)` SIDs. Rows are never filtered; only example lists cap.
2. **Two genuinely new hot-loop items fall out of the whole PO list**:
   the rare-glyph rule (#1, which doubles as the glyph-census
   accumulator) and the after-side spacing channel (#2, a second tally
   on the existing spacing walk). Everything else is either shipped,
   census-only, editor, onion, or already queued (source-paired tier,
   ADR 0051 word table).
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
