//! jhtmltopdf: pure-Rust HTML to PDF engine.
//!
//! Pipeline: parse, cascade, layout, write. Each stage is its own crate;
//! this facade threads them together. See SPEC.md in the repo root.

use jhtml_css::Stylesheet;
use jhtml_layout::layout;
use jhtml_parse::Document;
use jhtml_pdf::Metadata;

/// Render HTML bytes into PDF bytes.
pub fn render(html: &[u8]) -> Vec<u8> {
    let doc = Document::parse(html);
    let ss = Stylesheet::parse(&doc.style_rules());
    let result = layout(&doc, &ss);
    let author = find_meta_author(&doc);
    jhtml_pdf::write_pdf(
        &result.pages,
        &result.outline,
        &result.dests,
        &Metadata {
            title: doc.title(),
            author,
        },
    )
}

fn find_meta_author(doc: &Document) -> Option<String> {
    fn walk(node: &jhtml_parse::Node) -> Option<String> {
        if node.tag() == Some("meta") && node.attr("name") == Some("author") {
            if let Some(a) = node.attr("content") {
                return Some(a.to_string());
            }
        }
        node.children().iter().find_map(walk)
    }
    walk(&doc.root)
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
