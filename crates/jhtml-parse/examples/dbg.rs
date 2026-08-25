fn main() {
    let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let html = std::fs::read(cwd.join("../jhtmltopdf/benches/cases/case3_complex.html")).unwrap();
    let doc = jhtml_parse::Document::parse(&html);
    fn walk(n: &jhtml_parse::Node, d: &mut usize) {
        if n.tag() == Some("h2") && *d < 2 {
            println!("h2 id: {:?}", n.attr("id"));
            *d += 1;
        }
        for c in n.children() {
            walk(c, d);
        }
    }
    let mut d = 0;
    walk(&doc.root, &mut d);
}
