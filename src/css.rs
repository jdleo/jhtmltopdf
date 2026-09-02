//! Stage 2: CSS parsing and the cascade.
//!
//! Hand-rolled minimal parser for the CSS subset v0.1 supports:
//! simple selectors (tag, .class, #id, combos), :nth-child(even/odd),
//! descendant combinators, and @page rules. servo cssparser swap-in is
//! tracked in SPEC.md open questions; this keeps the engine dep-free.

use std::collections::HashMap;

/// A computed style box. `None` = inherit / not set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    pub font_size_pt: Option<f32>,
    pub bold: Option<bool>,
    /// Numeric CSS font-weight (400 normal .. 900 black), for real face matching.
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub color: Option<[f32; 3]>,
    pub background: Option<Option<[f32; 3]>>,
    pub text_align: Option<Align>,
    pub display: Option<Display>,
    pub line_height: Option<f32>,
    pub margin: Option<[f32; 4]>,
    pub padding: Option<[f32; 4]>,
    pub border: Option<Border>,
    pub page_break_before: Option<Break>,
    pub page_break_after: Option<Break>,
    pub page_break_inside: Option<Break>,
    pub font_family: Option<String>,
    pub white_space: Option<WhiteSpace>,
    pub width: Option<Width>,
    pub gap_pt: Option<f32>,
    pub justify: Option<Justify>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    Flex,
    InlineFlex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Start,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Width {
    Pt(f32),
    Pct(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    Auto,
    Avoid,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    Pre,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width_pt: f32,
    pub color: [f32; 3],
}

impl Style {
    pub fn font_size(&self, d: f32) -> f32 {
        self.font_size_pt.unwrap_or(d)
    }
}

/// One CSS rule: a selector chain (last compound matches the element,
/// earlier compounds must match ancestors) and its declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub compounds: Vec<Compound>,
    pub decls: HashMap<String, String>,
    /// Source order for cascade tie-breaking.
    pub order: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compound {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub nth_child: Option<Nth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nth {
    Even,
    Odd,
}

/// All @page rules found (last one wins for size/margins).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageRule {
    pub width_pt: Option<f32>,
    pub height_pt: Option<f32>,
    pub margins_pt: Option<[f32; 4]>,
    /// Margin-box content strings: "top-center" | "bottom-center" -> raw value.
    pub margin_boxes: HashMap<String, MarginBox>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarginBox {
    pub box_name: String,
    /// Raw content: literal text, or counter(page)/counter(pages) markers.
    pub content: Vec<ContentToken>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    Text(String),
    PageNumber,
    PageCount,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub page: PageRule,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        parse_css(css)
    }
}

/// Parse a stylesheet: @page blocks plus normal rules, comments stripped.
pub fn parse_css(css: &str) -> Stylesheet {
    let mut sheet = Stylesheet::default();
    let cleaned = strip_comments(css);
    let mut order = 0usize;

    let mut rest = cleaned.as_str();
    while let Some(start) = rest.find('{') {
        let selector_part = rest[..start].trim();
        let Some(end_rel) = find_matching_brace(&rest[start..]) else {
            break;
        };
        let body = &rest[start + 1..start + end_rel];

        if selector_part.starts_with("@page") {
            apply_page_body(body, &mut sheet.page);
            if let Some(boxes) = parse_margin_boxes(selector_part, body) {
                sheet.page.margin_boxes.extend(boxes);
            }
        } else if selector_part.starts_with('@') {
            // Unknown at-rule: skip.
        } else {
            let decls = parse_decls(body);
            for sel in selector_part.split(',') {
                let compounds = parse_selector(sel);
                if !compounds.is_empty() {
                    sheet.rules.push(Rule {
                        compounds,
                        decls: decls.clone(),
                        order,
                    });
                    order += 1;
                }
            }
        }
        rest = &rest[start + end_rel + 1..];
    }
    sheet
}

/// Index (relative to input) of the `}` matching the `{` at index 0.
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i..].find("*/") {
            Some(j) => rest = &rest[i + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

pub fn parse_decls(body: &str) -> HashMap<String, String> {
    body.split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect()
}

/// Parse a selector with descendant combinators into compounds
/// (document order: ancestors first, matched element last).
fn parse_selector(sel: &str) -> Vec<Compound> {
    sel.split_whitespace()
        .filter(|s| !s.eq_ignore_ascii_case("and"))
        .map(parse_compound)
        .collect()
}

fn parse_compound(c: &str) -> Compound {
    let mut out = Compound {
        tag: None,
        id: None,
        classes: Vec::new(),
        nth_child: None,
    };
    // nth-child pseudo
    if let Some(idx) = c.find(":nth-child(") {
        let rest = &c[idx + 11..];
        if let Some(end) = rest.find(')') {
            match rest[..end].trim() {
                "even" => out.nth_child = Some(Nth::Even),
                "odd" => out.nth_child = Some(Nth::Odd),
                _ => {}
            }
        }
        let base = &c[..idx];
        if !base.is_empty() {
            let mut simple = parse_compound_simple(base);
            simple.nth_child = out.nth_child;
            return simple;
        }
        return out;
    }
    parse_compound_simple(c)
}

fn parse_compound_simple(c: &str) -> Compound {
    let mut out = Compound {
        tag: None,
        id: None,
        classes: Vec::new(),
        nth_child: None,
    };
    let mut part = String::new();
    let flush = |out: &mut Compound, part: &mut String| {
        if part.is_empty() {
            return;
        }
        if let Some(id) = part.strip_prefix('#') {
            out.id = Some(id.to_string());
        } else if let Some(cls) = part.strip_prefix('.') {
            out.classes.push(cls.to_string());
        } else {
            out.tag = Some(part.to_ascii_lowercase());
        }
        part.clear();
    };
    for ch in c.chars() {
        match ch {
            '.' | '#' => {
                flush(&mut out, &mut part);
                part.push(ch);
            }
            _ => part.push(ch),
        }
    }
    flush(&mut out, &mut part);
    out
}

/// Parse `@page { @top-center { content: "..." } ... }` margin boxes.
fn parse_margin_boxes(_selector: &str, body: &str) -> Option<HashMap<String, MarginBox>> {
    let mut out = HashMap::new();
    let mut rest = body;
    while let Some(at) = rest.find("@") {
        let after = &rest[at + 1..];
        let Some(brace) = after.find('{') else { break };
        let name = after[..brace].trim().to_ascii_lowercase();
        let Some(close) = after[brace..].find('}') else {
            break;
        };
        let inner = &after[brace + 1..brace + close];
        if let Some(c) = parse_decls(inner).get("content") {
            let mb = MarginBox {
                box_name: name.clone(),
                content: parse_content(c),
            };
            out.insert(name, mb);
        }
        rest = &after[brace + close + 1..];
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_content(v: &str) -> Vec<ContentToken> {
    let mut out = Vec::new();
    let mut rest = v;
    while !rest.is_empty() {
        if rest.starts_with("counter(page)") {
            out.push(ContentToken::PageNumber);
            rest = &rest["counter(page)".len()..];
        } else if rest.starts_with("counter(pages)") {
            out.push(ContentToken::PageCount);
            rest = &rest["counter(pages)".len()..];
        } else if rest.starts_with('"') {
            match rest[1..].find('"') {
                Some(i) => {
                    out.push(ContentToken::Text(rest[1..1 + i].to_string()));
                    rest = &rest[1 + i + 1..];
                }
                None => break,
            }
        } else {
            rest = &rest[1..];
        }
    }
    out
}

fn apply_page_body(body: &str, page: &mut PageRule) {
    let decls = parse_decls(body);
    if let Some(size) = decls.get("size") {
        let parts: Vec<&str> = size.split_whitespace().collect();
        let dim = |s: &str| parse_pt(s);
        match parts.as_slice() {
            ["a4"] | ["A4"] => {
                page.width_pt = Some(595.0);
                page.height_pt = Some(842.0);
            }
            ["letter"] | ["LETTER"] => {
                page.width_pt = Some(612.0);
                page.height_pt = Some(792.0);
            }
            [w, h] => {
                page.width_pt = dim(w);
                page.height_pt = dim(h);
            }
            _ => {}
        }
    }
    if let Some(m) = decls.get("margin") {
        let parts: Vec<f32> = m.split_whitespace().filter_map(parse_pt).collect();
        if parts.len() == 1 {
            page.margins_pt = Some([parts[0]; 4]);
        } else if parts.len() == 2 {
            page.margins_pt = Some([parts[0], parts[1], parts[0], parts[1]]);
        } else if parts.len() == 4 {
            page.margins_pt = Some([parts[0], parts[1], parts[2], parts[3]]);
        }
    }
}

pub fn parse_pt(s: &str) -> Option<f32> {
    parse_pt_rel(s, 0.0)
}

/// Parse a CSS length. `em` resolves against `parent_size_pt`.
pub fn parse_pt_rel(s: &str, parent_size_pt: f32) -> Option<f32> {
    let s = s.trim();
    let (num, unit) = s.split_at(
        s.find(|c: char| c.is_alphabetic() || c == '%')
            .unwrap_or(s.len()),
    );
    let n: f32 = num.trim().parse().ok()?;
    Some(match unit {
        "pt" => n,
        "px" => n * 0.75,
        "in" => n * 72.0,
        "cm" => n * 28.3465,
        "mm" => n * 2.83465,
        "em" | "rem" if parent_size_pt > 0.0 => n * parent_size_pt,
        "%" => n / 100.0 * parent_size_pt.max(0.001),
        _ => n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules_selectors_and_decls() {
        let ss = Stylesheet::parse("h2, p.x { color: red; font-size: 14pt } #a.b { color: blue }");
        assert_eq!(ss.rules.len(), 3);
        assert_eq!(ss.rules[0].compounds[0].tag.as_deref(), Some("h2"));
        assert_eq!(ss.rules[1].compounds[0].tag.as_deref(), Some("p"));
        assert_eq!(ss.rules[1].compounds[0].classes, vec!["x"]);
        assert_eq!(ss.rules[2].compounds[0].id.as_deref(), Some("a"));
        assert_eq!(ss.rules[2].compounds[0].classes, vec!["b"]);
    }

    #[test]
    fn nth_child_and_comments() {
        let ss = Stylesheet::parse("/* c */ tr:nth-child(even) td { background: #eef4fb }");
        assert_eq!(ss.rules[0].compounds[0].tag.as_deref(), Some("tr"));
        assert_eq!(ss.rules[0].compounds[0].nth_child, Some(Nth::Even));
        assert_eq!(ss.rules[0].compounds[1].tag.as_deref(), Some("td"));
    }

    #[test]
    fn parses_page_rule() {
        let ss = Stylesheet::parse("@page { size: A4; margin: 2cm 1.8cm; }");
        assert_eq!(ss.page.width_pt, Some(595.0));
        let m = ss.page.margins_pt.unwrap();
        assert!((m[0] - 56.7).abs() < 0.05 && (m[1] - 51.0).abs() < 0.05);
    }

    #[test]
    fn parses_margin_boxes() {
        let ss = Stylesheet::parse(
            "@page { @bottom-center { content: \"Page \" counter(page) \" of \" counter(pages); } }",
        );
        let mb = ss.page.margin_boxes.get("bottom-center").expect("box");
        assert_eq!(mb.content.len(), 4);
        assert_eq!(mb.content[0], ContentToken::Text("Page ".into()));
        assert_eq!(mb.content[1], ContentToken::PageNumber);
        assert_eq!(mb.content[3], ContentToken::PageCount);
    }

    #[test]
    fn descendant_selectors_split() {
        let ss = Stylesheet::parse("ul li { color: red }");
        assert_eq!(ss.rules[0].compounds.len(), 2);
        assert_eq!(ss.rules[0].compounds[0].tag.as_deref(), Some("ul"));
    }
}
