//! Conservative lemma-family induction for low-resource corpora.
//!
//! Not a full morphology engine. A low-risk bridge:
//! group word forms that probably share a stem so future lexical typo rules
//! stop treating every inflected form as unrelated.
//!
//! Example problem:
//! - A corpus may have `walk`, `walked`, `walking`, `walks`.
//! - A plain word model sees four sparse types.
//! - A lemma-family view can say "these are probably one family" and let a
//!   downstream rule compare rarity against the family, not just the surface
//!   word.
//!
//! The implementation is intentionally explainable. It uses repeated-prefix
//! overlap plus token-count guards, not opaque ML. That makes it safe as
//! context for later rules without changing the engine's tokenizer or claiming
//! linguistically perfect lemmas.

use std::collections::{BTreeMap, BTreeSet};

use crate::project::NamedCorpus;
use crate::verse::TokenKind;

pub const DEFAULT_MIN_FAMILY_SIZE: usize = 2;
pub const DEFAULT_MIN_STEM_CHARS: usize = 4;
pub const DEFAULT_MIN_TOKEN_COUNT: u32 = 2;

#[derive(Debug, Clone, Copy)]
pub struct LemmaClusterConfig {
    pub min_family_size: usize,
    pub min_stem_chars: usize,
    pub min_token_count: u32,
}

impl Default for LemmaClusterConfig {
    fn default() -> Self {
        Self {
            min_family_size: DEFAULT_MIN_FAMILY_SIZE,
            min_stem_chars: DEFAULT_MIN_STEM_CHARS,
            min_token_count: DEFAULT_MIN_TOKEN_COUNT,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LemmaClusterStats {
    pub n_word_types: usize,
    pub n_candidate_families: usize,
    pub n_clustered_types: usize,
    pub families: Vec<LemmaFamily>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LemmaFamily {
    pub stem: String,
    pub total_count: u32,
    pub forms: Vec<LemmaForm>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LemmaForm {
    pub form: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct LemmaClusters {
    families: Vec<LemmaFamily>,
    by_form: BTreeMap<String, usize>,
    n_word_types: usize,
}

impl LemmaClusters {
    /// Induce surface-form families from the target corpus.
    ///
    /// This is a high-precision, low-recall pass. Missing a real family is
    /// acceptable; inventing a bad family can teach later rules the wrong
    /// baseline. For that reason a form must appear at least
    /// `min_token_count` times before it can anchor or join a family.
    pub fn build(corpus: &NamedCorpus<'_>, config: LemmaClusterConfig) -> Self {
        let mut counts: BTreeMap<String, u32> = BTreeMap::new();
        for verse in corpus.verses.values() {
            for (token, text) in verse.tokens_of(TokenKind::Word) {
                let _ = token;
                let form = normalize_word(text);
                if form.chars().count() >= config.min_stem_chars {
                    *counts.entry(form).or_default() += 1;
                }
            }
        }

        let n_word_types = counts.len();
        let eligible: Vec<(String, u32)> = counts
            .into_iter()
            .filter(|(_, count)| *count >= config.min_token_count)
            .collect();
        let mut by_stem: BTreeMap<String, Vec<LemmaForm>> = BTreeMap::new();
        for (form, count) in eligible {
            if let Some(stem) = candidate_stem(&form, config.min_stem_chars) {
                by_stem
                    .entry(stem)
                    .or_default()
                    .push(LemmaForm { form, count });
            }
        }

        let mut families = Vec::new();
        for (stem, mut forms) in by_stem {
            let unique_forms: BTreeSet<_> = forms.iter().map(|f| f.form.as_str()).collect();
            if unique_forms.len() < config.min_family_size {
                continue;
            }
            forms.sort_by(|a, b| b.count.cmp(&a.count).then(a.form.cmp(&b.form)));
            let total_count = forms.iter().map(|f| f.count).sum();
            families.push(LemmaFamily {
                stem,
                total_count,
                forms,
            });
        }
        families.sort_by(|a, b| b.total_count.cmp(&a.total_count).then(a.stem.cmp(&b.stem)));

        let mut by_form = BTreeMap::new();
        for (family_index, family) in families.iter().enumerate() {
            for form in &family.forms {
                by_form.insert(form.form.clone(), family_index);
            }
        }

        Self {
            families,
            by_form,
            n_word_types,
        }
    }

    pub fn family_for_form(&self, form: &str) -> Option<&LemmaFamily> {
        let key = normalize_word(form);
        self.by_form
            .get(&key)
            .and_then(|index| self.families.get(*index))
    }

    pub fn stats(&self) -> LemmaClusterStats {
        let n_clustered_types = self.families.iter().map(|family| family.forms.len()).sum();
        LemmaClusterStats {
            n_word_types: self.n_word_types,
            n_candidate_families: self.families.len(),
            n_clustered_types,
            families: self.families.clone(),
        }
    }
}

fn normalize_word(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect()
}

fn candidate_stem(form: &str, min_stem_chars: usize) -> Option<String> {
    let chars: Vec<char> = form.chars().collect();
    if chars.len() < min_stem_chars {
        return None;
    }

    // Longest useful prefix. This deliberately avoids language-specific
    // suffix lists; agglutinative languages are exactly where an English-style
    // suffix table would lie to us.
    Some(chars[..min_stem_chars].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::build_verse;
    use std::collections::BTreeMap;

    fn sid(v: u16) -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, v)
    }

    fn corpus() -> NamedCorpus<'static> {
        let rows = [
            "walk walked walking walks",
            "walk walked walking walks",
            "pray prayed praying prays",
            "pray prayed praying prays",
        ];
        let verses = rows
            .into_iter()
            .enumerate()
            .map(|(i, text)| {
                let s = sid((i + 1) as u16);
                (s, build_verse(s, text.to_string()))
            })
            .collect::<BTreeMap<_, _>>();
        NamedCorpus {
            name: "toy".to_string(),
            verses,
            _src: std::marker::PhantomData,
        }
    }

    #[test]
    fn groups_repeated_prefix_families() {
        let clusters = LemmaClusters::build(&corpus(), LemmaClusterConfig::default());
        let walk = clusters
            .family_for_form("walking")
            .expect("walking should join walk family");

        assert_eq!(walk.stem, "walk");
        assert!(walk.forms.iter().any(|form| form.form == "walked"));
    }
}
