//! Stage 3: box layout and pagination.
//!
//! Document-order walk -> styled blocks -> word-wrapped lines -> laid-out
//! pages of draw ops. The output `Page` list is the canonical intermediate:
//! jhtml-pdf turns it 1:1 into PDF objects. The layout tree IS the PDF.

use jhtml_css::{
    parse_pt, parse_pt_rel, Break, Compound, ContentToken, Display, Justify, Nth, Rule, Style,
    Stylesheet, Width,
};
use jhtml_parse::{Document, Node};
use jhtml_text::{measure, Font};

/// One drawing instruction on a page.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Text {
        font: Font,
        size: f32,
        /// PostScript points, origin bottom-left.
        x: f32,
        y: f32,
        text: String,
        color: [f32; 3],
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 3],
    },
    Link {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        target: Target,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Url(String),
    /// Internal destination name.
    Dest(String),
}

/// A laid-out page of draw ops.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub width_pt: f32,
    pub height_pt: f32,
    pub ops: Vec<Op>,
}

/// A bookmark in the document outline (h1-h6).
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    /// 1-based page index.
    pub page: usize,
}

/// Internal destinations: anchor id -> (1-based page, y from top in pt).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Destinations {
    pub map: HashMap<String, (usize, f32)>,
}

/// Full layout result: pages plus document furniture.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    pub pages: Vec<Page>,
    pub outline: Vec<OutlineItem>,
    pub dests: Destinations,
}

/// Layout a document into pages using the given stylesheet.
pub fn layout(doc: &Document, ss: &Stylesheet) -> LayoutResult {
    layout_scaled(doc, ss, 1.0)
}

/// Layout with a scale factor < 1.0: the document is laid out on a
/// proportionally larger virtual page (screen viewport semantics) and
/// scaled down to the physical page, reproducing wkhtmltopdf-style
/// shrink-to-fit for documents designed against a wide viewport.
pub fn layout_scaled(doc: &Document, ss: &Stylesheet, scale: f32) -> LayoutResult {
    let mut engine = Engine::new(doc, ss);
    if scale != 1.0 {
        engine.geo.width_pt /= scale;
        engine.geo.height_pt /= scale;
        engine.geo.margins = engine.geo.margins.map(|m| m / scale);
        engine.pages[0].width_pt = engine.geo.width_pt;
        engine.pages[0].height_pt = engine.geo.height_pt;
    }
    engine.run(doc);
    let mut r = LayoutResult {
        pages: engine.pages,
        outline: engine.outline,
        dests: engine.dests,
    };
    if scale != 1.0 {
        let k = scale;
        for page in &mut r.pages {
            page.width_pt *= k;
            page.height_pt *= k;
            scale_ops(&mut page.ops, k);
        }
        for (_, (_, y)) in r.dests.map.iter_mut() {
            *y *= k;
        }
    }
    r
}

fn scale_ops(ops: &mut [Op], k: f32) {
    for op in ops.iter_mut() {
        match op {
            Op::Text { x, y, size, .. } => {
                *x *= k;
                *y *= k;
                *size *= k;
            }
            Op::Rect { x, y, w, h, .. } | Op::Link { x, y, w, h, .. } => {
                *x *= k;
                *y *= k;
                *w *= k;
                *h *= k;
            }
        }
    }
}

struct Engine<'a> {
    ss: &'a Stylesheet,
    pages: Vec<Page>,
    geo: PageGeometry,
    // Cursor: current page + y position from top.
    page_idx: usize,
    y: f32,
    /// Loose inline content (text/inline elements outside blocks) waiting
    /// to be flushed as an anonymous block before the next block layout.
    pending: Vec<Segment>,
    outline: Vec<OutlineItem>,
    dests: Destinations,
}

#[derive(Debug, Clone, Copy)]
struct PageGeometry {
    width_pt: f32,
    height_pt: f32,
    margins: [f32; 4],
}

impl<'a> Engine<'a> {
    fn new(_doc: &'a Document, ss: &'a Stylesheet) -> Self {
        let pr = &ss.page;
        let geo = PageGeometry {
            width_pt: pr.width_pt.unwrap_or(595.0),
            height_pt: pr.height_pt.unwrap_or(842.0),
            margins: pr.margins_pt.unwrap_or([56.7; 4]),
        };
        Self {
            ss,
            geo,
            pages: vec![Page {
                width_pt: geo.width_pt,
                height_pt: geo.height_pt,
                ops: Vec::new(),
            }],
            y: 0.0,
            page_idx: 0,
            pending: Vec::new(),
            outline: Vec::new(),
            dests: Destinations::default(),
        }
    }

    fn content_width(&self) -> f32 {
        self.geo.width_pt - self.geo.margins[0] - self.geo.margins[2]
    }

    fn top(&self) -> f32 {
        self.geo.margins[1]
    }

    fn bottom(&self) -> f32 {
        self.geo.height_pt - self.geo.margins[3]
    }

    fn cur(&mut self) -> &mut Page {
        &mut self.pages[self.page_idx]
    }

    fn push_op(&mut self, op: Op) {
        self.cur().ops.push(op);
    }

    fn page_break(&mut self) {
        self.pages.push(Page {
            width_pt: self.geo.width_pt,
            height_pt: self.geo.height_pt,
            ops: Vec::new(),
        });
        self.page_idx += 1;
        self.y = self.top();
    }

