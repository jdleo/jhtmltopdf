//! Stage 4: fonts, font discovery, and text metrics.
//!
//! FontStore discovers system fonts via fontdb, extracts cmap + hmtx
//! metrics with ttf-parser, and resolves CSS font-family declarations to
//! embedded faces. Base-14 AFM metrics remain as the fallback when no
//! system font matches (and for tests / deterministic output).

use std::collections::HashMap;
use std::path::Path;

/// One of the PDF base-14 fallback fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    Courier,
    /// An embedded system face: index into FontStore.faces.
    Ttf(u32),
}

impl Font {
    /// PDF BaseFont name (base-14 only; embedded faces name themselves).
    pub fn base_name(self) -> &'static str {
        match self {
            Font::Helvetica => "Helvetica",
            Font::HelveticaBold => "Helvetica-Bold",
            Font::HelveticaOblique => "Helvetica-Oblique",
            Font::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Font::TimesRoman => "Times-Roman",
            Font::Courier => "Courier",
            Font::Ttf(_) => "Embedded",
        }
    }

    pub fn bold(self) -> Font {
        match self {
            Font::Helvetica => Font::HelveticaBold,
            Font::HelveticaOblique => Font::HelveticaBoldOblique,
            f => f,
        }
    }

    pub fn italic(self) -> Font {
        match self {
            Font::Helvetica => Font::HelveticaOblique,
            Font::HelveticaBold => Font::HelveticaBoldOblique,
            f => f,
        }
    }
}

/// A loaded system face with everything the metrics + PDF writer need.
#[derive(Debug, Clone)]
pub struct FaceData {
    /// Raw font file bytes (embedded as FontFile2).
    pub bytes: Vec<u8>,
    /// Sanitized PostScript-ish name, e.g. "Arial-Bold".
    pub name: String,
    pub units_per_em: f32,
    /// char -> glyph id.
    pub cmap: HashMap<char, u16>,
    /// glyph id -> advance in font units.
    pub advances: HashMap<u16, f32>,
    pub ascent: f32,
    pub descent: f32,
    pub bbox: [f32; 4],
    pub italic_angle: f32,
}

impl FaceData {
    /// Extract metrics from a font file. Returns None for unparseable or
    /// collection files we cannot embed standalone.
    pub fn load(path: &Path, index: u32) -> Option<FaceData> {
        let bytes = std::fs::read(path).ok()?;
        Self::from_bytes(bytes, index)
    }

    pub fn from_bytes(bytes: Vec<u8>, index: u32) -> Option<FaceData> {
        let face = ttf_parser::Face::parse(&bytes, index).ok()?;
        if index != 0 && ttf_parser::fonts_in_collection(&bytes).is_some() {
            // Can't embed a sub-face of a TTC as FontFile2; skip.
            return None;
        }
        let units_per_em = face.units_per_em() as f32;
        let mut cmap: HashMap<char, u16> = HashMap::new();
        if let Some(tables) = face.tables().cmap {
            for sub in tables.subtables {
                sub.codepoints(|cp: u32| {
                    if let Some(gid) = sub.glyph_index(cp) {
                        cmap.entry(char::from_u32(cp).unwrap_or('\0'))
                            .or_insert(gid.0);
                    }
                });
            }
        }
        let n = face.number_of_glyphs();
        let mut advances = HashMap::with_capacity(n as usize);
        for gid in 0..n {
            if let Some(a) = face.glyph_hor_advance(ttf_parser::GlyphId(gid)) {
                advances.insert(gid, a as f32);
            }
        }
        let (ascent, descent) = match (face.ascender(), face.descender()) {
            (a, d) if a != 0 || d != 0 => (a as f32, d as f32),
            _ => (
                face.typographic_ascender().unwrap_or(800) as f32,
                face.typographic_descender().unwrap_or(-200) as f32,
            ),
        };
        let bbox = face.global_bounding_box();
        let italic_angle = face.italic_angle();
        let style = match (face.style(), face.is_bold()) {
            (ttf_parser::Style::Italic, true) => "-BoldItalic",
            (ttf_parser::Style::Italic, false) => "-Italic",
            (ttf_parser::Style::Oblique, true) => "-BoldItalic",
            (ttf_parser::Style::Oblique, false) => "-Oblique",
            (_, true) => "-Bold",
            (_, false) => "",
        };
        let raw_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Font".to_string());
        let name = format!("{}{}", raw_name.replace(' ', ""), style);
        Some(FaceData {
            bytes,
            name,
            units_per_em,
            cmap,
            advances,
            ascent,
            descent,
            bbox: [
                bbox.x_min as f32,
                bbox.y_min as f32,
                bbox.x_max as f32,
                bbox.y_max as f32,
            ],
            italic_angle,
        })
    }

    pub fn glyph_id(&self, ch: char) -> u16 {
        *self.cmap.get(&ch).unwrap_or(&0)
    }

    /// Advance width of `text` in points at `size`.
    pub fn measure(&self, text: &str, size: f32) -> f32 {
        let units: f32 = text
            .chars()
            .map(|ch| {
                self.advances
                    .get(&self.glyph_id(ch))
                    .copied()
                    .unwrap_or(0.0)
            })
            .sum();
        units / self.units_per_em * size
    }
}

