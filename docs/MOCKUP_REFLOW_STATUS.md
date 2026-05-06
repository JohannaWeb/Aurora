# Aurora Mockup Reflow Status

Target mockup: [`mockup.png`](../tests/screenshots/mockup.png)  
Implementation plan: [`MOCKUP_REFLOW_IMPLEMENTATION_PLAN.md`](MOCKUP_REFLOW_IMPLEMENTATION_PLAN.md)

## Current Status

Aurora now renders browser chrome in the app/window layer, not as page markup.
That chrome includes the title strip, tab strip, address row, status metrics,
and identity control. The `fixtures/aurora-search` fixture is now only the page
content that renders below the browser chrome.

The original header failure was caused by a mix of stale viewport assumptions,
missing resize reflow, flex-row layout edge cases, and text fragments wrapping
inside controls that should stay single-line. Browser chrome is now painted for
all rendered pages, including remote URLs such as `https://example.com`.

## Render Commands

Render the mockup target size:

```bash
make mockup-screenshot
```

Render a custom fixture screenshot:

```bash
make screenshot FIXTURE=aurora-search SCREENSHOT=/tmp/aurora-search-render.png VIEWPORT_WIDTH=1338 VIEWPORT_HEIGHT=786
```

Render the narrower failure-size viewport:

```bash
make screenshot FIXTURE=aurora-search SCREENSHOT=/tmp/aurora-search-1238x939.png VIEWPORT_WIDTH=1238 VIEWPORT_HEIGHT=939
```

Run the example.com smoke test without opening a window:

```bash
AURORA_HEADLESS=1 AURORA_VIEWPORT_WIDTH=1338 AURORA_VIEWPORT_HEIGHT=786 cargo run -- https://example.com
```

## Verification

Last verified locally:

```bash
cargo test
rustfmt --check src/css.rs src/style.rs src/layout.rs src/window.rs src/gpu_paint.rs
rustfmt --check --config skip_children=true src/main.rs
make mockup-screenshot
```

The old Google fixture was also rendered successfully with the same screenshot
path, which checks that the new viewport and whitespace behavior did not break
the existing static homepage fixture.

## Implemented

- `LayoutTree` accepts `ViewportSize { width, height }`.
- `main.rs` reads viewport dimensions from `AURORA_VIEWPORT_WIDTH` and `AURORA_VIEWPORT_HEIGHT`.
- Screenshot output reads `AURORA_SCREENSHOT_WIDTH` and `AURORA_SCREENSHOT_HEIGHT`.
- Browser chrome is painted by `window.rs` in screenshot mode and by the GPU scene path in interactive mode.
- Window resize rebuilds style, layout, and image cache before redraw.
- Page layout uses the content viewport below the built-in browser chrome.
- Layout resolves `height`, `min-height`, and `max-height` with viewport-height units.
- Flex children that are themselves `display: flex` participate as block-flow rows.
- Percentage-width flex children keep their explicit width during flex measurement.
- Text wrapping no longer double-counts current line width.
- `white-space: nowrap` is inherited and honored by inline text fragments.
- The Aurora fixture uses supported CSS only: block, flex, fixed sizes, percentages, margins, padding, borders, background, color, and font size.

## Scrollbar

The content viewport scrollbar is style-driven by `overflow-y: scroll` on the
Aurora page box. It is painted as an overlay in both screenshot mode and the GPU
window path so visual verification is deterministic.

## Known Visual Differences

Aurora still uses its current text shaping and paint stack, so the result is not
pixel-identical to the mockup. The important reflow behavior is stable: the chrome
stays horizontal, the address bar remains one row, the hero stays centered, and
the footer does not overlap page content.
