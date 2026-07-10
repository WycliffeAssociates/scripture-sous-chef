//! Statistical machinery shared across corpus-relative rules.
//!
//! [`association`] holds the 2×2 significance tests (Dunning G² / Fisher's
//! exact) that casing's `terminal_strength` witness (ADR 0052) and future
//! positional rules consume.

pub mod association;
