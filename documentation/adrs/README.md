# Architecture Decision Records

Short records of non-obvious decisions: what we chose, why, what we
considered, and what it forecloses. Each ADR is dated and immutable
once accepted; changes happen by writing a new ADR that supersedes
the old one (link both directions).

ADRs are for decisions a future reader (you, in six months) would
reasonably second-guess without the context that made the decision
obvious at the time. Don't write an ADR for "use Rust" or "use the
existing BK-tree module" — those are derivable from the codebase.
Do write one for "Noisy-OR factors stay plain in Phase A even though
the architecture supports sub-cluster routing," because the choice
isn't visible from the code alone.

## Index

| #    | Date       | Title                                                              | Status   |
| ---- | ---------- | ------------------------------------------------------------------ | -------- |
| 0001 | 2026-05-07 | [Lane separation: per-token, verse, family](0001-lane-separation.md) | Accepted |
| 0002 | 2026-05-07 | [Phase A keeps factors plain; sub-clusters deferred](0002-plain-factors-phase-a.md) | Accepted |
| 0003 | 2026-05-07 | [Source co-rarity abstain: drop from product, not 0.7](0003-source-corarity-abstain.md) | Accepted |
| 0004 | 2026-05-07 | [char_ngram_backoff: one factor, bigram+trigram, no 4-grams](0004-ngram-backoff-one-factor.md) | Accepted |
| 0005 | 2026-05-07 | [Verse-NCD source mirror: arithmetic subtraction](0005-ncd-source-mirror-subtraction.md) | Accepted |
| 0006 | 2026-05-07 | [Verse length bucketing: graphemes, empirical quintiles](0006-verse-length-quintiles.md) | Accepted |
| 0007 | 2026-05-07 | [Source proper-noun match via BK-tree edit-distance](0007-source-proper-noun-bktree.md) | Accepted |
| 0008 | 2026-05-07 | [Multi-provenance surfacing: one verse entry, lanes in metadata](0008-multi-provenance-surfacing.md) | Accepted |
| 0009 | 2026-05-12 | [Delegate per-character script identity to `unicode-script`](0009-unicode-script-crate.md) | Accepted |
| 0010 | 2026-06-02 | [Reset master to a pure, addressable analyzer contract](0010-pure-analyzer-contract-v1-reset.md) | Accepted |
| 0011 | 2026-06-08 | [Statefulness, incrementality, and the consumer boundary for stateful rules](0011-statefulness-incrementality-strategy.md) | Accepted |
| 0012 | 2026-06-09 | [`RuleId` is a closed enum — the typed config & localization surface](0012-ruleid-closed-enum-config-surface.md) | Accepted |
| 0013 | 2026-06-09 | [Proportionality — the first cross-map rule, and the contract surface it grows](0013-proportionality-first-cross-map-rule.md) | Accepted |
| 0014 | 2026-06-09 | [The deterministic rule batch — tokenizer, eleven rules, and shipped defaults](0014-deterministic-rule-batch.md) | Accepted (punct.repeated-punct amended by 0024; hyg joiner allow-list removed by 0025) |
| 0015 | 2026-06-09 | [Script identity is a `Copy` tag enum, not a `&'static str`](0015-script-tag-enum-perf.md) | Accepted |
| 0016 | 2026-06-09 | [Bracket balance — book-scope, windowed, with a delimiter inventory](0016-bracket-balance-book-scope-windowed.md) | Accepted |
| 0017 | 2026-06-30 | [Stateful rules — reduce/merge/judge and a stats-returning `analyze`](0017-stateful-rules-stats-returning-analyze.md) | Accepted |
| 0018 | 2026-06-30 | [Parallelism behind a cargo feature, gated on feature not target](0018-parallelism-behind-a-feature.md) | Accepted |
| 0019 | 2026-06-30 | [Shared tokenization, token-rule traits, and the per-character cost of non-Latin scripts](0019-shared-tokenization-and-per-char-cost.md) | Accepted |
| 0020 | 2026-06-30 | [Per-character classification via a fused `ClassBits` lookup](0020-char-classification-fused-classbits-table.md) | Accepted (amended by 0021, 0022) |
| 0021 | 2026-07-01 | [Domain-tailored grapheme segmenter over one fused static table](0021-grapheme-segmenter-fast-path-fused-static-table.md) | Accepted |
| 0022 | 2026-07-01 | [Extend the fused table to General_Category groups and script](0022-fused-table-category-and-script.md) | Accepted |
| 0023 | 2026-07-01 | [U+200B is orthography-dependent — a corpus-relative anomaly, not hygiene](0023-zero-width-space-corpus-relative-anomaly.md) | Accepted (ZWNJ/ZWJ treatment amended by 0025; scorer superseded by 0027, hygiene half stands) |
| 0024 | 2026-07-01 | [Repeated/mixed punctuation is judged corpus-relative, not by a fixed allow-list](0024-punctuation-adjacency-corpus-relative.md) | Accepted |
| 0025 | 2026-07-06 | [Drop ZWNJ/ZWJ flagging from hygiene — flagging nothing beats flagging wrong](0025-drop-joiner-flagging-from-hygiene.md) | Accepted |
| 0026 | 2026-07-06 | [Drop `\|`/`^` from source-marker-leftover — a bare pipe is valid USFM text](0026-drop-pipe-caret-from-source-marker-leftover.md) | Accepted |
| 0027 | 2026-07-06 | [Retire the corpus-relative ZWSP scorer; adopt a deterministic redundant-ZWSP rule](0027-redundant-zwsp-deterministic-retire-corpus-relative.md) | Accepted (supersedes 0023 scorer) |
| 0028 | 2026-07-06 | [Repeated letter runs are judged against corpus recurrence](0028-repeated-character-run-corpus-relative.md) | Accepted |
| 0029 | 2026-07-06 | [Punctuation spacing is a per-mark corpus convention — flag the minority form](0029-punctuation-spacing-corpus-relative.md) | Accepted (amends 0014 space-before-punct) |
| 0030 | 2026-07-06 | [Punct-only tokens are judged against corpus recurrence](0030-punct-only-token-corpus-relative.md) | Accepted |
| 0031 | 2026-07-06 | [Punctuation adjacency is judged by breadth and run length, not frequency alone](0031-punctuation-adjacency-breadth-and-length.md) | Accepted (amends 0024; retires placeholder-leftover) |
| 0032 | 2026-07-06 | [One evidence library — the lexical rules adopt Wilson shrinkage](0032-evidence-library-wilson-unification.md) | Accepted (amends 0028, 0030) |
| 0033 | 2026-07-06 | [The separator-punctuation class is GC `Po`, not an ASCII list](0033-separator-class-is-po-not-ascii.md) | Accepted (amends 0029, 0031) |
| 0034 | 2026-07-06 | [`hyg.replacement-run` owns `?`-run damage; control chars report per run](0034-replacement-run-owns-mojibake.md) | Accepted (amends 0030, 0031) |
| 0035 | 2026-07-06 | [Casing joins the evidence library — dominance verdict, aggregate stats](0035-casing-recast-on-dominance.md) | Accepted (amends 0017 casing shape) |
| 0036 | 2026-07-06 | [Excess-whitespace reads Unicode classes — `Zs`+tab runs, STerm protection](0036-excess-whitespace-unicode-classes.md) | Accepted (amends 0014) |
| 0037 | 2026-07-06 | [Bracket balance — UCD inventory, book-stream pairing, corpus-relative verdicts](0037-bracket-balance-corpus-relative.md) | Accepted (amends 0016) |
| 0038 | 2026-07-06 | [The rule catalog — shipped plain-language cards and a two-tier config](0038-rule-catalog-two-tier-config.md) | Accepted |
| 0039 | 2026-07-07 | [Quote / discourse-marker balance stays deferred — now with census data](0039-quote-balance-deferred.md) | Deferred |
| 0040 | 2026-07-07 | [One corpus format — self-describing vref files from external producers](0040-vref-corpus-format-onion-builder.md) | Accepted |
| 0041 | 2026-07-07 | [Stateful-phase hot-path cleanup — grapheme::count, Po bit, Copy keys, bracket gate, offset chunking](0041-stateful-phase-hot-path-cleanup.md) | Accepted (extends 0017/0021/0022) |
| 0042 | 2026-07-07 | [The stateful phase fans out per book — books-shaped rules, shared grouping, judge on the token cache](0042-stateful-phase-book-fanout.md) | Accepted (extends 0017/0018) |
| 0043 | 2026-07-07 | [`changed` narrows counting, never emission — the complete-snapshot call](0043-changed-scope-complete-snapshot.md) | Accepted (extends 0017/0042) |
| 0044 | 2026-07-07 | [Reduce forwards its candidate sites to judge — within one call, never on the wire](0044-reduce-judge-site-forwarding.md) | Accepted (extends 0017/0043) |
| 0045 | 2026-07-07 | [The scalar tape — decode + classify each verse once, then every scan consumes the tape](0045-scalar-tape.md) | Accepted (extends 0021/0022/0041) |
| 0046 | 2026-07-08 | [Per-verse "dirty bits" prefilter — measured, and deferred](0046-per-verse-dirty-bits-prefilter.md) | Deferred (spike only; extends 0045/0022) |
| 0047 | 2026-07-08 | [Store the crate's full script set faithfully; push mixing policy into a probabilistic rule](0047-full-script-set-no-collapse-probabilistic-mixing.md) | Accepted (amends 0009/0022) |
| 0048 | 2026-07-08 | [Ship the raw convention share alongside the Wilson-bound score](0048-descriptive-share-args-for-dominance-rules.md) | Accepted (extends 0029/0010) |
| 0049 | 2026-07-09 | [CJK corner brackets are quotation marks — excluded from the bracket inventory](0049-cjk-corner-brackets-excluded-from-bracket-inventory.md) | Accepted (amends 0037; relates 0039) |
| 0050 | 2026-07-09 | [`punct.spacing-anomaly` scores dominance × minority-recurrence rarity](0050-spacing-minority-recurrence-factor.md) | Accepted (amends 0029/0033) |
| 0051 | 2026-07-10 | [Casing rebuilt on a word lexicon — two-factor scores, two rules from one module](0051-casing-two-factor-word-lexicon.md) | Accepted (supersedes 0035's scoring) |
| 0052 | 2026-07-10 | [`terminal_strength` — learned mark trust gates casing's positional flagging, weights its censoring discount](0052-terminal-strength-mark-trust.md) | Accepted (builds on 0051) |
| 0053 | 2026-07-10 | [`uni.rare-glyph` — the letter (L) lane, with a glyph-census substrate](0053-rare-glyph-letter-lane.md) | Accepted |
| 0054 | 2026-07-10 | [Spacing attachment signatures — pooled class-conditioned per-side conventions](0054-spacing-attachment-signatures.md) | Accepted (amends 0050) |
| 0055 | 2026-07-10 | [`case.mixed-case-word` — the interior-capital anomaly](0055-mixed-case-word.md) | Accepted |
| 0056 | 2026-07-10 | [Rare-glyph reduce: page-table census, surface-deferred attribution](0056-rare-glyph-reduce-page-table.md) | Accepted |
| 0057 | 2026-07-11 | [The event-stream engine — one fused book walk, every rule a listener](0057-event-stream-engine.md) | Accepted (supersedes 0044's fusion rejection) |
| 0058 | 2026-07-11 | [The census (absolute mode) — `census(map) → Inventory`, the event stream's first subscriber](0058-census-absolute-mode.md) | Accepted (ratifies the 2026-07-10 plan) |
| 0059 | 2026-07-11 | [Association goes G²-only — retire the Fisher fallback as the default](0059-association-g2-only.md) | Accepted |
| 0060 | 2026-07-13 | [Cross-call analysis caches — content-keyed per-book products](0060-cross-call-analysis-caches.md) | Draft |

## Format

Each ADR has six fields:

- **Date** — when accepted
- **Status** — Proposed / Accepted / Superseded (by NNNN) / Rejected
- **Context** — what problem we're solving and what's true at decision time
- **Decision** — the choice, in one or two sentences
- **Rationale** — why this and not the alternatives
- **Consequences** — what becomes easy, what becomes hard, what's foreclosed

When superseding, the new ADR cites the old; the old's status changes
to "Superseded by NNNN" with a link.
