//! Stage 5: PDF writer.
//!
//! Takes laid-out pages of ops plus document furniture (outline, internal
//! destinations, metadata) and serializes a valid PDF 1.4: xref table,
//! base-14 fonts, embedded TTF faces (CID/Identity-H) with real metrics,
//! per-page content streams, link annotations, outline tree.

use crate::layout::{Destinations, Op, OutlineItem, Page, Target};
use crate::text::{Font, FontStore};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};

/// Document metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
}

/// Serialize a full document. `fonts` supplies glyph ids and file bytes for
/// embedded faces (Font::Ttf); base-14 fonts need nothing from it.
pub fn write_pdf(
    pages: &[Page],
    outline: &[OutlineItem],
    dests: &Destinations,
    meta: &Metadata,
    fonts: &FontStore,
) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    // Slots for raw binary font file payloads, spliced in at assembly.
    let mut fontfile_slots: Vec<(usize, Vec<u8>)> = Vec::new();
    // 1: catalog, 2: pages tree, 3: info — filled at the end.
    objects.push(String::new());
    objects.push(String::new());
    objects.push(String::new());

    // Usage pre-pass: which glyphs of which embedded faces do we need?
    let mut usage: BTreeMap<u32, BTreeMap<u16, char>> = BTreeMap::new();
    for page in pages {
        for op in &page.ops {
            if let Op::Text {
                font: Font::Ttf(face),
                text,
                ..
            } = op
            {
                if let Some(fd) = fonts.face(Font::Ttf(*face)) {
                    let entry = usage.entry(*face).or_default();
                    for ch in text.chars() {
                        let gid = fd.glyph_id(ch);
                        entry.entry(gid).or_insert(ch);
                    }
                }
            }
        }
    }

    // Fonts: base-14 are single objects; embedded faces are a 5-object group.
    let fonts_used = collect_fonts(pages);
    let mut font_ids: HashMap<Font, u32> = HashMap::new();
    for f in &fonts_used {
        let id = objects.len() as u32 + 1;
        font_ids.insert(*f, id);
        match f {
            Font::Ttf(face) => {
                let fd = fonts
                    .face(*f)
                    .expect("embedded face missing from FontStore");
                let used = usage.get(face).cloned().unwrap_or_default();
                let cid = id + 1;
                let desc = id + 2;
                let file = id + 3;
                let tounicode = id + 4;
                objects.push(format!(
                    "<< /Type /Font /Subtype /Type0 /BaseFont /{} /Encoding /Identity-H /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>",
                    fd.name, cid, tounicode
                ));
                let widths: Vec<String> = used
                    .keys()
                    .map(|gid| {
                        let w = fd.advances.get(gid).copied().unwrap_or(0.0);
                        format!("{gid} {gid} {}", w.round() as i64)
                    })
                    .collect();
                objects.push(format!(
                    "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {} 0 R /DW 0 /W [{}] /CIDToGIDMap /Identity >>",
                    fd.name,
                    desc,
                    widths.join(" ")
                ));
                let upm = fd.units_per_em;
                objects.push(format!(
                    "<< /Type /FontDescriptor /FontName /{} /Flags 32 /FontBBox [{:.0} {:.0} {:.0} {:.0}] /ItalicAngle {:.1} /Ascent {:.0} /Descent {:.0} /CapHeight {:.0} /StemV 80 /FontFile2 {} 0 R >>",
                    fd.name,
                    fd.bbox[0],
                    fd.bbox[1],
                    fd.bbox[2],
                    fd.bbox[3],
                    fd.italic_angle,
                    fd.ascent,
                    fd.descent,
                    upm * 0.7,
                    file
                ));
                // Binary payload (zlib-compressed) spliced at assembly.
                use std::io::Write;
                let mut enc =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                enc.write_all(&fd.bytes).expect("font compress");
                let compressed = enc.finish().expect("font flush");
                objects.push(format!(
                    "<< /Length {} /Filter /FlateDecode /Length1 {} >>\nstream\n__FONTFILE__\nendstream",
                    compressed.len(),
                    fd.bytes.len()
                ));
                fontfile_slots.push((file as usize - 1, compressed));
                let tu = to_unicode(&used);
                objects.push(format!(
                    "<< /Length {} >>\nstream\n{tu}\nendstream",
                    tu.len()
                ));
            }
            f => {
                objects.push(format!(
                    "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                    f.base_name()
                ));
            }
        }
    }
    let font_dict = {
        let entries: Vec<String> = fonts_used
            .iter()
            .map(|f| format!("/F{} {} 0 R", font_key(*f), font_ids[f]))
            .collect();
        format!("<< {} >>", entries.join(" "))
    };

    // Pages, content streams, link annotations.
    let streams: Vec<String> = pages
        .par_iter()
        .map(|page| content_stream(&page.ops, fonts))
        .collect();
    let mut page_ids = Vec::new();
    let mut annot_meta: Vec<Vec<(u32, Target, [f32; 4])>> = Vec::new();
    for (page, stream) in pages.iter().zip(streams) {
        let page_id = objects.len() as u32 + 1;
        page_ids.push(page_id);
        objects.push(String::new()); // placeholder for page object

        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));

        let mut annots = Vec::new();
        for op in &page.ops {
            if let Op::Link { x, y, w, h, target } = op {
                let annot_id = objects.len() as u32 + 1;
                objects.push(String::new()); // placeholder
                annots.push((annot_id, target.clone(), [*x, *y, x + w, y + h]));
            }
        }
        annot_meta.push(annots);
    }

    for (i, page) in pages.iter().enumerate() {
        let page_id = page_ids[i];
        let annots = &annot_meta[i];
        let annot_str = if annots.is_empty() {
            String::new()
        } else {
            let refs: Vec<String> = annots.iter().map(|(id, ..)| format!("{id} 0 R")).collect();
            format!(" /Annots [{}]", refs.join(" "))
        };
        objects[page_id as usize - 1] = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.1} {:.1}] /Resources << /Font {} >> /Contents {} 0 R{} >>",
            page.width_pt,
            page.height_pt,
            font_dict,
            page_id + 1,
            annot_str
        );
    }

    for annots in &annot_meta {
        for (id, target, rect) in annots {
            let body = match target {
                Target::Url(u) => format!(
                    "<< /Type /Annot /Subtype /Link /Rect [{:.1} {:.1} {:.1} {:.1}] /Border [0 0 0] /A << /S /URI /URI ({}) >> >>",
                    rect[0],
                    rect[1],
                    rect[2],
                    rect[3],
                    pdf_string(u)
                ),
                Target::Dest(name) => {
                    if let Some((page1, y_top)) = dests.map.get(name) {
                        match page_ids.get(page1.saturating_sub(1)).copied() {
                            Some(pid) => format!(
                                "<< /Type /Annot /Subtype /Link /Rect [{:.1} {:.1} {:.1} {:.1}] /Border [0 0 0] /Dest [{} 0 R /XYZ 0 {:.1} 0] >>",
                                rect[0],
                                rect[1],
                                rect[2],
                                rect[3],
                                pid,
                                pages
                                    .get(page1.saturating_sub(1))
                                    .map(|p| p.height_pt - y_top)
                                    .unwrap_or(0.0)
                            ),
                            None => String::new(),
                        }
                    } else {
                        String::new()
                    }
                }
            };
            objects[*id as usize - 1] = body;
        }
    }

    // Outline tree.
    let outline_root = if outline.is_empty() {
        None
    } else {
        let root_id = objects.len() as u32 + 1;
        let first_item = root_id + 1;
        let item_ids: Vec<u32> = (0..outline.len()).map(|i| first_item + i as u32).collect();
        objects.push(String::new());
        for _ in 0..outline.len() {
            objects.push(String::new());
        }
        for (i, item) in outline.iter().enumerate() {
            let id = item_ids[i];
            let page_id = page_ids
                .get(item.page.saturating_sub(1))
                .copied()
                .unwrap_or(page_ids[0]);
            let prev = if i > 0 {
                format!("/Prev {} 0 R ", item_ids[i - 1])
            } else {
                String::new()
            };
            let next = if i + 1 < outline.len() {
                format!("/Next {} 0 R ", item_ids[i + 1])
            } else {
                String::new()
            };
            let (first, last, count) = child_range(outline, i, &item_ids);
            let children = if let Some((f, l)) = first.zip(last) {
                format!("/First {} 0 R /Last {} 0 R /Count {count} ", f, l)
            } else {
                format!("/Dest [{} 0 R /XYZ 0 {:.1} 0] ", page_id, 842.0)
            };
            objects[id as usize - 1] = format!(
                "<< /Title ({}) /Parent {root_id} 0 R {prev}{next}{children}>>",
                pdf_string(&item.title),
            );
        }
        objects[root_id as usize - 1] = format!(
            "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
            item_ids[0],
            item_ids[item_ids.len() - 1],
            outline.len()
        );
        Some(root_id)
    };

    let outline_str = outline_root
        .map(|r| format!(" /Outlines {r} 0 R"))
        .unwrap_or_default();
    objects[0] = format!("<< /Type /Catalog /Pages 2 0 R{outline_str} >>");
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    );
    let mut info = Vec::new();
    if let Some(t) = &meta.title {
        info.push(format!("/Title ({})", pdf_string(t)));
    }
    if let Some(a) = &meta.author {
        info.push(format!("/Author ({})", pdf_string(a)));
    }
    objects[2] = format!("<< {} >>", info.join(" "));

    assemble_binary(&objects, &fontfile_slots)
}

