//! Engine input. `Project` is what the dogfood CLI hands to `analyze()`.

use std::collections::BTreeMap;

use crate::analysis::lemma_feedback::LabelledLemmaIndex;
use crate::config::{Config, ExceptionSet};
use crate::sid::Sid;
use crate::verse::Verse;

/// One translation's worth of verses, plus an arbitrary name for
/// diagnostics output (`"bem_reg"`, `"en_ulb"`, …).
#[derive(Debug, Clone)]
pub struct NamedCorpus<'src> {
    pub name: String,
    pub verses: BTreeMap<Sid, Verse>,
    /// Ties Verse storage to the original ingested buffer when the ingest
    /// layer chooses to borrow rather than own. `()` when not used; reserved
    /// for a future zero-copy ingest path.
    pub _src: std::marker::PhantomData<&'src ()>,
}

/// Everything `analyze()` needs in one place.
#[derive(Debug, Clone)]
pub struct Project<'src> {
    pub target: NamedCorpus<'src>,
    pub source: Option<NamedCorpus<'src>>,
    pub config: Config,
    pub exceptions: ExceptionSet,
    /// Replayed lemma-family feedback. `Default` is the empty index;
    /// the dogfood CLI populates this from
    /// `<corpus>/.sous/events.jsonl` on each run.
    pub lemma_labels: LabelledLemmaIndex,
}
