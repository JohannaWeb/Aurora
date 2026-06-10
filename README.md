# Aurora

A **from‑scratch Rust browser engine** with GPU‑accelerated rendering, HTTPS fetching, and an embedded JavaScript runtime.

Aurora is not Servo, Chromium, WebKit, or a wrapper around an existing browser. It is an experimental browser engine written in Rust as part of the broader Bastion sovereign stack.

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
| **JavaScript** | SpiderMonkey (`js_sm`) | Primary JS engine with live DOM/BOM bridge |
| **JavaScript** | Boa (`js_boa`, `engine-boa` feature) | Alternative engine, used for specific tests |
| **Networking** | Aurora fetch | `http://`, `https://`, and capability‑gated `file://` |

## JavaScript Runtime

Aurora embeds **SpiderMonkey** as its default JavaScript engine and exposes a live DOM/BOM bridge.

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
6. **JS Bridge** — SpiderMonkey can inspect and mutate the DOM through the live DOM/BOM bridge.

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

1. Render YouTube homepage shell
2. Search results page loads without JS fatal errors
3. Video page renders title, player box, sidebar
4. Basic playback path works
5. Controls/input events work
6. Enough scheduling/timers/fetch/XHR survival for YouTube scripts
7. Performance pass

## License

Mozilla Public License 2.0 © 2024‑2026 Aurora Contributors
