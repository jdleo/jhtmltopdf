#![allow(clippy::if_same_then_else)]
//! Stage 4: fonts and text metrics.
//!
//! Metric tables mirror the AFM data verbatim; allow cosmetic clippy lints there.
//! v0.1 ships the PDF base-14 fonts (Helvetica family) with their exact
//! AFM advance widths. No font files, no embedding, deterministic metrics.
//! fontdb + rustybuzz embedding/subsetting arrives in a later milestone.

/// One of the PDF base-14 fonts we support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    Courier,
}

impl Font {
    /// PDF BaseFont name.
    pub fn base_name(self) -> &'static str {
        match self {
            Font::Helvetica => "Helvetica",
            Font::HelveticaBold => "Helvetica-Bold",
            Font::HelveticaOblique => "Helvetica-Oblique",
            Font::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Font::TimesRoman => "Times-Roman",
            Font::Courier => "Courier",
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

/// Advance width of `ch` in 1/1000 em units for base-14 Helvetica.
/// AFM-derived; anything outside the table falls back to 556.
pub fn helvetica_width(ch: char, bold: bool) -> f32 {
    use W::*;

    match ch.to_ascii_lowercase() {
        ' ' => {
            if bold {
                B_SPACE
            } else {
                SPACE
            }
        }
        'a' => {
            if bold {
                B_A
            } else {
                A
            }
        }
        'b' => {
            if bold {
                B_B
            } else {
                B
            }
        }
        'c' => {
            if bold {
                B_C
            } else {
                C
            }
        }
        'd' => {
            if bold {
                B_D
            } else {
                D
            }
        }
        'e' => {
            if bold {
                B_E
            } else {
                E
            }
        }
        'f' => {
            if bold {
                B_F
            } else {
                F
            }
        }
        'g' => {
            if bold {
                B_G
            } else {
                G
            }
        }
        'h' => {
            if bold {
                B_H
            } else {
                H
            }
        }
        'i' => {
            if bold {
                B_I
            } else {
                I
            }
        }
        'j' => {
            if bold {
                B_J
            } else {
                J
            }
        }
        'k' => {
            if bold {
                B_K
            } else {
                K
            }
        }
        'l' => {
            if bold {
                B_L
            } else {
                L
            }
        }
        'm' => {
            if bold {
                B_M
            } else {
                M
            }
        }
        'n' => {
            if bold {
                B_N
            } else {
                N
            }
        }
        'o' => {
            if bold {
                B_O
            } else {
                O
            }
        }
        'p' => {
            if bold {
                B_P
            } else {
                P
            }
        }
        'q' => {
            if bold {
                B_Q
            } else {
                Q
            }
        }
        'r' => {
            if bold {
                B_R
            } else {
                R
            }
        }
        's' => {
            if bold {
                B_S
            } else {
                S
            }
        }
        't' => {
            if bold {
                B_T
            } else {
                T
            }
        }
        'u' => {
            if bold {
                B_U
            } else {
                U
            }
        }
        'v' => {
            if bold {
                B_V
            } else {
                V
            }
        }
        'w' => {
            if bold {
                B_W
            } else {
                W_
            }
        }
        'x' => {
            if bold {
                B_X
            } else {
                X
            }
        }
        'y' => {
            if bold {
                B_Y
            } else {
                Y
            }
        }
        'z' => {
            if bold {
                B_Z
            } else {
                Z
            }
        }
        '0'..='9' => 556.0,
        _ => other_width(ch, bold),
    }
}

/// Non-alphabetic AFM widths (regular, bold).
fn other_width(ch: char, bold: bool) -> f32 {
    match ch {
        '!' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '"' => {
            if bold {
                474.0
            } else {
                355.0
            }
        }
        '#' => {
            if bold {
                556.0
            } else {
                556.0
            }
        }
        '$' => {
            if bold {
                556.0
            } else {
                556.0
            }
        }
        '%' => {
            if bold {
                889.0
            } else {
                889.0
            }
        }
        '&' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        '\'' => {
            if bold {
                238.0
            } else {
                191.0
            }
        }
        '(' => {
            if bold {
                333.0
            } else {
                333.0
            }
        }
        ')' => {
            if bold {
                333.0
            } else {
                333.0
            }
        }
        '*' => {
            if bold {
                389.0
            } else {
                389.0
            }
        }
        '+' => {
            if bold {
                584.0
            } else {
                584.0
            }
        }
        ',' => {
            if bold {
                278.0
            } else {
                278.0
            }
        }
        '-' => {
            if bold {
                333.0
            } else {
                333.0
            }
        }
        '.' => {
            if bold {
                278.0
            } else {
                278.0
            }
        }
        '/' => {
            if bold {
                278.0
            } else {
                278.0
            }
        }
        ':' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        ';' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '<' => {
            if bold {
                584.0
            } else {
                584.0
            }
        }
        '=' => {
            if bold {
                584.0
            } else {
                584.0
            }
        }
        '>' => {
            if bold {
                584.0
            } else {
                584.0
            }
        }
        '?' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        '@' => {
            if bold {
                1015.0
            } else {
                1015.0
            }
        }
        'A' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        'B' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        'C' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'D' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'E' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'F' => {
            if bold {
                611.0
            } else {
                611.0
            }
        }
        'G' => {
            if bold {
                778.0
            } else {
                778.0
            }
        }
        'H' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'I' => {
            if bold {
                278.0
            } else {
                278.0
            }
        }
        'J' => {
            if bold {
                556.0
            } else {
                500.0
            }
        }
        'K' => {
            if bold {
                722.0
            } else {
                667.0
            }
        }
        'L' => {
            if bold {
                611.0
            } else {
                556.0
            }
        }
        'M' => {
            if bold {
                833.0
            } else {
                833.0
            }
        }
        'N' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'O' => {
            if bold {
                778.0
            } else {
                778.0
            }
        }
        'P' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'Q' => {
            if bold {
                778.0
            } else {
                778.0
            }
        }
        'R' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'S' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'T' => {
            if bold {
                611.0
            } else {
                611.0
            }
        }
        'U' => {
            if bold {
                722.0
            } else {
                722.0
            }
        }
        'V' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'W' => {
            if bold {
                944.0
            } else {
                944.0
            }
        }
        'X' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'Y' => {
            if bold {
                667.0
            } else {
                667.0
            }
        }
        'Z' => {
            if bold {
                611.0
            } else {
                611.0
            }
        }
        '[' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '\\' => {
            if bold {
                278.0
            } else {
                278.0
            }
        }
        ']' => {
            if bold {
                333.0
            } else {
                278.0
            }
        }
        '^' => {
            if bold {
                584.0
            } else {
                469.0
            }
        }
        '_' => {
            if bold {
                556.0
            } else {
                556.0
            }
        }
        '`' => {
            if bold {
                333.0
            } else {
                333.0
            }
        }
        '{' => {
            if bold {
                389.0
            } else {
                334.0
            }
        }
        '|' => {
            if bold {
                280.0
            } else {
                260.0
            }
        }
        '}' => {
            if bold {
                389.0
            } else {
                334.0
            }
        }
        '~' => {
            if bold {
                584.0
            } else {
                584.0
            }
        }
        _ => {
            if bold {
                584.0
            } else {
                556.0
            }
        }
    }
}

/// Widths table namespace (AFM values, 1/1000 em).
#[allow(non_snake_case)]
mod W {
    pub const SPACE: f32 = 278.0;
    pub const A: f32 = 556.0;
    pub const B: f32 = 556.0;
    pub const C: f32 = 500.0;
    pub const D: f32 = 556.0;
    pub const E: f32 = 556.0;
    pub const F: f32 = 278.0;
    pub const G: f32 = 556.0;
    pub const H: f32 = 556.0;
    pub const I: f32 = 222.0;
    pub const J: f32 = 222.0;
    pub const K: f32 = 500.0;
    pub const L: f32 = 222.0;
    pub const M: f32 = 833.0;
    pub const N: f32 = 556.0;
    pub const O: f32 = 556.0;
    pub const P: f32 = 556.0;
    pub const Q: f32 = 556.0;
    pub const R: f32 = 333.0;
    pub const S: f32 = 500.0;
    pub const T: f32 = 278.0;
    pub const U: f32 = 556.0;
    pub const V: f32 = 500.0;
    pub const W_: f32 = 722.0;
    pub const X: f32 = 500.0;
    pub const Y: f32 = 500.0;
    pub const Z: f32 = 500.0;

    pub const B_SPACE: f32 = 278.0;
    pub const B_A: f32 = 556.0;
    pub const B_B: f32 = 611.0;
    pub const B_C: f32 = 556.0;
    pub const B_D: f32 = 611.0;
    pub const B_E: f32 = 556.0;
    pub const B_F: f32 = 333.0;
    pub const B_G: f32 = 611.0;
    pub const B_H: f32 = 611.0;
    pub const B_I: f32 = 278.0;
    pub const B_J: f32 = 278.0;
    pub const B_K: f32 = 556.0;
    pub const B_L: f32 = 278.0;
    pub const B_M: f32 = 889.0;
    pub const B_N: f32 = 611.0;
    pub const B_O: f32 = 611.0;
    pub const B_P: f32 = 611.0;
    pub const B_Q: f32 = 611.0;
    pub const B_R: f32 = 389.0;
    pub const B_S: f32 = 556.0;
    pub const B_T: f32 = 333.0;
    pub const B_U: f32 = 611.0;
    pub const B_V: f32 = 556.0;
    pub const B_W: f32 = 778.0;
    pub const B_X: f32 = 556.0;
    pub const B_Y: f32 = 556.0;
    pub const B_Z: f32 = 500.0;
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

/// Measure a string's advance width in points.
pub fn measure(text: &str, font: Font, size: f32) -> f32 {
    let bold = matches!(font, Font::HelveticaBold | Font::HelveticaBoldOblique);
    let units: f32 = text.chars().map(|c| helvetica_width(c, bold)).sum();
    units / 1000.0 * size
}

impl GlyphRun {
    pub fn new(text: impl Into<String>, font: Font, size: f32) -> Self {
        let text = text.into();
        let advance_pt = measure(&text, font, size);
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
    fn widths_match_afm() {
        // 'M' is 833/1000 em in regular Helvetica.
        assert!((measure("M", Font::Helvetica, 10.0) - 8.33).abs() < 0.01);
        // space is 278.
        assert!((measure(" ", Font::Helvetica, 10.0) - 2.78).abs() < 0.01);
        // Bold 'l' is 278 vs regular 222.
        assert!(measure("l", Font::HelveticaBold, 10.0) > measure("l", Font::Helvetica, 10.0));
    }

    #[test]
    fn run_measures_itself() {
        let run = GlyphRun::new("MM", Font::Helvetica, 12.0);
        assert!((run.advance_pt - 19.992).abs() < 0.01);
    }
}
