//! Dump a USFM corpus as a plain-text vref file.
//!
//! Output format (one line per verse, canonical Gen→Rev order):
//!   GEN 1:1: In the beginning God created...
//!
//! Usage:
//!   cargo run --release --bin vref-dump -- [--nt-only] <corpus-dir> [<output.txt>]
//!
//! With no output path, writes to stdout. Order is determined by canonical
//! book position, not filenames.

use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use ssc_core::sid::Sid;
use ssc_ingest::usfm;

// Standard Paratext 66-book canonical order (OT then NT).
// Books not in this list sort after all known books.
#[rustfmt::skip]
const CANON: &[&str] = &[
    "GEN","EXO","LEV","NUM","DEU","JOS","JDG","RUT","1SA","2SA",
    "1KI","2KI","1CH","2CH","EZR","NEH","EST","JOB","PSA","PRO",
    "ECC","SNG","ISA","JER","LAM","EZK","DAN","HOS","JOL","AMO",
    "OBA","JON","MIC","NAM","HAB","ZEP","HAG","ZEC","MAL",
    "MAT","MRK","LUK","JHN","ACT","ROM","1CO","2CO","GAL","EPH",
    "PHP","COL","1TH","2TH","1TI","2TI","TIT","PHM","HEB","JAS",
    "1PE","2PE","1JN","2JN","3JN","JUD","REV",
];

fn canonical_index(book: &str) -> usize {
    CANON.iter().position(|&b| b == book).unwrap_or(usize::MAX)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: vref-dump [--nt-only] <corpus-dir> [<output.txt>]");
        std::process::exit(2);
    }

    let mut nt_only = false;
    let mut corpus_dir: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;

    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--nt-only" => nt_only = true,
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
            _ if corpus_dir.is_none() => corpus_dir = Some(PathBuf::from(a)),
            _ => out_path = Some(PathBuf::from(a)),
        }
    }

    let Some(dir) = corpus_dir else {
        eprintln!("error: corpus directory required");
        std::process::exit(2);
    };

    let raw = match usfm::read_usfm_dir(&dir, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            std::process::exit(1);
        }
    };

    let mut verses: Vec<(&Sid, &String)> = raw.iter().collect();
    verses.sort_by_key(|(sid, _)| (canonical_index(sid.book.as_str()), sid.chapter, sid.verse));

    let result: io::Result<()> = match out_path {
        Some(ref p) => (|| {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = fs::File::create(p)?;
            write_vref(BufWriter::new(file), &verses)
        })(),
        None => write_vref(BufWriter::new(io::stdout().lock()), &verses),
    };

    if let Err(e) = result {
        eprintln!("write failed: {e}");
        std::process::exit(1);
    }

    let n = raw.len();
    if let Some(p) = out_path {
        eprintln!("wrote {n} verses → {}", p.display());
    } else {
        eprintln!("{n} verses");
    }
}

fn write_vref<W: Write>(mut w: W, verses: &[(&Sid, &String)]) -> io::Result<()> {
    for (sid, text) in verses {
        writeln!(w, "{sid}: {text}")?;
    }
    Ok(())
}
