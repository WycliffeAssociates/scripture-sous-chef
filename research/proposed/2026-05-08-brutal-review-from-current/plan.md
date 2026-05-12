# Plan — post-brutal-review pivot to a deterministic core

**Date:** 2026-05-08
**Source materials:** `2026-05-08_research-brief.md`, `response-soup.md`
(two independent agent reviews + the project owner's own distillation).

> Where this plan conflicts with the prior `2026-05-06_signal-architecture/
> plan.md`, this plan wins. Specifically: the rare-word triage Noisy-OR
> chassis is being demoted to "advisory" status and a deterministic-first
> core is being made the primary product.

## How this file relates to the others in this folder

This document is the **high-level plan and sequencing**. The companion files in this folder go deeper on the *what* and *how* of each rule and tool:

- **`conrete-examples-by-cat.md`** — the *symptom list*. Every error class a NT draft could contain, organized by category, with tractability ratings (🟢/🟡/🟠/🔴) and user annotations against individual symptoms. The closest thing to a coverage checklist; ultimately the source of truth for the test-case catalog.
- **`concrete-examples.md`** — the *detector grouping*. The ~14 detectors that collapse the symptom list. Use this when planning what to build.
- **`tools.md`** — the *toolbox by question*. The ~14 analytical questions and the tools we'd reach for, with competing tools clustered and one promoted. Use this when deciding which technique answers which question.
- **`response-soup.md`** and **`2026-05-08_research-brief.md`** — source material for this plan; not actively edited.

Rule of thumb: if you find yourself adding a long per-rule description to *this* file, that detail probably belongs in one of the three docs above, and this file should reference it. The implementation of each rule eventually becomes a test case; the test-case catalog will be derived from the symptom list.

---

## 0. The framing change

Two independent reviewers, plus the owner's own read, converged on one
verdict:

> The Bayesian probabilistic chassis is over-engineered for a no-labels,
> single-NT data scale. The deterministic / boolean checks (mixed scripts,
> duplicate words, intermedial punctuation, hygiene, paired-punct balance,
> proper-noun consistency) are *under*-developed relative to the chassis,
> and they are where actual user value lives at this stage.

The corrective action is **not** to rip out the probabilistic work that
exists. It is to **resequence**: ship a defensible deterministic core
first, and treat probabilistic signals as **advisory** until either real
labels arrive or eBible-derived priors give them empirical grounding.

This plan also locks in two non-negotiables that the reviewers got wrong:

1. **No Paratext / Translation Core integration as the primary surface.**
   Composability and organizational independence are deliberate
   architectural commitments. Wycliffe Associates is not part of the
   GBTC consortium and does not share its checking philosophy or
   ecosystem assumptions. A Paratext plugin is incompatible with that
   stance regardless of distribution upside. (See §1.1 below.)
2. **Content analysis, not "USFM linter."** The reviewers reached for the
   "linter" metaphor; it's the wrong product framing. USFM linting is a
   solved problem and not what this engine does.

---

## 1. Lock these in before sequencing

### 1.1 Composability is non-negotiable

The reviewers consistently recommended consuming Paratext plugin APIs,
ingesting Translation Core USFM 3, depending on Serval REST, etc.
Rejected for v1.

**Why.** Every organizational assumption baked into a dependency — a
required Paratext account, a GitHub-only backend, a ClearML tracking
hook, an opinionated git remote — is a constraint on portability. The
project owner is at Wycliffe Associates, which uses different
infrastructure (Gitea, not GitHub; non-GBTC checking philosophy; church-
owned drafts that often haven't been registered with DBL). The value of
this engine is that you hand it a USFM directory and get signal back
without negotiating with anyone's identity provider, license server, or
hosted API.

**What this means for v1.**
- The CLI stays the primary surface.
- File formats stay USFM in / JSON-or-Markdown out.
- Any external tool we evaluate (SIL Machine, Serval) is treated as a
  *spike* — "can it produce useful artefacts that we ingest as files"
  — not as a runtime dependency.

**Action item:** an ADR documenting this. See §11.

### 1.2 The Bayesian sub-cluster chassis is parked, not removed

Both reviewers called the Bayesian posterior chassis "mathematical
theater" at the current data scale (one NT, zero labels). The owner's
own read agrees in spirit: 20 labels won't move a `Beta(1, 4)` posterior
in any useful way; sub-cluster routing presupposes labels we don't have.

**Decision.** Keep the code, do nothing new with it.
- `analysis::posterior` stays.
- No new rules route into sub-clusters in v1.
- Existing rule-level posteriors continue to replay events; we just
  don't invest more in the chassis until label volume justifies it.
- Phase B item #10 (sub-cluster routing) is deferred indefinitely. It
  comes back when we have ≥200 labels per rule on ≥3 projects with
  evidence sub-clustering would change behavior.

### 1.3 The rare-word Noisy-OR is demoted to advisory

The current per-token Noisy-OR over `char_anomaly + char_ngram_backoff
+ source_co_rarity` has three known problems the reviewers flagged and
the owner agrees with:

1. `char_anomaly` and `char_ngram_backoff` are correlated — Noisy-OR's
   independence assumption breaks. Double-counting inflates scores.
2. `source_co_rarity`'s `0.0 / 0.3 / 0.7` placeholders are uncalibrated
   theater.
3. Without labels, Noisy-OR + sigmoid + temperature + cap is a stack of
   knobs with no ground truth to tune against.

**Decision for v1.**
- Keep the rare-word triage running as a separate output stream
  (`sous triage`).
- Label its output explicitly as **Advisory** in the UI / CLI output —
  not "findings" with the same status as the deterministic rules.
- Replace the Noisy-OR aggregator with `max(char_anomaly,
  char_ngram_backoff, source_co_rarity)` until either labels exist
  (switch to logistic regression) or eBible priors land
  (switch to ECDF-percentile-rank, see §6.2 and §10.B).
- Down-weight `source_co_rarity` heavily until alignment data exists
  (§7).
- Keep the existing `char_anomaly` / `char_ngram_backoff` temperature
  + cap tuning in place; it makes outputs less harmful at saturation
  but the underlying signal is still suspect.

This is the smallest defensible change. It does not throw away work; it
correctly labels the work as not-yet-trustworthy.

**Status:** ✅ landed in `0ddf905` (2026-05-12). Aggregator replaced
with `max(char_anomaly, char_ngram_backoff, source_co_rarity · 0.3)`;
B8 `triage_char_factor_weight` removed (it was a Noisy-OR-specific
independence-correction with no role under `max`); CLI triage output
now explicitly labels itself "Advisory, not findings."

---

## 2. The two-tier architecture

The product is now organized in two tiers with distinct surfacing
treatment:

### Tier A — Deterministic / convention-learned **findings**

Things we are willing to surface to a translator as actual findings.
Either deterministic ("this is a fact about the text") or learned-from-
the-corpus-with-a-sharp-threshold ("the corpus uses this convention; this
verse violates it"). Per ADR 0008, surfaced with provenance metadata.

### Tier B — Distributional **advisories**

Probabilistic signals that may be useful but cannot stand alone as
findings without calibration. Surfaced as a separate "advisory queue"
with explicit caveats. The translator can opt into reviewing them; the
default surfacing does not foreground them.

The CLI's output should reflect this split. Today's `sous check` emits
a flat list of findings; the deterministic-core findings should be the
primary output, and the rare-word triage / NCD verse anomaly results
should be a clearly-marked secondary section.

---
## 3. Tier A: the deterministic core

> **Pointer to detail docs:** the green/yellow classification, worked examples, and full symptom list for each rule below live in `conrete-examples-by-cat.md` (symptom view) and `concrete-examples.md` (detector view). This section gives the build order and the configuration shape; for "what exactly does this rule catch and where does it false-positive," follow the cross-refs.

This is what we should have built more thoroughly before the
probabilistic work. It is not glamorous. It is where actual signal
lives at zero-label scale.

### 3.1 Already implemented (audit + sharpen)

| Rule                          | File                                            | Action                                                                                                                                                                |
| ----------------------------- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hyg.tab-in-body`             | `signals/hygiene.rs`                            | Keep. Verify with a worked example.                                                                                                                                   |
| `hyg.control-chars`           | `signals/hygiene.rs`                            | Keep. Worked example.                                                                                                                                                 |
| `hyg.zero-width-misuse`       | `signals/hygiene.rs`                            | Keep. Add explicit script-allow-list config (ZWNJ legitimate in Indic/Arabic). Worked example.                                                                        |
| `hyg.empty-verse`             | `signals/hygiene.rs`                            | Keep. Worked example.                                                                                                                                                 |
| `pun.paired-punct-balance`    | `signals/punctuation.rs` + `discourse.rs`       | Keep. The span-index implementation is good. Worked example documenting the cross-verse cluster behavior.                                                             |
| `lex.proper-noun-consistency` | `signals/proper_noun_consistency.rs`            | Keep, worked example. Add a config knob for the `MIN_UPPER_OBS` threshold (currently 3; some users may want 4 or 5).                                                  |
| `pos.sentence-start-case`     | `signals/positional/sentence_start_case.rs`     | Keep. Worked example showing what "learned trigger" means.                                                                                                            |
| `pos.unexpected-sentence-end` | `signals/positional/unexpected_sentence_end.rs` | Keep. Worked example.                                                                                                                                                 |
| `src.proportionality`         | `signals/source_relative.rs`                    | Keep. Re-classify as Tier A: it's a coarse verse-length ratio with a robust z threshold; it's deterministic enough that calibrated-once-against-eBible would suffice. |

**"Worked example" deliverable for each rule.** A short markdown file
under `documentation/rules/<rule_id>.md` containing:
- The plain-English question the rule asks.
- A short before/after example (verse text on either side).
- The exact knobs the rule exposes and what each one does.
- The rule's failure modes (when it false-positives, when it false-
  negatives).
- The provenance fields it emits (cluster_key, lane, evidence).

This forces clarity, lives next to the code, and gives translators
something to read when they ask "why did this fire?"

### 3.2 Not yet implemented (high priority)

These are the rules the brutal review identified as missing from the
deterministic core. Each is mostly-deterministic with a thin
configurable layer.

> **Workflow note:** the per-rule descriptions below are kept here for build sequencing. Long-term, the detail (knobs, failure modes, worked examples) belongs in the symptom and detector docs; this file should eventually shrink to pointer-plus-build-order. The pattern of work is "one rule at a time, small tight test-driven loop" — green dots first, then yellow.

#### 3.2.1 Mixed-script detection — `orth.script-mixing`

A token containing graphemes from more than one script (Latin `o` glued
into a Cyrillic word; Devanagari combined with Latin digits inside a
single token). Near-100% precision when the script-allow-list is set
correctly. Code stub already exists at `signals/orthographic::SCRIPT_MIXING`.

**Configuration knobs:**
- `allowed_scripts`: set; defaults to inferred-from-corpus (the
  dominant script + any minority script crossing some threshold).
- `allow_digits`: bool; whether ASCII / native digits inside a word
  trigger the rule. Default false (digits are usually punctuation in
  scripture).

**Estimate:** 1 day. The `script.rs` / `script_of()` infrastructure is
in place — and now backed by the `unicode-script` crate (ADR 0009)
rather than hand-rolled codepoint ranges, which fixed two latent
bugs (ASCII digits attributed to Latin; Greek Extended returning
None).

**Status:** ✅ landed in `df51517` (2026-05-12). `allowed_scripts`
defaults to `[]` (empty = no allowlist; multi-script tokens fire
regardless); corpus-inference of the allowlist deferred until a real
corpus surfaces the need. `allow_digits` defaults to `false`. The
rule lives at `signals/orthographic/script_mixing.rs`.

#### 3.2.2 Duplicate consecutive words — `lex.duplicate-word-run`

"and the the man" — a real copy-paste artifact. Token-stream n=2 check at the verse level, with thoughtful design knobs because the rule is genuinely probabilistic in places (some languages allow doublings for some words but not others, punctuation between duplicates matters, case-sensitivity is debatable):

- `case_sensitive`: bool; default false (catches "And and"). User can set true to be stricter. Worth a corpus-introspection pass on first run to see which mode produces fewer obvious false positives.
- `punctuation_aware`: bool; default true. Treat `"Holy, holy"` differently from `"holy holy"` — punctuation between the duplicates often signals legitimate liturgical repetition.
- `allow_list`: set of forms that legitimately repeat in the corpus. Default seeded from a corpus pass — any form appearing as a duplicate ≥`min_corpus_occurrences` times is added to the allow-list. The user can override (extend or shrink) via `rules.json` per §3.4.

The plain-English motivation: in English, "Holy holy" is fine; in some agglutinative languages, certain particles repeat as a stylistic feature. The rule must not treat "always-fires" as "always wrong"; the corpus-pass auto-extension to the allow-list is what gives the rule its calibration. When in doubt, prefer false-negative over false-positive — the rule still fires on the obvious "the the" / "and and" cases without litigating particles.

**Estimate:** 2 days. Verse-token iteration + small allow-list builder + config plumbing.

**Status:** ✅ landed in `18988a4` (2026-05-12). Knobs as sketched:
`case_sensitive` (default false), `punctuation_aware` (default true),
`allow_list` (default `[]`), `min_corpus_occurrences` (default 3 →
the auto-allowlist threshold). The rule lives at
`signals/lexical/duplicate_word_run.rs`. Motivating reference:
greekroom.bttdev.org's vi_ulb duplicate-check output — 390 `đời đời`,
125 `ta ta`, 98 `tôi tôi` — is exactly the noise we silence via the
corpus-learned auto-allowlist while still flagging single-occurrence
duplicates as candidate typos.

#### 3.2.3 Intermedial punctuation — `pun.intermedial-clinging`

Punctuation that the corpus has learned should always be left-clinging or always right-clinging, but appears medially in a specific verse. The shape of the rule: for each punctuation codepoint, the corpus learns the dominant clinging direction (left-clinging like `,` or `.`, right-clinging like `(`, or both-fine like `)` in some traditions). Then *for codepoints whose convention is sharp* — e.g. "period almost never appears as character-period-space-then-letter rather than letter-period-space" — flag verse-level violations.

Design intent (from user feedback): the rule should be thoughtful, not absolute. A codepoint that appears in both clinging directions fairly often in the corpus is not a candidate; only codepoints with a sharp learned convention should fire. Where false positives do happen (e.g. legitimate intermedial commas in numbered lists), the §3.4 ignore-list handles them rather than weakening the rule.

The `ClingingClass` infrastructure in `signals/positional/punctuation_class.rs` already classifies clinging at the corpus level. The rule wrapper checks each verse's punctuation positions against the corpus-learned class and fires only when the corpus convention is strong enough.

**Knobs:**
- `min_corpus_occurrences`: how many corpus occurrences are needed before "this punct is left-clinging" becomes an enforceable convention. Defaults to something conservative (50?).
- `min_convention_strength`: the threshold at which a punct's dominant direction is treated as enforceable (e.g. ≥0.97 of observed positions match the dominant direction). Sharp by default.
- `ignore_codepoints` / per-rule ignore list (§3.4): codepoints that legitimately appear in multiple positions in this corpus; rule does not fire on these.

**Estimate:** 2 days.

#### 3.2.4 Case after interior punctuation — `pos.interior-punct-case`

Currently `sentence-start-case` covers terminal punctuation. There's a
corpus-learnable convention for what comes after colons, semicolons,
em-dashes — does this corpus capitalize after them, or not? If the
convention is consistently "capitalize after `:`" and a verse has lower
case after `:`, that's a finding.

The `learn_non_terminal_clusters` infrastructure in `context.rs`
already feeds `Lexicon` with safe interior clusters. The rule wrapper
just needs to: (1) for each interior-punct cluster the corpus has a
strong convention for, (2) flag verse occurrences that violate it.

**Estimate:** 1 day.

### 3.3 Build order for the deterministic core

| #   | Item                                                        | Estimate | Status                             |
| --- | ----------------------------------------------------------- | -------- | ---------------------------------- |
| 1   | Mixed-script detection (`orth.script-mixing`)               | ~1 day   | ✅ `df51517` (registry + rule)     |
| 2   | Duplicate consecutive words (`lex.duplicate-word-run`)      | ~2 days  | ✅ `18988a4`                       |
| 3   | Intermedial punctuation (`pun.intermedial-clinging`)        | ~2 days  | next                               |
| 4   | Case after interior punctuation (`pos.interior-punct-case`) | ~1 day   |                                    |
| 5   | Worked-example documentation for all Tier A rules           | ~2 days  |                                    |

**Total:** ~8 working days for the full deterministic core +
documentation.

**Module-split convention** (learned from commit 1): a rule grows
into its own file once it crosses ~200 lines, under a family
directory (precedent: `signals/positional/`, now
`signals/orthographic/`). For commits 2–4, start a rule in its
family's directory file directly — no need to gestate inside a
single-file module first.

**Fixture format** (learned from commit 1): inline table-driven
tests in the rule's own `#[cfg(test)] mod tests` block, unless an
input exceeds ~5 lines or a fixture must be readable outside the
test runner. The directory-per-case pattern is folder creep for
small string-in/string-out cases.

---

### 3.4 Rule registry, toggling, and ignore-list infrastructure

The user-feedback principle behind this section: every rule we ship as a deterministic finding makes a claim of the form "the corpus convention here is ~97% one way, and this verse is the other way." That claim is empirical, not absolute, so two infrastructure pieces must exist alongside the rule:

1. **Each rule is toggleable on/off per project.** No exceptions. A rule that turns out not to fit a particular corpus must be disable-able without code changes.
2. **An ignore-list lets the translator suppress known-legitimate firings.** The canonical examples: ALL-CAPS `JESUS` for the titulus in some traditions; long Revelation quotation spans that look like unmatched paired punctuation; corpus-specific reverential capitalization. These aren't bugs in the rule; they're known patches.

The ignore list is *not* a Bayesian posterior and doesn't need to be — it's a small typed allow-list per rule per project. The Bayesian chassis stays parked (§1.2); this is the *non*-probabilistic suppression mechanism the deterministic core needs anyway.

**Rule registry — proposed shape:**

A single file (per project, version-controlled with the project) listing every rule, its enabled state, its convention-strength threshold where applicable, and its per-rule ignore patches:

```json
{
  "_file": ".sous/rules.json",
  "rules": {
    "hyg.tab-in-body": {
      "enabled": true
    },

    "orth.script-mixing": {
      "comment": "Greek/Latin code-switching is OK here; digits in numbered labels are normal",
      "enabled": true,
      "allowed_scripts": ["Latin", "Greek"],
      "allow_digits": true,
      "ignore": {
        "verse_sids": ["ACT.19.24", "ACT.19.35"]
      }
    },

    "lex.duplicate-word-run": {
      "comment": "form must repeat ≥3× in corpus to enter auto-allowlist; allow_list is the rule's own knob, not a generic ignore facet",
      "enabled": true,
      "case_sensitive": false,
      "min_corpus_occurrences": 3,
      "allow_list": ["Holy", "verily", "Lord"]
    },

    "pun.intermedial-clinging": {
      "comment": "only enforce conventions ≥97%; verse_sids exempt liturgical/titulus passages",
      "enabled": true,
      "min_convention_strength": 0.97,
      "ignore": {
        "verse_sids": ["REV.4.8", "REV.4.11"]
      }
    },

    "lex.proper-noun-consistency": {
      "comment": "ALL-CAPS JESUS is titulus convention; the rule's own allowlist (lemmas it should not enforce uppercase consistency on) is `case_exempt_lemmas`, not a generic ignore facet",
      "enabled": true,
      "min_upper_obs": 3,
      "case_exempt_lemmas": ["JESUS"]
    }
  }
}
```

**`IgnorePatches` shape (revised after commit 1):**
Today only `verse_sids` is consumed by the engine pipeline. The
original sketch included `codepoints`, `tokens`, and `lemmas` —
those are out. Rules that want token- or lemma-level allowlists
should expose them as **rule-specific knobs** (e.g.
`allow_list`, `case_exempt_lemmas` in the example above), not as
generic `IgnorePatches` facets. Generic facets get re-added
typed-per-rule when a rule needs them.

**Format decision (was an open question in user feedback):**

- **`events.jsonl` stays JSONL** for the append-only event log (`lemma_family_reject`, `lemma_family_confirm`, future translator-feedback events). JSONL is the right shape for an append-only stream — each event is a self-contained record, no rewrite needed, easy to tail/grep.
- **Rule config is `rules.json`** (plain JSON, *not* JSONC). Why plain JSON over JSONC / TOML / YAML: arbitrary frontends may want to read and rewrite this file (a UI, an in-editor checker, a CI script). JSONC support is inconsistent across parsers — `serde_json` doesn't strip comments by default, and a JS frontend's `JSON.parse` rejects them outright. Locking the format to "every JSON parser everywhere already handles this" is worth more than inline-comment ergonomics. We already speak JSON for events; one parser, one format, every consumer.
- **Comments are opt-in `"comment"` keys** on any object that needs explanation, schema-allowed but ignored by the loader. Same human-editability benefit as JSONC, with no parser leak. (The example above uses this pattern.) If we ever generate the file programmatically, the loader preserves `"comment"` round-trip.
- **Ignore lists live inside `rules.json`** under each rule's `ignore` block. Per-rule ignores are tightly coupled to the rule that ignores them; splitting them into a separate file would put cohesive edits in two places. Only split into `ignore.json` later if a single project's ignore list ever exceeds the rule config in size (unlikely for v1).

The two files have different lifecycles:
- `events.jsonl` is *machine-written, append-only* — translator interaction produces records, the system reads them back.
- `rules.json` is *human-written* — the translator (or maintainer) edits it directly.

This split matches their natural use; don't unify them.

**Test-case catalog as a future artifact:**

The user's L145 framing: "if you have 150 hygiene test cases…" — the long-term shape is that each row in the symptom list (`conrete-examples-by-cat.md`) becomes one or more test inputs in `tests/`, with the rule_id and expected-fire flag attached. That catalog isn't part of v1 plan execution, but the symptom list is structured the way it is *because* it's the seed for that catalog. When we're deriving test cases, work from the symptom list, not from this plan.

**Effort for §3.4:** ~1 day for rule-registry scaffold + ignore-list per-rule plumbing + worked example in `documentation/configuration/rules.md`. The infrastructure is more important than the rule count.

**Status:** ✅ landed in `df51517` (2026-05-12) alongside §3.2.1.
`RulesConfig` + `RuleEntry` + `IgnorePatches` live in
`crates/core/src/config_rules.rs`; the engine pipeline consults
both the new `rules_config.enabled(id)` gate and the legacy
`Config.rules` flag (during the transition); `ignore.verse_sids`
is applied at the same stage as `ExceptionSet`. Worked example in
`documentation/configuration/rules.md`.

---

## 4. Tier B: probabilistic advisories — what stays and how

> **Tractability classification cross-ref:** the 🟢/🟡/🟠/🔴 ratings used throughout `conrete-examples-by-cat.md` and `concrete-examples.md` map onto this Tier A/B split as follows — 🟢 deterministic and 🟡 corpus-learned rules with sharp conventions go to Tier A; remaining 🟡 and most 🟠 signals are Tier B advisories; 🔴 rules are out of scope without infrastructure that would promote them (typically source alignment — see §7). For each signal demoted to Tier B below, the question to keep in mind is "what specifically would have to be true (data, alignment, labels) for this to promote to Tier A?" Each row records the promotion condition.

### 4.1 What we keep, demoted

| Signal                                             | Status              | Surfacing change                                                                                                                                            |
| -------------------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Verse-NCD compression-texture (length-conditioned) | Keep                | Promoted to Tier A *after* eBible prior calibration (§6.2). Tier B until then.                                                                              |
| Verse-NCD source mirror                            | Keep                | Tier B advisory until calibrated.                                                                                                                           |
| Rare-word `char_anomaly` factor                    | Keep                | Tier B; aggregation switched to `max()`, not Noisy-OR.                                                                                                      |
| Rare-word `char_ngram_backoff` factor              | Keep, swap smoother | Same. Switch internal smoothing from Laplace to Kneser-Ney (§5.2).                                                                                          |
| Rare-word `source_co_rarity` factor                | Heavy down-weight   | Tier B advisory; `0.0/0.3/0.7` stays placeholder, but now explicitly labeled as such in output. Real values come either from labels or alignment data (§7). |

### 4.2 What we cut from active development

- **Sub-cluster routing on the rare-word triage** — was deferred in the
  prior plan; now confirmed deferred indefinitely.
- **Phase B #6 Morfessor signal** — punted. The `morphology.rs` /
  `candidate_families.rs` infrastructure is fine where it is; we don't
  add a Morfessor-attested-morpheme Noisy-OR factor until the
  deterministic core is shipped and we have a measurable need.
- **Phase B #7 (segmentation.json move) and #9 (profile.yaml)** —
  remain deferred.

### 4.3 What we keep but stop tuning

The temperature + cap tuning we did on `char_anomaly` and
`char_ngram_backoff` stays. We're not going to keep tuning sigmoid
temperatures by hand; calibration belongs in the eBible-priors path
(§6.2), not in per-tweak constant adjustments.

---

## 5. Statistical issues and how we address each

The reviewers' stat-soundness checklist had specific calls. The owner
flagged uncertainty on several. Here's the disposition.

### 5.1 Noisy-OR with correlated factors → switch to `max`

**Verdict:** the reviewers are right; the chassis is wrong for our scale. **Confirmed by project owner.**
**Action:** §1.3 above — `max(...)` until labels exist for logistic regression.
**Effort:** trivial code change; the architectural acknowledgment in the plan is more important than the diff.

> *Open follow-on (Q9 in `tools.md`):* there are narrow cases where two signals co-firing on the same token should boost confidence beyond max — see the tomb/tome and Mary/Mark worked examples in `tools.md` Q9. Don't generalize, but consider a small explicit whitelist of additive co-firings for the proper-noun-aligned case. Tracked there, not here.

### 5.2 Laplace smoothing → Kneser-Ney

> **Confirmed by project owner**, including the side-effect that observed n-gram counts will go *down* under KN compared to add-1 — that's the right behavior (add-1 was inflating the long tail). Expect downstream thresholds tuned against Laplace's inflated mass to need recalibration.


**Verdict:** Laplace is empirically wrong at character-bigram /
character-trigram scale. We already have a Kneser-Ney implementation in
`crates/core/src/analysis/kn.rs` that nothing currently uses.

**Action:** swap `char_ngrams.rs`'s smoothing from add-1 to KN
continuation probability. Use the existing module.

**ELI5 on Kneser-Ney:** "smoothing" means deciding how much probability
to give to events you haven't seen yet (so you don't have zeroes that
break math). Add-1 smoothing pretends every possible bigram occurred
once, even ones that never could (`xq`, `qz`). That over-smooths the
distribution and hides real rarity. Kneser-Ney is smarter: it asks "has
this character been seen as a *continuation* of *anything* before? If
yes, it's the kind of character that shows up in lots of contexts and
deserves probability mass even in unseen contexts; if no, it doesn't."
This matches reality better for character-level text.

**Effort:** 2-3 days including tests and a regression check on the
existing `char_ngrams` tests.

### 5.3 Edit distance ≤ 2 → keep for now, defer phonemic upgrade

The reviewers diverged on this:
- Olly: "add Double Metaphone" (a multilingual phonetic encoder; maps
  words to a code that's invariant to spelling variation).
- Jim: "use phonemic hashing" (less specific).

The owner asked what phonemic hashing entails. Spelling that out:
- A **phonemic hash** maps a word to a code that captures its
  approximate pronunciation, ignoring spelling variations.
  Example: Soundex maps "Smith" and "Smyth" to the same code. Double
  Metaphone is more sophisticated and handles more languages.
- Edit distance on phonemic codes catches sound-alike variants that
  surface-distance misses (e.g., `Davidi` ↔ `David` ↔ `Dawood` —
  all the same person, different transliterations).

**Decision:** defer. Reasons:
1. Phonemic encoders are language-family specific. Double Metaphone is
   tuned for English; its accuracy on Bemba or Devanagari transliterations
   is not validated.
2. The current `source_co_rarity` is already heavily down-weighted
   (§4.1). Investing in better edit distance for a signal we don't
   trust is premature.
3. The right place for this work is *after* alignment-data integration
   (§7), where we'd be matching specific aligned source tokens, not
   doing per-verse fuzzy lookup.

**Note for revisit:** if alignment data (§7) doesn't pan out, phonemic
encoding becomes the natural alternative for source-side proper-noun
matching.

### 5.4 Quintile bucketing → consider rolling-window median

The owner asked what "rolling window median" means.

**ELI5:** quintile bucketing is what we do today: sort verses by
length, split into 5 buckets, compute median + MAD per bucket. The
problem is bucket boundaries are arbitrary — a 20-grapheme verse and a
21-grapheme verse might land in different buckets and get judged
against different baselines, even though they're nearly identical in
length.

**Rolling window:** instead of fixed buckets, every observation has its
own cohort. For verse X with length L, define X's cohort as "the N
verses whose lengths are closest to L" (e.g., the 500 nearest). Compute
median + MAD over that cohort. Score X against its own cohort.
- No boundaries → no boundary artifacts.
- Smooth gradient as length changes.
- Cost: more computation. Each verse needs its own median+MAD instead
  of using a precomputed-per-bucket value.

**Decision:** evaluate, don't ship blindly.
- Add a research spike (§9) to compare quintile vs. rolling-window
  median+MAD on the same corpus. See if findings actually change.
- If the difference is meaningful, switch. If not, the quintile
  approach is good enough.

### 5.5 Robust-z threshold of 3.0 → calibrate against eBible per script family

**Verdict:** the universal `z > 3.0` cut is inherited and uncalibrated.
This is exactly where eBible-derived priors (§6.2) earn their keep:
compute, per script family, the z-distribution under known-good
translations, and pick a threshold that flags a low-and-stable rate
(say <2%) on those.

This is a one-time computation per script family, runs offline, and the
result is committed as defaults.

**Effort:** part of §6.2; not separate.

### 5.6 `source_co_rarity` — separating the intuition from the placeholder values

The reviewers called this "theater." That critique applies to the **specific 0.0 / 0.3 / 0.7 placeholder values** — those are uncalibrated and not defensible as confidence numbers.

But the **underlying intuition is real and worth preserving:** if a word appears exactly once in the source NT, the concept it carries is rare, and we'd expect its target-language rendering to be rare in the target NT as well. The user's worked example: if `fox` is a hapax in the English source NT, the target-language word translating `fox` should plausibly also be a hapax in the target NT — and a target-side hapax in *that source-aligned slot* is therefore less surprising than a target-side hapax elsewhere. Conversely, a target-side hapax aligned to a source-side common word is more surprising than the score-as-currently-computed would suggest.

That intuition is *information* the engine can use; it just can't use it via the current placeholder weights without labels or alignment data to calibrate against.

**Action:**
1. **Keep the concept** under a clearer name (e.g. `src.rarity-parity` or similar) — the *direction* of the signal is right.
2. **Drop the unsupported placeholder constants** from Tier-A-eligible output. Until calibration data exists, surface only as a Tier B advisory with explicit "rationale: source hapax aligned to target non-hapax (or vice versa)" rather than a number.
3. **Real values come from one of two sources** — alignment data (Spike A, §7), giving per-source-token rarity, or labels giving relative weights. Either unblocks promoting back to a numeric score; neither exists yet.
4. **If neither lands within the next iteration**, then the placeholder-values version *should* be removed entirely rather than left as an uncalibrated artifact (the user's instinct on this is correct).

The distinction: don't throw out the conceptual signal because the implementation was theater. Throw out the implementation and preserve the conceptual question for the moment data exists to answer it.

### 5.7 Empirical CDF (ECDF) — what it is and why it's interesting

The owner asked for ECDF spelled out.

**ELI5:** the **empirical cumulative distribution function** of a
sample is just "for any value x, what fraction of observations are ≤ x?"
Plot that as a step function and you have the ECDF.

For our purposes: instead of mapping a z-score to a sigmoid in [0, 1]
with a temperature knob, we map the raw value through the ECDF of a
known-good distribution. The result is **percentile rank**: "this verse
is more anomalous than 87% of verses in vetted Bibles of this script
family."

**Why it's better than sigmoid+temperature:**
- No temperature to tune.
- No saturation cap to engineer around.
- Output is directly interpretable ("more anomalous than X% of
  vetted scripture").
- Robust to extreme values (the top 0.1% of vetted Bibles still ranks
  at the 99.9th percentile, not at 1.0 saturated).
**Decision:** ECDF against eBible distributions becomes the calibration target for compression-texture (§6.2). It's the right primitive for turning corpus statistics into a translator-readable confidence number.

> **Translator-facing label format (confirmed):** percentile-rank is the right shape for the human-readable confidence ("more anomalous than 87% of vetted scripture"). That label format ships as part of §5.8's calibration workstream — same data dependency, no extra effort to surface.

### 5.8 Rolling-window median, ECDF, KN smoothing — a single research-spike workstream

These three primitives fit together as the "make probabilistic signals
empirically grounded" pass. They get done together (§9, Spike B).

---

## 6. eBible-derived priors

This is the highest-leverage probabilistic improvement available with
no new data. Both reviewers agreed; the owner's intuition matches.

### 6.1 What we have

`profile_corpora.rs` already computes per-corpus statistics
(verse counts, type counts, hapax ratios, char-trigram-hapax ratios,
script families). The eBible corpus contains 100+ vetted New
Testaments and Bibles.

### 6.2 What we add

A pre-computation pass over the eBible corpus that produces
`calibration.json` containing per-script-family distributions for:

- Verse-level compression-texture ratios (median, MAD, deciles, full
  ECDF for percentile-rank lookup).
- Per-token character n-gram surprisal distribution.
- Verse-length-grapheme distribution (for the length-bucketing /
  rolling-window median work).
- Token-length-grapheme distribution.
- Per-rule "vetted-Bible flag rate" (what fraction of vetted-Bible
  verses each rule flags at the current threshold) — this is the
  sanity check Olly proposed.

Each script family (Latin, Cyrillic, Devanagari, Ethiopic, Arabic,
Hebrew, ...) gets its own profile. Within each family, we may also
stratify by morphological regime (analytic / fusional / agglutinative)
if the within-family variance is large enough to justify it (a question
the spike answers; §9, Spike B).

### 6.3 How rules consume it

> **Sequencing decision (was an open question from user feedback):** the deterministic-core green dots and the probabilistic-theater removal come *before* the eBible calibration spike. Reasons: (a) the calibration target needs a stable set of rules to calibrate against, and that set is what §3 produces; (b) it's much harder to tell which probabilistic signals are theater vs which are merely uncalibrated until you've watched them fail in the wild; (c) the deterministic core already produces shippable findings without calibration, so we get value sooner. M1 → M2 → M4 in §10's sequencing reflects this ordering.

When a project starts up:
1. Detect target script family.
2. Load the matching `calibration.json` profile.
3. Use the eBible distribution as the prior, the project's own
   distribution as the data, and combine appropriately.

For length-conditioned verse-NCD: use the eBible ECDF for the
percentile rank, then check if the project's own per-bucket median
deviates from the eBible expectation enough to suggest the project is
itself anomalous as a whole.

For per-rule flag-rate sanity: if the rule's flag rate on the project
is dramatically higher than the script-family-average eBible flag
rate, that's a calibration signal (probably don't believe the rule
right now) rather than a "this draft is way worse than vetted scripture"
signal.

### 6.4 Honest caveats

- Some eBible translations have errors. The aggregate is still useful
  with noise; we are not pretending the eBible corpus is ground truth.
- Cross-language transfer is limited. Latin-script vetted Bibles tell
  you about Latin-script texture, not about Bemba's typo distribution.
  We stratify by script family and morphological regime to get as close
  as we can.
- Rules with semantic placeholders (`source_co_rarity`'s 0.0/0.3/0.7)
  cannot be calibrated this way. ECDF doesn't fix semantic placeholders
  — only labels or alignment data do.

### 6.5 Effort

~3-4 engineering days to implement the eBible pass and write the
loader. Plus the research spike (§9, Spike B) to validate that the
within-script-family variance is small enough for transfer to be
meaningful.

---

## 7. Alignment data — research spike, not commitment

The reviewers split on alignment:
- Olly: alignment is the highest-leverage data → consume Translation
  Core USFM 3 / Serval REST.
- Jim: alignment is medium leverage; word list is highest.

The owner's read: alignment might be useful, but we don't know yet
whether existing tools (SIL Machine, Serval) produce alignment that
is (a) fast enough for an iterative UI loop, (b) stable enough across
re-runs, (c) ingestable as files (not as a runtime dependency).

**Decision:** research spike (§9, Spike A).

The spike answers:
1. Can SIL Machine / Serval produce verse-aligned output for one
   target/source pair in <60 seconds? Word-aligned in <5 minutes?
2. Is the output stable across reruns (or does it shift on every
   pass, breaking labelling persistence)?
3. Can we ingest the output as files without depending on Serval's
   live REST API?
4. Does the alignment quality on a real minority-language NT match
   what the marketing materials suggest?
5. If alignment is usable: what specific signals does it unlock?
   - tighter `source_co_rarity` (BK on aligned source token, not
     whole verse)
   - term consistency
   - word-omission detection

If the spike says "yes to all, alignment is fast and stable," the
plan amends to integrate it. If the spike says "no, alignment is slow
or unstable or impossible to consume as files," we don't.

**Per the composability principle (§1.1)**, we will not adopt SIL
Machine or Serval as a runtime dependency even if the spike succeeds.
The integration must be file-based: project produces aligned-output
JSON, our engine consumes it. If that shape isn't possible, we don't
integrate.

---

## 8. Word list — first proper-elicitation surface

Both reviewers ranked word list as the highest-leverage data
collection. The owner's instinct agrees but with a sharp question:
"if I show 100 words and ask 'which aren't real,' what does that
actually buy us, and is the rare-word-triage machinery the right
sorter?"

### 8.1 What a word list buys

A confirmed `known_good` form short-circuits all three rare-word
factors for that form:
- `char_anomaly`: ignored — translator says it's real.
- `char_ngram_backoff`: ignored — same.
- `source_co_rarity`: ignored — same.

A confirmed `known_bad` form gets surfaced at very high suspicion as
a Tier A finding (it's now a definite typo, not a probabilistic
guess).

`LabelledLemmaIndex` in `analysis/lemma_feedback.rs` is already built
for this. The missing piece is the elicitation surface.

### 8.2 What it does NOT buy

- The translator marking 100 words says nothing about character bigram
  validity. It's a per-form claim, not a per-substring claim.
- It doesn't help calibrate threshold-style rules (NCD, source
  proportionality).
- It doesn't help on the long tail of words the translator hasn't
  reviewed.

So the word list is a sharp, narrow input. Useful, but bounded.

### 8.3 The "use the rare-word triage to sort the word list" question

The owner's hunch: maybe rare-word triage's per-token suspicion is
useful as a sort order for the elicitation list — "review these 100
forms in suspicion-descending order so the most-likely-to-be-typos
surface first."

**Verdict:** yes, this is reasonable, *but* with caveats.
- The per-token suspicion is uncalibrated (§1.3), so the absolute
  ranking is noisy.
- The sort order is still better than random — high-suspicion forms
  are at least disproportionately likely to be either real typos or
  rare proper nouns, which is exactly what we want a translator's
  attention on.
- The downside: if the translator marks a high-suspicion form as
  `known_good`, it's a strong correction signal we can use to lower
  the rule's confidence; if they mark it `known_bad`, the rule's
  output is validated. Either way the labels feed back usefully.

**So:** sort the word list by current per-token suspicion (descending),
default-assume-good, click-to-mark-bad. This is the v1 elicitation
surface.

### 8.4 UX sketch

> **Sequencing note (confirmed):** v1 does *not* build the elicitation UI. The word-list surface comes *after* the deterministic core's orthography / script / punctuation / proportionality rules are solid. The markdown-form path below is the v1+1 shape; recording it here so we don't re-derive when the time comes. M3 in the §10 sequencing table reflects this — it's after M1 (deterministic core).

```
sous review-words <corpus-dir>

Reviewing top 100 rare forms in <corpus> sorted by current
per-token suspicion. Default for every form is "real word." Click
or press 'n' to mark a form as not-a-real-word.

  1. davidi      [n=2]  (suspicion 0.96)  [real]  [not-a-word]  [skip]
  2. abrahaman   [n=1]  (suspicion 0.94)  [real]  [not-a-word]  [skip]
  3. yesu        [n=482] (suspicion 0.10) [real]  [not-a-word]  [skip]
  ...

Progress: 0/100   Time elapsed: 0:00
```

Implementation: TUI (we already have `crossterm`-style infra options
in Rust) or a flat markdown form the translator fills in and we
re-ingest. The TUI is nicer; the markdown-form version is faster to
ship and maintains the composability principle (no organizational
sign-on, edit a file, save).
**v1 decision:** ship the markdown-form version first. TUI is a follow-up. And per §8.4's sequencing note, even the markdown-form version is gated on the deterministic core being solid first.

```
# .sous/review-list.md

> Default: every form below is assumed to be a real word. To mark a
> form as not-a-real-word, change `[ok]` to `[bad]`. To skip, change
> to `[skip]`. Save the file when done; sous will pick up the
> labels.

- [ok]  davidi      (n=2,   suspicion 0.96)
- [ok]  abrahaman   (n=1,   suspicion 0.94)
- [ok]  yesu        (n=482, suspicion 0.10)
- [ok]  ...
```

Persistence: each `[bad]` mark writes a `lemma_family_reject` event to
`events.jsonl`. Each `[ok]` after explicit edit writes a
`lemma_family_confirm` event. (Auto-`[ok]` because it's the default
does NOT write an event — only explicit user attention does.)

### 8.5 Effort

~2 days for the markdown-form ingestion path including events.jsonl
plumbing and a regenerate-the-file command.

---

## 9. Research spikes (timeboxed; do not bleed into committed work)

| Spike                                    | Question                                                                                                                                                                                                                             | Time-box             |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------- |
| **A. Alignment integration**             | Can SIL Machine / Serval produce stable, ingestable alignment fast enough for a UI loop? Is the output usable as files?                                                                                                              | 3 days               |
| **B. eBible calibration**                | Implement the eBible pre-computation pass. Validate within-script-family variance is small enough for transfer. Compare quintile vs. rolling-window median. Land per-script-family `calibration.json`.                               | 5 days               |
| **C. Layer 1 vs. consultant-checked NT** | Run the deterministic core (post-§3 build-out) on a single completed NT that has been through consultant checking. Document each flag the consultant did NOT catch. This is the "does the deterministic core have value" experiment. | 1 day after §3 ships |

**Spike A is gated**: do it before any code change to `source_co_rarity`
or before any new alignment-dependent rule. Outcome of spike determines
whether alignment becomes a v1 feature.

**Spike B can run in parallel with §3**: it's about producing
calibration data, not about rule logic. Once `calibration.json` lands,
the deterministic-core rules' threshold defaults can be replaced with
calibrated values.

**Spike C is the validation experiment**: does the deterministic core
catch real errors that informal review misses?

---

## 10. Sequencing — the next 3-6 weeks

| Milestone                                           | Items                                                                                                                                 | Estimate |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| **M0: Rule registry + ignore-list scaffold** (§3.4) | `rules.json` schema + per-rule toggling plumbing + ignore-list infrastructure. Lands before M1 rules so they can register cleanly.    | ~1 day   |
| **M1: Deterministic core complete**                 | §3.2.1 + §3.2.2 + §3.2.3 + §3.2.4 + worked-example docs for all Tier A rules                                                          | ~8 days  |
| **M2: Probabilistic demotion landed**               | Noisy-OR → max in `rare_words.rs`. Laplace → Kneser-Ney in `char_ngrams.rs`. Output split into Tier A findings and Tier B advisories. | ~3 days  |
| **M4: eBible calibration spike** (Spike B)          | Pre-compute, land `calibration.json`, switch verse-NCD threshold to calibrated value. *Runs after M1+M2 land per §6.3 sequencing.*    | ~5 days  |
| **M5: Layer 1 vs. consultant-checked NT** (Spike C) | Run on a vetted NT, document gap                                                                                                      | ~1 day   |
| **M3: Word-list elicitation v1**                    | Markdown-form review surface (§8.4) + events.jsonl integration. *Gated on deterministic core being solid; runs after M5.*             | ~2 days  |
| **M6: Alignment spike** (Spike A)                   | Evaluate SIL Machine / Serval; decide go/no-go on alignment-dependent features                                                        | ~3 days  |

**Total committed work:** ~20-24 working days for M0-M5 (M3 reordered to follow M5 per the user-feedback sequencing decision in §6.3 / §8.4).
**M6 is exploratory** and may not feed into v1.

After M5, decide:
- If the deterministic core + word-list + eBible-calibrated NCD
  catches enough that consultants find useful, ship a v1 alpha.
- If not, M6 (alignment spike) becomes critical for the next
  iteration.

---

## 11. ADRs to write

1. **ADR 0009 — Composability over ecosystem integration.** The
   architectural commitment that this project does not depend on
   Paratext, Translation Core, ClearML, or other ecosystem
   infrastructure as a runtime dependency. File-based integration
   only; CLI primary surface. References Wycliffe Associates'
   non-GBTC checking philosophy as the contextual reason.
2. **ADR 0010 — Two-tier surfacing: deterministic findings vs.
   probabilistic advisories.** The split between Tier A (surfaced
   as findings) and Tier B (surfaced as advisories with caveats)
   per §2.
3. **ADR 0011 — Probabilistic chassis demotion.** Document the
   decision to demote Noisy-OR to `max()` and to park the Bayesian
   sub-cluster routing indefinitely. Reverse condition: ≥200 labels
   per rule across ≥3 projects with evidence sub-clustering changes
   behavior.
4. **ADR 0012 — eBible-derived priors via ECDF.** The decision to
   calibrate Tier B thresholds against eBible-distributions rather
   than hand-tuned constants. Stratification by script family;
   re-evaluate stratification by morphological regime after Spike B.

---

## 12. Open questions left (genuinely)

1. **Alignment go / no-go.** Depends on Spike A.
2. **Phonemic encoding revisit.** If alignment doesn't pan out and
   `source_co_rarity` is still a priority, phonemic encoding becomes
   the alternative. Which encoder for which script family is open.
3. **Threshold values from eBible.** Specific numbers come out of
   Spike B; can't pre-commit.
4. **Where the elicitation TUI lives.** v1 is markdown-form. v2 might
   be an embedded TUI. v3 might be a small local web UI. The
   composability principle points toward "small local web UI" being
   the long-term path because translators don't want to live in CLIs;
   but a local web UI is its own can of dependency worms. Defer the
   decision.
5. **Morfessor revisit timing.** Punted indefinitely; revisit if a
   specific use case ever forces it.
6. **Narrow additive co-firing rules** (Q9 in `tools.md`). Max-of-evidence is the v1 combiner, but the tomb/tome and Mary/Mark cases show real co-firings where two independent signals should add. Open question: which specific signal pairs do we whitelist as additive, and how is that list represented in config? Lives in `tools.md` for now; revisit when the proper-noun-aligned variant-identity rule (Q4) ships and we can see what pairings actually co-occur.
7. **Word-scale compression vs char-n-gram-KN** (Q3 in `tools.md`). The user's intuition is that compression naturally captures multi-gram patterns and shouldn't be demoted at word scale without testing. Open: run a head-to-head on the current corpus before either becomes the default. Cheap to run; just hasn't been done.
8. **Test-case catalog format** (referenced in §3.4). The symptom list in `conrete-examples-by-cat.md` is the seed for ~150 hygiene test cases. *Working answer for v1:* per-rule fixture directories at `tests/fixtures/<rule_id>/<case_name>/` with `input.usfm` + `expected.jsonl`. Revisit if/when the count exceeds what's manageable as individual directories.

---

## 13. What this plan deliberately does NOT do

- Does not pivot to a Paratext plugin (§1.1).
- Does not adopt Serval / SIL Machine / Translation Core / ClearML as
  runtime dependencies. File-based integration only, gated on Spike A.
- Does not invest more in the Bayesian sub-cluster chassis until label
  volume demands it.
- Does not add a Morfessor-attested-morpheme Noisy-OR factor.
- Does not introduce phonemic encoding in v1.
- Does not implement a profile.yaml schema; deferred indefinitely.
- Does not abandon the probabilistic work. The work is correctly
  framed as advisory until calibrated, not torn out.

---

## 14. The honest one-line summary

We over-built the probabilistic chassis for our actual data scale,
under-built the deterministic core that was sitting right in front of
us, and wrapped both in a UX framing ("findings") that doesn't
distinguish high-confidence boolean checks from speculative
distributional guesses. This plan ships the deterministic core
properly, demotes the probabilistic stack to "advisory" until eBible
priors give it empirical grounding, and treats alignment / phonemics /
Bayesian sub-clustering as open spike questions rather than committed
features.
