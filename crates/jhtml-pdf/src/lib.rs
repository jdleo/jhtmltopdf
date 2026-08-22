//! Stage 5: PDF writer.
//!
//! Takes laid-out pages of ops and serializes a valid PDF 1.4: object xref,
//! base-14 fonts, per-page content streams, WinAnsi text. Zero deps.

use jhtml_layout::{Op, Page, Target};
use jhtml_text::Font;

/// Serialize laid-out pages into PDF bytes.
pub fn write_pdf(pages: &[Page], title: Option<&str>) -> Vec<u8> {
    let fonts_used = collect_fonts(pages);
    let mut font_ids = HashMap::new();
    for (i, f) in fonts_used.iter().enumerate() {
        font_ids.insert(*f, 4 + i as u32);
    }
    let font_objs: Vec<String> = fonts_used
        .iter()
        .map(|f| {
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>",
                f.base_name()
            )
        })
        .collect();

    let n_fonts = font_objs.len() as u32;
    let first_page_id = 4 + n_fonts;
    let mut objects: Vec<String> = Vec::new();
    // 1: catalog placeholder, 2: pages placeholder, 3: info placeholder.
    for _ in 0..3 {
        objects.push(String::new());
    }
    objects.extend(font_objs);

    let mut page_ids = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let page_id = first_page_id + (i as u32) * 2;
        page_ids.push(page_id);
        let content_id = page_id + 1;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.1} {:.1}] \
             /Resources << /Font {} >> /Contents {} 0 R >>",
            page.width_pt,
            page.height_pt,
            font_dict(&font_ids),
            content_id
        ));
        let stream = content_stream(&page.ops);
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    let _ = title;

    // Now fill in catalog/pages/info.
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects[0] = "<< /Type /Catalog /Pages 2 0 R >>".into();
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    );
    objects[2] = "<< >>".into(); // info: filled by write_pdf_meta

    assemble(&objects)
}

/// Metadata (title, author) written into the Info object.
pub fn write_pdf_meta(objects: &mut [String], title: Option<&str>, author: Option<&str>) {
    let mut info = Vec::new();
    if let Some(t) = title {
        info.push(format!("/Title ({})", pdf_string(t)));
    }
    if let Some(a) = author {
        info.push(format!("/Author ({})", pdf_string(a)));
    }
    objects[2] = format!("<< {} >>", info.join(" "));
}

fn font_dict(font_ids: &HashMap<Font, u32>) -> String {
    let entries: Vec<String> = font_ids
        .iter()
        .map(|(f, id)| format!("/F{} {} 0 R", font_key(*f), id))
        .collect();
    format!("<< {} >>", entries.join(" "))
}

pub fn font_key(f: Font) -> u8 {
    match f {
        Font::Helvetica => 1,
        Font::HelveticaBold => 2,
        Font::HelveticaOblique => 3,
        Font::HelveticaBoldOblique => 4,
        Font::TimesRoman => 5,
        Font::Courier => 6,
    }
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

fn content_stream(ops: &[Op]) -> String {
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
                s.push_str(&format!(
                    "BT {:.3} {:.3} {:.3} rg /F{} {:.2} Tf {:.2} {:.2} Td ({}) Tj ET\n",
                    color[0],
                    color[1],
                    color[2],
                    font_key(*font),
                    size,
                    x,
                    y,
                    pdf_string(text)
                ));
            }
            Op::Rect { x, y, w, h, color } => {
                s.push_str(&format!(
                    "{:.3} {:.3} {:.3} rg {:.2} {:.2} {:.2} {:.2} re f\n",
                    color[0], color[1], color[2], x, y, w, h
                ));
            }
            Op::Link { x, y, w, h, target } => match target {
                Target::Url(u) => s.push_str(&format!(
                    "%LINK-URI {x:.2} {y:.2} {w:.2} {h:.2} {}\n",
                    uri_escape(u)
                )),
                Target::Dest(d) => {
                    s.push_str(&format!("%LINK-DEST {x:.2} {y:.2} {w:.2} {h:.2} {d}\n"))
                }
            },
        }
    }
    s
}

fn uri_escape(u: &str) -> String {
    u.chars()
        .filter(|c| *c != '(' && *c != ')' && *c != '\\')
        .collect()
}

pub fn pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c if (c as u32) < 128 && c.is_ascii_graphic() || c == ' ' => out.push(c),
            c if (c as u32) < 256 => out.push(c as u8 as char),
            _ => out.push('?'),
        }
    }
    out
}

/// Assemble objects into a PDF with xref table.
fn assemble(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
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

use std::collections::HashMap;

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
        let pdf = write_pdf(&[page_with_text("a"), page_with_text("b")], Some("t"));
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.windows(5).any(|w| w == b"%%EOF"));
        let s = String::from_utf8_lossy(&pdf);
        assert_eq!(s.matches("/Type /Page ").count(), 2);
        assert!(s.contains("/Count 2"));
    }

    #[test]
    fn text_is_escaped() {
        let pdf = write_pdf(&[page_with_text("a(b) \\c")], None);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains(r"a\(b\) \\c"));
    }

    #[test]
    fn colors_in_stream() {
        let mut p = page_with_text("x");
        if let Op::Text { color, .. } = &mut p.ops[0] {
            *color = [0.5, 0.0, 0.0];
        }
        let bytes = write_pdf(&[p], None);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("0.500 0.000 0.000 rg"));
    }
}
