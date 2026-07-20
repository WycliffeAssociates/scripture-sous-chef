# Documentation map

Where things live, and which folder a new doc belongs in. Nothing floats at
this level except this index — every doc has a home below.

## The stable core (read these to understand the system)

- **[`overview/`](overview/)** — the big-picture narrative and the math.
  - `vision.md` — what sous is and why (the product/architecture north star)
  - `methods.md` — the signal/statistics specification
  - `v1-reset-design.md` — the pure-analyzer contract and graduation order
  - `worked_examples.md` — end-to-end walkthroughs when the notation stops helping
- **[`reference/`](reference/)** — exact surfaces you look things up in.
  - `config.md` — the config surface, knobs, scoring/aggregation model
  - `outputs.md` — output file paths and shapes
  - `posterior_feedback_plumbing.md` — the events.jsonl → posterior replay model
- **[`rules/`](rules/)** — per-rule reference, one file per id namespace family.
  Start at [`rules/README.md`](rules/README.md) for the full rule index.

## The decision & work logs (accreting; dated)

- **[`adrs/`](adrs/)** — Architecture Decision Records. Immutable once
  accepted; superseded by writing a new one. [`adrs/README.md`](adrs/README.md)
  is the index — its **Status** column tells you at a glance which decisions
  are live vs. superseded/amended. **This is the compaction** — don't rewrite
  ADR bodies; the index is how a reader skips the dead ones.
- **[`calibration/`](calibration/)** — dated write-ups of spikes and fleet
  measurements (the numbers behind a decision). Runnable one-off spike *code*
  lives in `../spike-bench/`, not here.
- **[`plans/`](plans/)** — fully-scoped implementation plans, dated.
- **[`ideas/`](ideas/)** — pre-plan proposals. Direction of travel:
  raw idea → `ideas/committed/` (blessed, unscoped) → `plans/` → build → ADR.
  Off-ramps: `ideas/rejected/`, `ideas/doubtful/`. See
  [`ideas/README.md`](ideas/README.md).
- **[`tmp/`](tmp/)** — durable-but-disposable scratch and cross-agent/tool
  handoffs. Delete once the lasting record has landed in an ADR/rule/calibration.

## Where does a new doc go?

- A decision a future reader would second-guess → **`adrs/`**
- Measured numbers from a spike/survey → **`calibration/`** (code → `spike-bench/`)
- A scoped build plan → **`plans/`**; a rough proposal → **`ideas/`**
- How a rule behaves → **`rules/`**; a knob/output/format fact → **`reference/`**
- Narrative on what/why the system is → **`overview/`**
- In-flight progress / a handoff note → **`tmp/`**
