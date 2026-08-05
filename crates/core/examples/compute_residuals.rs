//! THROWAWAY one-off — NOT part of the crate's shipped surface.
//!
//! Computes the COMPLETE, exact residual sets for the ALetter/Numeric bit
//! reuse in `token.rs`: every codepoint where `WordBreakProperty.txt` says
//! `ALetter` but `char::is_alphabetic()` is false, and every codepoint where
//! it says `Numeric` but the codepoint isn't `GeneralCategory::DecimalNumber`.
//! Run once to replace the two hand-picked examples (U+00B8, U+066B) with
//! the full, exact list.

use std::path::Path;

use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

fn parse_ucd(path: &Path) -> Vec<(u32, u32, String)> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(';');
        let cps = parts.next().unwrap().trim();
        let field = parts.next().unwrap_or("").trim().to_string();
        let (lo, hi) = match cps.split_once("..") {
            Some((a, b)) => (
                u32::from_str_radix(a.trim(), 16).unwrap(),
                u32::from_str_radix(b.trim(), 16).unwrap(),
            ),
            None => {
                let v = u32::from_str_radix(cps, 16).unwrap();
                (v, v)
            }
        };
        out.push((lo, hi, field));
    }
    out
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ranges = parse_ucd(&manifest_dir.join("src/testdata/ucd/WordBreakProperty.txt"));

    let mut aletter_residual: Vec<u32> = Vec::new();
    let mut numeric_residual: Vec<u32> = Vec::new();

    for (lo, hi, val) in &ranges {
        if val == "ALetter" {
            for cp in *lo..=*hi {
                if let Some(c) = char::from_u32(cp) {
                    if !c.is_alphabetic() {
                        aletter_residual.push(cp);
                    }
                }
            }
        } else if val == "Numeric" {
            for cp in *lo..=*hi {
                if let Some(c) = char::from_u32(cp) {
                    if c.general_category() != GeneralCategory::DecimalNumber {
                        numeric_residual.push(cp);
                    }
                }
            }
        }
    }

    println!(
        "=== ALetter residual (Word_Break=ALetter, is_alphabetic()==false): {} codepoints ===",
        aletter_residual.len()
    );
    for cp in &aletter_residual {
        let c = char::from_u32(*cp).unwrap();
        println!("  U+{cp:04X} {c:?} gc={:?}", c.general_category());
    }

    println!(
        "\n=== Numeric residual (Word_Break=Numeric, GC!=Nd): {} codepoints ===",
        numeric_residual.len()
    );
    for cp in &numeric_residual {
        let c = char::from_u32(*cp).unwrap();
        println!("  U+{cp:04X} {c:?} gc={:?}", c.general_category());
    }

    // Emit as Rust match-arm-ready hex list.
    println!("\n=== ALetter residual as | separated hex ===");
    println!(
        "{}",
        aletter_residual
            .iter()
            .map(|cp| format!("'\\u{{{cp:X}}}'"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    println!("\n=== Numeric residual as | separated hex ===");
    println!(
        "{}",
        numeric_residual
            .iter()
            .map(|cp| format!("'\\u{{{cp:X}}}'"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
}
