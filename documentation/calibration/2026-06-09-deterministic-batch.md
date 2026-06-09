# Calibration: the deterministic batch (v0.0.4)

- **Date:** 2026-06-09
- **Scope:** the twelve rules of the deterministic batch (P0+P1 enabled,
  P2 off — i.e. `Config::v1_defaults()`), run over every corpus in
  `corpora/` with the throwaway harness
  (`cargo run --release -p ssc-core --example calibrate -- <corpus-dir>`).
- **Bar:** vision §10 — a published reference Bible must produce
  *bounded* findings; a rule that floods gets fixed, downgraded, or
  default-disabled, recorded here.

## Final volumes (shipped defaults)

| corpus | verses | total | notes |
| --- | ---: | ---: | --- |
| `en_ulb` | 31,086 | **29** | incl. a real `joyfullly` typo, a tab, 4 `that that`-class dups (dup-word now off by default) |
| `es-419_ulb` | 31,103 | **87** | `?,`/`!,` combos, `---`, stray `´`, real `rrr` typos (`guerrras`, `tierrras`) |
| `ne_ulb` | 30,656 | **39** | 4 real `\|` ingest leftovers in GEN, `।,'` wreckage |
| `bap-x-rai_reg` | 7,949 | **30** | `।।` double-danda-as-two-chars, a tab |
| `vi_ulb` | 31,102 | **32** | `=`/`+` from measurement notes in 1SA/1CH |
| `anl-x-khawngtu_reg` | 7,953 | **2** | |
| `fa_nmv` | 31,102 | **62** | zero-width misuse (existing rule), stray `[` |
| `bem_reg` | 7,951 | **7** | |
| `fij-x-saqani_reg` | 7,947 | **46** | |
| `acz_reg` | 7,439 | **149** | draft-stage reg; `,,`/`?.`/`..` and stray brackets look real |
| `arb_avd` | 31,086 | **3** | |

Worst per-book count anywhere: 85 total bracket findings in `acz`
(ACT 22). Everything is far below the vision §9 noise-kill bar.

## Calibration decisions (the keep/downgrade/disable calls)

1. **`lex.duplicate-word` → default-DISABLED** (was P0 ship-enabled).
   Precision in non-reduplicative languages is superb — every en (4) and
   es (34: `sus sus`, `metros metros`, `y y`) hit is a real typo. But
   reduplication is core grammar across this tool's actual audience:
   vi 731 (`đời đời`), anl 753 (`boi boi`), acz 648, vi-worst-book
   PSA 160. A rule that floods four of eleven corpora cannot ship
   enabled. Severity stays Warning; consumers enable it per project
   where doubling is unusual.
2. **`punct.repeated-punct`: quote characters exempted from
   identical-run detection.** es-419 systematically writes `''` for a
   double quote and `""` at nested closes (393 hits → 54 after the
   exemption, the survivors being real `?,` `!,` `,,` `---` cases).
   Doubled quotes are convention, not typo.
3. **`lex.punct-only-token`: reduced to unambiguous wreckage.** As
   drafted it flagged every detached sentence mark — but detaching is a
   *convention*: Nepali spaces before danda/`?`/`!` (46,979 hits in
   `ne_ulb` alone), quotes/dashes/ellipses stand alone legitimately.
   Shipped semantics: quotes and closing brackets are stripped from the
   chunk; a single remaining ordinary mark (GC Po) or dash is allowed;
   what flags is multi-mark cores (`।।`, `,;`, `।,'`), stranded opening
   brackets, and symbols (`=`, `´`). Judging spacing conventions belongs
   to the opt-in `punct.space-before-punct` family. After this, ne 7,
   vi 11, es 6 — all real.
4. **`punct.bracket-balance` → Info** (was Info/Warning open). Nearly
   every reference hit is a legitimate parenthetical aside spanning
   verses (`1CH 7:14 (She gave birth… ↵ 7:15 …Maakah.)`) — en 24, es 18,
   fa 40. The per-verse scope can't see those close; Info keeps it
   visible without shouting. (Cross-verse balance is the book-scope
   future per ADR 0011.)
5. **Everything else ships as specced**: `struct.source-marker-leftover`
   (4 real `|` leftovers in ne_ulb GEN — exactly its purpose),
   `uni.combining-mark-without-base` (1–2 per corpus, all real: a bare
   `´` on a quote in es REV 2:19), `uni.mixed-script-in-token` (1 hit
   total, ne ZEP), `uni.mixed-numeral-systems` (1 ne + 1 fa, Warning
   confirmed), `lex.repeated-character-run` (Info; real `rrr`/`lll`
   typos in es/en, 14 max on acz), `punct.placeholder-leftover` (0 hits
   anywhere — fine, it's a draft-time rule). P2 (`space-before-punct`,
   `sentence-initial-lowercase`) ships default-off as planned.

## Notes

- The harness's naive USFM stripping is itself a finding source: the
  `ne_ulb` pipes and `2TH 1:1` tab are present in the raw files, not
  artifacts.
- `acz_reg` (149) is a work-in-progress draft corpus; its volume is the
  rule set working, not flooding — eyeballed samples (`,,`, `?.`,
  stray `]`) look like genuine draft damage.
