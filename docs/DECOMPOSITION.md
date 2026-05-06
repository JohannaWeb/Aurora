# Aurora `src/` Decomposition Plan

A practical guide to breaking up the current `src/` god-modules into a clean
folder-per-module tree, with a **hard cap of 200 lines per file** and SOLID /
clean code principles applied to Rust idioms.

---

## 0. Why this exists

Current state (lines per file, May 2026):

| File          | Lines | Status                |
|---------------|-------|-----------------------|
| `js_boa.rs`   | 3,418 | god module            |
| `layout.rs`   | 2,179 | god module            |
| `css.rs`      | 1,476 | god module            |
| `window.rs`   | 1,257 | god module            |
| `html.rs`     |   852 | over cap              |
| `paint.rs`    |   787 | over cap              |
| `fetch.rs`    |   757 | over cap              |
| `main.rs`     |   690 | over cap              |
| `style.rs`    |   502 | over cap              |
| `gpu_paint.rs`|   501 | over cap              |
| `font.rs`     |   319 | slightly over         |
| `dom.rs`      |   304 | slightly over         |
| `atlas.rs`    |   290 | slightly over         |
| `js.rs`       |     1 | placeholder, delete   |

Total: ~13.3K LOC in 14 files. Target: same LOC, ~110-130 files, **none > 200
lines**, each with one reason to change.

---

## 1. Ground rules

### 1.1 The 200-line cap is non-negotiable

- Counted as actual LOC, including blank lines and comments. If the file is
  200 lines, it's full. Plan for 150 as the "comfortable" working size so
  future edits don't immediately bust the cap.
- If a single item (e.g. a 220-line `match` over CSS properties) cannot be
  reduced further, split it into an extension `impl` block in a sibling file.
  Rust lets a type's `impl` blocks live across many files inside the same
  crate — use that.
- Generated/derived code (large `#[derive]` payloads) does not count against
  the budget visually but does count against the cap. Prefer breaking the
  enum into smaller related enums where possible.

### 1.2 Folder-per-module with `mod.rs`

Every current top-level file becomes a directory:

```
src/
  layout/
    mod.rs        ← facade only: declares submodules + re-exports
    tree.rs
    box.rs
    rect.rs
    block.rs
    inline.rs
    flex.rs
    image.rs
    control.rs
    constraints.rs
    text_metrics.rs
    debug.rs
    tests.rs
```

`mod.rs` is a **facade**, not a dumping ground. It contains:

1. `mod` declarations for every submodule (private by default).
2. `pub use` statements re-exporting the module's public API.
3. Module-level doc comment explaining the boundary.
4. **No type definitions, no function bodies.** Hard rule.

Result: callers keep writing `use crate::layout::LayoutBox` even though the
type now lives in `crate::layout::box::LayoutBox`. The public surface is
stable; the internal split is invisible.

### 1.3 `mod.rs` template

```rust
//! Layout: turn a styled DOM tree into positioned boxes.
//!
//! Public API: see re-exports below. Internal modules are private.

mod r#box;          // raw identifier — `box` is a Rust keyword
mod block;
mod constraints;
mod debug;
mod display;
mod flex;
mod image;
mod inline;
mod control;
mod rect;
mod text_metrics;
mod tree;

#[cfg(test)]
mod tests;

pub use r#box::{LayoutBox, LayoutKind};
pub use rect::Rect;
pub use tree::{LayoutTree, ViewportSize};
```

---

## 2. SOLID, recast for Rust modules

Rust does not have classes, so SOLID lands on *modules*, *traits*, and *types*:

| Principle | Rust translation                                                                 |
|-----------|----------------------------------------------------------------------------------|
| **SRP**   | One module = one reason to change. Tokenize ≠ parse ≠ classify tags.             |
| **OCP**   | Add new variants by adding files, not by editing a giant `match`. Use traits or sealed enums + dispatch tables. |
| **LSP**   | Trait impls must honor the trait's contract. If `Painter` says "idempotent", every backend must be. |
| **ISP**   | Don't make a caller depend on a 30-method trait when it only needs 2. Split traits by use case. |
| **DIP**   | High-level modules (the engine pipeline) depend on traits, not on concrete `BoaRuntime` / `Vello` types. Backends live behind `dyn Trait` or generic params. |

