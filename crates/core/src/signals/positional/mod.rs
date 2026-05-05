//! Positional / discourse signals. Operate over the discourse stream
//! (`crate::discourse`). Findings are mapped back to a Sid at emit time
//! so consumers see the same `Finding<'a>` shape regardless of whether
//! the rule worked on flowing text or per-verse.

// Shared infrastructure
mod shared;
pub use shared::*;

// Individual rules
mod sentence_start_case;
pub use sentence_start_case::*;

mod unexpected_sentence_end;
pub use unexpected_sentence_end::*;

#[cfg(test)]
mod tests {
    // Tests are in the individual rule modules
}
