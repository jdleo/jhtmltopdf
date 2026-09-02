//! Stage 1: HTML bytes into a lightweight DOM tree.
//!
//! Backed by html5ever for spec-true parsing. We flatten into our own
//! simple tree so downstream stages stay dependency-free.

use html5ever::parse_document;
use html5ever::tendril::{Tendril, TendrilSink};
use markup5ever_rcdom::{NodeData, RcDom};

/// A parsed document: a flat arena of nodes rooted at `root`.
#[derive(Debug, Clone)]
pub struct Document {
    pub root: Node,
}

#[derive(Debug, Clone)]
pub enum Node {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
    /// Comments, doctypes, processing instructions: parsed, never rendered.
    Ignored,
}

impl Node {
    pub fn element(tag: &str) -> Node {
        Node::Element {
            tag: tag.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn tag(&self) -> Option<&str> {
        match self {
            Node::Element { tag, .. } => Some(tag),
            _ => None,
        }
    }

    pub fn children(&self) -> &[Node] {
        match self {
            Node::Element { children, .. } => children,
            _ => &[],
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        match self {
            Node::Element { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// Concatenated direct text children (no recursion).
    pub fn direct_text(&self) -> String {
        match self {
            Node::Element { children, .. } => children
                .iter()
                .filter_map(|c| match c {
                    Node::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect(),
            Node::Text(t) => t.clone(),
            Node::Ignored => String::new(),
        }
    }
}

fn detach_rcdom(handle: &markup5ever_rcdom::Handle) {
    let mut stack = vec![handle.clone()];
    while let Some(node) = stack.pop() {
        let children: Vec<markup5ever_rcdom::Handle> =
            node.children.borrow_mut().drain(..).collect();
        stack.extend(children);
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        // Deep trees overflow the stack via recursive auto-drop; flatten.
        let mut stack = Vec::new();
        if let Node::Element { children, .. } = self {
            stack.append(children);
        }
        while let Some(mut node) = stack.pop() {
            if let Node::Element { children, .. } = &mut node {
                stack.append(children);
            }
        }
    }
}

impl Document {
    /// Parse raw HTML bytes (any encoding html5ever detects) into a tree.
    pub fn parse(html: &[u8]) -> Self {
        let dom: RcDom = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut std::io::Cursor::new(html))
            .expect("html5ever never fails, only recovers");
        let mut sink = Vec::new();
        let root = convert(&dom.document, &mut sink);
        // The rcdom tree drops recursively and overflows on deep input;
        // drain it iteratively before it dies.
        detach_rcdom(&dom.document);
        Document { root }
    }

    /// First `<title>` in the document, trimmed.
    pub fn title(&self) -> Option<String> {
        fn find(node: &Node) -> Option<String> {
            if node.tag() == Some("title") {
                let t = node.direct_text();
                if !t.trim().is_empty() {
                    return Some(t.trim().to_string());
                }
            }
            node.children().iter().find_map(find)
        }
        find(&self.root)
    }

    /// Concatenated text of every `<style>` element, in document order.
    pub fn style_rules(&self) -> String {
        fn walk(node: &Node, out: &mut String) {
            if node.tag() == Some("style") {
                out.push_str(&node.direct_text());
                out.push('\n');
            }
            for c in node.children() {
                walk(c, out);
            }
        }
        let mut out = String::new();
        walk(&self.root, &mut out);
        out
    }
}

fn convert(handle: &markup5ever_rcdom::Handle, sink: &mut Vec<()>) -> Node {
    let _ = sink;
    match &handle.data {
        NodeData::Document => Node::Element {
            tag: "#document".to_string(),
            attrs: Vec::new(),
            children: handle
                .children
                .borrow()
                .iter()
                .map(|c| convert(c, sink))
                .collect(),
        },
        NodeData::Element { name, attrs, .. } => Node::Element {
            tag: name.local.to_string(),
            attrs: attrs
                .borrow()
                .iter()
                .map(|a| (a.name.local.to_string(), a.value.to_string()))
                .collect(),
            children: handle
                .children
                .borrow()
                .iter()
                .map(|c| convert(c, sink))
                .collect(),
        },
        NodeData::Text { contents } => Node::Text(contents.borrow().to_string()),
        _ => Node::Ignored,
    }
}

/// Tendril sink plumbing re-exported so callers do not need tendril.
pub type AnyTendril = Tendril<tendril::fmt::UTF8>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree_with_attrs() {
        let doc = Document::parse(b"<div id='a' class='x'><p>hi</p>there</div>");
        assert_eq!(doc.root.tag(), Some("#document"));
        let html = &doc.root.children()[0];
        let body = &html.children()[1];
        assert_eq!(body.tag(), Some("body"));
        let div = &body.children()[0];
        assert_eq!(div.attr("id"), Some("a"));
        assert_eq!(div.children().len(), 2);
    }

    #[test]
    fn title_and_styles() {
        let doc = Document::parse(
            b"<html><head><title> t </title><style>p { color: red; }</style></head></html>",
        );
        assert_eq!(doc.title().as_deref(), Some("t"));
        assert!(doc.style_rules().contains("color: red"));
    }

    #[test]
    fn recovers_from_garbage() {
        let doc = Document::parse(b"<div><p>unclosed <b>tags");
        assert!(doc.root.tag() == Some("#document"));
    }
}