/// First/last child item ids of the subtree rooted at `i` in a flat
/// outline where children directly follow their parent.
fn child_range(outline: &[OutlineItem], i: usize, ids: &[u32]) -> (Option<u32>, Option<u32>, i64) {
    let level = outline[i].level;
    let mut first = None;
    let mut count = 0i64;
    let mut j = i + 1;
    while j < outline.len() && outline[j].level > level {
        if first.is_none() {
            first = Some(ids[j]);
        }
        count += 1;
        j += 1;
    }
    let last = if count > 0 { Some(ids[j - 1]) } else { None };
    (first, last, count)
}

fn collect_fonts(pages: &[Page]) -> Vec<Font> {
    let mut set: Vec<Font> = Vec::new();
    for p in pages {
        for op in &p.ops {
            if let Op::Text { font, .. } = op {
                if !set.contains(font) {
                    set.push(*font);
                }
            }
        }
    }
    if set.is_empty() {
        set.push(Font::Helvetica);
    }
    set
}

pub fn font_key(f: Font) -> u8 {
    match f {
        Font::Helvetica => 1,
        Font::HelveticaBold => 2,
        Font::HelveticaOblique => 3,
        Font::HelveticaBoldOblique => 4,
        Font::TimesRoman => 5,
        Font::Courier => 6,
        Font::Ttf(i) => 10 + (i as u8),
    }
}