### 2.1 Aurora-specific dependency rule

The pipeline is a one-way DAG:

```
fetch ──► html ──► dom ──► css ──► style ──► layout ──► paint ──► gpu_paint ──► window
                                          ▲                                  ▲
                                          └──── js (mutates dom + style) ────┘
```

**No upward edges.** `dom` must not import `style`. `layout` must not import
`window`. If you need a back-reference (e.g. JS triggering reflow), invert it
through a callback / channel passed *into* the lower layer — that's DIP at
the module level.

---

## 3. Clean code rules

These are the rules to apply *while* splitting, not after:

1. **Names beat comments.** `parse_attributes` does not need a doc explaining
   it parses attributes. Comments only for the *why* of a non-obvious choice.
2. **No `mod_helpers.rs`, no `mod_utils.rs`.** "Utils" is a confession that
   you didn't find the right home. Put helpers next to the type they serve,
   or — if shared by 3+ modules — give them their own named module.
3. **One `pub struct` / `pub enum` per file** is a useful default. Allowed
   exceptions: tightly coupled type + builder, or small enum + its `Display`.
4. **Tests live next to code.** Either `#[cfg(test)] mod tests;` in a sibling
   file (preferred at this size) or `#[cfg(test)]` block inline if < 30 lines.
5. **`pub` is a promise.** Every `pub` item in a `mod.rs` re-export is a
   contract you're committing to. Don't reflexively re-export everything —
   re-export what callers actually need.
6. **Newtype over comments for invariants.** A `struct Px(f32)` self-documents
   "pixels, not ems"; a `// in pixels` comment rots.

---

## 4. Module-by-module decomposition

Each section below: target tree, sized so every file fits the 200-line cap,
plus the SOLID concern that drove the split.

### 4.1 `js_boa.rs` (3,418 → ~20 files)

This is the worst offender and the highest-leverage split. Today it mixes:
runtime construction, JS↔Rust value conversion, DOM tree walking, selector
parsing, attribute reflection, Web API stubs (XHR, observers, storage), and
DOM mutation primitives.

```
js_boa/
  mod.rs                 # facade + pub use BoaRuntime
  runtime.rs             # BoaRuntime, install order, lifecycle           (~150)
  registry.rs            # NodeRegistry + Finalize                          (~80)
  capture.rs             # NodeCapture, DocCapture, WindowCapture           (~80)
  convert.rs             # native_to_jsfn, js_string_of, node_from_js       (~80)
  globals/
    mod.rs               # facade for install_globals
    location.rs          # window.location                                  (~150)
    history.rs           # window.history                                   (~120)
    navigator.rs         # window.navigator + screen + performance          (~120)
    timers.rs            # setTimeout / setInterval / rAF / rIC             (~150)
    media.rs             # matchMedia, getComputedStyle                     (~150)
  document/
    mod.rs               # install_document facade
    accessors.rs         # body / head / documentElement / readyState       (~150)
    queries.rs           # getElementById/byTagName/byClassName             (~150)
    factories.rs         # createElement/createTextNode/createDocumentFragment (~120)
    implementation.rs    # build_document_implementation                    (~100)
  node/
    mod.rs
    create.rs            # create_js_node                                   (~200)
    accessors.rs         # install_accessors + install_accessor             (~200)
    reflection.rs        # element reflection / attribute reflectors        (~200)
    style_binding.rs     # build_style_object                               (~120)
    classlist.rs         # build_classlist_object + classlist_modify        (~150)
  tree/
    mod.rs
    navigation.rs        # first/last child, sibling, parent, contains      (~150)
    mutation.rs          # append/insert/remove/replace child               (~120)
    traversal.rs         # find_by_id, find_by_tag, collect_by_*            (~180)
    text.rs              # collect_text, set_text_content                   (~60)
    clone.rs             # clone_node                                       (~50)
  selectors/
    mod.rs
    parse.rs             # parse_simple, parse_selector_groups              (~180)
    match.rs             # simple_matches, selector_matches                 (~100)
    query.rs             # query_first, query_all, build_nodelist           (~80)
  observers.rs           # Mutation/Intersection/Resize stubs               (~150)
  storage.rs             # localStorage / sessionStorage                    (~150)
  network.rs             # XHR + fetch survival stubs                       (~180)
  constructors.rs        # install_dom_constructors + prototype wiring      (~150)
```