    /// Ensure `needed_pt` fits on the current page, else break.
    fn ensure(&mut self, needed_pt: f32) {
        if self.y + needed_pt > self.bottom() && self.y > self.top() {
            self.page_break();
        }
    }

    fn run(&mut self, doc: &Document) {
        self.y = self.top();
        if let Node::Element { children, .. } = &doc.root {
            for c in children {
                self.walk(c, &Resolved::root());
            }
        }
        self.flush_pending();
        self.render_margin_boxes();
    }

    /// Render @page margin boxes (top-center / bottom-center) on every page.
    fn render_margin_boxes(&mut self) {
        let boxes: Vec<(String, Vec<ContentToken>)> = self
            .ss
            .page
            .margin_boxes
            .iter()
            .map(|(k, v)| (k.clone(), v.content.clone()))
            .collect();
        if boxes.is_empty() {
            return;
        }
        let n = self.pages.len();
        let (w, h) = (self.geo.width_pt, self.geo.height_pt);
        let (top, bottom) = (self.geo.margins[1], self.geo.margins[3]);
        for (i, page) in self.pages.iter_mut().enumerate() {
            for (name, tokens) in &boxes {
                let text: String = tokens
                    .iter()
                    .map(|t| match t {
                        ContentToken::Text(s) => s.clone(),
                        ContentToken::PageNumber => (i + 1).to_string(),
                        ContentToken::PageCount => n.to_string(),
                    })
                    .collect();
                if text.is_empty() {
                    continue;
                }
                let width = measure(&text, Font::Helvetica, 9.0);
                let x = (w - width) / 2.0;
                let y = if name.starts_with("top") {
                    h - top / 2.0
                } else {
                    bottom + bottom / 2.0 - 3.0
                };
                page.ops.push(Op::Text {
                    font: Font::Helvetica,
                    size: 9.0,
                    x,
                    y,
                    text,
                    color: [0.4, 0.4, 0.4],
                });
            }
        }
    }

    fn walk(&mut self, node: &Node, parent: &Resolved) {
        let Node::Element { tag, .. } = node else {
            return;
        };
        match tag.as_str() {
            "style" | "script" | "head" | "title" | "meta" | "link" => return,
            _ => {}
        }
        let style = self.resolve(node, parent);
        if style.display == Some(Display::None) {
            return;
        }
        if is_block_tag(tag) {
            self.flush_pending();
            self.layout_block(node, &style);
            self.flush_pending();
        } else {
            // Inline or transparent: queue text, recurse for descendants.
            for c in node.children() {
                if let Node::Text(t) = c {
                    for word in t.split_whitespace() {
                        self.pending.push(Segment {
                            text: word.to_string(),
                            size: style.font_size(12.0),
                            font: font_for(&style),
                            color: style.color.unwrap_or([0.08, 0.08, 0.08]),
                            x: 0.0,
                            link: None,
                        });
                    }
                } else {
                    self.walk(c, &Resolved::inherited(parent, style.clone()));
                }
            }
        }
    }

