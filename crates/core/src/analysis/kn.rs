//! Interpolated modified Kneser-Ney smoothing for n-gram language models.
//! Reference: Chen & Goodman (1999), *An Empirical Study of Smoothing
//! Techniques for Language Modeling*.
//!
//! Used by the char-LM (§3.1 orthographic) and the lexical-LM
//! (§3.2 lexical). NOT used for v1 positional / source-relative rules —
//! those go through `dunning` directly.
//!
//! ## TODO
//! - [ ] Pick: own implementation vs. depend on a crate? `oxidized-ngrams`
//!       and `kenlm`-bindings exist; both are heavier than what we need.
//!       Lean own-implementation since the math is well-specified and we
//!       want WASM-clean dependencies.
//! - [ ] Discount-estimation: Chen & Goodman's `D = n_1 / (n_1 + 2·n_2)`
//!       per n-gram order. Three-discount variant (D1, D2, D3+) is the
//!       "modified" in modified-KN.
//! - [ ] Continuation-count vs. raw-count distinction at lower orders —
//!       this is the part that's easy to get subtly wrong.
//! - [ ] API: `KnModel::train(order: usize, tokens: &[&str]) -> Self` +
//!       `model.logprob(history: &[&str], token: &str) -> f64`.
//! - [ ] Test against Chen & Goodman Table 4 (Wall Street Journal
//!       perplexities) on a small sample, or against a known-good
//!       implementation's outputs for a synthetic corpus.
