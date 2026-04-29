//! Render a 2D scatter of (morphology_score, orthographic_complexity) for the
//! ebible calibration corpus, plus marginal histograms. Used as a sanity-
//! check fixture for the two-axis recommendation in METHODS.md §5.9.2.
//!
//! Usage:
//!   cargo run --release --bin plot-calibration -- \
//!     --input data/calibration/ebible_profile.csv \
//!     --out   data/calibration/ebible_profile.svg

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use plotters::prelude::*;

#[derive(Debug, Clone)]
struct Row {
    regime: String,
    tokens_per_type: f64,
    bigram_hapax_ratio: f64,
    avg_token_grapheme_len: f64,
    char_trigram_hapax_ratio: f64,
    char_vocab_size: f64,
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn morphology_score(r: &Row) -> f64 {
    let log_tt = r.tokens_per_type.max(2.0).ln();
    let s_tt = sigmoid((log_tt - 3.0) / 0.4);
    let s_hap = sigmoid((0.70 - r.bigram_hapax_ratio) / 0.06);
    let s_len = sigmoid((4.5 - r.avg_token_grapheme_len) / 0.8);
    (s_tt + s_hap + s_len) / 3.0
}

fn orthographic_complexity(r: &Row) -> f64 {
    let s_ct = sigmoid((r.char_trigram_hapax_ratio - 0.18) / 0.04);
    let s_cv = sigmoid((r.char_vocab_size - 65.0) / 15.0);
    (s_ct + s_cv) / 2.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--input" => input = iter.next().map(PathBuf::from),
            "--out" => out = iter.next().map(PathBuf::from),
            _ => {
                eprintln!("unknown arg: {a}");
                std::process::exit(2);
            }
        }
    }
    let input = input.expect("--input required");
    let out = out.expect("--out required");

    let rows = load_csv(&input)?;
    eprintln!("loaded {} rows", rows.len());

    let pts: Vec<(f64, f64, &Row)> = rows
        .iter()
        .map(|r| (morphology_score(r), orthographic_complexity(r), r))
        .collect();

    // 1200x900 SVG: title strip, then top-marginal histogram + main scatter
    // + right-marginal histogram on a 3-panel grid below.
    let root = SVGBackend::new(&out, (1200, 900)).into_drawing_area();
    root.fill(&WHITE)?;

    // Reserve a header strip for the title; chart in the rest.
    let (header, body) = root.split_vertically(40);
    header.titled(
        "ebible calibration: morphology vs orthographic complexity (855 NTs)",
        ("sans-serif", 22).into_font(),
    )?;

    // Body layout: top marginal (120px), main + right marginal (rest).
    let (top_marginal, main_and_right) = body.split_vertically(120);
    let (main, right_marginal) = main_and_right.split_horizontally(940);

    // ---- Main scatter ----
    let mut chart = ChartBuilder::on(&main)
        .margin(20)
        .x_label_area_size(50)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0f64..1.0, 0.0f64..1.0)?;

    chart
        .configure_mesh()
        .x_desc("morphology score  (0 = agglutinative, 1 = analytic)")
        .y_desc("orthographic complexity score  (0 = simple, 1 = complex)")
        .light_line_style(WHITE.mix(0.5))
        .draw()?;

    // Regime threshold guide lines (where the discrete label flips)
    chart.draw_series(LineSeries::new(
        vec![(0.33, 0.0), (0.33, 1.0)],
        ShapeStyle::from(&BLACK.mix(0.18)).stroke_width(1),
    ))?;
    chart.draw_series(LineSeries::new(
        vec![(0.66, 0.0), (0.66, 1.0)],
        ShapeStyle::from(&BLACK.mix(0.18)).stroke_width(1),
    ))?;
    chart.draw_series(LineSeries::new(
        vec![(0.0, 0.5), (1.0, 0.5)],
        ShapeStyle::from(&BLACK.mix(0.18)).stroke_width(1),
    ))?;

    let color_for = |regime: &str| -> RGBColor {
        match regime {
            "Analytic" => RGBColor(60, 130, 200),
            "Fusional" => RGBColor(80, 175, 110),
            "Agglutinative" => RGBColor(220, 110, 60),
            _ => RGBColor(140, 140, 140),
        }
    };

    for regime in ["Analytic", "Fusional", "Agglutinative"] {
        let pts_for_regime: Vec<(f64, f64)> = pts
            .iter()
            .filter(|(_, _, r)| r.regime == regime)
            .map(|(m, o, _)| (*m, *o))
            .collect();
        let count = pts_for_regime.len();
        let label = format!("{} (n={})", regime, count);
        let color = color_for(regime);
        chart
            .draw_series(
                pts_for_regime
                    .iter()
                    .map(|(m, o)| Circle::new((*m, *o), 3, color.mix(0.55).filled())),
            )?
            .label(label)
            .legend(move |(x, y)| Circle::new((x + 8, y), 4, color.filled()));
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(WHITE.mix(0.9))
        .border_style(BLACK.mix(0.3))
        .draw()?;

    // ---- Top marginal: morphology histogram ----
    let bins = 30;
    let mut counts = vec![0u32; bins];
    for (m, _, _) in &pts {
        let b = ((*m * bins as f64).floor() as usize).min(bins - 1);
        counts[b] += 1;
    }
    let max_count = *counts.iter().max().unwrap_or(&1);
    let mut top = ChartBuilder::on(&top_marginal)
        .margin_left(80)
        .margin_right(260)
        .x_label_area_size(15)
        .y_label_area_size(50)
        .build_cartesian_2d(0.0f64..1.0, 0u32..(max_count + 5))?;
    top.configure_mesh()
        .disable_x_mesh()
        .x_desc("morphology distribution")
        .draw()?;
    top.draw_series(counts.iter().enumerate().map(|(i, c)| {
        let lo = i as f64 / bins as f64;
        let hi = (i + 1) as f64 / bins as f64;
        Rectangle::new([(lo, 0), (hi, *c)], RGBColor(120, 120, 200).filled())
    }))?;

    // ---- Right marginal: ortho complexity histogram ----
    let mut ocounts = vec![0u32; bins];
    for (_, o, _) in &pts {
        let b = ((*o * bins as f64).floor() as usize).min(bins - 1);
        ocounts[b] += 1;
    }
    let max_oc = *ocounts.iter().max().unwrap_or(&1);
    let mut right = ChartBuilder::on(&right_marginal)
        .margin_top(20)
        .margin_bottom(70)
        .x_label_area_size(30)
        .y_label_area_size(15)
        .build_cartesian_2d(0u32..(max_oc + 5), 0.0f64..1.0)?;
    right
        .configure_mesh()
        .disable_y_mesh()
        .y_desc("ortho complexity")
        .draw()?;
    right.draw_series(ocounts.iter().enumerate().map(|(i, c)| {
        let lo = i as f64 / bins as f64;
        let hi = (i + 1) as f64 / bins as f64;
        Rectangle::new([(0, lo), (*c, hi)], RGBColor(200, 120, 120).filled())
    }))?;

    root.present()?;
    eprintln!("wrote {}", out.display());
    Ok(())
}