    /// Emit queued loose inline content as an anonymous block.
    fn flush_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let segs = std::mem::take(&mut self.pending);
        self.wrap_and_emit(&segs, &Style::default(), None);
    }

    fn layout_block(&mut self, node: &Node, style: &Style) {
        let tag = node.tag().unwrap_or("").to_string();
        // Page break directives.
        if style.page_break_before == Some(Break::Always)
            && (self.y > self.top() || self.page_idx > 0)
        {
            self.page_break();
        }

        // Block margins (CSS + defaults).
        let fs = style.font_size(12.0);
        let margin = style.margin.unwrap_or(default_margin(&tag, fs));
        let padding = style.padding.unwrap_or([0.0; 4]);
        self.y += margin[0] + padding[0];

        // Horizontal rule: thin gray line.
        if tag == "hr" {
            let line_color = style.border.map(|b| b.color).unwrap_or([0.72; 3]);
            self.push_op(Op::Rect {
                x: self.geo.margins[0],
                y: pdf_y(self.y, self.geo.height_pt, 0.7),
                w: self.content_width(),
                h: 0.7,
                color: line_color,
            });
            self.y += 4.0;
            self.y += margin[3] + padding[3];
            return;
        }

        // Record anchor destination.
        if let Some(id) = node.attr("id") {
            self.dests
                .map
                .insert(id.to_string(), (self.page_idx + 1, self.y));
        }

        // Table handling: rows are blocks of cells laid out horizontally.
        if tag == "table" {
            self.layout_table(node, style);
            self.y += margin[3] + padding[3];
            if style.page_break_after == Some(Break::Always) {
                self.page_break();
            }
            return;
        }

        // Flex row: children laid out horizontally.
        if style.display == Some(Display::Flex) || style.display == Some(Display::InlineFlex) {
            self.layout_flex_row(node, style);
            self.y += margin[3] + padding[3];
            if style.page_break_after == Some(Break::Always) {
                self.page_break();
            }
            return;
        }

        // Headings feed the outline (title from full inline text).
        if let Some(level) = heading_level(&tag) {
            let mut all = Vec::new();
            let mut none = None;
            self.collect_inline(node, style, &mut none, &mut all);
            let title: Vec<String> = all
                .iter()
                .filter(|s| s.text != "\n")
                .map(|s| s.text.clone())
                .collect();
            self.outline.push(OutlineItem {
                level,
                title: title.join(" "),
                page: self.page_idx + 1,
            });
        }

        // Walk children: accumulate inline runs, recurse into nested blocks.
        let mut segments: Vec<Segment> = Vec::new();
        if tag == "li" {
            segments.push(Segment {
                text: "\u{2022}".into(),
                size: style.font_size(12.0),
                font: font_for(style),
                color: style.color.unwrap_or([0.08, 0.08, 0.08]),
                x: 0.0,
                link: None,
            });
        }
        for c in node.children() {
            match c {
                Node::Text(t) => {
                    for word in t.split_whitespace() {
                        segments.push(Segment {
                            text: word.to_string(),
                            size: style.font_size(12.0),
                            font: font_for(style),
                            color: style.color.unwrap_or([0.08, 0.08, 0.08]),
                            x: 0.0,
                            link: None,
                        });
                    }
                }
                Node::Element { .. } => {
                    let ct = c.tag().unwrap_or("");
                    if is_block_tag(ct) {
                        self.wrap_and_emit(&segments, style, None);
                        segments = Vec::new();
                        self.walk(c, &Resolved::inherited(&Resolved::default(), style.clone()));
                    } else {
                        let child_style = self
                            .resolve(c, &Resolved::inherited(&Resolved::default(), style.clone()));
                        let mut child_href = c.attr("href").map(str::to_string);
                        self.collect_inline(c, &child_style, &mut child_href, &mut segments);
                    }
                }
                Node::Ignored => {}
            }
        }
        self.wrap_and_emit(&segments, style, None);

        self.y += margin[3] + padding[3];

        if style.page_break_after == Some(Break::Always) {
            self.page_break();
        }
    }
    fn layout_flex_row(&mut self, node: &Node, style: &Style) {
        let content_w = self.content_width();
        let children: Vec<&Node> = node
            .children()
            .iter()
            .filter(|c| c.tag().is_some())
            .collect();
        if children.is_empty() {
            return;
        }

        // Resolve widths and pre-collect inline segments (borrow-safe).
        let mut child_data: Vec<(Style, f32, Vec<Segment>)> = Vec::new();
        for c in &children {
            let cs = self.resolve(c, &Resolved::root());
            let mut segs = Vec::new();
            self.collect_inline(c, &cs, &mut None, &mut segs);
            let w = match cs.width {
                Some(Width::Pt(w)) => w,
                Some(Width::Pct(p)) => content_w * p / 100.0,
                None => {
                    // Natural single-line width of the child's content.
                    let mut nat = 0.0f32;
                    for s in &segs {
                        nat += measure(&s.text, s.font(), s.size);
                    }
                    nat += segs.len().saturating_sub(1) as f32
                        * measure(" ", Font::Helvetica, cs.font_size(12.0));
                    nat.max(1.0)
                }
            };
            child_data.push((cs, w.min(content_w), segs));
        }

        let n = child_data.len();
        let mut total: f32 = child_data.iter().map(|(_, w, _)| *w).sum();
        // Columns wider than the row: shrink them proportionally so
        // space-between never produces negative gaps or overlaps.
        if total > content_w && total > 0.0 {
            let k = content_w / total;
            for (_, w, _) in child_data.iter_mut() {
                *w *= k;
            }
            total = content_w;
        }
        let gap = match (style.justify, style.gap_pt) {
            (Some(Justify::SpaceBetween), _) if n > 1 => (content_w - total) / (n - 1) as f32,
            (_, g) => g.unwrap_or(0.0),
        };

        let mut x = self.geo.margins[0];
        let start_y = self.y;
        let mut max_h = 0.0f32;
        for (cs, w, segs) in &child_data {
            let fs = cs.font_size(12.0);
            let lh = cs.line_height.unwrap_or(1.15) * fs;
            let lines = self.wrap_segments(segs, *w, lh);
            let mut ly = start_y;
            for line in &lines {
                let line_w: f32 = line
                    .last()
                    .map(|s| s.x + measure(&s.text, s.font(), s.size))
                    .unwrap_or(0.0);
                let x0 = match cs.text_align.unwrap_or(jhtml_css::Align::Left) {
                    jhtml_css::Align::Left => x,
                    jhtml_css::Align::Center => x + (w - line_w) / 2.0,
                    jhtml_css::Align::Right => x + w - line_w,
                };
                ly += lh;
                let baseline = pdf_y(ly, self.geo.height_pt, 0.0);
                for seg in line {
                    if seg.text == "\n" {
                        continue;
                    }
                    self.push_op(Op::Text {
                        font: seg.font(),
                        size: seg.size,
                        x: x0 + seg.x,
                        y: baseline,
                        text: seg.text.clone(),
                        color: seg.color,
                    });
                }
            }
            max_h = max_h.max(lines.len() as f32 * lh);
            x += w + gap;
        }
        self.y += max_h;
    }

    /// Tables: rows stack vertically, cells flow horizontally in columns.
    fn layout_table(&mut self, table: &Node, style: &Style) {
        let fs = style.font_size(12.0);
        let rows: Vec<&Node> = collect_rows(table);
        let ncols = rows.iter().map(|r| r.children().len()).max().unwrap_or(1);
        let content_w = self.content_width() - style.padding.map(|p| p[1] + p[3]).unwrap_or(0.0);
        let col_w = content_w / ncols as f32;
        let mut nth = 0usize;

        for row in rows {
            let row_style = self.resolve(row, &Resolved::root());
            let row_fs = row_style.font_size(fs);
            let cells = row.children();
            // Measure cell heights: wrap each cell's words in its column.
            let mut cell_lines: Vec<Vec<Vec<Segment>>> = Vec::new();
            let mut max_h = 0.0f32;
            for cell in cells.iter() {
                let cell_style = self.resolve(
                    cell,
                    &Resolved::inherited(&Resolved::root(), row_style.clone()),
                );
                let mut segs = Vec::new();
                self.collect_inline(cell, &cell_style, &mut None, &mut segs);
                let lines = self.wrap_segments(
                    &segs,
                    col_w - 8.0,
                    cell_style.line_height.unwrap_or(1.15) * cell_style.font_size(row_fs),
                );
                let h = lines.len() as f32
                    * cell_style.line_height.unwrap_or(1.15)
                    * cell_style.font_size(row_fs)
                    + cell_style.padding.map(|p| p[0] + p[3]).unwrap_or(6.0);
                max_h = max_h.max(h);
                cell_lines.push(lines);
            }
            let row_h = max_h.max(12.0);

            // Zebra striping via :nth-child(even) on the row.
            nth += 1;
            if self.rule_matches_nth(row, nth) {
                if let Some(bg) = row_style
                    .background
                    .unwrap_or(None)
                    .or(self.bg_from_nth_rule())
                {
                    let y_top = self.y;
                    self.push_op(Op::Rect {
                        x: self.geo.margins[0],
                        y: pdf_y(y_top, self.geo.height_pt, row_h),
                        w: content_w,
                        h: row_h,
                        color: bg,
                    });
                }
            }

            // Emit cells.
            let mut x = self.geo.margins[0];
            for (lines, cell) in cell_lines.iter().zip(cells.iter()) {
                let cell_style = self.resolve(
                    cell,
                    &Resolved::inherited(&Resolved::root(), row_style.clone()),
                );
                let pad = cell_style.padding.unwrap_or([3.0, 8.0, 3.0, 8.0]);
                let lh = cell_style.line_height.unwrap_or(1.15) * cell_style.font_size(row_fs);
                let mut ty = self.y + pad[0] + lh * 0.8;
                for line in lines {
                    for seg in line {
                        let pdf_y_v = pdf_y(ty, self.geo.height_pt, 0.0);
                        self.push_op(Op::Text {
                            font: seg.font(),
                            size: seg.size,
                            x: x + pad[3] + seg.x,
                            y: pdf_y_v,
                            text: seg.text.clone(),
                            color: seg.color,
                        });
                    }
                    ty += lh;
                }
                // Cell border.
                if let Some(b) = cell_style.border {
                    self.push_op(Op::Rect {
                        x,
                        y: pdf_y(self.y, self.geo.height_pt, row_h),
                        w: 0.5,
                        h: row_h,
                        color: b.color,
                    });
                }
                x += col_w;
            }
            if let Some(b) = row_style.border.or(self.border_from_table()) {
                let yv = pdf_y(self.y + row_h, self.geo.height_pt, 0.5);
                self.push_op(Op::Rect {
                    x: self.geo.margins[0],
                    y: yv,
                    w: content_w,
                    h: 0.5,
                    color: b.color,
                });
            }
            self.y += row_h;
        }
    }

    fn bg_from_nth_rule(&self) -> Option<[f32; 3]> {
        self.rule_background_for(&["td".into()])
    }

    fn border_from_table(&self) -> Option<jhtml_css::Border> {
        None
    }

    fn rule_matches_nth(&self, node: &Node, nth_one_based: usize) -> bool {
        let even = nth_one_based.is_multiple_of(2);
        self.ss.rules.iter().any(|r| {
            r.compounds.last().map(|c| match c.nth_child {
                Some(Nth::Even) => even,
                Some(Nth::Odd) => !even,
                None => false,
            }) == Some(true)
        }) && node.tag() == Some("tr")
    }

    fn rule_background_for(&self, tags: &[String]) -> Option<[f32; 3]> {
        let mut best: Option<[f32; 3]> = None;
        for r in &self.ss.rules {
            let last = r.compounds.last()?;
            if last.tag.as_ref().map(|t| tags.contains(t)) == Some(true) {
                if let Some(v) = r.decls.get("background") {
                    if let Some(c) = parse_color(v) {
                        best = Some(c);
                    }
                }
            }
        }
        best
    }

    /// Gather inline words (and <br> markers) from a subtree.
    fn collect_inline(
        &self,
        node: &Node,
        style: &Style,
        href: &mut Option<String>,
        out: &mut Vec<Segment>,
    ) {
        let tag = node.tag().unwrap_or("");
        if tag == "br" {
            out.push(Segment {
                text: "\n".into(),
                ..Segment::default()
            });
            return;
        }
        // `style` is this node's already-resolved style; only child elements
        // get resolved (against a chain that carries this style but no
        // extra rules) so element rules never apply twice.
        if style.display == Some(Display::None) {
            return;
        }
        for c in node.children() {
            match c {
                Node::Text(t) => {
                    let ws = style.white_space.unwrap_or(jhtml_css::WhiteSpace::Normal);
                    if ws == jhtml_css::WhiteSpace::Pre {
                        out.push(Segment {
                            text: t.clone(),
                            size: style.font_size(12.0),
                            font: font_for(style),
                            color: style.color.unwrap_or([0.08, 0.08, 0.08]),
                            x: 0.0,
                            link: href.clone(),
                        });
                    } else {
                        for word in t.split_whitespace() {
                            out.push(Segment {
                                text: word.to_string(),
                                size: style.font_size(12.0),
                                font: font_for(style),
                                color: style.color.unwrap_or([0.08, 0.08, 0.08]),
                                x: 0.0,
                                link: href.clone(),
                            });
                        }
                    }
                }
                Node::Element { .. } => {
                    let child_style =
                        self.resolve(c, &Resolved::inherited(&Resolved::default(), style.clone()));
                    let mut child_href =
                        c.attr("href").map(str::to_string).or_else(|| href.clone());
                    self.collect_inline(c, &child_style, &mut child_href, out);
                    *href = child_href;
                }
                Node::Ignored => {}
            }
        }
    }

    /// Word-wrap segments to content width and emit text ops.
    fn wrap_and_emit(&mut self, segments: &[Segment], style: &Style, _anchor: Option<&str>) {
        let width = self.content_width();
        let fs = style.font_size(12.0);
        let lh = style.line_height.unwrap_or(1.15) * fs;
        let lines = self.wrap_segments(segments, width, lh);
        let align = style.text_align.unwrap_or(jhtml_css::Align::Left);
        self.ensure(lines.len() as f32 * lh);
        for line in lines {
            let line_w: f32 = line
                .last()
                .map(|s| s.x + measure(&s.text, s.font(), s.size))
                .unwrap_or(0.0);
            let x0 = match align {
                jhtml_css::Align::Left => self.geo.margins[0],
                jhtml_css::Align::Center => self.geo.margins[0] + (width - line_w) / 2.0,
                jhtml_css::Align::Right => self.geo.margins[0] + width - line_w,
            };
            self.y += lh;
            if self.y > self.bottom() {
                self.page_break();
            }
            let baseline = pdf_y(self.y, self.geo.height_pt, 0.0);
            for seg in &line {
                if seg.text == "\n" {
                    continue;
                }
                self.push_op(Op::Text {
                    font: seg.font(),
                    size: seg.size,
                    x: x0 + seg.x,
                    y: baseline,
                    text: seg.text.clone(),
                    color: seg.color,
                });
                if let Some(href) = &seg.link {
                    let target = if let Some(name) = href.strip_prefix('#') {
                        Target::Dest(name.to_string())
                    } else {
                        Target::Url(href.clone())
                    };
                    self.push_op(Op::Link {
                        x: x0 + seg.x,
                        y: baseline - 2.0,
                        w: measure(&seg.text, seg.font(), seg.size),
                        h: seg.size,
                        target,
                    });
                }
            }
        }
    }

    fn wrap_segments(&self, segments: &[Segment], width: f32, lh: f32) -> Vec<Vec<Segment>> {
        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut line: Vec<Segment> = Vec::new();
        let mut x = 0.0f32;
        for seg in segments {
            if seg.text == "\n" {
                lines.push(std::mem::take(&mut line));
                x = 0.0;
                continue;
            }
            let w = measure(&seg.text, seg.font(), seg.size);
            let space_w = measure(" ", seg.font(), seg.size);
            let needed = if line.is_empty() { w } else { w + space_w };
            if x + needed > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                x = 0.0;
            }
            let mut s = seg.clone();
            s.x = if line.is_empty() { 0.0 } else { x + space_w };
            x = s.x + w;
            line.push(s);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        let _ = lh;
        lines
    }

    /// Resolve an element's style: defaults by tag, then cascade rules,
    /// then inheritance from parent resolved style.
    fn resolve(&self, node: &Node, parent: &Resolved) -> Style {
        let tag = node.tag().unwrap_or("").to_string();
        let mut s = default_style(&tag);

        // Cascade: collect matching rules, sort by specificity then order.
        let mut matched: Vec<(&Rule, f32)> = Vec::new();
        for r in &self.ss.rules {
            if let Some(specificity) = matches_chain(&r.compounds, node, parent) {
                matched.push((r, specificity));
            }
        }
        matched.sort_by_key(|(r, sp)| ((sp * 1000.0) as u64, r.order));
        for (r, _) in matched {
            apply_decls(&mut s, &r.decls, parent.style.font_size(12.0));
        }
        // Inline style attribute wins over everything.
        if let Some(inline) = node.attr("style") {
            let decls = jhtml_css::parse_decls(inline);
            apply_decls(&mut s, &decls, parent.style.font_size(12.0));
        }
        // Inherit unset properties.
        let p = &parent.style;
        if s.font_size_pt.is_none() {
            s.font_size_pt = p.font_size_pt;
        }
        if s.bold.is_none() {
            s.bold = p.bold;
        }
        if s.italic.is_none() {
            s.italic = p.italic;
        }
        if s.color.is_none() {
            s.color = p.color;
        }
        if s.line_height.is_none() {
            s.line_height = p.line_height;
        }
        if s.font_family.is_none() {
            s.font_family = p.font_family.clone();
        }
        if s.white_space.is_none() {
            s.white_space = p.white_space;
        }
        s
    }
}