**SRP wins:** "selector parsing" is now ~180 lines you can read end-to-end,
not buried inside a 3,400-line file.

**OCP wins:** adding a new Web API stub means adding a sibling file under
`globals/` and one line in `globals/mod.rs`. No diff in unrelated code.

**ISP wins:** `node::reflection` only depends on `NodeCapture`, not on
`BoaRuntime`. Tests for selectors don't drag in the entire JS engine.

### 4.2 `layout.rs` (2,179 → ~12 files)

The 1,372-line `impl LayoutBox` is the meat. Split it by **layout mode**, not
by helper-vs-method.

```
layout/
  mod.rs
  tree.rs               # LayoutTree, ViewportSize                         (~130)
  box.rs                # LayoutBox struct, LayoutKind enum, ctors          (~180)
  rect.rs               # Rect + Display                                    (~50)
  block.rs              # block-flow layout                                 (~200)
  inline.rs             # inline-flow + line breaking                       (~200)
  flex.rs               # flex layout                                       (~200)
  image.rs              # <img> sizing & intrinsic ratios                   (~150)
  control.rs            # <input>/<button> control labels & sizing          (~150)
  scroll.rs             # overflow + scrollbar reservation                  (~120)
  constraints.rs        # clamp_content_width/height, parse_html_length_px  (~150)
  text_metrics.rs       # font_size_from_styles, measure_text_width, etc.   (~80)
  debug.rs              # Display impls + format_styles                     (~80)
  tests.rs              # extracted from inline #[cfg(test)] mod            (~200)
```

**Where impl-block-split helps:** `LayoutBox` is one type, but each layout
mode lives in its own file as an `impl LayoutBox { fn layout_block(...) }`
block. Rust merges all `impl` blocks of a type at compile time.

### 4.3 `css.rs` (1,476 → ~10 files)

Today this conflates the parser, the AST, the cascade, and value-level
shorthand expansion.

```
css/
  mod.rs
  stylesheet.rs         # Stylesheet + parsing entry                       (~200)
  cascade.rs            # collect_styles                                   (~80)
  ast.rs                # Rule, Selector, SimpleSelector, Declaration      (~80)
  style_map.rs          # StyleMap struct + core impls                     (~200)
  style_map_resolve.rs  # resolved getters (color/font/etc.) on StyleMap   (~200)
  box_model.rs          # EdgeSizes, Margin, MarginValue                   (~100)
  properties.rs         # DisplayMode/FlexDirection/JustifyContent/etc.    (~180)
  length.rs             # LengthValue + parse_length_value/px              (~100)
  selector_match.rs     # impl Selector + impl SimpleSelector matching     (~200)
  shorthand.rs          # parse_margin_*, parse_box_shorthand,
                        # parse_border_width_shorthand, etc.               (~200)
  parse_helpers.rs      # is_identifier_char, strip_pseudo_suffix,
                        # strip_at_rules, extract_import_url               (~150)
  display.rs            # Display impls                                    (~80)
```

**OCP:** adding a new shorthand goes in `shorthand.rs`. Adding a new
property enum goes in `properties.rs`. No edits to the `Stylesheet` parser.

### 4.4 `window.rs` (1,257 → ~8 files)

Cleanly splits along: app lifecycle vs. scene composition vs. screenshot.

```
window/
  mod.rs
  input.rs              # WindowInput                                      (~50)
  open.rs               # open() entry point, identity wiring              (~80)
  app.rs                # AuroraApp struct + ApplicationHandler impl       (~200)
  app_handlers.rs       # event handlers extracted from impl               (~200)
  chrome.rs             # browser chrome rendering + url truncation        (~200)
  scrollbar.rs          # render_scrollbars, draw_scrollbar_if_needed      (~120)
  scene_helpers.rs      # fill_scene_rect, stroke_scene_rect               (~50)
  screenshot/
    mod.rs
    render.rs           # render_to_file, render_layout_with_text          (~200)
    text.rs             # render_text_simple, draw_glyph_bitmap            (~150)
    primitives.rs       # draw_border, draw_rect, parse_screenshot_color   (~150)
  scroll_metrics.rs     # scroll_content_height, max_box_bottom            (~50)
```

