# Idea — census site-cap policy (flat first-per-book cap vs count threshold)

Date: 2026-07-20 (extracted from the finding-address-representation idea doc
when that doc was deleted after ADR 0061 landed; this is its one thread that
stayed open). Status: **open — parked behind a real need.**

## The question

The census caps example sites per row (`CensusOptions.example_cap`, default 8),
and the cap is *first-per-book* — at most one site per book per row. For a
"comprehensive long-tail triage" tool the natural ask is *every* occurrence of
the rare rows. The candidate policy: a **count threshold** — store *all* sites
for any row with total count ≤ K, sample above K. Mechanically that needs the
`Firsts<K>` collector to become a `Vec` for below-threshold rows, storing the
location-only packed site record (the 6-byte `SiteAddr` from ADR 0061 makes
each site cheap).

What it would buy:

- an exhaustive long tail (rare rows show every site, not ≤1 per book);
- an **exact** census↔rules overlay (see
  `2026-07-14-census-vs-rules-overlay.md` — the sampled overlay is computed
  over ≤8 examples per row today).

What it deliberately avoids: eager storage of *all* sites for *all* rows —
measured at ~31 MB/corpus (~4.5–4.7M sites on en_ulb), ~90% common letters
nobody expands. The cap policy is the lever, not the site encoding.

## Current adjudication (2026-07-14, both-forms thread)

**Do not build the count-threshold machinery yet.** The flat first-per-book
cap already ≈ store-all for rare rows *spread across books*; the only gap is a
rare row *concentrated in one book* (first-per-book keeps 1 site), which triage
hasn't needed. A cap-default bump (above 8, value TBD) plus both-forms tagged
examples covers the known asks. Revisit store-all only if that
concentrated-in-one-book gap bites, or if the overlay is promoted from
"sampled is good enough" to "must be exact."

An alternative shape, if revisited: a census **site-enumeration entrypoint**
(return every `(address, span)` for a given row/lane on demand) instead of
storing everything up front — same enabling power for the overlay, no
retention cost on every census call.

## Relates to

- ADR 0058 (census; `example_cap`), ADR 0061 (6-byte `SiteAddr`).
- `committed/2026-07-14-census-both-forms-mark-examples.md` (the cap-bump +
  keep-it-flat adjudication lives there).
- `2026-07-14-census-vs-rules-overlay.md` (exact overlay is the main customer).
