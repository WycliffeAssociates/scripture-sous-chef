# `lex.*` — Lexical & content-whitespace

Token-aware rules over the UAX #29 word stream (`crate::token::tokenize`),
plus the content-whitespace scan. The `lex.` namespace spans two source
files: `lexical.rs` (the token rules) and `whitespace.rs`
(`lex.excess-h-whitespace`). The id is the stable surface — don't read the
prefix as a promise about which file the code lives in.

---

## `lex.excess-h-whitespace` — doubled horizontal whitespace mid-clause

> **Severity** Warning · **Default** on · **Scope** per-verse · **Knobs** none · **Source** `whitespace.rs`

**Flags** — A run of 2+ horizontal whitespace inside verse content, where
horizontal whitespace is Unicode `Zs` plus tab — so NBSP counts, and a
**mixed** run is still one run:
- `a  b` → the double space
- `word<NBSP><SPACE>word` → the mixed-width gap (a common paste/IME artifact)
- `word<NBSP><NBSP>word` → the doubled NBSP

**Clean** — `a b` (single space); `End.  Next` and `…थिए।  अर्को` (a double
space *after* a sentence terminal is a legitimate spacing convention, in any
script); `   a` (a leading run is not content whitespace).

**Why it matters** — A doubled or mixed-width gap mid-clause is almost always
a stray keystroke or an invisible paste artifact. But the classic "two spaces
after a sentence" is a real typographic convention, so a run that immediately
follows a UCD `Sentence_Terminal` character is protected and not flagged —
that covers `. ! ?` and equally the danda `।`, Ethiopic `።`, Arabic `۔`,
Burmese `။`, and every other script's terminals, by property rather than
enumeration (ADR 0036).

**Config** — On/off only.

**Nuance & ADR ties** — Leading runs (before any real content in the verse)
are skipped. Embedded newlines are *not* detected: newlines are absent from
the slice-1 vref projection, and a line break isn't cleanly highlightable
(ADR 0010). The scan walks `char_indices` with both predicates read from the
fused character table (ADR 0036, replacing the original ASCII byte scan of
ADR 0014, which saw only space/tab and protected only `. ! ? : ;`). Note `:`
and `;` were in the old protection set but are **not** `Sentence_Terminal` —
a double space after them now flags, which is the correct reading: they don't
end sentences, and the two-space convention is a sentence-boundary
convention. The 106-corpus survey shows what the widening bought: the ASCII
scan found **zero** runs anywhere; the Unicode scan surfaces 5,934 findings
of real invisible NBSP+space damage, concentrated in kmr-IQ (5,620,
a systematic input-method artifact), with no two-space-after-terminal storm
in any corpus (see the 2026-07-06 bracket-balance/whitespace calibration
report).

**Open issues / future work** — Newline-in-body detection is deferred
(ADR 0010). A corpus that deliberately doubled NBSP as typography would storm
here; none of the 106 does. If one appears, this rule follows casing/spacing
into the corpus-relative tier (recorded margin in the calibration report).

---

## `lex.duplicate-word` — the same word twice in a row

> **Severity** Warning · **Default** OFF · **Scope** per-verse · **Knobs** none · **Source** `lexical.rs`

**Flags** — Two consecutive identical tokens (case-insensitive) separated by
**whitespace only**:
- `in the the beginning` → `the the`
- `And And he said` → `And And`

**Clean** — `yes, yes` / `truly, truly I say`: the gap holds a comma, not just
whitespace, so it reads as rhetorical repetition, not a typo.

**Why it matters** — In non-reduplicative languages, `the the` is a
near-perfect typo signal (every en/es ULB hit is a real doubling error).
**But** reduplication is core grammar across much of this tool's audience —
Vietnamese `đời đời` ("forever"), Bantu doubling, and many more — producing
600+ legitimate hits per NT in a reduplicative language (deterministic-batch
calibration). So it **ships disabled** and is enabled per-project where
doubling is genuinely unusual.

**Config** — On/off only, and **off by default**: `Config::v1_defaults`
disables it; `Config::all()` includes it. Turn it on in a project's
`.sous/rules.json` where reduplication isn't a feature of the language.

**Nuance & ADR ties** — The whitespace-only-gap requirement is exactly what
separates a typo (`the the`) from rhetoric (`yes, yes`). Matching is
case-insensitive (`The the` flags). See ADR 0014 and the
`2026-06-09-deterministic-batch` calibration report.

