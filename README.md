# Aurora

A **from‑scratch Rust browser engine** with GPU‑accelerated rendering, HTTPS fetching, and an embedded JavaScript runtime.

Aurora is not Servo, Chromium, WebKit, or a wrapper around an existing browser. It is an experimental browser engine written in Rust as part of the broader Bastion sovereign stack.

## What the crate does

`aurora-engine` (imported as `aurora`) takes HTML/CSS — and runs the JavaScript on the page — lays it out, and renders it to pixels on the GPU. You can use it headlessly to turn a document into a PNG, or as the engine behind a windowed browser.

Its headline differentiator is **capability gating**: you grant a `Browser` instance only the powers a page should have (currently network egress and workspace read), so untrusted content can be run fully sandboxed.

The public embedding API is a small facade re-exported from the crate root — `Browser`, `BrowserBuilder`, `Capabilities`, `Page`, `Error` — while the engine internals (DOM, CSS, layout, style, rendering, and the JS engines) stay private.

```rust
use aurora::{Browser, Capabilities};

// A browser that cannot reach the network.
let browser = Browser::builder()
    .capabilities(Capabilities::sandboxed())
    .build();

let page = browser.load_html("<h1>Hello, Aurora</h1>");
let png = page.render_png(800, 600).unwrap();
std::fs::write("hello.png", png).unwrap();
```

Under the hood: GPU rasterisation via **Vello + wgpu**, DOM/layout via the **blitz** crates and **taffy**, CSS via **stylo/cssparser/selectors**, text via **parley/rustybuzz**, HTTPS fetching via **reqwest/rustls**, and an embedded JavaScript runtime. Aurora currently ships with **V8** as the authoritative JS backend.

## Architecture

Aurora has consolidated around a single authoritative DOM and layout path driven by the **blitz-dom** and **stylo** crates. The legacy dual-path layout has been deprecated in favor of a unified engine that provides both visual rendering and layout metrics for JavaScript.

The browser follows a canonical event-loop model where JS execution, style recomputation, layout, and painting are driven through explicit, ordered phases. This allows for reliable handling of modern SPA frameworks like Polymer while maintaining a testable execution model.

## YouTube Benchmark

Aurora targets YouTube as a hostile, modern-web integration benchmark. The current milestone is not full YouTube rendering or playback; it is proving that Aurora can bootstrap enough YouTube application data, DOM mutation, custom elements, style, and layout to render one real content-bearing route reliably.

The benchmark is intentionally narrow. A reliable route with real content is the gate before broader YouTube navigation, media playback, account state, and performance work.

## Rendering Test

<img width="1437" height="1066" alt="image" src="https://github.com/user-attachments/assets/5d23b17a-3cd4-4aa8-84f9-711e49e8ad69" />
<img width="1440" height="1059" alt="image" src="https://github.com/user-attachments/assets/caadc2aa-a3d4-4db6-a321-16451d79a404" />


## Mockup

