# Aurora

<img width="1238" height="939" alt="image" src="https://github.com/user-attachments/assets/647ddace-cbdc-4ed9-9e5b-bf45a2dad9fa" />


Aurora is a from-scratch Rust browser-engine experiment for the Bastion project.

It is not a complete browser. The current codebase implements a narrow rendering slice because I wanted to explore layout, painting, and client-surface ideas without leaning on Chromium or a WebView.

## Current Scope

Longer-term ideas include:

- **DID-Native Identity**: Identity resolution built into the browser's core.
- **AT Protocol Integration**: Native support for decentralized coordination.
- **Sovereign Render Path**: A GPU-accelerated rendering pipeline owned by the user.

What the current code actually does today:

- tokenize a narrow HTML subset
- build a simple DOM tree
- extract and parse a tiny CSS subset from `<style>` tags
- match tag, `.class`, `#id`, and descendant rules into computed styles
- build a style tree with basic color inheritance
- derive a block-oriented layout tree
- shape text via **rustybuzz** (HarfBuzz port)
- paint the result via **Vello** (WGPU-accelerated vector graphics)
- interactive window with scrolling support

## What It Does Not Do

Aurora is not trying to pass as a general-purpose browser yet. In particular, it does not currently claim:

- full HTML parsing
- broad CSS coverage
- browser-grade JavaScript/runtime behavior
- web compatibility
- spec compliance across normal browser test suites

If you want to judge it harshly, judge it as a rendering/layout prototype, not as a Chrome replacement.

## Rendering Path

Aurora currently uses a GPU-backed rendering path:

1.  **Event Loop**: `winit` manages the window and user input (scrolling, resizing).
2.  **Scene Construction**: On every frame, a new `vello::Scene` is initialized.
3.  **Painting**: `GpuPainter` traverses the **Layout Tree**, emitting vector commands (rects, text runs) into the scene.
4.  **Text Shaping**: `rustybuzz` converts UTF-8 strings into positioned glyphs, which are then sampled from a pre-baked **Glyph Atlas** texture.
5.  **Rasterization**: The `vello::Renderer` compiles the scene and executes compute shaders on the GPU via `wgpu` to produce the final pixels.
6.  **Presentation**: The resulting texture is blitted to the window's surface for display.

## Run

```bash
cargo run
```

To fetch a page over the network:

```bash
cargo run -- http://example.com/
```

To render the bundled static Google homepage fixture:

```bash
cargo run -- --fixture google-homepage
```

To save a screenshot from the fixture:

```bash
AURORA_SCREENSHOT=/tmp/google-homepage.png cargo run -- --fixture google-homepage
```

Optional debug dumps:

```bash
cargo run -- --fixture google-homepage --debug-dom --debug-style --debug-layout
```

Fetch support is intentionally small:

- `http://` and `https://`
- `file://`
- basic redirects
- remote images render as placeholders using `<img>` layout and alt text

HTTPS now uses normal certificate validation. Local `file://` fetches are only allowed when the provided identity has `workspace.read`.

## Test

```bash
cargo test
```

At the time of this edit, `cargo test` passes in this directory. That matters more than any marketing sentence in the README.

## Docker

Aurora can be built as a Docker image from the parent `projects` directory because
it depends on the sibling `Opus` crate:

```bash
cd ..
docker build -f Aurora/Dockerfile -t aurora .
```

From this directory, the same build is available as:

```bash
make docker-build
```

See [DOCKER.md](DOCKER.md) for run examples.

## Next Steps

1. Add a real tokenizer state machine.
2. Add more inherited properties and better CSS value handling.
3. Improve layout with inline flow, wrapping, and margins.
4. Support dynamic glyph atlas growth and multi-font chains.
5. Explore protocol-native identity integration once the rendering core is more stable.