**DIP:** `screenshot/` should depend on a `Renderer` trait, not on `Vello`
directly. Today the dependency is hard-coded; the split is the moment to
introduce the seam.

### 4.5 `html.rs` (852 → ~6 files)

```
html/
  mod.rs
  parser.rs             # Parser struct + parse loop                       (~200)
  tokenizer.rs          # tokenize() state machine                         (~200)
  tokens.rs             # Token, TagToken                                  (~50)
  tag_parsing.rs        # parse_open_tag, parse_attributes                 (~200)
  classify.rs           # is_raw_text_tag, is_void_tag, find_tag_end       (~80)
  text.rs               # collapse_whitespace, decode_entities             (~80)
  tests.rs                                                                  (~200)
```

### 4.6 `paint.rs` (787 → ~5 files)

CPU/ASCII painter. Already roughly bimodal: real paint vs. debug.

```
paint/
  mod.rs
  framebuffer.rs        # FrameBuffer + Display                            (~200)
  painter.rs            # Painter, paint_box dispatch                      (~150)
  elements.rs           # paint_surface/paint_input/paint_image            (~180)
  fill.rs               # box_fill_char, background_fill_char,
                        # border_fill_char, truncate_label                 (~150)
  debug.rs              # BoxInfo, DebugFrame, DebugPainter, debug_box     (~200)
```

### 4.7 `fetch.rs` (757 → ~7 files)

The cleanest natural split — networking already has clear seams.

```
fetch/
  mod.rs
  api.rs                # fetch_html, fetch_string, fetch_bytes            (~80)
  url.rs                # ParsedUrl, Scheme                                (~150)
  resolve.rs            # resolve_relative_url, resolve_relative_file_url,
                        # normalize_path                                   (~150)
  http.rs               # send_request, HttpResponse, read_response_bytes  (~200)
  redirects.rs          # fetch_with_redirects, fetch_bytes_with_redirects,
                        # is_redirect                                      (~120)
  headers.rs            # header_value, find_header_end,
                        # strip_header_separator                           (~50)
  chunked.rs            # decode_chunked_body, find_crlf                   (~80)
  tls.rs                # tls_config                                       (~30)
  capability.rs         # require_file_access                              (~30)
  errors.rs             # FetchError + From impls                          (~80)
  tests.rs                                                                  (~200)
```

**ISP:** `chunked.rs` doesn't need to know what `Identity` is. The current
file makes everything import everything.

### 4.8 `main.rs` (690 → ~5 files)

The binary entry should be tiny. Push everything else into modules.

```
main.rs                 # #[allow(...)], mod decls, fn main()              (~80)
cli/
  mod.rs
  options.rs            # CliOptions + parsing                             (~200)
  env.rs                # env_flag, env_f32                                (~30)
fixtures/
  mod.rs
  resolve.rs            # fixture_url                                      (~50)
  demo.rs               # demo_html() — extract big string to .html file   (~30)
pipeline/
  mod.rs                # the actual run_browser() function                (~150)
  scripts.rs            # extract_scripts                                  (~150)
  images.rs             # collect_image_srcs + ImageCache type             (~80)
```

**Move `demo_html()`'s string body into `fixtures/demo.html`** and load via
`include_str!`. Keeps source file size honest.

### 4.9 `style.rs` (502 → 4 files)

```
style/
  mod.rs
  inherited.rs          # InheritedStyles                                  (~50)
  tree.rs               # StyleTree + Display                              (~150)
  node.rs               # StyledNode + impl                                (~200)
  tests.rs                                                                  (~100)
```

### 4.10 `gpu_paint.rs` (501 → 5 files)

```
gpu_paint/
  mod.rs
  painter.rs            # GpuPainter + dispatch                            (~150)
  element.rs            # paint_element_with_opacity                       (~150)
  text.rs               # paint_text_label, paint_text_with_opacity        (~180)
  image.rs              # paint_image, image_color_cache                   (~120)
  scrollbar.rs          # paint_scrollbar_if_needed + scroll metrics       (~80)
  color.rs              # parse_color                                      (~50)
```

