The full taxonomy of errors a New Testament draft could contain, with honest signal/noise assessments for each.

Organized by error class rather than by detection method, because the same kind of error can be caught by different signals depending on context. Each rule is numbered sequentially across the whole document. Tractability rating:

- 🟢 Deterministic — boolean rule, near-zero false positives
- 🟡 High-signal probabilistic — corpus statistics give clear answer
- 🟠 Recoverable with auxiliary data — needs word list, alignment, morphology, or similar
- 🔴 Likely buried in noise — too easy to confuse with legitimate variation

The NT-as-corpus advantage threads through this: ~7,700 verses, ~150k tokens, repetitive content (genealogies, formulaic phrases, parallel synoptic passages), and a stable canonical structure (every verse has a known reference). Enough text to learn conventions but small enough that hapax legomena are common, which limits some statistical approaches.

For each rule below: a worked example, the signal we'd use, and the noise/failure modes.

---

# I. Character-Level Errors

## A. Invisible / Encoding Errors

### 1. Tab characters in verse body 🟢
- **Example:** `"Jesus\twept."` — a TAB inside the verse string.
- **Signal:** ASCII 0x09 in a verse body is never intentional in NT text.
- **Noise:** None.
- **Verdict:** Yes + always + test.

### 2. C0/C1 control characters (excluding TAB/LF/CR) 🟢
- **Example:** `"Jesus\x0Bwept"` (vertical tab), `\x1F` (unit separator) — stray from binary file mis-extraction.
- **Signal:** Any U+0000–U+001F or U+007F–U+009F outside the whitelist is an artifact.
- **Noise:** None.
- **Verdict:** Yes + always + test.

### 3. Stray BOM (U+FEFF) mid-text 🟢
- **Example:** `"In the﻿beginning"` — file concatenation artifact.
- **Signal:** U+FEFF anywhere except the very first byte of the file.
- **Noise:** None.
- **Verdict:** Yes + always + test.

### 4. Zero-width space (U+200B) misuse 🟢
- **Example:** `"Christ​wept"` — invisible split inserted by a paste from a web source.
- **Signal:** U+200B inside a Latin/Greek/etc. word boundary is an artifact in scripts that don't use it for line-break hinting.
- **Noise:** Thai/Khmer/etc. legitimately use ZWSP for word-segmentation hints, so the rule must be script-gated.
- **Verdict:** Yes + always + test (with script gate).

### 5. ZWJ/ZWNJ misuse in non-joining scripts 🟢
- **Example:** `"hello‍world"` in English.
- **Signal:** U+200C/U+200D in Latin/Cyrillic/Greek/etc. blocks. Indic/Arabic-family scripts use these legitimately.
- **Noise:** None when script-gated correctly.
- **Verdict:** Yes + always + test.

### 6. Soft hyphen (U+00AD) in verse body 🟢
- **Example:** `"resur­rection"` — leftover from PDF/Word soft-wrap.
- **Signal:** U+00AD is a layout hint, never part of source text.
- **Noise:** None.
- **Verdict:** Yes + always + test.

### 7. Non-breaking space (U+00A0) where regular space expected 🟡
- **Example:** `"the LORD"` vs `"5 kg"`.
- **Signal:** Frequency in the corpus. If U+00A0 is rare globally but clusters in a handful of verses, those are paste artifacts.
- **Noise:** Some translation styles use NBSP between number and unit, between Lord's-name particles, or in French before `?` and `:`. Need to learn the corpus convention.
- **Verdict:** Flag minority usage per corpus.

### 8. Variation selectors after non-CJK characters 🟡
- **Example:** `"a️"` after a Latin letter.
- **Signal:** U+FE00–FE0F should follow only CJK, math, or emoji bases.
- **Noise:** Emoji presentation selectors in modern translations targeting digital media (rare in NT but exists).
- **Verdict:** Flag with low false-positive rate; review.

### 9. Wrong-direction overrides (U+202A–202E, U+2066–2069) 🟢
- **Example:** Verse contains RLO/LRE/PDF marks in a left-to-right corpus.
- **Signal:** Body text never legitimately needs explicit bidi overrides; logical order suffices.
- **Noise:** Some Hebrew/Arabic editorial conventions, but unusual.
- **Verdict:** Flag always; allow per-corpus opt-out.

### 10. Stacked combining marks (same mark twice) 🟢
- **Example:** `"é́"` — base + acute + another acute.
- **Signal:** Identical combining codepoints adjacent on one base. Never intentional in NT-target languages I can think of.
- **Noise:** Vietnamese stacks distinct tone marks but not duplicates.
- **Verdict:** Yes; cheap to detect via grapheme iteration.

### 11. Orphan combining mark (no base) 🟢
- **Example:** Verse starts with `́`.
- **Signal:** A combining codepoint with nothing to combine with (start of token, or follows whitespace).
- **Noise:** None.
- **Verdict:** Yes + always.