/// Resolved style + ancestry info for selector matching.
#[derive(Debug, Clone)]
struct Resolved {
    style: Style,
    chain: Vec<(String, Vec<String>, Option<String>)>, // (tag, classes, id)
}

impl Resolved {
    fn root() -> Self {
        Resolved {
            style: Style::default(),
            chain: Vec::new(),
        }
    }

    fn default() -> Self {
        Self::root()
    }

    fn inherited(parent: &Resolved, style: Style) -> Self {
        Resolved {
            style,
            chain: parent.chain.clone(),
        }
    }
}

/// Chain matching: last compound must match `node`; earlier compounds must
/// match some ancestor chain in order. Returns specificity (id=100, class=10,
/// tag=1, per compound in the matched chain).
fn matches_chain(compounds: &[Compound], node: &Node, parent: &Resolved) -> Option<f32> {
    let last = compounds.last()?;
    if !compound_matches(last, node) {
        return None;
    }
    let mut spec = compound_specificity(last);
    // Walk remaining compounds up the ancestor chain (greedy).
    let mut idx = parent.chain.len();
    for c in compounds[..compounds.len() - 1].iter().rev() {
        let mut found = false;
        while idx > 0 {
            idx -= 1;
            let (tag, classes, id) = &parent.chain[idx];
            let matches = c.tag.as_ref().map(|t| t == tag).unwrap_or(true)
                && c.classes.iter().all(|cl| classes.contains(cl))
                && c.id
                    .as_ref()
                    .map(|i| Some(i) == id.as_ref())
                    .unwrap_or(true);
            if matches {
                spec += compound_specificity(c);
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(spec)
}

fn compound_matches(c: &Compound, node: &Node) -> bool {
    let tag_ok = c
        .tag
        .as_ref()
        .map(|t| node.tag() == Some(t.as_str()))
        .unwrap_or(true);
    let id_ok =
        c.id.as_ref()
            .map(|i| node.attr("id") == Some(i.as_str()))
            .unwrap_or(true);
    let cls_ok = node
        .attr("class")
        .map(|cls| {
            c.classes
                .iter()
                .all(|want| cls.split_whitespace().any(|have| have == want))
        })
        .unwrap_or(c.classes.is_empty());
    tag_ok && id_ok && cls_ok
}

fn compound_specificity(c: &Compound) -> f32 {
    (if c.id.is_some() { 100.0 } else { 0.0 })
        + c.classes.len() as f32 * 10.0
        + if c.tag.is_some() { 1.0 } else { 0.0 }
}

fn apply_decls(s: &mut Style, decls: &HashMap<String, String>, parent_fs: f32) {
    // Pass 1: font-size first so em-based lengths below resolve against it.
    if let Some(v) = decls.get("font-size") {
        let fs = parse_pt_rel(v, parent_fs).unwrap_or(parent_fs);
        s.font_size_pt = Some(fs);
    }
    let own_fs = s.font_size_pt.unwrap_or(parent_fs);
    let lengths = |v: &str| -> Vec<f32> {
        v.split_whitespace()
            .filter_map(|p| parse_pt_rel(p, own_fs))
            .collect()
    };
    for (k, v) in decls {
        match k.as_str() {
            "font-size" => {}
            "font-weight" => {
                let n: f32 = v.parse().unwrap_or(match v.as_str() {
                    "bold" | "bolder" => 700.0,
                    "light" | "lighter" | "normal" => 400.0,
                    _ => 400.0,
                });
                s.bold = Some(n >= 600.0);
            }
            "font-style" => s.italic = Some(v == "italic" || v == "oblique"),
            "color" => s.color = parse_color(v),
            "background" | "background-color" => s.background = Some(parse_color(v)),
            "text-align" => {
                s.text_align = Some(match v.as_str() {
                    "center" => jhtml_css::Align::Center,
                    "right" => jhtml_css::Align::Right,
                    _ => jhtml_css::Align::Left,
                })
            }
            "display" => {
                s.display = Some(match v.as_str() {
                    "block" => Display::Block,
                    "none" => Display::None,
                    "flex" | "-webkit-box" | "-ms-flexbox" => Display::Flex,
                    "inline-flex" => Display::InlineFlex,
                    _ => Display::Inline,
                })
            }
            "justify-content" => {
                s.justify = Some(if v == "space-between" {
                    jhtml_css::Justify::SpaceBetween
                } else {
                    jhtml_css::Justify::Start
                })
            }
            "gap" => {
                s.gap_pt = v
                    .split_whitespace()
                    .next()
                    .and_then(|g| parse_pt_rel(g, own_fs))
            }
            "width" => {
                s.width = Some(if let Some(pct) = v.strip_suffix('%') {
                    Width::Pct(pct.trim().parse().unwrap_or(100.0))
                } else {
                    Width::Pt(parse_pt_rel(v, own_fs).unwrap_or(0.0))
                })
            }
            "line-height" => {
                // Unitless = multiplier; with units = absolute line height.
                s.line_height = Some(v.parse::<f32>().unwrap_or_else(|_| {
                    parse_pt_rel(v, own_fs).unwrap_or(16.0) / own_fs.max(0.001)
                }));
            }
            "margin" => {
                let parts = lengths(v);
                s.margin = Some(match parts.len() {
                    1 => [parts[0]; 4],
                    2 => [parts[0], parts[1], parts[0], parts[1]],
                    3 => [parts[0], parts[1], parts[2], parts[1]],
                    4 => [parts[0], parts[1], parts[2], parts[3]],
                    _ => [0.0; 4],
                });
            }
            "margin-top" => s.margin = upsert_margin(s.margin, 0, lengths(v).first().copied()),
            "margin-right" => s.margin = upsert_margin(s.margin, 1, lengths(v).first().copied()),
            "margin-bottom" => s.margin = upsert_margin(s.margin, 3, lengths(v).first().copied()),
            "margin-left" => s.margin = upsert_margin(s.margin, 2, lengths(v).first().copied()),
            "padding" => {
                let parts = lengths(v);
                s.padding = Some(match parts.len() {
                    1 => [parts[0]; 4],
                    2 => [parts[0], parts[1], parts[0], parts[1]],
                    3 => [parts[0], parts[1], parts[2], parts[1]],
                    4 => [parts[0], parts[1], parts[2], parts[3]],
                    _ => [0.0; 4],
                });
            }
            "border" | "border-bottom" | "border-top" => {
                if let Some(w) = v.split_whitespace().find_map(parse_pt) {
                    let color = v
                        .split_whitespace()
                        .find_map(parse_color)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    s.border = Some(jhtml_css::Border {
                        width_pt: w.max(0.5),
                        color,
                    });
                }
            }
            "page-break-before" | "break-before" => {
                s.page_break_before = Some(parse_break(v));
            }
            "page-break-after" | "break-after" => {
                s.page_break_after = Some(parse_break(v));
            }
            "page-break-inside" | "break-inside" => {
                s.page_break_inside = Some(parse_break(v));
            }
            "font-family" => s.font_family = Some(v.to_string()),
            "white-space" => {
                s.white_space = Some(if v == "pre" {
                    jhtml_css::WhiteSpace::Pre
                } else {
                    jhtml_css::WhiteSpace::Normal
                })
            }
            _ => {}
        }
    }
}

fn parse_break(v: &str) -> Break {
    match v {
        "avoid" => Break::Avoid,
        "always" | "page" | "left" | "right" => Break::Always,
        _ => Break::Auto,
    }
}

fn upsert_margin(cur: Option<[f32; 4]>, idx: usize, v: Option<f32>) -> Option<[f32; 4]> {
    let mut m = cur.unwrap_or([0.0; 4]);
    if let Some(v) = v {
        m[idx] = v;
    }
    Some(m)
}

fn font_for(style: &Style) -> Font {
    // Map CSS font-family keywords to base-14 fonts.
    let serif = style
        .font_family
        .as_deref()
        .map(|f| {
            let f = f.to_ascii_lowercase();
            f.contains("georgia")
                || f.contains("times")
                || f.contains("serif") && !f.contains("sans")
        })
        .unwrap_or(false);
    let mono = style
        .font_family
        .as_deref()
        .map(|f| {
            f.to_ascii_lowercase().contains("mono") || f.to_ascii_lowercase().contains("courier")
        })
        .unwrap_or(false);
    let mut f = if mono {
        Font::Courier
    } else if serif {
        Font::TimesRoman
    } else {
        Font::Helvetica
    };
    if style.bold == Some(true) {
        f = f.bold();
    }
    if style.italic == Some(true) {
        f = f.italic();
    }
    f
}

fn default_style(tag: &str) -> Style {
    let mut s = Style::default();
    match tag {
        "h1" => {
            s.font_size_pt = Some(24.0);
            s.bold = Some(true);
            s.margin = Some([13.4, 0.0, 13.4, 0.0]);
        }
        "h2" => {
            s.font_size_pt = Some(18.0);
            s.bold = Some(true);
            s.margin = Some([13.4, 0.0, 13.4, 0.0]);
        }
        "h3" => {
            s.font_size_pt = Some(15.0);
            s.bold = Some(true);
            s.margin = Some([12.0, 0.0, 12.0, 0.0]);
        }
        "h4" | "h5" | "h6" => {
            s.font_size_pt = Some(13.0);
            s.bold = Some(true);
            s.margin = Some([12.0, 0.0, 12.0, 0.0]);
        }
        "p" => s.margin = Some([8.0, 0.0, 8.0, 0.0]),
        "blockquote" => {
            s.margin = Some([8.0, 24.0, 8.0, 24.0]);
            s.italic = Some(true);
        }
        "strong" | "b" => s.bold = Some(true),
        "em" | "i" => s.italic = Some(true),
        "a" => s.color = Some([0.0, 0.0, 0.9]),
        "pre" => s.white_space = Some(jhtml_css::WhiteSpace::Pre),
        "th" => s.bold = Some(true),
        _ => {}
    }
    s
}

fn default_margin(tag: &str, fs: f32) -> [f32; 4] {
    match tag {
        "p" => [fs * 0.7, 0.0, fs * 0.7, 0.0],
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => [fs * 0.6, 0.0, fs * 0.4, 0.0],
        _ => [0.0; 4],
    }
}

/// An inline word with its resolved style, positioned within a line.
#[derive(Debug, Clone, PartialEq)]
struct Segment {
    text: String,
    size: f32,
    font: Font,
    color: [f32; 3],
    x: f32,
    /// href when this word came from an <a> element.
    link: Option<String>,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            text: String::new(),
            size: 12.0,
            font: Font::Helvetica,
            color: [0.0; 3],
            x: 0.0,
            link: None,
        }
    }
}

impl Segment {
    fn font(&self) -> Font {
        self.font
    }
}

/// Widest declared CSS width (pt) in a subtree, for flex children that
/// delegate their size to an inner wrapper div.
fn descendant_width(node: &Node) -> Option<f32> {
    fn walk(n: &Node) -> Option<f32> {
        let mut best: Option<f32> = None;
        if let Some(style_width) = n.attr("style") {
            let decls = jhtml_css::parse_decls(style_width);
            if let Some(w) = decls.get("width") {
                if let Some(pt) = parse_pt_rel(w, 12.0) {
                    best = Some(best.unwrap_or(0.0).max(pt));
                }
            }
        }
        for c in n.children() {
            if let Some(w) = walk(c) {
                best = Some(best.unwrap_or(0.0).max(w));
            }
        }
        best
    }
    walk(node)
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "table"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "figure"
            | "figcaption"
            | "hr"
            | "br"
    )
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn pdf_y(y_from_top: f32, page_h: f32, h: f32) -> f32 {
    page_h - y_from_top - h
}

fn collect_rows(table: &Node) -> Vec<&Node> {
    let mut rows = Vec::new();
    fn walk<'n>(node: &'n Node, rows: &mut Vec<&'n Node>) {
        for c in node.children() {
            match c.tag() {
                Some("tr") => rows.push(c),
                Some("thead") | Some("tbody") | Some("tfoot") => walk(c, rows),
                _ => {}
            }
        }
    }
    walk(table, &mut rows);
    rows
}

