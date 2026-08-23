fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let root = std::path::Path::new(manifest).to_path_buf();
    for c in ["case1_simple", "case2_resume", "case3_complex"] {
        let html = std::fs::read(root.join(format!("benches/cases/{c}.html"))).unwrap();
        let pdf = jhtmltopdf::render(&html);
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join(format!("target/{c}.pdf")), &pdf).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        println!(
            "{c}: {} bytes, {} pages",
            pdf.len(),
            s.matches("/Type /Page ").count()
        );
    }
}
