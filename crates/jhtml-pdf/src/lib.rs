//! Stage 5: the PDF writer.
//!
//! Contract: laid-out pages in, PDF bytes out. The layout tree IS the PDF —
//! every box emits operators directly. Font subsetting and object streams
//! land in M1/M2. M0 ships a minimal but *valid* single-page PDF emitter,
//! which doubles as the end-to-end M0 proof.

/// Emit a minimal valid one-page PDF with the given title as document text.
///
/// Deliberately tiny: hand-rolled objects, no compression, exists so the
/// whole pipeline can be tested end-to-end before any stage is real.
pub fn write_stub_pdf(title: &str) -> Vec<u8> {
    let title = sanitize(title);
    let content = format!("BT /F1 18 Tf 72 770 Td ({title}) Tj ET");
    let objects: [&str; 5] = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        &format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];

    let mut out = b"%PDF-1.4\n".to_vec();
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
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{initial_at}\n%%EOF\n",
            objects.len() + 1,
            initial_at = xref_at
        )
        .as_bytes(),
    );
    out
}

fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(200)
        .collect();
    cleaned
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_valid_pdf_envelope() {
        let pdf = write_stub_pdf("jhtmltopdf M0");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.windows(5).any(|w| w == b"%%EOF"));
        assert!(pdf.windows(4).any(|w| w == b"xref"));
    }

    #[test]
    fn escapes_pdf_string_specials() {
        let pdf = write_stub_pdf(r"a(b) \");
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains(r"a\(b\) \\"));
    }
}
