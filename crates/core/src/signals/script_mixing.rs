//! Mixed-script-in-token anomaly (corpus-relative, aggregate-only stateful).
//!
//! A word mixing two writing systems is often a look-alike/homoglyph or a
//! stray marker — a Latin `o` inside a Kannada word, a Cyrillic `а` in Latin
//! text. But it is just as often a *convention*: an orthography that borrows a
//! foreign letter (`ŏ` in a Cyrillic language, `π` as a letter, a Canadian
//! Syllabics final clinging to Latin), or a systematic transliteration
//! artifact. A fixed "two scripts ⇒ flag" predicate (the rule's deterministic
//! predecessor) cannot tell these apart and buried the real errors under
//! thousands of convention hits (ADR 0047 census: 30,098 categorical hits, the
//! overwhelming majority pervasive conventions).
//!
//! So this rule keeps the same candidate extraction — a token whose distinct
//! non-`None` [`ScriptTag`]s number ≥2 — but replaces the fixed verdict with a
//! corpus-rate one, exactly the shape of `punct.adjacency-anomaly` (ADR 0031):
//! each **script signature** (the sorted script set, `Latin+Cyrillic`) is
//! judged by two independent convention axes combined by noisy-OR —
//!
//! - **frequency**: the signature's mixed-token count `k` against
//!   `N`, the number of tokens containing the signature's **dominant** script
//!   (the `max` over its scripts' token counts). The dominant-script
//!   denominator is load-bearing: in every convention the *intruder* script is
//!   exclusive to the mix (a language's `ŏ` never appears outside a Cyrillic
//!   word), so a denominator on the rarer script pins the observed rate at 1.0
//!   and reads the convention as an anomaly. The dominant script's token count
//!   asks the right question — "what share of the main script's words does this
//!   contaminate?" — which is tiny for a homoglyph and large for a borrowed
//!   letter.
//! - **breadth**: the signature's book count against the corpus book count —
//!   a pair spanning most books is a house convention, one concentrated in a
//!   book or two is not (ADR 0031).
//!
//! A signature that either axis establishes as a convention goes silent; a rare,
//! concentrated one surfaces at `Severity::Info` with a continuous score. A
//! systematic *widespread* cross-script contamination is suppressed exactly like
//! a convention — corpus counts alone cannot tell them apart (the documented
//! limitation shared with the punctuation anomalies).
//!
//! **Aggregate-only state** (ADR 0017): each book caches per-signature counts and
//! per-script token counts — never sites — so `Stats` stays a few KB. At `judge`
//! spans come from the forwarded reduce sites where this call scanned the book
//! (ADR 0044), or by re-scanning any book carried from the prior.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::MixedScriptConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_rate, clamp_unit, clamp_z, from_strengths, strength};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::script::{script_of, ScriptTag};
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;
use crate::token::{tokenize, Token};
use crate::verse::{Books, VerseMap};

pub const MIXED_SCRIPT_IN_TOKEN: RuleId = RuleId::MixedScriptInToken;

/// The distinct non-`None` scripts in a token, in `ScriptTag` order. `None`
/// (Common/Inherited/Unknown — digits, punctuation, marks, unassigned) carries
/// no script identity and never participates, so a word around a comma or a
/// digit is not "mixed".
///
/// `pub(crate)`: `signals::rare_glyph` reuses this exact predicate (ADR 0053) so
/// the "mixed-script tokens are this rule's" ownership boundary uses one
/// definition (a token is mixed iff `token_scripts(word).len() >= 2`).
pub(crate) fn token_scripts(word: &str) -> Vec<ScriptTag> {
    let mut set: BTreeSet<ScriptTag> = BTreeSet::new();
    for c in word.chars() {
        if let Some(t) = script_of(c) {
            set.insert(t);
        }
    }
    set.into_iter().collect()
}

/// A script's stable key in the aggregates: its ISO 15924 short name
/// (`"Latn"`, `"Cyrl"`, `"Zmth"`), which is stable across `unicode-script`
/// versions — unlike the fused-table byte, which is a build artifact.
fn tag_key(t: ScriptTag) -> String {
    t.name().to_string()
}

/// The canonical signature of a mixed token: its scripts' keys, joined by `+`
/// in `ScriptTag` order (`Cyrl+Latn`). Two scripts is the overwhelming case;
/// three-script tokens (a stray Latin letter in an Arabic transliteration of
/// Devanagari) key the same way.
fn signature(scripts: &[ScriptTag]) -> String {
    scripts.iter().map(|&t| tag_key(t)).collect::<Vec<_>>().join("+")
}