use std::collections::HashMap;

/// Parse CSS color literals: names + #rgb/#rrggbb + rgb().
pub fn parse_color(v: &str) -> Option<[f32; 3]> {
    let v = v.trim();
    let named: Option<[f32; 3]> = match v.to_ascii_lowercase().as_str() {
        "red" => Some([1.0, 0.0, 0.0]),
        "blue" => Some([0.0, 0.0, 1.0]),
        "green" => Some([0.0, 0.5, 0.0]),
        "black" => Some([0.0; 3]),
        "white" => Some([1.0; 3]),
        "gray" | "grey" => Some([0.5; 3]),
        "orange" => Some([1.0, 0.65, 0.0]),
        "yellow" => Some([1.0, 1.0, 0.0]),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if let Some(hex) = v.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let c: Vec<u8> = hex
                    .chars()
                    .filter_map(|c| c.to_digit(16).map(|d| (d * 17) as u8))
                    .collect();
                if c.len() == 3 {
                    Some([
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                    ])
                } else {
                    None
                }
            }
            6 => {
                let c = u32::from_str_radix(hex, 16).ok()?;
                Some([
                    ((c >> 16) & 0xff) as f32 / 255.0,
                    ((c >> 8) & 0xff) as f32 / 255.0,
                    (c & 0xff) as f32 / 255.0,
                ])
            }
            _ => None,
        };
    }
    if let Some(rgb) = v.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<f32> = rgb
            .split(',')
            .filter_map(|p| p.trim().parse().ok())
            .collect();
        if parts.len() == 3 {
            return Some([parts[0] / 255.0, parts[1] / 255.0, parts[2] / 255.0]);
        }
    }
    None
}

