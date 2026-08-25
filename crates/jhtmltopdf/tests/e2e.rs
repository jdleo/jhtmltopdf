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
    let s = String::from_utf8_lossy(&pdf);
    assert!(s.contains("(Simple)") && s.contains("(Benchmark:)") && s.contains("(HTML)"));
}

#[test]
fn case3_paged_media_furniture() {
    let html = include_str!("../benches/cases/case3_complex.html");
    let pdf = render(html.as_bytes());
    let s = String::from_utf8_lossy(&pdf);
    // Page counter footers on every page.
    assert!(s.contains("Page 1 of 121"), "missing footer page 1");
    assert!(s.contains("Page 121 of 121"), "missing footer page 121");
    // Outline: h1 + 60 section headings.
    assert!(s.contains("/Type /Outlines"));
    assert!(s.contains("/Title (Financial Data Compendium)"));
    // Internal link annotations resolve to destinations.
    assert!(s.contains("/Dest ["), "no internal links");
    // Running title in top margin box.
    assert!(s.contains("Financial Data Compendium") == true);
}

#[test]
fn case1_external_links_annotated() {
    let html = include_str!("../benches/cases/case1_simple.html");
    let pdf = render(html.as_bytes());
    let s = String::from_utf8_lossy(&pdf);
    assert!(s.contains("/URI (https://example.com)"));
}