fn content_stream(ops: &[Op], fonts: &FontStore) -> String {
    let mut s = String::new();
    for op in ops {
        match op {
            Op::Text {
                font,
                size,
                x,
                y,
                text,
                color,
            } => {
                let payload = match font {
                    Font::Ttf(_) => {
                        let mut hex = String::new();
                        if let Some(fd) = fonts.face(*font) {
                            for ch in text.chars() {
                                hex.push_str(&format!("{:04X}", fd.glyph_id(ch)));
                            }
                        }
                        format!("<{hex}>")
                    }
                    _f => format!("({})", pdf_string(text)),
                };
                s.push_str(&format!(
                    "BT {:.3} {:.3} {:.3} rg /F{} {:.2} Tf {:.2} {:.2} Td {} Tj ET\n",
                    color[0],
                    color[1],
                    color[2],
                    font_key(*font),
                    size,
                    x,
                    y,
                    payload
                ));
            }
            Op::Rect { x, y, w, h, color } => {
                s.push_str(&format!(
                    "{:.3} {:.3} {:.3} rg {:.2} {:.2} {:.2} {:.2} re f\n",
                    color[0], color[1], color[2], x, y, w, h
                ));
            }
            Op::Link { .. } => {} // rendered as /Annots, not stream content
        }
    }
    s
}

pub fn pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c if ((c as u32) < 128 && c.is_ascii_graphic()) || c == ' ' => out.push(c),
            '\u{2022}' => out.push('\u{95}'),
            '\u{2013}' => out.push('\u{96}'),
            '\u{2014}' => out.push('\u{97}'),
            '\u{2018}' | '\u{2019}' => out.push('\u{27}'),
            '\u{201C}' | '\u{201D}' => out.push('\u{22}'),
            c if (c as u32) < 256 => out.push(c as u8 as char),
            _ => out.push('?'),
        }
    }
    out
}

