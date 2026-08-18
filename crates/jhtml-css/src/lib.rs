//! Stage 2: CSS parsing and the cascade.
//!
//! Contract: raw CSS + DOM in, computed values out. Backed by servo cssparser
//! (shipment 3). M0 ships the stylesheet container only.

/// One parsed stylesheet. Real rule representation lands in M2.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stylesheet {
    pub source: String,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        Self {
            source: css.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_source() {
        let ss = Stylesheet::parse("h1 { color: red; }");
        assert_eq!(ss.source, "h1 { color: red; }");
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(Stylesheet::default().source, "");
    }
}
