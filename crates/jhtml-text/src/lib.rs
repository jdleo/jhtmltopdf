//! Stage 4: font discovery and text shaping.
//!
//! Contract: font database in, shaped glyph runs out. fontdb + rustybuzz land
//! in M1. M0 ships the glyph-run measurement types.

/// A horizontal glyph run measured in PostScript points.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub text: String,
    pub font_size_pt: f32,
    pub advance_pt: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_holds_measurements() {
        let run = GlyphRun {
            text: "hi".into(),
            font_size_pt: 12.0,
            advance_pt: 10.5,
        };
        assert_eq!(run.text, "hi");
        assert_eq!(run.advance_pt, 10.5);
    }
}
