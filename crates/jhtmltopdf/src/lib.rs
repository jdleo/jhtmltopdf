//! jhtmltopdf: pure-Rust HTML to PDF engine.
//!
//! Pipeline: parse, cascade, layout, write. Each stage is its own crate;
//! this facade threads them together. See SPEC.md in the repo root.

use jhtml_css::Stylesheet;
use jhtml_layout::layout;
use jhtml_parse::Document;

/// Render HTML bytes into PDF bytes.
pub fn render(html: &[u8]) -> Vec<u8> {
    let doc = Document::parse(html);
    let ss = Stylesheet::parse(&doc.style_rules());
    let pages = layout(&doc, &ss);
    jhtml_pdf::write_pdf(&pages, doc.title().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_doc_renders_text() {
        let pdf = crate::render(
            b"<html><head><title>t</title></head><body><p>hello world</p></body></html>",
        );
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("hello") && s.contains("world"));
    }
}