### 12. Non-canonical Unicode ligatures (ﬁ ﬂ ﬀ) 🟢
- **Example:** `"ofﬁce"` for "office" — PDF copy-paste artifact.
- **Signal:** Codepoints in U+FB00–FB06 (Latin presentation forms) inside flowing text. These should be normalized to their decomposed letter sequence at ingest.
- **Noise:** Hebrew presentation forms (U+FB1D+) are legitimate; restrict to Latin block.
- **Verdict:** Yes.

---

## B. Look-Alike Substitution (Homoglyphs)

### 13. Cyrillic 'а' for Latin 'a' (and similar) 🟢
- **Example:** `"Mаrk"` where the `а` is U+0430 (Cyrillic).
- **Signal:** Mixed-script analysis within one token. `script_of('а')` ≠ `script_of('a')`. If the token's majority script is Latin and a single character is Cyrillic, it's a copy-paste artifact.
- **Noise:** None in NT contexts where the target language uses one script. Multi-script corpora need per-token script-set vetting.
- **Verdict:** Yes; near-zero FP.

### 14. Greek lookalikes (ο, ν, ρ, υ, ι) in Latin text 🟢
- Same approach as #13. A Latin-majority token with one Greek codepoint is almost certainly an error.
- **Caveat:** In Greek NT or transliterations, this rule is inverted (flag stray Latin in Greek). Run script-majority detection per token, flag minority codepoints.

### 15. Mathematical alphanumerics in flowing text 🟢
- **Example:** `"𝐌ark"` (Mathematical Bold M, U+1D400).
- **Signal:** Codepoints in U+1D400–U+1D7FF inside verse body.
- **Noise:** None — never legitimate in NT.
- **Verdict:** Yes.

### 16. Fullwidth Latin letters in Latin-majority script 🟢
- **Example:** `"Ｍark"` (U+FF2D).
- **Signal:** U+FF21–FF5A in non-CJK-majority text. CJK IME artifact.
- **Verdict:** Yes.

### 17. Zero (0) / capital O confusion 🟠
- **Example:** `"L0rd"` for "Lord".
- **Signal:** Digit inside a Latin-letter token where no token elsewhere in the corpus contains digits adjacent to letters. Very rare in NT; product codes don't appear.
- **Noise:** Some translations include verse markers like "1st" that mix; rare. Could also flag `"1"` for `"l"` and `"I"`.
- **Verdict:** Yes; cheap and high signal in NT specifically.

### 18. Lowercase l / capital I / digit 1 confusion 🟠
- **Example:** `"Iesus"` vs `"lesus"` vs `"1esus"`.
- **Signal:** Words that case-fold to the same string but differ on the `l`/`I` axis, when one variant is dominant in the corpus.
- **Noise:** In some translation traditions `Iesus` is intentional (older spellings). Need corpus-learned convention.

### 19. Dotless ı vs dotted i confusion 🟠 → Skip
Turkish-relevant. Without language knowledge, can't distinguish error from valid Turkish orthography.
**User: Skip for now.**

### 20. Smart quotes vs straight quotes mixed inconsistently 🟡
- **Example:** Most verses use `"…"` (U+201C/U+201D); one uses ASCII `"…"`.
- **Signal:** Per-corpus convention. If 95%+ of openers are curly, ASCII `"` is the error.
- **Noise:** Could legitimately appear if the translation predates Unicode awareness, but then it'd be consistent.
- **Verdict:** Yes; hard rule on curly-vs-straight after learning the dominant convention.
**User: Just hard rule this in terms of straight vs curly quotes. Do implement.**

### 21. Apostrophe variants (' ' ʼ ʻ ` ´) 🟡
- **Example:** `"Jesus’ disciples"` vs `"Jesusʼ disciples"` vs `"Jesus' disciples"`.
- **Signal:** Same convention-learning as #20. Corpus picks one and we flag minorities.
- **Noise:** ʻokina (U+02BB) in Hawaiian-orthography corpora is intentional and primary. Saamia/glottal-stop languages similar.
- **Verdict:** Learn-then-enforce.

### 22. Hyphen / en-dash / em-dash / minus confusion 🟡
- **Example:** `"father–in–law"` (en-dash) where the corpus uses hyphen-minus for compounds; or `"5 – 10"` for a numeric range.
- **Signal:** Per-corpus convention by position context (between letters vs between digits vs flanking spaces).
- **Noise:** Style guides differ; need the corpus to vote.

### 23. Triple-dot vs ellipsis (… vs ...) 🟡
- **Example:** Mixed `"..."` and `"…"` in the same translation.
- **Signal:** Pick the majority form and flag minority.
- **Noise:** Mid-word "..." (Bible software) artifacts are different from inter-word ellipsis; consider context.

