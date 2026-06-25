# Aurora
<img width="1441" height="1061" alt="image" src="https://github.com/user-attachments/assets/6908436c-f621-4e2a-802e-1238ceb72b12" />
<img width="1080" height="794" alt="image" src="https://github.com/user-attachments/assets/6cc98bcf-0cea-42de-8d1c-707923a3ef52" />

Aurora is an experimental Rust browser-engine prototype. Today it is a crate
and a binary for loading HTML from a string, URL, or fixture; running a partial
V8-backed DOM/BOM bridge; laying out and painting enough of the document to
produce pixels; and showing the result in a native window or writing it to an
image file.

It is not a production browser, a spec-complete engine, or a from-scratch
implementation of every browser subsystem. Aurora integrates the hard browser
parts it needs: V8 for JavaScript, blitz-dom/Stylo for much of DOM/CSS/layout,
blitz-paint plus Vello/WGPU for rendering, and reqwest/rustls for network
fetching. The Aurora-owned work is the integration layer, runner, capability
model, DOM bridge, rendering glue, and the path toward an agent-controlled
browser surface.

## Current Status

- Package: `aurora-engine`; import path: `aurora`.
- Binary: `aurora`.
- Rust: edition 2024, minimum toolchain 1.85.
- JavaScript: V8 only. Earlier SpiderMonkey and Boa experiments were removed;
  unsupported `AURORA_JS_ENGINE` values fall back to V8.
- Rendering: windowed rendering uses winit, WGPU, Vello, blitz-dom, and
  blitz-paint. Headless rendering uses an offscreen renderer and has a legacy
  layout fallback.
- Fetching: `http://`, `https://`, `file://`, and `data:` are supported by the
  internal fetch path. `file://` access is gated by the runner identity's
  `ReadWorkspace` capability.
- Public API: small facade re-exported from the crate root:
  `Browser`, `BrowserBuilder`, `Capabilities`, `Page`, and `Error`.
- Main benchmark: make one real, content-bearing YouTube route bootstrap and
  paint reliably. This is not full YouTube navigation, login, or playback.

The most important technical debt is that Aurora still has both a legacy DOM /
layout path and a Blitz render document. The live renderer is moving toward
Blitz as the authoritative rendering path, but the legacy tree is still used for
tests, screenshots, JS layout accessors, and some input paths.

## Embedding API

```rust
use aurora::{Browser, Capabilities};

let browser = Browser::builder()
    .capabilities(Capabilities::sandboxed())
    .build();

let page = browser.load_html("<h1>Hello, Aurora</h1>");
let png = page.render_png(800, 600).unwrap();
std::fs::write("hello.png", png).unwrap();
```

`Browser::load_html` renders in-memory HTML without fetching. `Browser::load_url`
currently checks the public `network` capability before accepting a URL. The
binary runner has the fuller identity/capability path for network and local
workspace reads.

## Architecture

| Area | Current implementation |
|------|------------------------|
| Runner | CLI parsing, fixture loading, startup pipeline, script fetching |
| Window | `winit` event loop, scroll/input handling, browser chrome |
| JavaScript | V8 runtime in `src/js_v8`, behind the `JsRuntime` trait |
| DOM bridge | Rust-side node registry plus partial DOM/BOM bindings |
| Rendering document | `blitz-dom` plus `blitz-paint` for the primary render path |
| Legacy DOM/layout | Hand-rolled parser, style tree, and layout tree still used by tests and compatibility paths |
| GPU rendering | Vello and WGPU through `anyrender_vello` |
| Networking | Custom fetch module over reqwest/rustls plus local file/data URL support |
| Media | Optional FFmpeg-backed video frames with `media-ffmpeg` |

The JavaScript bridge is intentionally partial. It includes enough document,
element, node, timer, observer, storage, location, fetch, and XHR surface area
for real-world scripts to initialize more often, but many APIs are stubs or
compatibility shims rather than browser-correct implementations.

## What Works Best

- Rendering local fixtures from `fixtures/<name>/index.html`.
- Capturing screenshots for visual regression tests.
- Exercising HTML/CSS/layout/rendering behavior in focused tests.
- Running simple pages and some modern-page bootstrap paths.
- Investigating YouTube hydration and custom-element behavior as a benchmark.

## Known Limits

Aurora does not yet claim:

- full HTML parser correctness;
- broad CSS property and layout coverage;
- browser-grade JavaScript scheduling semantics;
- full DOM, BOM, Web API, storage, cookie, navigation, or media behavior;
- robust security isolation suitable for hostile content;
- full YouTube rendering, navigation, account state, or playback;
- Web Platform Tests compliance.

## Run

```bash
# Bundled demo page
cargo run

# Fetch a URL
cargo run -- https://example.com/

# Run a fixture from fixtures/<name>/index.html
cargo run -- --fixture google-homepage
cargo run -- --fixture aurora-search
cargo run -- --fixture demo

# Local files require explicit workspace-read permission in the runner
cargo run -- --allow-workspace-read file:///absolute/path/to/page.html

# Debug dumps
cargo run -- --fixture google-homepage --debug-dom --debug-style --debug-layout
```

If no display server is available, the runner skips window creation unless a
screenshot path is provided.

## Screenshots

```bash
# Write one render
AURORA_SCREENSHOT=tests/screenshots/google-homepage.png \
  cargo run -- --fixture google-homepage

# Makefile helpers
make screenshot FIXTURE=google-homepage
make mockup-screenshot
make all-renders
```

The screenshot helpers set viewport and output dimensions through environment
variables such as `AURORA_VIEWPORT_WIDTH`, `AURORA_VIEWPORT_HEIGHT`,
`AURORA_SCREENSHOT_WIDTH`, and `AURORA_SCREENSHOT_HEIGHT`.

Generated fixture renders live under `tests/screenshots/`.

## Test

```bash
cargo test

# Visual regression tests
make check-snapshots

# Refresh visual snapshots
make update-snapshots
```

## Docker

```bash
docker build -t aurora .
docker run --rm aurora --fixture google-homepage

mkdir -p /tmp/aurora-renders
docker run --rm \
  -e AURORA_SCREENSHOT=/out/google-homepage.png \
  -v /tmp/aurora-renders:/out \
  aurora --fixture google-homepage
```

The image copies the release binary plus `fixtures/` and `fonts/`.

## Longer-Term Direction

Aurora is being shaped toward a capability-gated, user-owned browser surface.
The longer-term direction includes DID-native identity, AT Protocol integration,
and stronger explicit authority boundaries for local and remote resources.
Those are goals, not completed product claims.

## Roadmap

The near-term work is narrower:

1. Keep the V8 DOM bridge stable enough for large modern bundles.
2. Reduce dual-DOM divergence between the JS-visible tree and the Blitz render
   document.
3. Render one real, content-bearing YouTube route reliably in both windowed and
   screenshot runs.
4. Expand from that route toward broader navigation, media, input, and
   performance work.

## License

Mozilla Public License 2.0, Copyright 2024-2026 Aurora Contributors
