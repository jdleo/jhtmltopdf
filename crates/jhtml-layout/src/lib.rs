//! Stage 3: box tree and paged layout.
//!
//! Contract: computed styles in, laid-out pages out. The box tree lives in
//! arenas; page layout fans out across rayon workers in M5. M0 ships the
//! page geometry types only.

/// Page geometry from @page rules (A4 default).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    /// Width in PostScript points (1/72 inch).
    pub width_pt: f32,
    /// Height in PostScript points.
    pub height_pt: f32,
    pub margins_pt: [f32; 4],
}

impl Default for PageGeometry {
    fn default() -> Self {
        Self {
            width_pt: 595.0,
            height_pt: 842.0,
            margins_pt: [72.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_a4() {
        let g = PageGeometry::default();
        assert_eq!(g.width_pt, 595.0);
        assert_eq!(g.height_pt, 842.0);
    }
}