![Mockup](https://github.com/user-attachments/assets/7c9210f4-d161-4404-946d-36869cecd1f2)

## Architecture

Aurora is built on a layered stack of focused crates:

| Layer | Crate | Role |
|-------|-------|------|
| **DOM** | `blitz-dom` | Document model, HTML parsing, CSS styling, layout |
| **Painting** | `blitz-paint` | Traverses the layout tree and emits draw commands |
| **Rendering** | `anyrender_vello` | GPU rasterisation via Vello + WGPU |
| **Windowing** | `winit` | Window, input events, resize |
| **Text** | `parley` | Text layout and shaping |
| **JavaScript** | V8 (`js_v8`, default) | Default JS engine with live DOM/BOM bridge |
| **JavaScript** | SpiderMonkey (`js_sm`, `engine-sm` feature) | Alternative engine with live DOM/BOM bridge |
| **JavaScript** | Boa (`js_boa`, `engine-boa` feature) | Alternative engine, used for specific tests |
| **Networking** | Aurora fetch | `http://`, `https://`, and capability‑gated `file://` |

## JavaScript Runtime

Aurora embeds **V8** as its default JavaScript engine (SpiderMonkey and Boa are selectable alternatives) and exposes a live DOM/BOM bridge.

Each JavaScript node object carries a `__node_id` that points back into a Rust-side `NodeRegistry`. Methods recover the underlying Rust `NodePtr` from the registry on each call, so mutations from the parser, renderer, or JavaScript bridge remain visible through the same JS handle.

The bridge includes partial support for:

* `document` — `body`, `head`, `documentElement`, `title`, `readyState`, `cookie`, `createElement`, `createTextNode`, `createDocumentFragment`, `getElementById`, `getElementsByTagName`, `getElementsByClassName`, `querySelector`, `querySelectorAll`, event listener stubs
* `Element` / `Node` — `tagName`, `nodeName`, `nodeType`, `id`, `className`, `textContent`, `innerHTML`, `innerText`, `outerHTML`, `children`, `childNodes`, `firstChild`, `lastChild`, `parentNode`, `parentElement`, `style`, `classList`, `dataset`, `attributes`, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `cloneNode`, `contains`, `setAttribute`, `getAttribute`, `removeAttribute`, `hasAttribute`, `querySelector`, `querySelectorAll`, `getBoundingClientRect`, `focus`, `blur`, `click`
* `window` — `document`, `window`, `self`, `top`, `parent`, `globalThis`, viewport fields (`innerWidth`, `innerHeight`, `devicePixelRatio`, `scrollX`, `scrollY`), `setTimeout`, `setInterval`, `requestAnimationFrame`, `requestIdleCallback`, `matchMedia`, `getComputedStyle`, `localStorage`, `sessionStorage`, `location`, `history`, `navigator`, `performance`, `screen`, `MutationObserver`, `IntersectionObserver`, `ResizeObserver`, `fetch`, `XMLHttpRequest` survival stubs

The bridge prioritises compatibility survival over full correctness — timers, observers, storage, XHR, and fetch are intentionally partial so real-world scripts can initialise without panicking while the engine evolves.

## Rendering Pipeline

1. **Event Loop** — `winit` manages the window, resizing, and user input.
2. **Document** — `blitz-dom` parses HTML, resolves CSS, and runs layout.
3. **Painting** — `blitz-paint` traverses the layout tree and emits vector commands to a `VelloScenePainter`.
4. **Rasterisation** — `anyrender_vello` compiles the scene and executes GPU compute work through Vello + WGPU.
5. **Presentation** — the final texture is presented to the window surface.
6. **JS Bridge** — the embedded JS engine (V8 by default) can inspect and mutate the DOM through the live DOM/BOM bridge.

## Networking

* `http://` and `https://` with TLS certificate validation
* `file://` gated by `workspace.read` capability
* Basic redirect following
* In-process net cache to avoid redundant fetches

## What Aurora Does Not Claim Yet

Aurora is an early engine prototype, not a production browser. It does not yet claim:

* Full HTML parsing or broad CSS coverage
* Complete layout correctness
* Browser-grade JavaScript scheduling semantics
* Full DOM, BOM, or Web API compliance
* Full YouTube rendering, navigation, or playback
* Spec compliance across standard browser test suites

## Run

```bash
# Default startup
cargo run

# Fetch a page
cargo run -- https://example.com/

# Use a bundled fixture
cargo run -- --fixture aurora-search
cargo run -- --fixture google-homepage
cargo run -- --fixture demo

# Optional FFmpeg video support (requires FFmpeg dev packages)
cargo run --features media-ffmpeg -- file:///path/to/page.html

# Debug dumps
cargo run -- --fixture google-homepage --debug-dom --debug-style --debug-layout
```

## Screenshots

```bash
make screenshot FIXTURE=google-homepage
make mockup-screenshot
make all-renders
```

Generated renders are saved to `tests/screenshots/`.

## Test

```bash
cargo test
```

## Docker

```bash
docker build -t aurora .
# or
make docker-build
```

See [docs/DOCKER.md](docs/DOCKER.md) for run examples.

## Longer‑Term Direction

* **DID‑Native Identity** — identity resolution built into the browser core.
* **AT Protocol Integration** — native support for decentralised coordination.
* **Sovereign Render Path** — a GPU‑accelerated pipeline owned by the user.
* **Capability‑Oriented Fetching** — local and remote resources mediated through explicit authority.
* **User‑Owned Runtime Surface** — browser APIs shaped around user agency rather than platform capture.
* **AI‑Native** — a browser built from the ground up to be a first-class environment for AI agents and AI-assisted browsing, not an afterthought bolted onto a legacy web platform.

## Roadmap

1. Bootstrap YouTube without fatal JavaScript, custom-element, or DOM-mutation failures
2. Extract enough real YouTube application data for a route that contains actual content
3. Instantiate the route's content-bearing component tree through Aurora's DOM/custom-element path
4. Apply enough scoped style and layout for that route to paint stable, inspectable content
5. Make the route reliable in repeated windowed and screenshot runs
6. Expand from that proven route toward broader YouTube navigation, media, input, and performance work

## License

Mozilla Public License 2.0 © 2024‑2026 Aurora Contributors
