//! Edge-case torture suite: nothing panics, everything produces valid PDFs.

use jhtmltopdf::{render, render_with, Options};

fn assert_valid(pdf: &[u8]) {
    assert!(pdf.starts_with(b"%PDF-1.4"), "bad header");
    assert!(pdf.windows(5).any(|w| w == b"%%EOF"), "no EOF");
    assert!(pdf.windows(4).any(|w| w == b"xref"), "no xref");
}

#[test]
fn empty_document() {
    assert_valid(&render(b""));
}

#[test]
fn garbage_bytes() {
    assert_valid(&render(&[0x00, 0xff, 0xfe, b'<', b'/', 0x80, b'x']));
}

#[test]
fn deeply_nested_elements() {
    let mut html = String::from("<body>");
    for _ in 0..500 {
        html.push_str("<div>");
    }
    html.push_str("deep");
    for _ in 0..500 {
        html.push_str("</div>");
    }
    html.push_str("</body>");
    assert_valid(&render(html.as_bytes()));
}

#[test]
fn malformed_css_ignored() {
    let html = br#"<style>p { color: ;;; font-size: ???; } @media nonsense { { } h1 {{</style>
        <p>text</p>"#;
    assert_valid(&render(html));
}

#[test]
fn unclosed_tags_everywhere() {
    assert_valid(&render(
        b"<div><p><b><i><u>never closed <li>orphans <table><tr><td>x",
    ));
}

#[test]
fn massive_page_count() {
    let mut html = String::from(r#"<style>section { page-break-before: always; }</style>"#);
    for i in 0..300 {
        html.push_str(&format!("<section>Section {i}</section>"));
    }
    let pdf = render(html.as_bytes());
    assert_valid(&pdf);
    let s = String::from_utf8_lossy(&pdf);
    assert_eq!(s.matches("/Type /Page ").count(), 300);
}

#[test]
fn hostile_strings_escaped() {
    let html = br#"<p>(())))\ \ <script>not run as layout</script>"#;
    assert_valid(&render(html));
}

#[test]
fn viewport_scaling_does_not_panic() {
    let html = b"<body><p>scale me</p></body>";
    for vp in [1.0, 100.0, 794.0, 990.0, 100000.0] {
        let pdf = render_with(
            html,
            Options {
                viewport_px: Some(vp),
                ..Options::default()
            },
        );
        assert_valid(&pdf);
    }
}

#[test]
fn unicode_and_emoji_degrade() {
    let html: &[u8] =
        "<p>caf\u{e9} na\u{ef}ve \u{2022} \u{2764}\u{fe0f} \u{1f600} r\u{e9}sum\u{e9}</p>"
            .as_bytes();
    let pdf = render(html);
    assert_valid(&pdf);
}

#[test]
fn very_long_unbroken_word() {
    let word = "x".repeat(50_000);
    let html = format!("<p>{word}</p>");
    assert_valid(&render(html.as_bytes()));
}

#[test]
fn huge_table() {
    let mut html = String::from("<table>");
    for i in 0..200 {
        html.push_str(&format!("<tr><td>cell {i} a</td><td>cell {i} b</td></tr>"));
    }
    html.push_str("</table>");
    assert_valid(&render(html.as_bytes()));
}
