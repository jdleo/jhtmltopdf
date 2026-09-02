# jhtmltopdf

[![Crates.io](https://img.shields.io/crates/v/jhtmltopdf.svg)](https://crates.io/crates/jhtmltopdf)
[![Docs.rs](https://docs.rs/jhtmltopdf/badge.svg)](https://docs.rs/jhtmltopdf)
[![CI](https://github.com/jdleo/jhtmltopdf/actions/workflows/ci.yml/badge.svg)](https://github.com/jdleo/jhtmltopdf/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pure-Rust HTML to PDF engine. No WebKit, no browser engine, no native
dependencies. One binary, print-first, fast.

Blog post: TBD

## Install

From crates.io:

```
cargo install jhtmltopdf
```

Or from source:

```
git clone https://github.com/jdleo/jhtmltopdf
cd jhtmltopdf
cargo install --path crates/jhtmltopdf
```

Or download a binary from [Releases](https://github.com/jdleo/jhtmltopdf/releases) and put it on your PATH.

## Use

Render a file:

```
jhtmltopdf render report.html
```

Render a page designed for a wide screen viewport (like wkhtmltopdf's
shrink-to-fit, but explicit):

```
jhtmltopdf render --viewport-width=1024 --page-size=letter --margin-mm=7 invoice.html
```

As a library:

```rust
let pdf = jhtmltopdf::render(b"<h1>hello</h1>");
std::fs::write("out.pdf", pdf).unwrap();
```

## What it supports

- CSS 2.1 core: cascade, colors, backgrounds, borders, tables, flex rows
- CSS Paged Media: `@page`, margin boxes, `counter(page)` / `counter(pages)`, page breaks
- Real font embedding: system fonts via fontdb, CIDFontType2 + Identity-H, ToUnicode
- PDF bookmarks from headings, internal + external link annotations, metadata
- Compute-only JS (optional): scripts run in a sandboxed Boa realm (no DOM, no
  network, no fs), values inject via `{{key}}` placeholders

Not supported (yet): RTL/bidi text, CSS grid, footnotes, PDF/A or PDF/UA profiles.

## Benchmarks

Same 3 cases as the deep-dive benchmark, cold-start single run, Apple Silicon Mac.
Baseline is WeasyPrint 69.0 (Pango 1.57.1) on identical inputs.

| Case | Input | WeasyPrint | jhtmltopdf | Speedup |
|------|-------|------------|------------|---------|
| Simple (1 page demo) | 1.6 KB | 0.31s | ~0.14s | ~2x |
| Resume (real one-pager) | 10.6 KB | 0.36s | ~0.08s | ~4.5x |
| Complex (60-section report) | 310 KB, 121 pages | 6.3s | ~0.35s | ~18x |

For context, wkhtmltopdf 0.12.6 does the complex case in ~0.85s but only by
silently shrinking the layout (renders 61 pages instead of the 121 the CSS
asks for). jhtmltopdf renders all 121 pages with correct pagination.

## Design, briefly

Print-first pipeline, no browser engine. HTML parses via html5ever, CSS goes
through a hand-rolled cascade, a custom layout engine does paged layout
(block/inline/tables/flex/fragmentation) against the CSS specs, and the layout
tree converts directly into PDF operators — nothing is painted then converted.
Pages fan out through rayon for parallel content-stream generation. JavaScript
runs in a compute-only Boa sandbox before layout and can only inject data,
never touch layout. Each stage is its own crate: `jhtml-parse`, `jhtml-css`,
`jhtml-layout`, `jhtml-text`, `jhtml-pdf`, `jhtml-js`.

## License

MIT: free to use, modify, and redistribute for any purpose. The only requirement is keeping the copyright notice (attribution). No warranty.
