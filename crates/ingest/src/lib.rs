//! Format adapters: USFM / USX / USJ → `Project`.
//!
//! The split between `ingest` and `core` is deliberate. Anything that
//! knows about scripture file formats lives here; `core` only sees
//! `Verse`s and `Sid`s. That keeps the engine WASM-cleaner (no
//! filesystem assumptions in core) and lets us add USX/USJ support
//! without churning the engine.

pub mod build;
#[cfg(feature = "usfm")]
pub mod usfm;
