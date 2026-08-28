//! jhtmltopdf: pure-Rust HTML to PDF engine.
//!
//! Pipeline: parse, cascade, layout, write. Each stage is its own crate;
//! this facade threads them together. See SPEC.md in the repo root.

use jhtml_css::Stylesheet;
use jhtml_layout::layout_scaled;
use jhtml_parse::Document;
use jhtml_pdf::Metadata;

/// Render options.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Screen viewport width in CSS pixels the document was designed for.
    /// When set, the render is scaled from that viewport down to the paper
    /// width (wkhtmltopdf-style shrink-to-fit). None = render at paper size.
    pub viewport_px: Option<f32>,
    /// Paper size: ("a4", 595, 842) or ("letter", 612, 792) in pt.
    pub page_size: Option<(&'static str, f32, f32)>,
    /// Page margin in mm (all sides), used when the CSS has no @page rule.
    pub margin_mm: Option<f32>,
}

/// Render HTML bytes into PDF bytes with default options.
pub fn render(html: &[u8]) -> Vec<u8> {
    render_with(html, Options::default())
}

/// Render HTML bytes into PDF bytes with explicit options.
pub fn render_with(html: &[u8], opts: Options) -> Vec<u8> {
    let doc = Document::parse(html);
    let ss = Stylesheet::parse(&doc.style_rules());
    let mut ss = ss;
    let mut phys_w = 595.0f32;
    let mut margin = 28.35f32;
    if let Some((_, w, h)) = opts.page_size {
        ss.page.width_pt = Some(w);
        ss.page.height_pt = Some(h);
        phys_w = w;
    }
    if let Some(mm) = opts.margin_mm {
        let m = mm * 72.0 / 25.4;
        ss.page.margins_pt = Some([m; 4]);
        margin = m;
    }
    let phys_content = phys_w - 2.0 * margin;
    let scale = opts
        .viewport_px
        .map(|vp| phys_content / (vp * 0.75))
        .unwrap_or(1.0);
    let result = layout_scaled(&doc, &ss, scale);
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
