//! jhtmltopdf: pure-Rust HTML to PDF engine.
//!
//! Pipeline: parse, cascade, layout, write, one crate with one module per
//! stage. Print-first, no browser engine, no native dependencies.

pub mod css;
pub mod js;
pub mod layout;
pub mod parse;
pub mod pdf;
pub mod text;

pub use crate::css::Stylesheet;
pub use crate::js::{run_scripts, substitute, ScriptError, ScriptOutput};
pub use crate::layout::{layout, layout_with, LayoutResult, Op, Page, Target};
pub use crate::parse::Document;
pub use crate::pdf::{write_pdf, Metadata};
pub use crate::text::{Font, FontStore};

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
///
/// Runs on a dedicated thread with a large stack: document trees nest
/// recursively (parse, walk, collect) and real-world HTML can nest
/// hundreds of levels deep, past the default main-thread stack.
pub fn render_with(html: &[u8], opts: Options) -> Vec<u8> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn_scoped(scope, move || render_inner(html, opts))
            .expect("spawn render thread")
            .join()
            .expect("render thread panicked")
    })
}

fn render_inner(html: &[u8], opts: Options) -> Vec<u8> {
    let mut doc = Document::parse(html);

    // Compute-only JS: run scripts, then inject `{{key}}` values.
    let scripts = collect_scripts(&doc);
    let data = if scripts.is_empty() {
        Default::default()
    } else {
        crate::js::run_scripts(&scripts).unwrap_or_default().data
    };
    if !data.is_empty() {
        substitute_texts(&mut doc, &data);
    }

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
    let fonts = crate::text::FontStore::with_system_fonts();
    let result = layout_with(&doc, &ss, &fonts, scale);
    let author = find_meta_author(&doc);
    crate::pdf::write_pdf(
        &result.pages,
        &result.outline,
        &result.dests,
        &Metadata {
            title: doc.title(),
            author,
        },
        &fonts,
    )
}

fn collect_scripts(doc: &Document) -> Vec<String> {
    fn walk(node: &crate::parse::Node, out: &mut Vec<String>) {
        if node.tag() == Some("script") && node.attr("src").is_none() {
            let code = node.direct_text();
            if !code.trim().is_empty() {
                out.push(code);
            }
        }
        for c in node.children() {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(&doc.root, &mut out);
    out
}

fn substitute_texts(doc: &mut Document, data: &HashMap<String, String>) {
    fn walk(node: &mut crate::parse::Node, data: &HashMap<String, String>) {
        match node {
            crate::parse::Node::Text(t) => *t = crate::js::substitute(t, data),
            crate::parse::Node::Element { children, .. } => {
                for c in children {
                    walk(c, data);
                }
            }
            crate::parse::Node::Ignored => {}
        }
    }
    walk(&mut doc.root, data);
}

use std::collections::HashMap;

fn find_meta_author(doc: &Document) -> Option<String> {
    fn walk(node: &crate::parse::Node) -> Option<String> {
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
