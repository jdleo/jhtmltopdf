//! End-to-end contract: every stage touched, valid PDF out, for all 3
//! benchmark cases. These are the gates every shipment must keep green.

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

#[test]
fn all_benchmark_cases_produce_valid_pdfs() {
    for (name, html) in CASES {
        let pdf = render(html.as_bytes());
        assert!(pdf.starts_with(b"%PDF-1.4"), "{name}: bad header");
        assert!(pdf.windows(5).any(|w| w == b"%%EOF"), "{name}: no EOF");
        assert!(pdf.windows(4).any(|w| w == b"xref"), "{name}: no xref");
        assert!(pdf.len() > 400, "{name}: suspiciously small output");
    }
}

#[test]
fn case1_title_lands_in_output() {
    let pdf = render(include_str!("../benches/cases/case1_simple.html").as_bytes());
    assert!(String::from_utf8_lossy(&pdf).contains("(Simple Benchmark)"));
}