fn load_csv(p: &PathBuf) -> Result<Vec<Row>, Box<dyn std::error::Error>> {
    let f = File::open(p)?;
    let r = BufReader::new(f);
    let mut rows = Vec::new();
    let mut header_idx: HashMap<String, usize> = HashMap::new();
    for (i, line) in r.lines().enumerate() {
        let line = line?;
        let cols = parse_csv_row(&line);
        if i == 0 {
            for (j, c) in cols.iter().enumerate() {
                header_idx.insert(c.clone(), j);
            }
            continue;
        }
        let g = |k: &str| -> String {
            header_idx
                .get(k)
                .and_then(|&j| cols.get(j))
                .cloned()
                .unwrap_or_default()
        };
        let parse_f = |k: &str| -> f64 { g(k).parse().unwrap_or(0.0) };
        rows.push(Row {
            regime: g("regime"),
            tokens_per_type: parse_f("tokens_per_type"),
            bigram_hapax_ratio: parse_f("bigram_hapax_ratio"),
            avg_token_grapheme_len: parse_f("avg_token_grapheme_len"),
            char_trigram_hapax_ratio: parse_f("char_trigram_hapax_ratio"),
            char_vocab_size: parse_f("char_vocab_size"),
        });
    }
    Ok(rows)
}

fn parse_csv_row(line: &str) -> Vec<String> {
    let mut cols = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => {
                cols.push(std::mem::take(&mut cur));
            }
            other => cur.push(other),
        }
    }
    cols.push(cur);
    cols
}
