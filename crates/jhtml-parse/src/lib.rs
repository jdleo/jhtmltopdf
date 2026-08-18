//! Stage 1: HTML bytes into a document tree.
//!
//! Contract: owns parsing, nothing else. Backed by html5ever (shipment 3);
//! M0 ships a placeholder that only extracts the title.

/// A parsed input document. Structure will grow with html5ever's DOM in M1+.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub title: Option<String>,
}

impl Document {
    /// Parse raw HTML bytes into a document tree.
    pub fn parse(html: &[u8]) -> Self {
        let text = String::from_utf8_lossy(html);
        let title = text
            .rsplit_once("<title>")
            .and_then(|(_, rest)| rest.split_once("</title>"))
            .map(|(t, _)| t.trim().to_string());
        Self { title }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title() {
        let doc = Document::parse(b"<html><head><title> hi </title></head></html>");
        assert_eq!(doc.title.as_deref(), Some("hi"));
    }

    #[test]
    fn no_title_is_none() {
        let doc = Document::parse(b"<p>hello</p>");
        assert_eq!(doc.title, None);
    }
}
