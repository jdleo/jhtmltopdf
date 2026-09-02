use std::path::PathBuf;
use std::time::Instant;

use jhtmltopdf::{render, render_with, Options};

const CASES: &[(&str, &str)] = &[
    (
        "case1_simple",
        include_str!("../benches/cases/case1_simple.html"),
    ),
    (
        "case2_resume",
        include_str!("../benches/cases/case2_resume.html"),
    ),
    (
        "case3_complex",
        include_str!("../benches/cases/case3_complex.html"),
    ),
];

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "bench".into());
    match mode.as_str() {
        "render" => {
            let mut viewport = None;
            let mut page_size: Option<(&'static str, f32, f32)> = None;
            let mut margin_mm = None;
            let mut positional: Vec<String> = Vec::new();
            for a in std::env::args().skip(2) {
                if let Some(v) = a.strip_prefix("--viewport-width=") {
                    viewport = v.parse::<f32>().ok();
                } else if let Some(v) = a.strip_prefix("--page-size=") {
                    page_size = match v {
                        "letter" => Some(("letter", 612.0, 792.0)),
                        _ => Some(("a4", 595.0, 842.0)),
                    };
                } else if let Some(v) = a.strip_prefix("--margin-mm=") {
                    margin_mm = v.parse::<f32>().ok();
                } else {
                    positional.push(a);
                }
            }
            let src = PathBuf::from(
                positional
                    .first()
                    .expect("usage: jhtmltopdf render [--viewport-width=N] <file.html> [out.pdf]"),
            );
            let out = positional
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| src.with_extension("pdf"));
            let t0 = Instant::now();
            let pdf = render_with(
                &std::fs::read(&src).expect("read input"),
                Options {
                    viewport_px: viewport,
                    page_size,
                    margin_mm,
                },
            );
            std::fs::write(&out, &pdf).expect("write output");
            println!(
                "{} -> {} ({} bytes, {:.1}ms)",
                src.display(),
                out.display(),
                pdf.len(),
                t0.elapsed().as_secs_f64() * 1e3
            );
        }
        "bench" => {
            println!("jhtmltopdf benchmark harness (cold, single run per case)");
            for (name, html) in CASES {
                let t0 = Instant::now();
                let pdf = render(html.as_bytes());
                println!(
                    "  {name:14} {:8.1}ms  {:9.1} KB",
                    t0.elapsed().as_secs_f64() * 1e3,
                    pdf.len() as f64 / 1024.0
                );
            }
        }
        other => {
            eprintln!(
                "unknown mode: {other}. usage: jhtmltopdf [bench | render <file.html> [out.pdf]]"
            );
            std::process::exit(2);
        }
    }
}
