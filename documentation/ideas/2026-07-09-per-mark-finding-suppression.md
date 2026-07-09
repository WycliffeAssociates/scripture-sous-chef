# Idea — suppressing a mark's findings: attention-filter vs population-exemption

**What.** A way to stop `punct.spacing-anomaly` (and its kin) from surfacing a
mark a translator has judged acceptable — the motivating case being a corpus
that attaches its danda `।` ~77% of the time, so the ~23% *spaced* dandas all
flag, even though the translator knows spaced danda is legitimate house style
here. The insight is that "suppress this" hides **two different intents** that
want **two different mechanisms**, and the tell for which one you're in is
whether the finding's `num/denom` (e.g. "20658 of 26802, 77.076%") *should*
change when you suppress:

- **Attention filter** — "I don't want to *look* at danda spacing." The corpus
  fact (danda is 77% attached, with a real spaced minority) is still true; you
  just don't want it on screen. The num/denom should **not** change — hiding
  findings you've chosen to ignore shouldn't rewrite a truthful measurement.
  Mechanism: an **app-side per-`(rule, mark)` filter** (`hide where code ==
  "punct.spacing-anomaly" && mark == "।"`). No engine change; the emitted stats
  stay an honest record; reversible. This is what most linters do
  (`eslint-disable` a rule for X).

- **Population exemption** — "danda spacing isn't something this corpus should
  be judged on at all; it's a free variant here." A claim about the *data*, not
  attention. The num/denom **should** change: recompute the spacing verdicts
  with danda excluded from the opportunity set entirely. Mechanism: a
  **per-corpus config exemption** (`sous.json`: don't judge `।` spacing).
  Tradeoff to accept: you also stop catching a genuinely *broken* danda spacing
  — fine, if it's truly free-variant.

**Why.** The corpus-relative rules are pure statistics; at 77/23 the finding is
*correct* — the corpus really does attach 3:1. A translator saying "both forms
are fine" is overriding the data with outside editorial knowledge, which the
stats can't and shouldn't self-correct into (more data won't help; it's not
trending to 50/50). So the override has to live *somewhere external*, and the
two intents above are the two honest homes for it. Global `emit_score_min` is
the wrong shape: raising it past danda's 0.77 silences every mark under 0.77
everywhere — a global dial, not a per-mark decision.

**Note on ADR 0029.** A per-corpus config exemption does **not** re-violate
[ADR 0029](../adrs/0029-punctuation-spacing-corpus-relative.md). What 0029
killed was a *hardcoded, universal, engine-baked* allow-list ("the engine
believes `.,` is fine everywhere"). A per-corpus config toggle is the opposite
— the user's editorial input for their own corpus, which is exactly what config
is for. (This corrects an over-absolute "app-side only, never engine" claim
from the conversation that seeded this note.)

**Open questions for the conversation.** Likely both mechanisms eventually,
since they serve different intents — but start with the app-side filter (cheap,
reversible, keeps the stats truthful, covers the common "I've reviewed these,
hide them" case) and add the config exemption only when a real corpus needs the
recompute semantics. Where the filter state lives (per-user? per-project?
persisted where?); whether config exemptions should be per-`(rule, mark)` or
coarser (per rule, per mark across rules); how either surfaces the *count* of
what it hid so suppression never reads as "clean" when it isn't; and whether a
muted mark still contributes to the corpus stats it's part of (it should — the
measurement is true regardless of what you choose to see).