/// ToUnicode CMap stream body for a glyph -> char mapping.
fn to_unicode(used: &BTreeMap<u16, char>) -> String {
    let mut lines = Vec::new();
    for (gid, ch) in used {
        let mut buf = [0u16; 2];
        let n = ch.encode_utf16(&mut buf).len();
        let hex: String = buf[..n].iter().map(|u| format!("{u:04X}")).collect();
        lines.push(format!("<{gid:04X}> <{hex}>"));
    }
    let mut out = String::new();
    out.push_str("/CIDInit /ProcSet findresource begin\n");
    out.push_str("12 dict begin\nbegincmap\n");
    out.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    out.push_str("/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n");
    out.push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    for chunk in lines.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for line in chunk {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}

/// Assemble objects into a PDF with xref table, splicing binary font file
/// payloads into streams whose text bodies contain the __FONTFILE__ marker.
fn assemble_binary(objects: &[String], fontfiles: &[(usize, Vec<u8>)]) -> Vec<u8> {
    const MARKER: &str = "__FONTFILE__";
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    let fontfile_map: HashMap<usize, &Vec<u8>> = fontfiles.iter().map(|(i, b)| (*i, b)).collect();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        if let Some(bytes) = fontfile_map.get(&i) {
            let head = obj.find(MARKER).unwrap_or(obj.len());
            out.extend_from_slice(&obj.as_bytes()[..head]);
            out.extend_from_slice(bytes);
            out.extend_from_slice(&obj.as_bytes()[head + MARKER.len()..]);
        } else {
            out.extend_from_slice(obj.as_bytes());
        }
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len() as u32;
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with_text(t: &str) -> Page {
        Page {
            width_pt: 595.0,
            height_pt: 842.0,
            ops: vec![Op::Text {
                font: Font::HelveticaBold,
                size: 24.0,
                x: 72.0,
                y: 770.0,
                text: t.into(),
                color: [0.0, 0.0, 0.0],
            }],
        }
    }

    #[test]
    fn multipage_valid_envelope() {
        let pdf = write_pdf(
            &[page_with_text("a"), page_with_text("b")],
            &[],
            &Destinations::default(),
            &Metadata {
                title: Some("t".into()),
                author: None,
            },
            &FontStore::empty(),
        );
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.windows(5).any(|w| w == b"%%EOF"));
        let s = String::from_utf8_lossy(&pdf);
        assert_eq!(s.matches("/Type /Page ").count(), 2);
        assert!(s.contains("/Count 2"));
        assert!(s.contains("/Title (t)"));
    }

    #[test]
    fn uri_links_become_annotations() {
        let mut p = page_with_text("x");
        p.ops.push(Op::Link {
            x: 72.0,
            y: 700.0,
            w: 50.0,
            h: 10.0,
            target: Target::Url("https://example.com".into()),
        });
        let bytes = write_pdf(
            &[p],
            &[],
            &Destinations::default(),
            &Metadata::default(),
            &FontStore::empty(),
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Link"));
        assert!(s.contains("/URI (https://example.com)"));
        assert!(s.contains("/Annots ["));
    }

    #[test]
    fn internal_links_resolve_to_dests() {
        let mut p = page_with_text("x");
        p.ops.push(Op::Link {
            x: 72.0,
            y: 700.0,
            w: 50.0,
            h: 10.0,
            target: Target::Dest("sec1".into()),
        });
        let mut d = Destinations::default();
        d.map.insert("sec1".into(), (2, 100.0));
        let mut p2 = page_with_text("y");
        p2.ops.clear();
        let bytes = write_pdf(&[p, p2], &[], &d, &Metadata::default(), &FontStore::empty());
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Dest ["));
        assert!(s.contains("/XYZ 0 "));
    }

    #[test]
    fn outline_tree_serializes() {
        let p2 = Page {
            width_pt: 595.0,
            height_pt: 842.0,
            ops: vec![],
        };
        let outline = vec![
            OutlineItem {
                level: 1,
                title: "Ch 1".into(),
                page: 1,
            },
            OutlineItem {
                level: 2,
                title: "Sec 1.1".into(),
                page: 2,
            },
        ];
        let bytes = write_pdf(
            &[page_with_text("a"), p2],
            &outline,
            &Destinations::default(),
            &Metadata::default(),
            &FontStore::empty(),
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Type /Outlines"));
        assert!(s.contains("/Title (Ch 1)"));
        assert!(s.contains("/Count 1"));
    }

    #[test]
    fn text_is_escaped() {
        let mut p = page_with_text("a(b) \\c");
        let bytes = write_pdf(
            &[p],
            &[],
            &Destinations::default(),
            &Metadata::default(),
            &FontStore::empty(),
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains(r"a\(b\) \\c"));
    }

    #[test]
    fn colors_in_stream() {
        let mut p = page_with_text("x");
        if let Op::Text { color, .. } = &mut p.ops[0] {
            *color = [0.5, 0.0, 0.0];
        }
        let bytes = write_pdf(
            &[p],
            &[],
            &Destinations::default(),
            &Metadata::default(),
            &FontStore::empty(),
        );
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("0.500 0.000 0.000 rg"));
    }
}
