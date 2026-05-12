//! USFM ingest via `usfm_onion`. Produces a `Sid -> raw text` map that
//! `build::project_from_raw_map` then turns into a `Project`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use ssc_core::script::is_nt_book;
use ssc_core::sid::Sid;
use usfm_onion::Usfm;

/// Read every `*.usfm` file in `dir`, parse it via `usfm_onion`, and
/// merge into a single `Sid -> raw text` map. `nt_only` filters by
/// USFM book code.
///
/// Files are read and parsed in parallel via rayon — each USFM is
/// independent, and `usfm_onion::Usfm::from_str` is a self-contained
/// parse with no shared state. Merging into the `BTreeMap` is
/// sequential because BTreeMap is not lock-free, but the inserts
/// are cheap relative to the per-file read+parse.
///
/// Sids that fail to parse (unexpected key shape from `usfm_onion`)
/// are silently skipped — vanishingly rare in practice.
pub fn read_usfm_dir(dir: &Path, nt_only: bool) -> io::Result<BTreeMap<Sid, String>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("usfm"))
        .collect();
    files.sort();

    // Per-file parse runs in parallel; result is (Sid, text) pairs
    // pre-filtered by `nt_only`. Each file gets `Result<Vec<...>>`
    // so an IO error on one file propagates up.
    let per_file: Vec<Vec<(Sid, String)>> = files
        .par_iter()
        .map(|path| -> io::Result<Vec<(Sid, String)>> {
            let src = fs::read_to_string(path)?;
            let m = Usfm::from_str(&src).to_vref();
            let mut out = Vec::with_capacity(m.len());
            for (sid_str, text) in m {
                let Some(sid) = Sid::parse(&sid_str) else {
                    continue;
                };
                if nt_only && !is_nt_book(sid.book.as_str()) {
                    continue;
                }
                out.push((sid, text));
            }
            Ok(out)
        })
        .collect::<io::Result<Vec<_>>>()?;

    let mut out: BTreeMap<Sid, String> = BTreeMap::new();
    for entries in per_file {
        for (sid, text) in entries {
            out.insert(sid, text);
        }
    }
    Ok(out)
}