/// Metric-compatible fallbacks for the common web/MS fonts on systems
/// (like Linux CI or minimal servers) that lack them.
fn alias_candidates(name: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = vec![Box::leak(name.to_owned().into_boxed_str())];
    match name.to_ascii_lowercase().as_str() {
        "arial" | "helvetica" => v.extend(["Liberation Sans", "DejaVu Sans"]),
        "times new roman" | "times" | "georgia" => v.extend(["Liberation Serif", "DejaVu Serif"]),
        "courier new" | "courier" => v.extend(["Liberation Mono", "DejaVu Sans Mono"]),
        _ => {}
    }
    v
}

/// System font discovery + CSS family resolution.
#[derive(Debug, Default)]
pub struct FontStore {
    db: Option<fontdb::Database>,
    faces: std::sync::RwLock<Vec<FaceData>>,
    cache: std::sync::Mutex<HashMap<(String, bool, bool), Font>>,
}

impl FontStore {
    /// Empty store: every resolve falls back to base-14.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Store with all system fonts discovered.
    pub fn with_system_fonts() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self {
            db: Some(db),
            faces: std::sync::RwLock::new(Vec::new()),
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Resolve a CSS font-family list (plus bold/italic) to a Font.
    /// Falls back to base-14 when no system face matches.
    pub fn resolve(&self, families: Option<&String>, bold: bool, italic: bool) -> Font {
        let base = match (bold, italic) {
            (true, true) => Font::HelveticaBoldOblique,
            (true, false) => Font::HelveticaBold,
            (false, true) => Font::HelveticaOblique,
            (false, false) => Font::Helvetica,
        };
        let list = match families {
            Some(l) if !l.is_empty() => l,
            _ => return base,
        };
        let weight = if bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        };
        let style = if italic {
            fontdb::Style::Italic
        } else {
            fontdb::Style::Normal
        };
        // Remember generic requests: they drive the base-14 fallback.
        let mut wants_serif = false;
        let mut wants_mono = false;
        for raw in list.split(',') {
            let name = raw.trim().trim_matches('"').trim_matches('\'');
            if name.is_empty() {
                continue;
            }
            match name.to_ascii_lowercase().as_str() {
                "serif" => {
                    wants_serif = true;
                    continue;
                }
                "monospace" => {
                    wants_mono = true;
                    continue;
                }
                "sans-serif" | "cursive" | "fantasy" | "system-ui" => continue,
                _ => {}
            }
            let key = (name.to_ascii_lowercase(), bold, italic);
            if let Some(f) = self.cache.lock().unwrap().get(&key) {
                return *f;
            }
            // Try the requested family, then metric-compatible aliases
            // (Linux ships Liberation/DejaVu instead of the MS core fonts).
            for candidate in alias_candidates(name) {
                if let Some(f) = self.try_load(candidate, weight, style) {
                    self.cache.lock().unwrap().insert(key, f);
                    return f;
                }
            }
        }
        // No system face matched: honor serif/mono generics via base-14.
        if wants_serif {
            return match (bold, italic) {
                (true, true) => Font::HelveticaBoldOblique, // closest available
                (true, false) => Font::TimesRoman.bold(),
                (false, true) => Font::TimesRoman.italic(),
                (false, false) => Font::TimesRoman,
            };
        }
        if wants_mono {
            return match (bold, italic) {
                (true, true) => Font::Courier,
                (true, false) => Font::Courier,
                (false, true) => Font::Courier,
                (false, false) => Font::Courier,
            };
        }
        base
    }

    fn try_load(&self, family: &str, weight: fontdb::Weight, style: fontdb::Style) -> Option<Font> {
        let db = self.db.as_ref()?;
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight,
            style,
            stretch: fontdb::Stretch::Normal,
        };
        let id = db.query(&query)?;
        let info = db.face(id)?;
        let path = match &info.source {
            fontdb::Source::File(p) => p.clone(),
            _ => return None,
        };
        let mut faces = self.faces.write().unwrap();
        let idx = faces.len() as u32;
        let data = FaceData::load(&path, info.index)?;
        faces.push(data);
        Some(Font::Ttf(idx))
    }

    pub fn face(&self, f: Font) -> Option<FaceData> {
        match f {
            Font::Ttf(i) => self.faces.read().unwrap().get(i as usize).cloned(),
            _ => None,
        }
    }

    /// Measure text in points.
    pub fn measure(&self, text: &str, font: Font, size: f32) -> f32 {
        match font {
            Font::Ttf(i) => self
                .faces
                .read()
                .unwrap()
                .get(i as usize)
                .map(|f| f.measure(text, size))
                .unwrap_or(0.0),
            f => base14_measure(text, f, size),
        }
    }
}

