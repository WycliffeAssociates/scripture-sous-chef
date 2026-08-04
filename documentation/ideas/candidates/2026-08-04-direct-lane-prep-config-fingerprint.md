# Candidate — narrow the direct lane's prep invalidation to its enabled set

- Date: 2026-08-04
- Status: candidate only; found while landing the chapter-outer scheduler and
  deliberately **not** changed there (owner ruling: record it, do not touch
  behavior in the nonletter-usage epic)
- Context: [`crates/core/src/cache.rs`](../../../crates/core/src/cache.rs)
  `PrepSection::ensure_fingerprint`, ADR 0067, and the two tests whose doc
  comments record the finding —
  `phase_f_tests::a_judging_only_change_maps_and_reduces_nothing` and
  `phase_f_tests::enabling_one_rule_maps_only_its_own_substrate` in
  [`crates/core/src/lib.rs`](../../../crates/core/src/lib.rs)

## What

The direct per-verse lane's cached chapter products are keyed by the **whole
config fingerprint**:

```rust
fn ensure_fingerprint(&mut self, config: &Config) {
    let fingerprint = config_fingerprint(config);
    if self.fingerprint != Some(fingerprint) {
        self.clear();
        self.fingerprint = Some(fingerprint);
    }
}
```

So **any** config movement clears the lane and re-maps every chapter of every
book — including a movement that cannot possibly change a per-verse rule's
records, such as a purely judging-only knob on an unrelated corpus-relative
rule, or a Review Depth position.

Every typed observation substrate is narrower than this by construction: its
`ObservationInputStamp` folds only its own schema and its own *extraction* config,
so a judging-knob change reuses every observation and maps zero chapters (ADR
0067's central property). The direct lane is the one participant that does not
get that property, and the chapter-outer scheduler made the asymmetry visible
because both lanes are now planned side by side in one pass.

## Why it might matter

The lane is not cheap: it is the sixth reader of the chapter tape and it runs
every enabled per-verse rule over every verse. On a resident `Galley` the
scenario that stings is the editor's Review Depth slider — a control the user is
expected to drag. Each position resolves to a different `Config`, so today each
drag step re-maps the entire direct lane for a set of rules whose output cannot
have moved.

It is parked rather than scheduled because no measurement has been taken yet, and
because the current behavior is *sound* — a per-verse rule's records are a
function of the enabled per-verse set, and clearing on any config change is a
conservative superset of that.

## The shape a fix would take

The honest invalidation key is the **enabled per-verse rule set** (plus whatever
extraction-only config a per-verse rule ever gains), not the whole `Config`:

- the records a chapter's product holds are exactly `per_verse_rules()` filtered
  by `config.is_enabled`, so two configs agreeing on that filtered set agree on
  the product;
- disabling a per-verse rule could then drop that rule's records rather than the
  whole lane, though a whole-lane clear on a per-verse-set change is already a
  large improvement and much simpler;
- the finding lane's own committed stamps stay exactly as they are — this is
  about the prep product's validity, not about what gets patched.

## Evidence required before promotion

- Measure the real cost: criterion `changed_edit_` / `incremental_edit_` with a
  judging-only config move between calls, and the same on a Review Depth step,
  against the current whole-fingerprint behavior.
- Prove equivalence the same way the substrates do: a config-only change must
  reach the byte-identical complete snapshot with the lane reused, and the
  existing fault/retry cases must still hold.
- Confirm no per-verse rule reads any config beyond its enabled bit today (a
  future one that did would need its own extraction fingerprint, exactly like a
  substrate's `extractor_fp`).

## Non-goals

- Widening the finding lane's partition stamps, which are a separate concern.
- Giving per-verse rules a sensitivity/Review Depth surface. They are
  deterministic by design (ADR 0070 keeps them fixed); this is purely about not
  discarding a valid cached product.