### 24. CJK punctuation in Latin text (、。「」) 🟢
- **Example:** `"Jesus wept。"` in an English verse.
- **Signal:** Codepoints in U+3000–U+303F or fullwidth punctuation in Latin-majority verses.
- **Verdict:** Yes; very high signal.

---

## C. Diacritic Errors

### 25. Missing diacritics on words that elsewhere have them 🟡
- **Example:** `"cafe"` in one verse vs `"café"` elsewhere; in NT context, more likely `"Jesus"` vs `"Jesús"` in Spanish translations.
- **Signal:** Case-fold + diacritic-fold lookup against the corpus lexicon. Proper-noun consistency machinery extends to this.
- **Noise:** Real morphological distinctions: Spanish `hablo` (1sg present) vs `habló` (3sg past) are different words. Sparse corpora may miss the legitimate other form and incorrectly cluster them.
- **Mitigation:** Only flag when the diacritic-folded cluster has one dominant form (>~90%) AND the minority form's other inflectional neighbors don't exist in the corpus.
- **Verdict:** Implement, but conservatively, and prefer proper-noun-only scope first.
**User: Little worried this might blow up in some repos, but I get the value. Need to think about how would do it though. I'd fear on sparse data that catch some real false positive of words that do and don't take diacritics. E.g. even in Spanish, hablo vs habló is a real difference, so sort of doubtful here.**

### 26. Wrong combining order (NFD where NFC expected, or vice versa) 🟢
- **Example:** `"é"` (NFD) vs `"é"` (NFC).
- **Signal:** Detect by comparing pre-normalization to post-normalization at ingest.
- **Noise:** Some corpora legitimately want NFD (linguistic analysis pipelines). The question is whether to normalize at all.
- **Verdict:** Detect mixed-form usage within a single file even if we don't choose a global normalization.
**User: Eh, I think we only did NFC not because repo claimed it. Maybe we shouldn't normalize all actually?**

### 27. Combining mark on wrong base character 🟠
- **Example:** `"é"` where the acute should be on the next vowel.
- **Signal:** Requires language-specific orthography rules — which graphemes legitimately bear which marks.
- **Verdict:** Skip without a per-language allow-list.
**User: agree, doubtful.**

### 28. Spacing diacritic where combining expected 🟡
- **Example:** `"e´"` (Latin e + standalone acute U+00B4) instead of `"é"` or `"é"`.
- **Signal:** U+00B4 (acute), U+02D8 (breve), U+02D9 (dot above), U+00B8 (cedilla) appearing between letters or adjacent to a letter without intervening whitespace.
- **Noise:** Some IPA/linguistic conventions use them standalone; rare in NT body text.
- **Verdict:** Implementable; investigate corpus first.
**User: Uncertain.**

---

## D. Transposition / Substitution

### 29. Adjacent letter transposition producing nonword 🟠
- **Example:** `"teh"` for `"the"`; `"adn"` for `"and"`.
- **Signal:** Zipfian frequency rank of the *intended* word matters. The top 20–30 words of the corpus (English NT: the, and, of, he, to, in, that, was…) are so dominant that a transposition producing a nonword is detectable by lexicon-absence alone. Char-n-gram surprisal also lights up on `"teh"` because `"eh"` word-internal in a 3-letter word is rare in English.
- **Noise:** Outside the top 20–30, lots of legitimate hapaxes look like transpositions. Restrict the strong rule to high-Zipf positions; let char-n-gram backoff handle the rest at lower confidence.
- **Verdict:** Top-N-Zipf rule, paired with the existing char-trigram surprisal.
**User: Maybe should only check against the top 20/30 of zipfian words? Not sure if compression texture is really gonna be sufficent to catch in a single word transpotion like this.**
**A: The resulting bigram "eh" is not terrible common, BUT likely not enough signal. For "teh", it is towards top of zipf and gonna be likely overwhelmingly the most common word in the corpus, hence the zipf part. This is part of a broader category of typist errors, and one that is maybe worth trying to surface.**

### 30. Adjacent letter transposition producing valid word 🔴
- **Example:** `"Mark went to the tome"` instead of `"tomb"`.
- **Signal:** Both `tomb` and `tome` are valid English words with common bigrams. The only purely-internal signal is whether `"tome"` appears anywhere else in the corpus (probably not in NT) and whether the verse's parallel-passage or near-context fits `"tomb"` (Easter narratives use `tomb` often). Without source alignment, this requires a strong parallel-passage model.
- **Noise:** Tomes and tombs both exist; any single occurrence could be legitimate vocabulary.
- **Verdict:** Source alignment is the only reliable path. Without source, flagging is reckless.
**User: only way we might eve[r catch is via source alignment]**

