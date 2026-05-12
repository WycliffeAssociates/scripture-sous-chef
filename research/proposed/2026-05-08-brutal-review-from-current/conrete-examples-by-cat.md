The taxonomy of errors a New Testament draft could contain, organized by *which detector answers the question* rather than by error symptom. The previous symptom-list version surfaced ~138 entries; most collapse into ~14 detectors. Symptoms below each detector are kept as sub-bullets — they double as test cases.

Tractability rating:

- 🟢 Deterministic — boolean rule, near-zero false positives
- 🟡 High-signal probabilistic — corpus statistics give clear answer
- 🟠 Recoverable with auxiliary data — needs word list, alignment, morphology, or similar
- 🔴 Likely buried in noise — too easy to confuse with legitimate variation

The NT-as-corpus advantage threads through this: ~7,700 verses, ~150k tokens, repetitive content (genealogies, formulaic phrases, parallel synoptic passages), and a stable canonical structure. Enough text to learn conventions but small enough that hapax legomena are common, which limits some statistical approaches.

---

# The detectors

## 1. Codepoint denylist 🟢

**Question:** Is this codepoint on a per-script forbidden-in-body-text list?

**What it catches:**
- Control characters (TAB, C0/C1 outside whitelist)
- BOM (U+FEFF) mid-text
- Zero-width space / ZWJ / ZWNJ in non-joining scripts
- Soft hyphen (U+00AD)
- Bidi direction overrides (U+202A–202E, U+2066–2069)
- Non-canonical Latin presentation forms (ﬁﬂﬀ, U+FB00–FB06)
- Mathematical alphanumerics (U+1D400+) in body text
- Fullwidth Latin (U+FF21–FF5A) in Latin-majority text
- CJK punctuation (、。「」) in Latin-majority text
- Variation selectors (U+FE00–FE0F) after non-CJK bases

**Data needed:** per-script allow/denylist tables; corpus's dominant script.

**Signal/noise:** near-zero FP when script-gated. This is "data, not rules" — once the table exists, adding a new entry is config not code.

---

## 2. Script consistency per token 🟢

**Question:** For each token, what scripts are present, and is the minority intentional?

**What it catches:**
- Cyrillic 'а' / Greek 'ο' / mathematical italics inside a Latin token
- Isolated single-token script switch in a monoscript verse (one Greek word in English)
- Tokens with letters from 3+ scripts
- Fullwidth + halfwidth mixed in one token

**Data needed:** Unicode script property table; per-corpus and per-verse script majority; allowed script-pair set (some translations legitimately mix scripts for transliterated names).

**Signal/noise:** very high precision once "expected scripts" is set per project.

---

## 3. Codepoint majority-class conformity 🟡

**Question:** For codepoints that come in stylistic variants doing the same job, does this token use the corpus-dominant variant?

