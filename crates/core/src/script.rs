//! Coarse Unicode block → script-name lookup, plus the NT book table.
//! Used by the calibration profiler and (eventually) by signals that
//! gate behaviour on script identity.

pub fn script_of(c: char) -> Option<&'static str> {
    let cp = c as u32;
    Some(match cp {
        0x0000..=0x024F => "Latin",
        0x0370..=0x03FF => "Greek",
        0x0400..=0x04FF => "Cyrillic",
        0x0530..=0x058F => "Armenian",
        0x0590..=0x05FF => "Hebrew",
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            "Arabic"
        }
        0x0700..=0x074F => "Syriac",
        0x0780..=0x07BF => "Thaana",
        0x07C0..=0x07FF => "Nko",
        0x0900..=0x097F => "Devanagari",
        0x0980..=0x09FF => "Bengali",
        0x0A00..=0x0A7F => "Gurmukhi",
        0x0A80..=0x0AFF => "Gujarati",
        0x0B00..=0x0B7F => "Oriya",
        0x0B80..=0x0BFF => "Tamil",
        0x0C00..=0x0C7F => "Telugu",
        0x0C80..=0x0CFF => "Kannada",
        0x0D00..=0x0D7F => "Malayalam",
        0x0D80..=0x0DFF => "Sinhala",
        0x0E00..=0x0E7F => "Thai",
        0x0E80..=0x0EFF => "Lao",
        0x0F00..=0x0FFF => "Tibetan",
        0x1000..=0x109F => "Myanmar",
        0x10A0..=0x10FF => "Georgian",
        0x1100..=0x11FF | 0xAC00..=0xD7AF => "Hangul",
        0x1200..=0x137F => "Ethiopic",
        0x13A0..=0x13FF => "Cherokee",
        0x1400..=0x167F => "CanadianAboriginal",
        0x1780..=0x17FF => "Khmer",
        0x1800..=0x18AF => "Mongolian",
        0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF => "CJK",
        _ => return None,
    })
}

pub fn is_nt_book(book: &str) -> bool {
    matches!(
        book,
        "MAT"
            | "MRK"
            | "LUK"
            | "JHN"
            | "ACT"
            | "ROM"
            | "1CO"
            | "2CO"
            | "GAL"
            | "EPH"
            | "PHP"
            | "COL"
            | "1TH"
            | "2TH"
            | "1TI"
            | "2TI"
            | "TIT"
            | "PHM"
            | "HEB"
            | "JAS"
            | "1PE"
            | "2PE"
            | "1JN"
            | "2JN"
            | "3JN"
            | "JUD"
            | "REV"
    )
}