/// Advance width of `ch` in 1/1000 em units for base-14 Helvetica.
pub fn helvetica_width(ch: char, bold: bool) -> f32 {
    let b = |reg: f32, brd: f32| if bold { brd } else { reg };
    match ch.to_ascii_lowercase() {
        ' ' => 278.0,
        'a' => b(556.0, 556.0),
        'b' => b(556.0, 611.0),
        'c' => b(500.0, 556.0),
        'd' => b(556.0, 611.0),
        'e' => b(556.0, 556.0),
        'f' => b(278.0, 333.0),
        'g' => b(556.0, 611.0),
        'h' => b(556.0, 611.0),
        'i' => b(222.0, 278.0),
        'j' => b(222.0, 278.0),
        'k' => b(500.0, 556.0),
        'l' => b(222.0, 278.0),
        'm' => b(833.0, 889.0),
        'n' => b(556.0, 611.0),
        'o' => b(556.0, 611.0),
        'p' => b(556.0, 611.0),
        'q' => b(556.0, 611.0),
        'r' => b(333.0, 389.0),
        's' => b(500.0, 556.0),
        't' => b(278.0, 333.0),
        'u' => b(556.0, 611.0),
        'v' => b(500.0, 556.0),
        'w' => b(722.0, 778.0),
        'x' => b(500.0, 556.0),
        'y' => b(500.0, 556.0),
        'z' => b(500.0, 500.0),
        '0'..='9' => 556.0,
        c => base14_other(c, bold),
    }
}

fn base14_other(ch: char, bold: bool) -> f32 {
    let b = |reg: f32, brd: f32| if bold { brd } else { reg };
    match ch {
        '!' => b(278.0, 333.0),
        '"' => b(355.0, 474.0),
        '#' | '$' => 556.0,
        '%' => 889.0,
        '&' => b(667.0, 722.0),
        '\'' => b(191.0, 238.0),
        '(' | ')' => 333.0,
        '*' => 389.0,
        '+' | '<' | '=' | '>' | '~' => 584.0,
        ',' | '.' | '/' | '[' | '\\' | ']' => 278.0,
        '-' => 333.0,
        ':' | ';' => b(278.0, 333.0),
        '?' => b(556.0, 611.0),
        '@' => 1015.0,
        'A' | 'B' => b(667.0, 722.0),
        'C' | 'D' | 'H' | 'N' | 'O' | 'Q' | 'R' | 'S' | 'U' | 'X' | 'Y' => b(722.0, 722.0),
        'E' => b(667.0, 667.0),
        'F' => b(611.0, 611.0),
        'G' | 'M' | 'W' => b(778.0, 778.0),
        'I' => 278.0,
        'J' => b(500.0, 556.0),
        'K' | 'V' => b(667.0, 667.0),
        'L' => b(556.0, 611.0),
        'P' => b(667.0, 667.0),
        'T' | 'Z' => b(611.0, 611.0),
        '^' => b(469.0, 584.0),
        '_' | '`' => b(556.0, 333.0),
        '{' | '}' => b(334.0, 389.0),
        '|' => b(260.0, 280.0),
        _ => 556.0,
    }
}

/// Measure a string's advance width in points (base-14).
pub fn base14_measure(text: &str, font: Font, size: f32) -> f32 {
    let bold = matches!(font, Font::HelveticaBold | Font::HelveticaBoldOblique);
    let units: f32 = text.chars().map(|c| helvetica_width(c, bold)).sum();
    units / 1000.0 * size
}

/// A horizontal glyph run in PostScript points.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub text: String,
    pub font: Font,
    pub font_size_pt: f32,
    /// Total advance width in points.
    pub advance_pt: f32,
}

impl GlyphRun {
    pub fn new(text: impl Into<String>, font: Font, size: f32) -> Self {
        let text = text.into();
        let advance_pt = base14_measure(&text, font, size);
        Self {
            text,
            font,
            font_size_pt: size,
            advance_pt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base14_widths_match_afm() {
        assert!((base14_measure("M", Font::Helvetica, 10.0) - 8.33).abs() < 0.01);
        assert!((base14_measure(" ", Font::Helvetica, 10.0) - 2.78).abs() < 0.01);
        assert!(
            base14_measure("l", Font::HelveticaBold, 10.0)
                > base14_measure("l", Font::Helvetica, 10.0)
        );
    }

    #[test]
    fn empty_store_falls_back_to_base14() {
        let mut store = FontStore::empty();
        assert_eq!(store.resolve(None, false, false), Font::Helvetica);
        assert_eq!(
            store.resolve(Some(&"Georgia, serif".into()), false, false),
            Font::TimesRoman
        );
        assert_eq!(
            store.resolve(Some(&"Arial".into()), false, false),
            Font::Helvetica
        );
    }

    #[test]
    fn face_metrics_extract() {
        // Build a minimal store from any system font we can find; skip
        // gracefully on systems without fonts.
        let store = FontStore::with_system_fonts();
        let mut store = store;
        let f = store.resolve(Some(&"Arial".into()), false, false);
        if let Font::Ttf(_) = f {
            let face = store.face(f).expect("face loaded");
            assert!(face.units_per_em > 0.0);
            assert!(!face.cmap.is_empty());
            let w = face.measure("MM", 12.0);
            assert!(w > 0.0);
        }
    }
}