**What it catches:**
- Smart vs straight quotes (`"…"` vs `"…"`)
- Apostrophe family (`'` `'` `ʼ` `ʻ` ` ` `'` ``` ` ``` `´`)
- Hyphen / en-dash / em-dash / minus
- Triple-dot vs ellipsis (`...` vs `…`)
- NBSP vs regular space
- NFC vs NFD inconsistency within a file (rather than across)

**Data needed:** per-function variant family table (which codepoints do the same job); per-corpus majority within each family.

**Signal/noise:** high precision when corpus majority is strong (≥95%); noisy when no real convention has emerged yet.

**User decisions captured:**
- Straight/curly quotes: hard rule once dominant convention is learned (was prior rule #20).
- Diacritic-presence variants of words (was #25 / #62): conservative — proper-noun-only scope first. Spanish `hablo`/`habló` and similar are real morphological pairs and sparse corpora will mis-cluster. Push the riskier general-vocabulary version into a later phase.
- NFC/NFD (was #26): may not normalize at all; detect mixed-form within a file regardless of whether we choose a global normalization.

---

## 4. Combining-mark well-formedness 🟢

**Question:** Are combining marks attached to valid bases in valid orders?

**What it catches:**
- Orphan combining mark (no base — start of token or after whitespace)
- Stacked duplicate marks (same combining codepoint twice on one base)
- Spacing-diacritic where combining expected (U+00B4 acute between letters instead of U+0301)

**Data needed:** Unicode combining-class tables; grapheme iteration (per memory: walk graphemes via `unicode-segmentation`, don't hand-roll mark predicates).

**Out:** "combining mark on wrong base" (prior #27) — needs language-specific orthography. User agreed doubtful.

---

## 5. Lexicon membership 🟡

**Question:** Is this token a real word in this language? (yes / no / maybe)

**What it catches:**
- Typo producing nonword: `teh`, `holu`, `Jeus`, `Jesuss`, `ressurection`, `thru`
- Untranslated source-language phrase leak
- Foreign-script content leak
- Editor/translator markers (`[CHECK]`, `FIXME`, `TBD`, `??`) — overlap with #14
- Hapax in genealogy-style context where it shouldn't be

**Tools, in leverage order:**
1. Translator-supplied wordlist (cheapest source of truth, ~10-min elicitation)
2. Bootstrap from corpus — form appearing 20+ times presumed real (free but self-referential)
3. Related-language lexicon transfer (gateway language with a dictionary)
4. Character n-gram (Kneser-Ney) — answers implicitly via "does this look like this language?"

**User decision (was #29):** for transposition-to-nonword, restrict the strong typo rule to the top 20–30 zipfian words where the resulting bigram surprisal is genuinely high signal. Char-trigram backoff catches the rest at lower confidence.

**Signal/noise:** high precision on top-Zipf typos; medium for general typos; bounded by lexicon completeness.

---

## 6. Surface-pattern normality 🟠

**Question:** Does this string's character sequence look like other strings in this language?

**What it catches (word-level):**
- Keyboard garbage (`;lkjasdf`)
- Transposition / deletion / insertion that produces a nonword (overlaps #5 — surface-pattern is the backoff when lexicon is unavailable)
- Repeated-letter typos (`ressurection`)
- Tokens with unusual character entropy

**What it catches (verse-level):**
- Whole-verse compression-texture anomalies
- Verses with garbled spans

**Tools:**
- Char n-gram surprisal (word scale)
- Compression texture (verse scale)

**Already implemented:** char_ngram_backoff (recent tuning in 48ce473, 5a486f7).

**Honest take:** word-level surface-pattern and lexicon membership genuinely compete for the same job. Lexicon-where-available wins. Compression texture should live at verse scale rather than ensembling at the word scale (drops the Noisy-OR correlated-factor problem).

---

## 7. Distributional / formulaic-phrase normality 🟡

**Question:** Is this token / n-gram / position appearing where the corpus says it should?

**What it catches:**
- Formulaic phrase variation: `"Verily I say unto you"` (78×) vs `"Verily I say to you"` (1×)
- Adjacent doubled word (`the the`) — with corpus-derived skip-list for `Holy, Holy, Holy`, `verily, verily`, `Lord, Lord`
- Doubled-with-one-gap (`the man the`)
- Duplicate phrase 3+ tokens
- Cross-verse boundary copy-paste (trailing n-gram of verse N == leading n-gram of verse N+1)
- Genealogy pattern break (within a genealogy span, per-verse template should be near-identical)
- Hapax density spike in one verse
- Punctuation density spike

**Tools:**
- Raw n-gram frequency
- Frequency conditioned on book/genre (Hebrews ≠ 1 John in vocabulary)
- Burstiness (variance of inter-occurrence distance) — function words should be uniform; topical words naturally cluster

**Signal/noise:** NT is highly formulaic — strong signal for synoptic parallels and epistolary openings. Skip-lists derivable from the corpus itself.

---

## 8. Variant identity stack 🟡

**Question:** Are these two surface forms the same underlying word?

**What it catches:**
- Proper-noun spelling inconsistency (`Peter` / `Petros` / `Petro`)
- Diacritic-presence variant for proper nouns
- Abbreviation vs full form (`St.` / `Saint`)
- Cross-script transliteration variants (`David` / `Dawid` / `Dafyd`)
- Inflectional clusters that should share a stem

**Tools, cheap to expensive:**
1. Lowercase + diacritic-fold + exact match
2. Damerau-Levenshtein (prefer over plain Levenshtein — transposition is a single op, which matters for typos)
3. Phonetic encoding (Double Metaphone) — for cross-script and cross-spelling
4. Consonant skeleton overlap — for case-marked variants in case-marking languages
5. Stem clustering (`lemma_cluster.rs`, `candidate_families.rs`)

**Already implemented:** `proper_noun_consistency.rs` (46c7e36).

**Signal/noise:** high for proper nouns; risk on common-word diacritic distinctions (Spanish `hablo`/`habló`) — keep general-vocabulary diacritic checks behind a flag.

---

## 9. Span / pair integrity 🟢

**Question:** For every opening codepoint, is there a matching closer within its scope?

**What it catches:**
- Unmatched paired punctuation
- Quote close before quote open
- Bracket-family mismatch (`[` closed by `)`)
- Wrong pair (curly opener + straight closer)
- Nested-quote level convention violations
- Open quote with no close within `max_span_sids`

**Already implemented:** `discourse.rs` span index.

**Data needed:** pair family table; per-corpus `max_span_sids`; corpus-learned nesting convention.

**Signal/noise:** very high once convention is established.

---

## 10. Sentence-position conventions 🟡

**Question:** At position P (sentence-initial, sentence-final, verse-initial, verse-final, after-punct, before-punct), what character class is dominant — and does this token match?

**What it catches:**
- Sentence-initial lowercase
- Verse-start orphan punctuation (`", and Jesus said"` at verse start)
- Missing terminal punctuation
- Wrong terminal punctuation (limited subset — needs interrogative detection or source)
- Intermedial clinging punctuation (comma free-floating with whitespace on both sides)
- Missing space after sentence boundary (`"ended.Then"`)
- Space-before-punctuation in non-French corpora
- Trailing punctuation chain (`?!.`)

**Already implemented:** `sentence_start_case.rs`, `ClingingClass`.

**Signal/noise:** high for strong conventions (95%+); each sub-rule needs corpus introspection before enabling to confirm the convention exists at the expected strength.

---

## 11. Casing-distribution conformity 🟡

**Question:** For this token, what case-pattern does it normally take in the corpus, and does this instance match?

**What it catches:**
- Proper-noun lowercased mid-sentence (`david went to jerusalem`)
- Common noun improperly Capitalized — with corpus-learned reverence list (`the Word`, `the Father`, `the Way`) as exception
- ALL CAPS outlier — with divine-name convention (`LORD`, `GOD`) learned per corpus
- Mixed-case interior (`jeSus`, `cAmel`) — 🟢 boolean
- Lowercase pronoun `i` in English

**Already implemented:** `proper_noun_consistency.rs`.

---

## 12. Verse-shape / proportionality 🟡

**Question:** Given this verse's slot and (optionally) source counterpart, is its size and content shape normal?

**What it catches:**
- Verse far longer / shorter than parallel source verse
- Merged verses (slot length 2× typical, neighbor near-empty)
- Split verses (inverse — two short consecutive verses summing to one source-length)
- Verse-internal token-length distribution outlier
- Single ultra-long token (40+ chars — concatenation artifact, threshold corpus-relative for agglutinative languages)
- Empty verse / whitespace-only verse
- Whole-verse duplication (exact match, length-gated)
- Verse-number leaked into verse text (`"3 And Judah begat..."`)
- Chapter/section heading leaked into first verse

**Already implemented:** `source_relative.rs`, `empty_verse`.

**Tools:** length z-score (length-bucketed), per-book conditioning, MAD over token-length distribution.

---

## 13. Source-aligned questions 🔴 → 🟡 with alignment

**Question:** Given a source-target verse pair (or word alignment), does the target match?

**What this gates:**
- Wrong proper noun substitution (Mark → Matthew)
- Wrong number (4000 vs 5000)
- Wrong pronoun / tense / aspect
- Missing function word, missing content word
- Same source term translated inconsistently across verses (`agape` → `love` / `charity`)
- Different source terms collapsed to one target
- Transposition-to-valid-word (`tomb` / `tome` — only path)
- Wrong verse reference (text of Matt 1:1 in Matt 1:2 slot, when length difference flags it)
- Quoted OT passage matching OT reference

**Honest framing:** this is *not* 15 separate rules. It's one build-or-don't-build infrastructure decision. Without source alignment all of these are 🔴. With source alignment most become 🟡.

**Tools, ordered by cost:**
1. Length-ratio z-score (already in #12)
2. Conditional NCD between source-target pairs
3. Word-level alignment via IBM-style models (high cost, high payoff)

---

## 14. Markup-leak patterns 🟢

**Question:** Does this verse contain content that's clearly not body text?

**What it catches:**
- USFM/USX markers (`\v`, `\p`, `\q`, `\id`) leaked into body
- Version-control conflict markers (`<<<<<<< HEAD`, `=======`)
- Translator notes / TODOs (`[CHECK]`, `FIXME`, `TBD`, `??`, `(translation needed)`)
- URLs, emails, filenames (`http://`, `.docx`)
- Date / time stamps (`2024-03-15`)
- Currency / modern units (`USD`, `$`, `km`)
- Footnote / cross-reference markers (`†`, `[a]`, `(1)`)

**Data needed:** per-format pattern bundle; corpus-learned project-specific marker additions.

**Signal/noise:** near-100% on canonical patterns; project-specific markers (e.g., `<<TRANS>>`) need to be learned per project.

---

# Out of scope (call out so we don't promise it)

- **Theological errors** — needs human-level semantics.
- **Nuance shifts** (`trespasses` / `sins` / `debts`) — translation philosophy, not error.
- **Cultural register** errors.
- **Naturalness / readability** — perplexity approximates, precision low.
- **Genre violations** (poetry-as-prose).
- **Source text errors propagated** — tool can't verify the source.
- **Verses out of order within a chapter** — no purely internal signal.
- **Speaker attribution / discourse parsing** — needs discourse model.
- **Pronoun reference ambiguity** — needs semantic parsing.
- **Dotless ı vs dotted i** — Turkish-specific. User decision: skip.
- **Combining mark on wrong base** — needs language orthography. User decision: skip.

---

# Cross-cutting design notes

- **Corpus-relative everything.** Almost every detector uses a corpus-learned threshold. Generic frequency tables are too coarse for ~150k tokens; NT-specific conventions can be measured directly.
- **Skip-lists from the corpus itself.** Doubling exceptions, reverential capitalization, divine-name casing, formulaic-phrase whitelist — derivable rather than hand-curated.
- **Source alignment is one decision, not many rules.** ~15 prior rules collapse into detector #13. Worth treating as a single roadmap question.
- **Pre-alpha license to under-flag.** Each 🟡 detector ships with a high confidence threshold first; lower as convention-learning matures.
- **Per-rule corpus introspection before enabling.** Before turning on any 🟡 detector, dump its statistic over the current corpus to confirm the convention exists at the expected strength. Don't ship rules that assume conventions the current draft hasn't established.
- **Don't ensemble overlapping detectors at the same scale.** Word-level char-n-gram + word-level compression both measure character-pattern normality; they're correlated and break Noisy-OR independence. Lexicon-membership-then-surface-pattern as backoff, not parallel.

---

# Detector → question-tooling map

These align with the "questions being asked" framing:

| Detector                         | Question form                                  |
| -------------------------------- | ---------------------------------------------- |
| 1 Codepoint denylist             | Is this codepoint allowed in body text?        |
| 2 Script consistency             | What scripts are in this token?                |
| 3 Majority-class conformity      | Is this the dominant variant of its function?  |
| 4 Combining-mark well-formedness | Are marks attached to valid bases?             |
| 5 Lexicon membership             | Is this a real word in this language?          |
| 6 Surface-pattern normality      | Does this look like this language?             |
| 7 Distributional normality       | Is this where the corpus says it should be?    |
| 8 Variant identity               | Are these the same underlying word?            |
| 9 Span / pair integrity          | Does every opener have a closer?               |
| 10 Sentence-position conventions | Does position P expect class C?                |
| 11 Casing distribution           | Is this case-pattern normal for this token?    |
| 12 Verse-shape                   | Is this verse's size and content shape normal? |
| 13 Source-aligned                | Does target match source?                      |
| 14 Markup-leak                   | Is this content clearly not body text?         |