### 31. Keyboard-adjacent substitution producing nonword 🟠
- **Example:** `"holu"` for `"holy"` (u next to y).
- Same approach as #29; falls out of char-n-gram and lexicon checks.

### 32. Keyboard-adjacent substitution producing valid word 🔴
- **Example:** `"find"` → `"fine"` (d/e adjacent on QWERTY). Both common words.
- **Signal:** Source alignment recovers it. Within target alone, undetectable.

### 33. Repeated letter where single intended 🟠
- **Example:** `"ressurection"`, `"holly"` for `"holy"`.
- **Signal:** Char-trigram surprisal flags `"ssu"` if rare; lexicon-absence helps.
- **Noise:** Genuine geminates: English `"cattle"`, Spanish `"perro"`, Italian `"verità"` paths. The rule needs to compare against the *corpus's* trigram distribution, not a generic one.

### 34. Missing letter (deletion) 🟠
- **Example:** `"thru"` for `"through"`, `"Jeus"` for `"Jesus"`.
- **Signal:** Lexicon-absence + edit-1 cluster. If `"Jeus"` is one edit from `"Jesus"` (which occurs 900+ times) and `"Jeus"` occurs once, the cluster picks it up.
- **Noise:** Genuine abbreviations: `"thru"` in conversational registers. Corpus convention determines tolerance.

### 35. Extra letter (insertion) 🟠
- **Example:** `"Jesuss"`, `"hollly"`.
- Same approach as #34: edit-1 cluster + lexicon dominance.

### 36. Word with letters from 3+ scripts 🟢
- **Example:** Cyrillic + Greek + Latin in a single token.
- **Signal:** Almost always copy-paste contamination.
- **Verdict:** Yes; cheap to add.

---

## E. Case Errors

### 37. Sentence-initial lowercase 🟡
- **Example:** `"He went out. and wept."` — `"and"` should start with `A`.
- **Signal:** Already implemented in `sentence_start_case.rs` via corpus-learned triggers. Strong when convention is ~95%+ uppercase after terminal punctuation.
- **Noise:** Verse starts that continue a sentence from the previous verse; the rule needs cross-verse context.

### 38. Proper noun lowercase mid-sentence 🟡
- **Example:** `"david went to jerusalem"`.
- Already implemented in `proper_noun_consistency.rs`. Strong when corpus has many capitalized instances of the same lemma.

### 39. Improper capitalization (lowercased noun becomes Capitalized) 🟡
- **Example:** `"He ran to the House"` — common noun unexpectedly capitalized.
- Inverse of #38. Detectable when a normally-lowercase word appears Title Case unexpectedly.
- **Noise:** Reverential capitalization (`"the Word"`, `"the Father"`, `"the Way"`) is legitimate and corpus-conventional. The rule must learn the corpus's reverence list.

### 40. ALL CAPS word in flowing text 🟡
- **Example:** `"the LORD said"`.
- **Signal:** Outlier from the token's normal casing distribution.
- **Noise:** Divine names are often ALL CAPS by convention (`LORD`, `GOD` for the Tetragrammaton). Corpus convention decides tolerance.

### 41. Mixed case within token (cAmel or vArIaNt) 🟢
- **Example:** `"jeSus"`.
- **Signal:** Internal case transitions in a token whose corpus form is consistent.
- **Noise:** Very rare in scripture text. Almost always an error.

### 42. Lowercase "i" as a pronoun (English) 🟢
- **Example:** `"i am the way"`.
- **Signal:** Standalone `i` between word boundaries in English-language verses.
- **Verdict:** Yes for English; trivial.

### 43. Sentence-ending capitalized non-proper-noun 🟠
- **Example:** `"He went to the House."` — but really this is #39 in another guise.
- Combine with #39 rather than separate.

---

# II. Word-Level Errors

## A. Duplication

### 44. Adjacent duplicate word (case-insensitive) 🟡
- **Example:** `"the the man"`, `"And and Jesus said"`.
- **Signal:** Trivial scan.
- **Noise:** Legitimate doublings: `"Holy, Holy, Holy"` (Rev 4:8), `"verily, verily"` (John), `"Lord, Lord"` (Matt 7:21). Skip-list is corpus-derivable: tokens that appear in legitimate doublings more than N times become exceptions.
- **Verdict:** Yes; ship with corpus-learned skip-list.

### 45. Adjacent duplicate word (case-sensitive distinct) 🟡
- **Example:** `"Holy holy"` where corpus convention is `"Holy, Holy"` or `"holy holy"`.
- **Signal:** Case mismatch within a doubling that should be uniform.
- **Verdict:** Worth implementing.

### 46. Duplicate word separated by one token 🟠
- **Example:** `"the man the"` — fragment from cut-paste; vs `"the man, the woman"` which is legitimate.
- **Signal:** Word-X word-Y word-X where Y is short (function word). Add comma-aware filter.
- **Noise:** Common in lists, parallelisms. Lower precision than adjacent.

