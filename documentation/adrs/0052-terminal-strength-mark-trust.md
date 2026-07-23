# ADR 0052: `terminal_strength` — learned mark trust gates casing's positional flagging, weights its censoring discount

- **Date:** 2026-07-10
- **Status:** Accepted
- **Builds on:** [ADR 0051](0051-casing-two-factor-word-lexicon.md) (which
  reserved this exact consumer contract), the shortlist 2/3 spec
  (two witnesses, noisy-OR, Zipf-gated jurors, anti-circularity — spec text in
  the pre-2026-07-20 git history of
  `documentation/ideas/2026-07-07-next-checks-shortlist.md`; that doc was
  since condensed to live items, this ADR being the record), and the labs
  `association.rs` G²/Fisher machinery (ported; see below).

## Context

Casing v2 (ADR 0051) shipped taking the terminal-glyph inventory at face
value: every candidate mark fully trusted, every quote-adjacent boundary
(`."`, `:"`) unpoliceable by fiat (those sites fall to mid-flow), and every
capital after a quote-context boundary counted as mid-flow lexicon evidence —
a known contamination. The spike (committed `42eae49`, `c8508fc`; full
methodology and tables in the
[2026-07-10 calibration doc](../calibration/2026-07-10-terminal-strength-spike.md))
built the learned replacement and measured it wired into casing v2 on all
1,504 vref corpora.

## Decision

**Per-corpus, per boundary class** — where a class is a mark *or* a
mark-plus-close-quote context (`.` and `."` are separate classes, each earning
its own trust) — compute:

```
trust(class) = 1 − (1 − s_case)(1 − s_reshuffle)        # noisy-OR
```

- `s_case` — the case-follow witness (bicameral only): capitalize rate of
  lexicon-lowercase words after the class, Wilson-shrunk. This is ADR 0051's
  lexicon-restricted habit, reused.
- `s_reshuffle` — the word-reshuffle witness (case-free): do the class's
  following words differ from the corpus baseline (Dunning G² fast path,
  Fisher's exact on sparse tables, jurors = words seen ≥10×), **guarded by
  agreement**: the differentness signal is multiplied by the aftermath's
  total-variation agreement with the corpus's reference terminal aftermath.
- A witness that cannot see (caseless script; too few jurors; class below the
  minimum event count) contributes 0 to the OR — absent, not a veto.

**The load-bearing negative result:** raw differentness cannot rank marks —
the comma's standardized G² deviate (median 302) is the same magnitude as the
period's (401), because list separators reshape their neighborhoods too, just
not boundary-shaped. All discriminating power lives in the agreement guard
(fleet medians under it: `.` 1.00, `?` 0.97, `!` 0.94, `:` 0.55 split, `,`
0.30, `;` 0.07, quotes/hyphens ≈ 0 — every acceptance anchor met, commas low
everywhere). This kills the labs premise that a G²→[0,1] sigmoid needed
refitting: no sigmoid at any scale separates terminals from separators.

**Consumption by casing — verdicts gate, evidence weighs:**

- **Flagging (gate):** a positional site is scored — with the *unchanged*
  ADR 0051 `habit × rarity` — only if `trust(site's class) ≥ 0.90`; below the
  gate the site is not scored at all. Trust never multiplies into the score:
  three honest ~0.97 factors would compound a confident finding below the
  floor (measured: multiplicative wiring eroded 373 genuine-mark findings,
  including French `!` sites at trust 0.990). The frozen `T = 0.90` sits in a
  measured plateau — surfaced totals are **identical (4,005) at every
  T ∈ [0.50, 0.95]**, only 14 sites fleet-wide ever change sides — and is
  deliberately below the 0.95 emit floor so the two constants cannot be
  conflated.
- **Learning (weight):** the censoring discount becomes `trust × habit` — a
  capital after a distrusted mark is not position-explained and re-enters the
  word's profile. Here trust genuinely is a proportional question, so it
  multiplies.

**Frozen constants:** `trust_gate = 0.90` (config knob on `CasingConfig`);
witness internals are documented constants, not knobs (juror Zipf gate ≥ 10,
minimum class events 30, agreement = TV-normalized against the reference
terminal, Wilson z shared with the rule).

## Rationale

Fleet effect of the gate wiring at frozen ADR 0051 knobs, vs the shipped
3,547: **4,005 (+458)**, decomposed as **519** newly-policeable quote-context
sites (`."` median trust 0.99 — parametric review of samples found real draft
errors: Spanish quote-openings, Swahili quotative frames; one new FP mode:
quoted *fragments* after `:"`, lowercase legitimate) plus **373** readmitted
erosion victims (only 3 in major-language corpora; the fraLSG `disent-ils`
French-continuation FP is knowingly readmitted — a documented FP class beats
suppression by arithmetic luck). All 7 adjudicated true-positive anchors and
all 5 false-positive anchors hold at every swept T. 55–61 tiny NT-only
corpora (base positional ≤ 3) lose positional coverage entirely — mixed
evidence, correct silence.

Honest limits, accepted:

- **Within-mark polysemy is not solved.** A bare list-colon and a bare
  speech-colon are the same character and the same class; the Indonesian/
  Dutch list-colon FPs die by floor margin, not by trust (those corpora
  genuinely capitalize after most colons — the colon carries *high* trust
  there). Counting cannot split what punctuation does not distinguish;
  inventory-mode territory.
- **Caseless scripts:** W2 alone cannot confidently validate a terminal
  (cmn `。` trust 0.448). Harmless for casing — it self-silences on caseless
  corpora — but a future caseless consumer of `terminal_strength` must not
  assume validated classes exist.
- **Leading-apostrophe orthographies** (glottal-stop `'` as a letter) create
  quote-lookalike classes; handled by design — such a class earns trust only
  from the corpus's own boundary behavior — but reviewers should expect
  promoted findings there.

## Consequences

- The casing walk records the boundary *class* (mark + adjacent close-quote),
  not just the glyph; per-word forced tallies key by class. Stats stay raw
  per-book tallies, mergeable; trust, the gate, and the discount are
  judge-time arithmetic over the merged table (ADR 0051's discipline). Wire
  schema changes → wasm regeneration.
- W2 additionally needs per-class following-word (juror) counts and the
  baseline word-start distribution — the second word-level aggregate; same
  size-design attention as ADR 0051 (raw counts per book, prune what judge
  never reads).
- `dev/association.rs` (ported labs G² + Fisher, textbook fixtures) graduates
  to a production analysis module.
- `terminal_strength` becomes shared substrate: future positional rules and
  the planned inventory mode (a per-mark trust table is a natural report
  page) read the same numbers.
- The casing rules remain default-off; the gate does not change that
  calculus.

Sweep tables, witness distributions, readmission samples, and reproduce
commands: [2026-07-10 calibration doc](../calibration/2026-07-10-terminal-strength-spike.md)
(§7 gate-threshold sweep).
