//! Assemble a `Project` from a raw `Sid -> text` map. Single point
//! where `build_verses` is called for every verse — by convention, no
//! other code path constructs `Verse` directly.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use scc_core::config::{Config, ExceptionSet};
use scc_core::project::{NamedCorpus, Project};
use scc_core::sid::Sid;
use scc_core::verse::build_verses;

pub fn project_from_raw_map(
    target_name: String,
    target_raw: BTreeMap<Sid, String>,
    source: Option<(String, BTreeMap<Sid, String>)>,
    config: Config,
    exceptions: ExceptionSet,
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
    Project { target, source, config, exceptions }
}
