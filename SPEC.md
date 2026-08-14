# SPEC.md — jhtmltopdf

> Pure-Rust HTML→PDF engine. No WebKit, no browser engine, no native deps.
> Blazing fast, print-first, embeddable JS. One static binary.

## 1. Mission

A from-scratch HTML→PDF converter in 100% safe Rust that:

- **Renders the CSS as written.** Print-first, like WeasyPrint — no "smart shrinking" lies.
- **Is fast.** Target 10x over WeasyPrint on our benchmark suite (`HTML2PDF Deep Dive/benchmark/`). 3x+ = magnificent. Should also beat wkhtmltopdf cold start (~2s Qt init → we target <100ms warm, <300ms cold).
- **Is pragmatic.** Not a browser. Not a layout engine for screens. HTML document → paginated PDF, done extremely well.
- **Is simple & beautiful.** Small crate graph, thin vertical slice first, every module has one job.

Non-goals (v1): screen rendering, cookies/sessions/browsing, event-driven interactivity, RTL/bidi text, full GCPM.

## 2. Locked Decisions

| Area | Decision | Rationale |
|------|----------|-----------|
| JS engine | **Boa** (pure Rust) | Keeps the zero-native-deps story. Safe, embeddable. |
| JS scope | **Compute-only, no DOM** | Scripts run in a sandbox with user-defined host functions. Charts/data must be computed, not drawn. The "tough part" gets scoped to: script execution → results feed the template/data layer, never the layout. |
| CSS bar (v1) | **CSS 2.1 + Paged Media 3** | @page, margin boxes, page counters, page breaks, named pages. Beats dead-2012 WebKit, matches WeasyPrint where it matters most. |
| Parsers | **html5ever + servo cssparser** | Battle-tested, spec-true, blazing. Don't reinvent. |
| Text shaping | **rustybuzz/swash, pure Rust** | Kills Pango/glib/fontconfig dependency hell (we hit it live). v1: LTR Latin-first, minor complex-script deltas accepted. |
| Fonts | **fontdb + system scan + @font-face** | TTF/OTF/WOFF2. Subset + embed via own writer. |
| Images | **Raster first** (PNG/JPEG), basic SVG later | Covers benchmark + typical docs. |
| Speed tactics | **Rayon parallel pages** | Page layout is embarrassingly parallel. Arena allocation, zero-copy parsing, SIMD where hot. |
| PDF features (v1) | **Core**: links (ext+int), bookmarks from h1–h6, metadata, font embed/subset | The crossover list. PDF/UA, PDF/A → v1.1+. |
| Surface | **CLI + Rust library** | Same core. FFI/WASM later — the core knows nothing about CLI. |
| Platforms | **macOS + Linux** | Static-ish binaries via CI. Windows later. |
| Strategy | **Thin vertical slice first** | End-to-end tiny HTML→PDF in week one. Benchmarkable from day one. Then deepen stages. |
| License/name | **MIT OR Apache-2.0, `jhtmltopdf`** | As-is. |

## 3. Architecture

Pure pipeline. Each stage is a crate; the core is renderer-agnostic.

```
HTML bytes
  → [jhtml-parse]   html5ever → DOM; cssparser → stylesheet objects
  → [jhtml-css]     cascade, selectors, computed values, @page rules
  → [jhtml-layout]  box tree → paged layout (parallel via rayon)
  → [jhtml-text]    fontdb, rustybuzz shaping, glyph runs (shared service)
  → [jhtml-pdf]     pydyf-equivalent: layout tree → PDF operators + objects
  → [jhtml-js]      Boa sandbox, compute-only, host API
  → [jhtmltopdf]    facade crate: library API + binary
```

**Pipeline invariants:**

1. **Layout tree IS the PDF.** No paint-then-convert. Each laid-out box emits PDF operators directly (WeasyPrint's proven model).
2. **Pages are independent after global layout.** Stage A: single-threaded global pass (page break decisions, counters, running headers). Stage B: `rayon` fan-out — per-page glyph shaping + PDF page emission. This is where the 10x lives.
3. **Zero-copy where possible.** Borrow input bytes through parse; arena-allocate the box tree; shape glyphs into pooled buffers.
4. **JS never touches layout.** Boa runs scripts pre-layout over a JSON-ish document model + host functions (e.g. `set_data`, fetch-equivalents are out; users inject data). Output merges into the DOM as text/attributes. This keeps the engine deterministic and the hard part tractable.
5. **Font cache is global + content-addressed.** Shaping results keyed by (font, size, text) — benchmark case3 repeats glyphs massively.

## 4. Milestones (thin vertical slices)

- **M0 — skeleton:** workspace, crates, CI (mac+linux), bench harness ported from our benchmark suite. Pass/fail: `cargo bench` runs.
- **M1 — text PDF:** HTML → paragraphs → PDF with embedded subset fonts. Ugly but real. Pass/fail: case1 renders correctly.
- **M2 — CSS 2.1 core:** cascade, block/inline/tables, colors, borders, images. Pass/fail: case2 (resume) within 1 page parity of WeasyPrint.
- **M3 — paged media:** @page, margin boxes, counters, breaks, bookmarks, links. Pass/fail: case3 page-for-page with WeasyPrint (121), footers correct.
- **M4 — go fast:** rayon page fan-out, arenas, shaping cache, profiling against the 10x target. Pass/fail: case3 < 630ms (10x) / < 2.1s (3x), correct output.
- **M5 — Boa JS:** sandbox, host API, docs. Pass/fail: compute-driven invoice renders.
- **M6 — polish:** error messages, `--help` like a grown-up, publish crate.

## 5. Benchmark Contract

Our own suite (same 3 cases as the deep dive):

| Case | Input | WeasyPrint baseline | jhtmltopdf target |
|------|-------|---------------------|-------------------|
| 1 Simple | 1.6 KB | 0.31s | < 50ms |
| 2 Resume | 10.6 KB | 0.36s | < 50ms |
| 3 Complex | 310 KB, 121pp | 6.3s | < 630ms (10x) / < 2.1s (3x) |

Correctness gate: byte-different is fine; **page count + text extraction must match WeasyPrint** on all 3 cases before any speed claim counts.

## 6. Open Questions

- SVG: resvg dependency vs hand-rolled minimal path renderer (defer to v1.1).
- WOFF2 decode: `woff2` crate vs fontTools-style own decoder.
- Hyphenation: port of Pyphen dictionaries (they're just XML) — v1.1?
- ToC generation: `target-counter()` (real WeasyPrint-style) vs wkhtmltopdf-style XSLT. Leaning: target-counter, it's the honest way.
- Error strategy: thiserror crates + `--diagnose` flag? Decided at M2.