/// One mixed token, forwarded reduce→judge within a call (ADR 0044). Carries
/// the signature so judge's per-signature verdict needs no re-derivation, and
/// the token span to highlight. Clean-book products may be retained by the
/// content-keyed analysis cache between calls.
#[derive(Clone)]
pub struct MixedScriptSite {
    pub(crate) sid: Sid,
    pub(crate) sig: String,
    pub(crate) span: Span,
}

/// One book's aggregate contribution: per-signature mixed-token counts and
/// per-script token counts (how many tokens contain each script at all — the
/// dominant-script denominator's raw material). **No sites.**
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookMixedScript {
    signature_counts: BTreeMap<String, u64>,
    script_tokens: BTreeMap<String, u64>,
}

/// Cached mixed-script aggregates, keyed by book so an edit supersedes only its
/// book. Corpus-wide counts are the sums over books, derived at `judge`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct MixedScriptStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookMixedScript>"))]
    pub(crate) per_book: BTreeMap<BookId, BookMixedScript>,
}

impl MixedScriptStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: MixedScriptStats) -> MixedScriptStats {
        for (book, b) in other.per_book {
            self.per_book.insert(book, b);
        }
        self
    }

    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
    }
}

pub struct MixedScriptInToken {
    pub cfg: MixedScriptConfig,
}

impl StatefulRule for MixedScriptInToken {
    fn id(&self) -> RuleId {
        MIXED_SCRIPT_IN_TOKEN
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&VerseMap>,
        tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        // Thin driver over the shared listener (the fused walk feeds the same
        // `MixedScriptAcc`); kept for calibration/tests. The shared token
        // cache is ignored — the driver tokenizes each verse once, which is
        // exactly what the cache would supply.
        let _ = tokens;
        let mut per_book = BTreeMap::new();
        let mut sites = BTreeMap::new();
        for (book, (counts, book_sites)) in rule::map_books(books, |book, verses| {
            (
                book,
                stream::drive_book(
                    verses,
                    stream::Needs { tokens: true, ..Default::default() },
                    MixedScriptAcc::new(true),
                    |a, v, _| a.verse(v),
                    MixedScriptAcc::finish,
                ),
            )
        }) {
            per_book.insert(book, counts);
            sites.insert(book, book_sites);
        }
        (
            RuleStats::MixedScript(MixedScriptStats { per_book }),
            rule::RuleSites::MixedScript(sites),
        )
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let RuleStats::MixedScript(stats) = stats else {
            return Vec::new();
        };

        // Corpus-wide aggregates: sum per-book signature counts + script token
        // counts, and count in how many books each signature occurs (breadth).
        let mut sig_k: BTreeMap<&str, u64> = BTreeMap::new();
        let mut sig_books: BTreeMap<&str, u64> = BTreeMap::new();
        let mut script_n: BTreeMap<&str, u64> = BTreeMap::new();
        for book in stats.per_book.values() {
            for (sig, &k) in &book.signature_counts {
                *sig_k.entry(sig.as_str()).or_default() += k;
                *sig_books.entry(sig.as_str()).or_default() += 1;
            }
            for (sc, &n) in &book.script_tokens {
                *script_n.entry(sc.as_str()).or_default() += n;
            }
        }
        let corpus_books = stats.per_book.len() as u64;

        let rate = clamp_rate(self.cfg.convention_rate);
        let z = clamp_z(self.cfg.confidence_z);
        let breadth_rate = clamp_rate(self.cfg.breadth_convention_rate);
        let breadth_z = clamp_z(self.cfg.breadth_z);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        let breadth_active = corpus_books >= u64::from(self.cfg.breadth_min_books);

        // Evidence depends only on the signature; compute it once each.
        // Frequency (share of the DOMINANT script's tokens — the max
        // denominator that fixes the exclusive-intruder pathology) and breadth
        // are independent convention axes combined by noisy-OR (ADR 0031).
        let evidence: BTreeMap<&str, f64> = sig_k
            .iter()
            .map(|(&sig, &k)| {
                let n = sig
                    .split('+')
                    .map(|sc| script_n.get(sc).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                let freq = strength(k, n, rate, z);
                let breadth = if breadth_active {
                    let b = sig_books.get(sig).copied().unwrap_or(0);
                    strength(b, corpus_books, breadth_rate, breadth_z)
                } else {
                    0.0
                };
                (sig, from_strengths(&[freq, breadth]))
            })
            .collect();

        // The raw counts behind each signature's score, for the descriptive
        // message (ADR 0048): frequency `k / n` and breadth `books / corpus`.
        let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        let details: BTreeMap<&str, (u32, u32, u32, u32)> = sig_k
            .iter()
            .map(|(&sig, &k)| {
                let n = sig
                    .split('+')
                    .map(|sc| script_n.get(sc).copied().unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                let b = sig_books.get(sig).copied().unwrap_or(0);
                (sig, (sat(k), sat(n), sat(b), sat(corpus_books)))
            })
            .collect();

        // Recover spans (aggregate-only state holds none): from the forwarded
        // reduce sites where this call scanned the book (ADR 0044), by
        // re-scanning otherwise. Scores stay corpus-wide via `evidence`; both
        // paths fan out per book (ADR 0042).
        let forwarded = match sites {
            Some(rule::RuleSites::MixedScript(m)) => Some(m),
            _ => None,
        };
        let score = |sid: Sid, sig: &str, span: Span, found: &mut Vec<Finding>| {
            let ev = evidence.get(sig).copied().unwrap_or(1.0);
            if ev < floor {
                return;
            }
            let (k, n, books, corpus) = details.get(sig).copied().unwrap_or((0, 0, 0, 0));
            found.push(Finding {
                sid,
                code: MIXED_SCRIPT_IN_TOKEN,
                severity: Severity::Info,
                range: span,
                score: Some(ev as f32),
                args: Some(FindingArgs::ScriptMixEvidence { k, n, books, corpus }),
            });
        };
        let mut out: Vec<Finding> = rule::map_books(books, |book, verses| {
            let mut found = Vec::new();
            if let Some(book_sites) = forwarded.and_then(|m| m.get(&book)) {
                for s in book_sites {
                    score(s.sid, &s.sig, s.span, &mut found);
                }
            } else {
                for &(sid, text) in verses {
                    for (sig, span) in mixed_tokens(text, verse_tokens(sid, text, tokens).as_ref()) {
                        score(sid, &sig, span, &mut found);
                    }
                }
            }
            found
        })
        .into_iter()
        .flatten()
        .collect();
        out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
        out
    }
}

/// The verse's shared tokens when the runner built a cache, else a fresh
/// tokenization owned by the caller — the single-consumer fallback.
fn verse_tokens<'a>(
    sid: Sid,
    text: &str,
    cache: Option<&'a TokenCache>,
) -> std::borrow::Cow<'a, [Token]> {
    match cache.and_then(|c| c.get(&sid)) {
        Some(t) => std::borrow::Cow::Borrowed(t),
        None => std::borrow::Cow::Owned(tokenize(text)),
    }
}

