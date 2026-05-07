//! Assemble a `Project` from a raw `Sid -> text` map. Single point
//! where `build_verses` is called for every verse — by convention, no
//! other code path constructs `Verse` directly.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use ssc_core::analysis::lemma_feedback::LabelledLemmaIndex;
use ssc_core::config::{Config, ExceptionSet};
use ssc_core::project::{NamedCorpus, Project};
use ssc_core::sid::Sid;
use ssc_core::verse::build_verses;

pub fn project_from_raw_map(
    target_name: String,
    target_raw: BTreeMap<Sid, String>,
    source: Option<(String, BTreeMap<Sid, String>)>,
    config: Config,
    exceptions: ExceptionSet,
) -> Project<'static> {
    project_from_raw_map_with_labels(
        target_name,
        target_raw,
        source,
        config,
        exceptions,
        LabelledLemmaIndex::default(),
    )
}

pub fn project_from_raw_map_with_labels(
    target_name: String,
    target_raw: BTreeMap<Sid, String>,
    source: Option<(String, BTreeMap<Sid, String>)>,
    config: Config,
    exceptions: ExceptionSet,
    lemma_labels: LabelledLemmaIndex,
) -> Project<'static> {
    let target = NamedCorpus {
        name: target_name,
        verses: build_verses(target_raw),
        _src: PhantomData,
    };
    let source = source.map(|(name, raw)| NamedCorpus {
        name,
        verses: build_verses(raw),
        _src: PhantomData,
    });
    Project {
        target,
        source,
        config,
        exceptions,
        lemma_labels,
    }
}