### 4.11 `font.rs` (319 → 4 files)

```
font/
  mod.rs
  resources.rs          # statics: face, ab_font, glyph_atlas              (~80)
  metrics.rs            # get_glyph_metrics, measure_text                  (~50)
  glyph.rs              # RasterGlyph, PositionedGlyph, TextRun            (~80)
  shape.rs              # layout_text_run                                  (~100)
  raster.rs             # rasterize_glyph                                  (~80)
```

Drop `get_glyph` (the legacy 8x8 bitmap stub) if it's truly unused — confirm
with `cargo +nightly rustc -- -W dead_code` before deleting.

### 4.12 `dom.rs` (304 → 3 files)

Small, but the 220-line `impl Node` should split into ops by category.

```
dom/
  mod.rs
  node.rs               # Node enum, ElementNode, ctors                    (~150)
  ops.rs                # impl Node — traversal, mutation                  (~180)
  display.rs            # Display impl                                     (~50)
```

### 4.13 `atlas.rs` (290 → 3 files)

```
atlas/
  mod.rs
  metrics.rs            # GlyphMetrics                                     (~30)
  atlas.rs              # GlyphAtlas + impl                                (~150)
  packer.rs             # AtlasPacker, PackRow                             (~80)
  tests.rs                                                                  (~50)
```

### 4.14 `js.rs`

Delete it. It's a 1-line placeholder. If `js_boa` ever needs to be swapped
behind a trait, introduce `js/engine.rs` with a `trait JsEngine` then —
don't keep dead module declarations around.

---

## 5. Migration order

Don't try to split everything at once. The PR risk gradient:

1. **Start with `atlas` and `dom`** (small, well-bounded, low blast radius).
   Use them to nail down the `mod.rs` template and the test convention.
2. **`fetch`** — natural seams, no callers care about internals.
3. **`html`** and **`style`** — also clean splits.
4. **`paint`** and **`gpu_paint`** — moderate; touches layout types.
5. **`css`** — large but well-bounded.
6. **`layout`** — big, but the impl-block split is mechanical.
7. **`window`** — touches the event loop; do this when the dust has settled
   from the rendering-side splits.
8. **`js_boa`** last. It's the largest, riskiest, and most cross-cutting.
   By the time you get here you'll have validated the approach 13 times.

Each split should be **one PR per top-level module**, ideally landing as:

- Commit 1: pure file-move (no behavior change). `git mv` + edits to make
  it compile. Diff should be 95% imports.
- Commit 2: any structural cleanup the move enabled (extracted traits,
  collapsed duplication, killed `pub` items that no longer need to be).

Reviewers can verify commit 1 by inspection (it's mechanical) and focus
attention on commit 2.

### 5.1 Verification per split

After each module split:

```bash
cargo build           # must pass
cargo test            # must pass — tests are the safety net
cargo clippy -- -D warnings
git diff --stat main  # sanity check size
wc -l src/<module>/*.rs | sort -n   # confirm cap
```

Add a CI check:

```bash
# fail if any src/ file exceeds 200 lines
find src -name '*.rs' -exec wc -l {} + | awk '$1 > 200 && $2 != "total" { print; bad=1 } END { exit bad }'
```

---

## 6. After the split

The decomposition is the start, not the end. Once files are < 200 lines,
the *real* SOLID work becomes visible:

- **Trait extraction.** `Painter` should be a trait with `CpuPainter`,
  `GpuPainter`, `DebugPainter` as impls. The pipeline shouldn't `match`
  on a backend.
- **Event-loop inversion.** `window::app` should not call into `js_boa`
  directly. Define a `BrowserHost` trait the JS bridge consumes; `AuroraApp`
  implements it. That's DIP at the application boundary.
- **Selector engine deduplication.** `css::selector_match` and
  `js_boa::selectors` solve overlapping problems. Once both are < 200 LOC
  and visible side-by-side, the path to merging them becomes obvious.
- **Newtypes for units.** `f32` is used today for px, em, fractions, and
  ratios interchangeably. Introduce `Px`, `Em`, `Fraction` newtypes — the
  compiler will then find every place units were silently mixed.

These are follow-ups, not preconditions. The 200-line cap is the forcing
function that makes them tractable.