**Open issues / future work** — A corpus-observed reduplication-rate gate
(auto-enable only where doubling is statistically rare in *this* corpus) is
the obvious graduation path into a corpus-relative substrate, but isn't built —
today it's a manual per-project toggle.

---

## `lex.punct-only-token` — corpus-unusual punctuation-only chunks

> **Severity** Warning · **Default** on · **Scope** substrate-backed corpus · **Knobs** `convention_rate_per_10k`, `confidence_z`, `emit_score_min` · **Source** `lexical.rs`

**Flags** — A whitespace-delimited chunk that is entirely punctuation/symbols
and whose pattern is unusual for this corpus:

- `word ,; word`, a stray `=` or `´`, a stranded `(` — one-off wreckage,
  score ≈ 0.9–1.0 in calibration
- byn `፡፡፡` (tripled Ethiopic wordspace) at 0.91 — beside 1,210 suppressed
  `፡፡` full stops
- a lone `<` in kn_ulb at 0.90 — beside 482 suppressed `<<`/`>>` ASCII
  guillemets
- sparse systematic damage the pre-Wilson ramp read as conventions: plt's
  `_` placeholder blanks (×36), te's stray `<<` (×31 in a corpus that
  doesn't use guillemets) — un-suppressed by the ADR 0032 recalibration

**Clean** — Digit-only chunks (numerals); a *single* ordinary mark or dash
(detached sentence punctuation is a spacing convention somewhere — Nepali
`…थिए ।` — and `punct.spacing-anomaly` judges spacing); `...`; riding quotes
and closing brackets; runs of 3+ `<`/`=`/`>`/`|` (committed merge conflicts —
`struct.merge-conflict-marker`'s finding, skipped here to avoid
double-reporting); chunks whose core is a run of 3+ `?`
(`hyg.replacement-run`'s finding — encoding damage, excluded from candidacy;
ADR 0034); and every pattern the corpus establishes as its own typography:
the ur-deva danda-substitute `|` (×2,261), Burmese spaced finals `၏။`,
`<<`/`>>` quotes, spaced-open-paren styles.

**Why it matters** — Deterministic exemptions can name a lone danda but can
never enumerate every project's detached-punctuation typography; the stateless
verdict stormed 8,934 Warnings across 106 corpora, ~96% conventions — and
contradicted `punct.adjacency-anomaly`, which had already learned `፡፡` as
byn's full stop. The corpus itself shows which chunks are house style. No
language or script identity is consulted.

**Scoring** — The fixed candidate scan supplies chunks. One corpus-relative
factor, the shared Wilson `strength` primitive (ADR 0032):

```text
convention = strength(core-pattern count, whitespace lexical units,
                      convention_rate_per_10k / 10⁴, confidence_z)
evidence   = 1 - convention
```

The recurrence key is the chunk's **core** — riding quotes/closers stripped
(closers by the UCD bracket inventory, not an ASCII `)]}` list), the same
reduction the scan's verdict uses — so `۔!` and `۔!)` pool as one convention.
There is no word factor: a punct-only chunk has no containing word.

**Config** — Defaults are `1.0` occurrences per 10k lexical units,
`confidence_z = 1.96`, and floor `0.5`, frozen against sharply bimodal corpus
histograms (conventions ≈ 0.0, wreckage ≥ 0.9, empty middle). The rule is
default-on. The Wilson shrinkage sets the small-corpus behaviour: a hapax
wreck starts emitting at ~3.6k lexical units (a few chapters of drafting —
down ~6× from the retired linear ramp's ~20k, which silently suppressed
early-draft NTs), while a single tiny epistle (~500 units) still
conservatively abstains — with that little text, "this corpus rarely does X"
is not knowable.

**Nuance & ADR ties** — Evidence is aggregate-only and partitioned per book:
core-pattern counts plus a lexical-unit count. The substrate derives affected
chapter products and patches the resident partition from its current sites.
Sparse conventions (pt-br `—,` ×17, stranded `(` house styles ×~20)
surface at moderate scores — the systematic-pattern tradeoff ADR 0024
documents, tunable via the rate and floor. The old judge-side mojibake bypass
(3+ `?` chunks scored 1.0 regardless of recurrence) is gone: the special case
is now a rule, `hyg.replacement-run`, and the pattern is excluded from
candidacy here (ADR 0034). See ADR 0030 (the corpus-relative conversion),
ADR 0032 (Wilson unification), and the 2026-07-06 calibration reports.

