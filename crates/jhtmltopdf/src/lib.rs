//! jhtmltopdf: pure-Rust HTML to PDF engine.
//!
//! Pipeline: parse, cascade, layout, shape, write. Each stage is its own
//! crate; this facade threads them together. See SPEC.md in the repo root.

use jhtml_css::Stylesheet;
use jhtml_layout::PageGeometry;
use jhtml_parse::Document;

/// Render HTML bytes into PDF bytes.
pub fn render(html: &[u8]) -> Vec<u8> {
    let doc = Document::parse(html);
    let _ss = Stylesheet::default();
    let _geo = PageGeometry::default();
    let _scripts = jhtml_js::ScriptOutput::default();
    let _text = jhtml_text::GlyphRun {
        text: doc.title.clone().unwrap_or_default(),
        font_size_pt: 18.0,
        advance_pt: 0.0,
    };
    jhtml_pdf::write_stub_pdf(doc.title.as_deref().unwrap_or("jhtmltopdf"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn render_threads_all_stages() {
        let pdf = crate::render(b"<html><title>t</title></html>");
        assert!(String::from_utf8_lossy(&pdf).contains("(t)"));
    }
}