/// The mixed tokens of a verse: each token whose distinct non-`None` scripts
/// number ≥2, paired with its signature and span.
fn mixed_tokens(text: &str, tokens: &[Token]) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    for tok in tokens {
        let scripts = token_scripts(tok.span.slice(text));
        if scripts.len() >= 2 {
            out.push((signature(&scripts), tok.span));
        }
    }
    out
}

/// The mixed-script counting listener: one book's aggregate counts plus the
/// candidate sites (forwarded reduce→judge within a call, ADR 0044; the
/// *stats* carry no sites). Fed per verse by the fused walk.
pub(crate) struct MixedScriptAcc {
    signature_counts: BTreeMap<String, u64>,
    script_tokens: BTreeMap<String, u64>,
    sites: Vec<MixedScriptSite>,
    /// `false` on a prior-carried book (anchor mode): mixed tokens still
    /// feed the sites; the signature/script tallies are skipped.
    counting: bool,
}

impl MixedScriptAcc {
    pub(crate) fn new(counting: bool) -> Self {
        MixedScriptAcc {
            signature_counts: BTreeMap::new(),
            script_tokens: BTreeMap::new(),
            sites: Vec::new(),
            counting,
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        for tok in v.tokens {
            let scripts = token_scripts(tok.span.slice(v.text));
            if self.counting {
                for &s in &scripts {
                    *self.script_tokens.entry(tag_key(s)).or_default() += 1;
                }
            }
            if scripts.len() >= 2 {
                let sig = signature(&scripts);
                if self.counting {
                    *self.signature_counts.entry(sig.clone()).or_default() += 1;
                }
                self.sites.push(MixedScriptSite { sid: v.sid, sig, span: tok.span });
            }
        }
    }

    pub(crate) fn finish(self) -> (BookMixedScript, Vec<MixedScriptSite>) {
        (
            BookMixedScript {
                signature_counts: self.signature_counts,
                script_tokens: self.script_tokens,
            },
            self.sites,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verse::by_book;

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }
    fn rule(cfg: MixedScriptConfig) -> MixedScriptInToken {
        MixedScriptInToken { cfg }
    }
    fn default_rule() -> MixedScriptInToken {
        rule(MixedScriptConfig::default())
    }
    fn run(map: &VerseMap, r: &MixedScriptInToken) -> Vec<Finding> {
        let books = by_book(map);
        r.judge(&r.reduce(&books, None, None).0, &books, None, None)
    }

    // ── extraction ──────────────────────────────────────────────────────

    #[test]
    fn common_and_inherited_never_count() {
        // A Latin word around a comma / digit / combining mark is one script.
        assert!(token_scripts("word,").len() <= 1);
        assert!(token_scripts("word2").len() <= 1);
        // A combining acute (Inherited) carries no script; only the Latin base.
        let s = token_scripts("cafe\u{0301}");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name(), "Latn");
    }

    #[test]
    fn two_scripts_signature_is_sorted_and_canonical() {
        // Cyrillic 'а' (U+0430) inside a Latin word, either order → same sig.
        let a = signature(&token_scripts("c\u{0430}t"));
        let b = signature(&token_scripts("\u{0430}bc"));
        assert_eq!(a, b, "signature is order-independent");
        assert!(a.contains("Latn") && a.contains("Cyrl"), "sig was {a}");
        assert!(a.contains('+'));
    }

    // ── corpus verdict ──────────────────────────────────────────────────

    /// A homoglyph: a single Latin+Cyrillic word in one book, against an
    /// overwhelmingly Latin corpus. Rare + narrow ⇒ surfaces near-certain.
    #[test]
    fn rare_homoglyph_surfaces() {
        let mut v: Vec<(u16, String)> = (1..=200)
            .map(|i| (i, "the word is here".to_string()))
            .collect();
        v.push((900, "c\u{0430}t here".to_string())); // Latin+Cyrillic homoglyph
        let map: VerseMap = v.into_iter().map(|(i, t)| (sid("GEN", i), t)).collect();
        let f = run(&map, &default_rule());
        assert_eq!(f.len(), 1, "the lone homoglyph surfaces");
        assert_eq!(f[0].severity, Severity::Info);
        assert!(f[0].score.unwrap() > 0.8, "score {:?}", f[0].score);
        assert_eq!(f[0].range.slice(map.get(&f[0].sid).unwrap()), "c\u{0430}t");
    }

    /// A borrowed-letter convention: a Latin `o` in most words of a Kannada
    /// text, across every book. The intruder script is exclusive to the mix
    /// (Latin appears only mixed), which the dominant-script denominator
    /// handles — frequency establishes the convention and it goes silent.
    #[test]
    fn pervasive_borrowed_letter_is_silent() {
        // Kannada base 'ಕ' with a Latin 'o' fused, in every verse of 10 books —
        // Latin is exclusive to the mix, Kannada dominates.
        let mut map = VerseMap::new();
        for bk in ["GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA"] {
            for v in 1..=40u16 {
                map.insert(sid(bk, v), "ಕoಕ ಕಕ ಕಕ".to_string());
            }
        }
        assert!(
            run(&map, &default_rule()).is_empty(),
            "a pervasive borrowed letter must be learned as convention"
        );
    }

    /// Breadth alone: the same Latin+Cyrillic pair spread thinly across most
    /// books (never frequent) is a house convention on dispersion grounds.
    #[test]
    fn widespread_low_frequency_pair_suppresses_on_breadth() {
        let mut map = VerseMap::new();
        for bk in ["GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA"] {
            for v in 1..=40u16 {
                map.insert(sid(bk, v), "the word here now".to_string());
            }
            // one mixed token per book → 10/10 books, tiny frequency
            map.insert(sid(bk, 100), "c\u{0430}t here".to_string());
        }
        assert!(
            run(&map, &default_rule()).is_empty(),
            "a pair spanning all books suppresses on breadth alone"
        );
    }

    /// The same total count concentrated in one book (low breadth) still
    /// surfaces — isolates breadth from frequency.
    #[test]
    fn concentrated_pair_still_surfaces() {
        let mut map = VerseMap::new();
        for bk in ["GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA"] {
            for v in 1..=40u16 {
                map.insert(sid(bk, v), "the word here now".to_string());
            }
        }
        for v in 100..=109u16 {
            map.insert(sid("GEN", v), "c\u{0430}t here".to_string()); // 10 in one book
        }
        assert!(
            !run(&map, &default_rule()).is_empty(),
            "concentrated pair (1/10 books) must still surface"
        );
    }

    // ── stateful plumbing ───────────────────────────────────────────────

    #[test]
    fn incremental_score_is_corpus_wide_not_book_local() {
        let r = default_rule();
        // GEN establishes a dominant-Latin corpus; EXO edited later carries one
        // homoglyph. Its score must reflect the corpus, not EXO alone.
        let mut all: Vec<(Sid, String)> = (1..=200)
            .map(|i| (sid("GEN", i), "the word is here".to_string()))
            .collect();
        all.push((sid("EXO", 1), "c\u{0430}t here".to_string()));
        let full: VerseMap = all.iter().cloned().collect();
        let gen_only: VerseMap = full.iter().filter(|(s, _)| s.book == BookId::from_str("GEN").unwrap()).map(|(s, t)| (*s, t.clone())).collect();
        let exo_only: VerseMap = full.iter().filter(|(s, _)| s.book == BookId::from_str("EXO").unwrap()).map(|(s, t)| (*s, t.clone())).collect();

        let full_score = run(&full, &r).into_iter().find(|f| f.sid == sid("EXO", 1)).unwrap().score;

        let merged = r.reduce(&by_book(&gen_only), None, None).0.merge(r.reduce(&by_book(&exo_only), None, None).0);
        let inc = r.judge(&merged, &by_book(&exo_only), None, None);
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].sid, sid("EXO", 1));
        assert_eq!(inc[0].score, full_score, "incremental score is corpus-wide");
    }

    #[test]
    fn removing_a_book_drops_its_contribution() {
        let r = default_rule();
        let mut full: VerseMap = (1..=200)
            .map(|i| (sid("GEN", i), "the word is here".to_string()))
            .collect();
        full.insert(sid("EXO", 1), "c\u{0430}t here".to_string());
        let RuleStats::MixedScript(mut stats) = r.reduce(&by_book(&full), None, None).0 else {
            unreachable!()
        };
        // With GEN present, EXO's homoglyph is rare corpus-wide → surfaces.
        let before = r.judge(&RuleStats::MixedScript(stats.clone()), &by_book(&full), None, None);
        assert!(before.iter().any(|f| f.sid == sid("EXO", 1)));
        // Drop GEN: EXO alone is 1 Latin+Cyrillic of 1 Latin token → rate 1.0,
        // and a single-book corpus (breadth inactive) → still rare → surfaces.
        // Removing simply drops GEN's counts; assert the aggregate shrank.
        stats.remove_book(BookId::from_str("GEN").unwrap());
        let RuleStats::MixedScript(s) = RuleStats::MixedScript(stats) else { unreachable!() };
        assert!(!s.per_book.contains_key(&BookId::from_str("GEN").unwrap()));
    }

    #[test]
    fn invalid_config_produces_finite_scores() {
        let mut v: Vec<(u16, String)> = (1..=50).map(|i| (i, "the word here".to_string())).collect();
        v.push((900, "c\u{0430}t".to_string()));
        let map: VerseMap = v.into_iter().map(|(i, t)| (sid("GEN", i), t)).collect();
        let bad = MixedScriptConfig {
            convention_rate: f32::NAN,
            confidence_z: -3.0,
            breadth_convention_rate: f32::NAN,
            breadth_z: f32::NEG_INFINITY,
            breadth_min_books: 0,
            emit_score_min: f32::NAN,
        };
        for f in run(&map, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn aggregate_stats_round_trip_through_serde() {
        let r = default_rule();
        let mut v: Vec<(u16, String)> = (1..=10).map(|i| (i, "the word here".to_string())).collect();
        v.push((900, "c\u{0430}t here".to_string()));
        let map: VerseMap = v.into_iter().map(|(i, t)| (sid("GEN", i), t)).collect();
        let stats = r.reduce(&by_book(&map), None, None).0;
        let back: RuleStats = serde_json::from_str(&serde_json::to_string(&stats).unwrap()).unwrap();
        assert_eq!(stats, back);
        assert_eq!(
            r.judge(&stats, &by_book(&map), None, None),
            r.judge(&back, &by_book(&map), None, None)
        );
    }
}
