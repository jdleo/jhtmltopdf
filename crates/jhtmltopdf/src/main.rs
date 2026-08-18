use std::path::PathBuf;
use std::time::Instant;

use jhtmltopdf::render;

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
            let src = PathBuf::from(
                std::env::args()
                    .nth(2)
                    .expect("usage: jhtmltopdf render <file.html> [out.pdf]"),
            );
            let out = std::env::args()
                .nth(3)
                .map(PathBuf::from)
                .unwrap_or_else(|| src.with_extension("pdf"));
            let t0 = Instant::now();
            let pdf = render(&std::fs::read(&src).expect("read input"));
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
