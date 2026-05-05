# Scripture Sous Chef: The Rule Playbook

This document is a catalog of the specific rules the engine uses to find errors. For each rule, we explain the 5W1H (Who, What, Where, When, Why, How) using concrete examples.

---

## 1. Hygiene Rules: The "Always Wrong" Category
These rules don't use statistics. They look for invisible or universally invalid characters that usually get into the text via accidental keystrokes or bad copy-pasting.

### `hyg.tab-in-body` (Tabs in Text)
* **What:** Flags literal Tab characters (`\t`) inside a verse.
* **Why:** Bible formatting uses standard spaces. Tabs are almost always an accident (e.g., a translator's finger slipped from the 'Q' key).
* **How:** It scans the verse. If it sees a Tab, it flags it.
* **Example:** `In the beginning God created the heaven and the	earth.` (Tab between 'the' and 'earth').

### `hyg.zero-width-misuse` (Invisible Characters)
* **What:** Flags invisible Unicode formatting characters (like the "Byte Order Mark" or "Zero-Width Space").
* **Why:** These characters cause layout engines and search tools to break, but the translator can't see them on the screen.
* **How:** It checks the text against a known list of invisible characters. *Note: It is smart enough to allow certain invisible "joiner" characters if the language's alphabet (like Arabic or Devanagari) actually requires them!*
* **Example:** The translator pastes a name from a website: `Jeru[INVISIBLE_CHAR]salem`. The engine catches it.

---

## 2. Positional Rules: Learning Language Conventions
These rules observe the whole text to figure out how punctuation and capitalization work in this specific translation, and then flag moments where the translator breaks their own habits.

### `pos.sentence-start-case` (Missing Capital Letters)
* **What:** Flags lowercase letters that appear immediately after a sentence-ending punctuation mark.
* **Why:** To catch typos where a translator forgot to hit the Shift key, or accidentally used a period instead of a comma.
* **How:** 
  1. The engine scans the whole book and notices that 98% of the time, the character `.` is followed by a space and a Capital Letter. 
  2. It officially "learns" that `.` is a sentence terminator.
  3. It then searches for any time `.` is followed by a lowercase letter.
* **Example:** `Jesus wept. then he prayed.` (Flags the lowercase 't' in 'then').
* **Smart Feature:** It knows to ignore proper nouns! If it sees `He said to Jesus, "hello"`, it won't get confused by the quotation mark, because it knows "Jesus" is a capitalized name, not the start of a new sentence.

### `pos.unexpected-sentence-end` (Orphaned Punctuation)
* **What:** Flags common "glue" words (like *and*, *the*, *of*) when they appear right before a period or question mark.
* **Why:** To catch copy-paste errors, accidental line breaks, or typos.
* **How:** 
  1. It asks the engine: "What punctuation marks end sentences?" (Re-using the knowledge from the rule above).
  2. It finds words that appear frequently (e.g., "and" appears 5,000 times).
  3. It notices that "and" has *never once* been followed by a period.
  4. If it suddenly finds "and.", it flags it.
* **Example:** `God created the heavens and. the earth.` (Flags "and.").

---

## 3. Punctuation Rules: Basic Integrity
Checking that pairs of things actually match.

### `punct.paired-balance` (Unclosed Brackets & Quotes)
* **What:** Flags opening brackets, parentheses, or quotes that never get closed.
* **Why:** A missing quotation mark can change the meaning of a biblical passage (e.g., who is speaking?).
* **How:** It walks through the text keeping a tally. If it sees a `(`, it waits for a `)`. If it reaches the end of the text and never saw the closing bracket, it flags the opening bracket.
* **Example:** `Jesus said, "I am the way.` (Flags the `"` because there is no closing `"`).
* **Example 2:** `He went (to the city]` (Flags the `]` because it expected a `)`).

---

## 4. Source-Relative Rules: The Macro View
These rules compare the target translation against the original source text (usually English, Spanish, or French) to find systemic issues.

### `src.proportionality` (Verse Length Anomalies)
* **What:** Flags verses that are vastly longer or shorter than we would expect, based on the source text.
* **Why:** To catch major structural errors: accidentally skipping a sentence, pasting a whole chapter into a single verse, or verse-numbering misalignments.
* **How:** 
  1. It counts the characters in every source verse and every translated verse.
  2. It figures out the normal "ratio" (e.g., "Spanish verses in this book are usually 1.2 times longer than the English source").
  3. It uses a robust statistical method (ignoring extreme outliers) to find verses that violently break this ratio.
* **Example:** Verse 4 usually has 100 characters. The translator's verse 4 has 850 characters. The engine flags it. (Likely, the translator accidentally combined verses 4 through 10 into one box).
* **Smart Feature:** It checks the ratio against *the whole Bible* AND against *just that specific book*. This handles situations where one specific translator working on the Book of Romans naturally writes longer sentences than the translator working on Genesis.
```