### 47. Duplicate phrase (3+ tokens repeated) 🟡
- **Example:** `"Jesus wept Jesus wept"`.
- **Signal:** Repeated n-gram (n≥3) within a single verse, or directly adjacent across two verses.
- **Noise:** Some doxologies/blessings repeat phrases intentionally — corpus-learned exception.
- **Verdict:** Yes; high precision.

### 48. Duplicate phrase across consecutive verses (boundary copy-paste) 🟡
- **Example:** Verse N ends with `"and they were amazed"`; verse N+1 starts with `"and they were amazed and"` — the editor pasted into the wrong cell.
- **Signal:** Trailing 3-gram of verse N matches leading 3-gram of verse N+1.
- **Noise:** Parallelism does happen in poetry, but rarely with exact lexical repetition at the boundary.

---

## B. Omission / Addition

### 49. Missing function word 🔴
- **Example:** `"went tomb"` for `"went to the tomb"`.
- **Signal:** Without source alignment, function words are too common to flag.

### 50. Missing content word 🟠
- **Example:** `"Jesus said disciples"` missing `"to his"`.
- **Signal:** Source alignment makes it tractable; without it, only catchable if the resulting grammar produces something structurally unusual.

### 51. Extra function word 🔴
- **Example:** `"went to to the tomb"` — adjacent duplicates (#44) catch the obvious case; non-adjacent extras are mostly invisible.

### 52. Whole-verse omission 🟡
- **Example:** Verse 12 is empty in target but populated in source.
- Already handled by `empty_verse` and by source proportionality detecting length collapse.

### 53. Whole-verse duplication 🟡
- **Example:** Verse text of Matt 5:3 appears verbatim at Matt 5:4.
- **Signal:** Exact-match scan across the corpus.
- **Noise:** Some verses are short enough (`"Jesus wept."`) to legitimately collide; restrict to verses above a length threshold.

### 54. Verse content displaced (text in wrong verse slot but adjacent) 🟠
- **Example:** Matt 5:3 contains what should be in Matt 5:4 because the editor pasted at the wrong row.
- **Signal:** Source proportionality drift: a verse is double-length while the neighbor is empty (covered partially by #66/67).
- **Noise:** Verse boundary differences across translations are real.

---

## C. Substitution

### 55. Wrong proper noun (Mark → Matthew) 🔴
- **Example:** Genealogy lists `"Matthew the son of Jacob"` where it should be `"Joseph the son of Jacob"`.
- **Signal:** Source alignment catches it. Within target, only detectable when context strongly conflicts (e.g., `"Mark, son of Zebedee"` — but that requires semantic knowledge).
- **Verdict:** Skip without source.

### 56. Wrong number 🔴
- **Example:** `"four thousand"` instead of `"five thousand"` in the feeding.
- Source alignment essential.

### 57. Wrong pronoun (he/she/they) 🔴
- Same.

### 58. Wrong tense / aspect 🔴
- **Example:** `"said"` → `"says"`.

### 59. Wrong word from same semantic field 🔴
- **Example:** `"house"` → `"home"`; `"sin"` → `"trespass"`.

---

## D. Spelling Variants

### 60. Two spellings of the same proper noun (Jerusalem / Yerushalayim) 🟡
- **Example:** `"Peter"` and `"Petros"` both used in the same translation inconsistently.
- **Signal:** Edit-distance clustering or phonetic encoding (Metaphone/Soundex variants tuned for the target language). Flag the minority spelling per cluster.
- **Noise:** Old vs New Testament conventions, narrator vs character speech. Corpus may legitimately have two forms.

### 61. Transliteration variants (Christ / Cristo / Mesias) 🟠
- **Signal:** Cross-lingual variant detection requires phonetic encoding or pre-known equivalence classes. BK-tree on raw forms misses phonological structure.
- **Verdict:** Needs phonetic step.

### 62. Diacritic-presence variant of same word 🟡
- **Example:** `"angel"` vs `"ángel"`.
- Corpus convention flags the minority spelling. Same caveat as #25 about real morphological pairs.

### 63. Abbreviation inconsistency (St. / Saint, Jr. / Junior) 🟡
- **Example:** Most verses use `"Saint Paul"`; one uses `"St. Paul"`.
- **Signal:** Pattern detection + abbreviation expansion table.
- **Noise:** Some translations deliberately mix for headings vs body; rare in NT body.

### 64. Number format inconsistency (digit vs spelled-out) 🟡
- **Example:** `"5 thousand"` vs `"five thousand"` in parallel feeding accounts.
- **Signal:** If the corpus dominantly spells numbers out (most NT translations do), flag digit usage and vice versa.
- **Noise:** Verse numbers vs body numbers.

### 65. Number format separator inconsistency (1,000 / 1.000 / 1 000) 🟡
- Locale-specific. Pick corpus convention.

---

# III. Punctuation / Spacing Errors

## A. Spacing

### 66. Double space between words 🟢
- **Example:** `"Jesus  wept."`.
- Boolean.

### 67. Multiple consecutive spaces (3+) 🟢
- Stronger version of #66. Almost certainly an artifact.

### 68. Missing space after sentence boundary 🟢
- **Example:** `"ended.Then"`.
- Letter directly after terminal punctuation with no space.
- **Noise:** Abbreviations (`"St.Paul"`) — rare in NT body.

### 69. Space before punctuation (English/most-Latin convention) 🟡
- **Example:** `"Jesus wept ."`.
- **Signal:** Per-corpus convention. French legitimately requires space before `?` `!` `:` `;` and inside `« »`.
- **Verdict:** Corpus-conditional.

### 70. Missing space after punctuation 🟡
- **Example:** `",hello"` vs `", hello"`.
- Same convention caveat.

### 71. Space inside paired punctuation 🟢/🟡
- **Example:** `"( hello )"` vs `"(hello)"`.
- Corpus convention. French again differs.

### 72. Trailing whitespace on verse 🟢
- Boolean.

### 73. Leading whitespace on verse 🟢
- Boolean. Verse strings should start with a non-whitespace.

### 74. Whitespace adjacent to combining mark 🟢
- **Example:** `"e ́"` — space-then-combining.
- Almost always a copy-paste artifact.

### 75. Verse contains only whitespace (non-empty but blank) 🟢
- Trim-then-check. Already partially covered by `empty_verse`.

---

## B. Punctuation Conventions

### 76. Intermedial punctuation 🟢
- **Example:** A character the corpus learned as left-clinging (`","`) suddenly appears with whitespace on both sides.
- **Signal:** `ClingingClass` already exists; rule needs writing.
- **Verdict:** Near-100% precision once corpus convention is established.

### 77. Unmatched paired punctuation 🟢
- Already implemented via `discourse.rs` span index.

### 78. Wrong punctuation pair (curly open + straight close) 🟡
- **Example:** `"opened "but closed"`.
- **Signal:** Pair-family consistency: opener and closer must come from the same pair.
- **Noise:** Single-verse spans of dialogue may not show this; cross-verse spans need bookkeeping.

### 79. Repeated punctuation (!!! ??? !?!?) 🟡
- **Signal:** Outlier in the corpus.
- **Noise:** Some translations use `?!` legitimately.

### 80. Punctuation at verse start (orphan comma/period) 🟡
- **Example:** Verse starts with `", and Jesus said"`.
- Often a copy-paste artifact (comma left over from previous verse). Corpus convention determines tolerance — some translations begin verses mid-sentence and leading punctuation is legitimate.

### 81. Missing terminal punctuation 🟡
- **Signal:** If 98% of verses end with `.` `?` `!` and one doesn't, flag.
- **Noise:** Mid-sentence verse boundaries are legitimate; the rule should look at narrative position.

### 82. Wrong terminal punctuation (question becomes period) 🟠
- **Example:** `"What did he say."`.
- **Signal:** Interrogative-word detection in target language can catch a subset. Source alignment is reliable.
- **Noise:** Indirect questions legitimately end in periods.

### 83. Trailing punctuation chain (?!. or .!) 🟡
- **Example:** `"What?!."`.
- **Signal:** Three or more terminal punctuation chars at a verse end is almost always an artifact.

### 84. Bracket type mismatch ([) (]) 🟢
- **Example:** Open `[` matched by close `)`.
- Same machinery as #77 with cross-family detection.

### 85. Editorial brackets vs translation brackets confusion 🟠
- **Example:** Some translations use `[words added]` for translator additions, others use `(...)`. Mixed within one corpus.
- Pattern-based, corpus-conventional.

### 86. Inconsistent ellipsis style (.../…) within corpus 🟡
- Covered by #23 but worth tracking separately at the discourse level.

---

## C. Quotation Marks

### 87. Quote opened in one verse, never closed 🟡
- Already covered by paired-punct rule but quotes legitimately span verses, so the rule needs span-length limits (configurable via `max_span_sids`).

### 88. Nested quotes with wrong levels 🟠
- **Example:** Outer `"…'…'…"` becomes `"…"…"…"`.
- Corpus-learned convention.

### 89. Curly quote facing wrong direction 🟡
- **Example:** Closing `"` where opening expected.
- **Signal:** Position-in-pair vs character.

### 90. Quote close before quote open 🟢
- **Example:** Verse contains `"` (closer) before any `"` (opener) and no prior open hangs from previous verse.
- High-precision via span tracker.

---

# IV. Verse-Level Errors

## A. Structural

### 91. Wrong verse reference (text of Matt 1:1 in slot Matt 1:2) 🔴
- **Signal:** Catchable only if a reference text exists OR numeric content makes it obvious (a genealogy in the wrong slot).
- **Noise:** High; skip without external reference.

### 92. Two verses merged 🟡
- **Example:** Slot 5 has text of length 2x typical; slot 6 empty or trivially short.
- Source proportionality flags this.

### 93. One verse split into two 🟡
- Inverse: two short consecutive verses combining to one source-verse length.

### 94. Verses out of order 🔴
- Within a single chapter, invisible without semantic comparison or source alignment.

### 95. Verse number duplicated in verse text 🟢
- **Example:** Slot Matt 1:3 contains `"3 And Judah begat..."`.
- **Signal:** Leading digit-and-space (or digit-only) at verse start matching the verse number.
- **Verdict:** Yes; common ingest leakage.

### 96. Chapter heading in verse body 🟡
- **Example:** Verse 1 of a chapter starts with `"Chapter 5"` or `"The Beatitudes"`.
- **Signal:** First verse of each chapter has a leading short capitalized phrase that doesn't appear elsewhere.

### 97. Section heading in verse body 🟡
- Similar to #96 mid-chapter.

---

## B. Content Leakage

### 98. English (or source language) text leaked into target 🟡
- **Example:** Target is Spanish; one verse has `"Then Jesus said unto them"`.
- **Signal:** Language identification on token sequences. If 99% of tokens look like target language and a 5-token run looks like source language, flag.

### 99. Untranslated source phrase 🟡
- Same approach; particularly relevant for "translation in progress" markers.

### 100. Translator note / TODO leaked into verse body 🟢
- **Example:** `"[CHECK]"`, `"FIXME"`, `"??"`, `"(translation needed)"`, `"TBD"`.
- Pattern-based. Corpus may need to learn project-specific markers.

### 101. Markup leaked from source format 🟢
- **Example:** `"<<<<<<< HEAD"`, `"\f"`, `"[["`, `"{{"`, `"\\v"`, `"\\p"`, `"\\q1"`.
- Specific to source formats and version control.

### 102. USFM/USX marker leak (\v, \p, \q, \id) 🟢
- **Example:** Verse body contains `"\\v 5 And he said"`.
- Pattern-based; near-zero false positives outside USFM bodies.

### 103. Footnote / cross-reference marker leak 🟡
- **Example:** Verse contains `"†"` or `"[a]"` or `"(1)"` mid-text.
- **Signal:** Bracketed/superscripted tokens that don't fit corpus prose conventions.
- **Noise:** Some translations include in-line cross-references intentionally.

### 104. URL or filename in verse body 🟡
- **Example:** `"http://"`, file extensions like `".docx"`, `".pdf"`.
- Pattern detection.

### 105. Date/time stamp in verse body 🟡
- **Example:** `"2024-03-15"`.
- Pattern detection in unexpected places.

### 106. Numeric anomaly (years far outside biblical range) 🟠
- **Example:** `"in 2024"` in an NT context.
- Range checks help but get noisy with genealogy ages, fish counts, etc.

### 107. Email address in verse body 🟢
- Pattern detection. Never legitimate.

### 108. Currency / modern unit (km, USD, $) 🟢
- **Example:** `"5 USD"` or `"$5"`.
- Specific symbol/abbreviation detection.

### 109. Isolated script run (one Greek word in English verse) 🟡
- **Example:** A single token in a non-corpus script in an otherwise monoscript verse.
- **Signal:** Token-level script detection.
- **Noise:** Transliterations of names sometimes include source-script in parentheses.

---

## C. Proportionality

### 110. Verse far longer than parallel source verse 🟡
- Already implemented in `source_relative.rs`.

### 111. Verse far shorter than parallel source verse 🟡
- Same.

### 112. Verse-internal token-length distribution unusual 🟠
- **Example:** Average token length within verse far outside corpus norm.
- Could indicate keyboard garbage or wrong script.

### 113. Single ultra-long token (40+ chars) 🟢
- **Example:** Concatenation artifact: `"JesussaidIamthewaythetruthandthelife"`.
- **Signal:** Token length above corpus 99.9th percentile.
- **Noise:** German compounds, agglutinative-language tokens — needs corpus-relative threshold.

### 114. Punctuation density spike 🟡
- **Example:** A verse with 12 commas in 20 tokens, when corpus median is 2.
- Outlier-based.

### 115. Hapax density spike 🟡
- **Example:** A verse with 80% hapaxes in a corpus where the median is 15%.
- **Signal:** High concentration of corpus-unique tokens suggests garbled text, untranslated source, or a name list.
- **Noise:** Genealogies legitimately spike this; allow named exceptions.

---

# V. Cross-Verse Consistency

## A. Term Consistency

### 116. Same source term translated multiple ways inconsistently 🟠
- **Example:** Greek `agape` rendered `"love"` in 8 verses and `"charity"` in 2.
- Catchable only with source-target alignment.

### 117. Different source terms collapsed to one target term 🟠
- Inverse problem; same alignment requirement.

### 118. Proper noun spelled differently across the NT 🟡
- **Example:** `"Peter"` / `"Petros"` / `"Petro"`.
- Edit-distance clustering + phonetic encoding.

### 119. Title or epithet used inconsistently 🟠
- **Example:** `"Son of Man"` / `"the Son of Man"` / `"Son of man"`.
- Pattern-based within corpus.

---

## B. Formulaic Phrase Consistency

### 120. Standard phrase varies in places where it shouldn't 🟡
- **Example:** `"Verily I say unto you"` appears 78 times consistently and once as `"Verily I say to you"`.
- **Signal:** Synoptic gospels are highly repetitive — outliers in formulaic phrases stand out via n-gram frequency.

### 121. Doxology / blessing formula varies 🟡
- Same approach; particularly strong in NT epistolary openings/closings.

### 122. Genealogy pattern break 🟡
- **Example:** Matt 1 lists `"A begat B, and B begat C..."` consistently and one verse says `"A was the father of B"`.
- **Signal:** Within a genealogy span, the per-verse template should be nearly identical except for names.

---

## C. Numerical Consistency

### 123. Genealogy numbers inconsistent with cross-references 🔴
- Requires cross-reference database. Outside scope of corpus-internal analysis.

### 124. Quoted OT passage doesn't match an OT translation of the same passage 🔴
- Requires linked OT corpus and citation index.

---

# VI. Discourse Structure

### 125. Speaker attribution inconsistency 🔴
- **Example:** `"Jesus said..."` attributed to wrong speaker.
- Requires discourse parsing.

### 126. Quote marks suggest a speaker change where none occurred 🟠
- **Signal:** Quote-open/close pattern inconsistent with surrounding narration.

### 127. Direct/indirect speech inconsistency 🔴
- Translation-philosophy variation, usually not error.

### 128. Pronoun reference ambiguity 🔴
- Real problem; rarely detectable without semantic parsing.

---

# VII. Morphological / Inflectional (Language-Dependent)

### 129. Number agreement violation 🟠
- **Example:** `"the men was"`.
- Detectable in languages with overt morphological agreement.

### 130. Tense inconsistency within narrative 🟠
- Switching past/present mid-narrative.

### 131. Wrong morphological form (3sg where 1sg needed) 🟠
- Requires morphological analyzer.

### 132. Invalid morpheme combination 🟠
- **Example:** Stem + suffix combinations that don't occur elsewhere in corpus.
- `lemma_cluster.rs` work points this direction.

---

# VIII. Things You Won't Catch (be honest about scope)

### 133. Theological errors 🔴
- Doctrinally wrong translation; invisible to NLP without human-level semantics.

### 134. Nuance shifts 🔴
- `"trespasses"` / `"sins"` / `"debts"` — three valid translations of the same source. Choosing the wrong one for context is judgment.

### 135. Cultural register errors 🔴
- Too-formal / too-casual for target culture.

### 136. Naturalness / readability 🔴
- Stilted but grammatical. Perplexity metrics approximate this; precision low.

### 137. Genre violations 🔴
- Translating poetry as prose, narrative as instruction.

### 138. Source text errors propagated 🔴
- If the source has an error, target inherits it. The tool can't tell you the source was wrong.

---

# Appendix: Cross-Cutting Considerations

- **Corpus-relative everything.** Almost every rule above either uses a corpus-learned threshold or could benefit from one. Generic English/Spanish/etc. frequency tables are too coarse; the NT corpus is small enough that the dominant convention can be measured directly and minorities flagged.
- **Skip-lists from the corpus itself.** Doubling exceptions (#44), reverential capitalization (#39, #40), divine name conventions, formulaic-phrase whitelist — all derivable from the corpus rather than hand-curated.
- **Source alignment unlocks a whole class.** Substitution errors (#55–59) and most omission/addition (#49–51) are essentially undetectable without a source. If source alignment is on the roadmap, ~15 rules above shift from 🔴 to 🟡.
- **Pre-alpha license to be conservative.** Better to under-flag (silently miss errors) than over-flag (cry-wolf on legitimate variation). For each 🟡 rule, prefer a high confidence threshold first and lower it as the corpus convention-learning matures.
- **Per-rule corpus introspection mode.** Before turning on any new 🟡 rule, dump its statistic over the current corpus to confirm the convention exists at the expected strength. Don't ship rules that assume a convention that the current draft hasn't yet established.
