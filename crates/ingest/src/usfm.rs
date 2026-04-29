//! USFM ingest via `usfm_onion`. Produces a `Sid -> raw text` map that
//! `build::project_from_raw_map` then turns into a `Project`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use scc_core::script::is_nt_book;
use scc_core::sid::Sid;
use usfm_onion::Usfm;

/// Read every `*.usfm` file in `dir`, parse it via `usfm_onion`, and
/// merge into a single `Sid -> raw text` map. `nt_only` filters by
/// USFM book code.
///
/// Sids that fail to parse (unexpected key shape from `usfm_onion`)
/// are silently skipped — they're vanishingly rare and an error log
/// would just noise. TODO: surface them as ingest diagnostics.
pub fn read_usfm_dir(dir: &Path, nt_only: bool) -> io::Result<BTreeMap<Sid, String>> {
    let mut out: BTreeMap<Sid, String> = BTreeMap::new();
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("usfm"))
        .collect();
    files.sort();

    for path in files {
        let src = fs::read_to_string(&path)?;
        let m = Usfm::from_str(&src).to_vref();
        for (sid_str, text) in m {
            let Some(sid) = Sid::parse(&sid_str) else { continue };
            if nt_only && !is_nt_book(sid.book.as_str()) {
                continue;
            }
            out.insert(sid, text);
        }
    }
    Ok(out)
}