**Open issues / future work** — A corpus that conventionalizes a genuinely
mixed chunk would need enough volume to learn itself down. (The former open
question — whether the `?`-run mojibake carve-out should graduate to a
dedicated damaged-text hygiene rule — is resolved: it did, as
`hyg.replacement-run`, ADR 0034.)

---

## `lex.repeated-character-run` — corpus-unusual repeated letter graphemes

> **Severity** Info · **Default** on · **Scope** substrate-backed corpus · **Knobs** `convention_rate_per_10k`, `word_recurrence_k`, `confidence_z`, `emit_score_min` · **Source** `lexical.rs`

**Flags** — Three or more identical extended grapheme clusters where both the
cluster and its containing word are unusual for this corpus:

- `joyfullly` in English → `lll`, score 0.994 in calibration
- `guerrras` in Spanish → `rrr`, score 0.974
- a copied `destruccción` occurring twice still surfaces at 0.790
- Thai `ภรรรยา` (a tripled ro han in `ภรรยา`, "wife") → `รรร` — rare
  corpus-wide, so it surfaces even though no UAX #29 token contains it

**Clean** — Double letters (`bookkeeper`); digits/punctuation; U+0640 tatweel
kashida stretching; established vowel length/ideophones; and recurring
scriptio-continua joins such as Thai `ขอออก` where the `อออ` spans two words.

**Why it matters** — A third repeated letter is a strong typo clue in many
languages, but a universal verdict creates thousands of false positives in
languages that use long vowels, expressive repetition, or unspaced word joins.
The rule learns those conventions from the project itself. No language or
script identity is consulted.

**Scoring** — The fixed threshold-three grapheme scan supplies candidates.
Cluster recurrence and word recurrence are independent convention axes,
combined as the noisy-OR residual (ADR 0032):

```text
cluster_strength = strength(cluster-run count, whitespace lexical units,
                            convention_rate_per_10k / 10⁴, confidence_z)
word_strength    = clamp((containing_word_frequency - 1) / word_recurrence_k, 0, 1)
evidence         = (1 - cluster_strength) · (1 - word_strength)
```

`strength` is the shared Wilson primitive from `evidence.rs`; either axis
fully establishing a convention zeroes the evidence. When UAX #29 supplies no
containing token, `word_strength = 0`; raw run recurrence still suppresses
scriptio-continua conventions. The denominator is whitespace-delimited
lexical units, not UAX token count: Thai/Lao UAX word segmentation produced
one token per grapheme and diluted real recurrence.

**Config** — Defaults are `2.0` runs per 10k lexical units, word recurrence
`K = 5`, `confidence_z = 1.96`, and emission floor `0.5`. Lower the convention
rate or raise the floor for fewer findings. The rule is default-on; map its
`RuleId` to `false` to skip both reduction and judgment. The Wilson shrinkage
(ADR 0032) has two visible effects against the retired linear ramp: small
corpora start emitting (hapax-wreckage threshold ~1.8k lexical units, down
from ~10k; a ~500-unit corpus still abstains), and sparse systematic damage in
full corpora un-suppresses — scg/bds keyboard-bounce runs (`ooo`×23, `hhh`×17)
recurring ~10–25× corpus-wide now surface, while established conventions
(Burmese finals, tatweel-adjacent lengthening) stay silent.

**Nuance & ADR ties** — Tatweel (U+0640) is excluded in the scan itself, not
by scoring: kashida is a stretching control whose repetition is inherently
typographic (`الإيمــــــان` is one word, elongated), so runs of it can never
be the doubled-letter error this rule hunts. That is a one-character
Unicode-semantics carve-out, not a script allow-list — the no-script-identity
principle (ADR 0023/0025) stands. Evidence is aggregate-only and partitioned per book:
cluster counts, run-containing word counts, and lexical-unit count. The
substrate retains the chapter-local sites needed to patch current findings
after a relevant evidence change. The cluster key is the full first grapheme lowercased, so case variants
pool while combining marks remain significant. Run length above three adds no
weight. See ADR 0028, ADR 0032 (Wilson unification), and the 2026-07-06
calibration reports.

**Open issues / future work** — Systematic typos suppress like conventions;
corpus counts cannot infer intent. Multi-grapheme morphological reduplication
such as Gujarati `દાદાદાદી` is outside this detector and remains a known
conflation if it happens to contain a single-cluster triple.
