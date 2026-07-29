# Idea — the ranked road ahead (condensed)

Originally 2026-07-11, written at the close of the event-stream-port arc
(ADRs 0057–0059); condensed 2026-07-20 to what is still live. Since the
original take: the port branch merged; the anchor cache landed (ADR 0060,
later `PrepCache`); finding addresses re-keyed (ADR 0061); the resident
`Galley` landed and merged to dev (ADR 0062 — warm whole-corpus re-analyze
5.2–18.9 ms, cold seed ~257 ms on en_ulb); mixed-normalization shipped
default-off (ADR 0063); the word-break fast path landed (ADR 0064); and the
findings wire format was measured (packed buffer 20–160× faster end-to-end —
`calibration/2026-07-18-findings-wire-format-survey.md`).

## Standing disciplines (unchanged, load-bearing)

1. Every new signal must state its **self-gating story** (how it goes silent
   when the corpus can't support it).
2. The **verse invariant** (repo CLAUDE.md) needed enforcement repeatedly,
   including against evidence-quality reasoning. Treat proposals that touch
   verse seams with suspicion by default.
3. The known ceiling: corpus-internal statistics cannot see **consistent
   errors** and cannot resolve the mid-band. The census covers the
   human-scannable slice; genuinely new catches live in the source-paired
   tier and (much later) alignment — not in more corpus-internal rules.

## Live priorities

1. **Close the loop to users: editor adoption of the resident `Galley`.**
   The editor (scripture-editor-proto-2) still runs the stateless one-shot
   surface; adopting the Galley verbs is what makes findings/census real for
   translators. With it comes a **Galley-owned ignore/suppression layer**
   (see `committed/2026-07-09-per-mark-finding-suppression.md`) so callers
   don't each build their own hide-this store. (Bayesian adjudication
   *labels* are a separate, far-future TBD — suppression is not labels, and
   no label-collection machinery is planned now.)
2. **Run the preset-derivation truncation experiment**
   ([plan](../plans/2026-07-09-preset-derivation-plan.md) — the one open
   plan; the config recommender is folded into it). The calibrated rules
   wait on it for their conservative/normal/aggressive rows; it's what makes
   the one-knob product story real.
3. **Packed binary findings wire format** — triage complete 2026-07-21:
   committed plan at `../plans/2026-07-21-packed-findings-wire-plan.md`
   (wire-level diff/tombstones rejected there; interning spun off as
   `2026-07-21-grapheme-interning-enabler.md`; chapter-granularity
   invalidation proposed as
   `2026-07-21-chapter-granularity-invalidation.md`).
4. **Cheap adds on the settled substrate:** compression texture and the
   source-paired tier (`2026-07-07-next-checks-shortlist.md` items 1–3);
   the small census batch (both-forms examples — committed; quotes lane —
   open; site-cap policy — parked).
5. **Boundary-trust unification** (design pass → ADR):
   `committed/2026-07-11-boundary-trust-substrate.md`.

## Extracted 2026-07-29 (backlog reorganization)

- The "heresy worth debating" (fleet-derived presentation prior for
  statistical cold-start) → `candidates/2026-07-29-cold-start-problem.md`.
- wasm parallelism → `rejected/2026-07-29-macro-rejections.md`.
- Census case-variants lane size → `discussing/2026-07-29-census-workstream.md`.

## Risks / debt worth naming

- **Playground is unversioned** (survey baselines, samply runner,
  extract-profile.mjs live outside git). Cheap to fix; embarrassing to lose.

## The point of all of it (owner note, 2026-07-29)

Everything above serves one goal: get this engine into an editor in front of
a translator. That integration is the roadmap now.