#[cfg(test)]
mod m2_tests {
    use super::*;
    use jhtml_css::Stylesheet;

    fn render_ops(html: &str) -> Vec<Op> {
        let doc = Document::parse(html.as_bytes());
        let ss = Stylesheet::parse(&doc.style_rules());
        layout(&doc, &ss).pages[0].ops.clone()
    }

    #[test]
    fn flex_space_between_spreads_children() {
        let ops = render_ops(
            r#"<style>#r { display: flex; justify-content: space-between; }
               #r div { width: 50pt; }</style>
               <body><div id="r"><div>a</div><div>b</div><div>c</div></div></body>"#,
        );
        let xs: Vec<f32> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Text { x, text, .. } => Some((*x, text.clone())),
                _ => None,
            })
            .map(|(x, _)| x)
            .collect();
        assert_eq!(xs.len(), 3);
        // First at left margin, last ends at right margin.
        assert!((xs[0] - 56.7).abs() < 0.5, "x0={}", xs[0]);
        assert!(xs[2] > 400.0, "x2={}", xs[2]);
    }

    #[test]
    fn em_font_sizes_scale_from_parent() {
        let ops = render_ops(
            r#"<style>#big { font-size: 1.5em; }</style>
               <body style="font-size: 12pt"><div id="big">X</div></body>"#,
        );
        let sizes: Vec<f32> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Text { size, .. } => Some(*size),
                _ => None,
            })
            .collect();
        assert!(!sizes.is_empty());
        assert!((sizes[0] - 18.0).abs() < 0.5, "size={}", sizes[0]);
    }

    #[test]
    fn serif_family_maps_to_times() {
        let ops = render_ops(
            r#"<style>p { font-family: Georgia, serif; }</style><body><p>text</p></body>"#,
        );
        match &ops[0] {
            Op::Text { font, .. } => assert_eq!(*font, Font::TimesRoman),
            _ => panic!("expected text op"),
        }
    }

    #[test]
    fn loose_inline_content_stays_on_one_line() {
        let ops = render_ops("<body><a href=\"#\">one</a> <b>two</b> <span>three</span></body>");
        let ys: Vec<f32> = ops
            .iter()
            .filter_map(|o| match o {
                Op::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(ys.len(), 3);
        assert_eq!(ys[0], ys[1]);
        assert_eq!(ys[1], ys[2]);
    }

    #[test]
    fn hr_emits_line_rect() {
        let ops = render_ops("<body><p>x</p><hr><p>y</p></body>");
        assert!(ops.iter().any(|o| matches!(o, Op::Rect { .. })));
    }

    #[test]
    fn numeric_font_weight_500_is_not_bold() {
        let ops = render_ops(r#"<style>h2 { font-weight: 500; }</style><body><h2>t</h2></body>"#);
        match &ops[0] {
            Op::Text { font, .. } => assert_eq!(*font, Font::Helvetica),
            _ => panic!("expected text op"),
        }
    }
}
