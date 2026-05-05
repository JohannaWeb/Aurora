# Aurora

A from-scratch Rust browser engine with GPU rendering, HTTPS fetch, and an embedded Boa-based JavaScript DOM/BOM runtime bridge.

Aurora is not Servo, Chromium, WebKit, or a wrapper around an existing browser. It is an experimental browser engine written in Rust as part of the broader Bastion sovereign stack.

## Actual Render

![Actual render](https://github.com/user-attachments/assets/647ddace-cbdc-4ed9-9e5b-bf45a2dad9fa)

## Mockup

![Mockup](https://github.com/user-attachments/assets/7c9210f4-d161-4404-946d-36869cecd1f2)

## Rendering Test for Misc Glyphs

![Glyphs](https://github.com/user-attachments/assets/66462680-4dad-4e26-b449-e1a06e2bb200)

## Current Scope

Aurora currently focuses on the core pieces of a browser engine:

* HTML tokenization for a narrow but useful subset
* DOM tree construction
* CSS extraction and parsing from `<style>` tags
* selector matching for tag, `.class`, `#id`, attributes, descendant selectors, and comma groups
* computed style tree generation with basic inheritance
* block-oriented layout tree construction
* text shaping through **rustybuzz**
* GPU painting through **Vello** and **WGPU**
* interactive windowing and scrolling through **winit**
* HTTP and HTTPS fetch support with normal TLS certificate validation
* local `file://` fetch support gated by identity/capability checks
* placeholder rendering for remote images using `<img>` layout and alt text
* embedded **Boa** JavaScript runtime
* live Rust DOM/BOM bridge exposed to JavaScript

Aurora is now more than a static renderer. It has an experimental runtime surface that allows many modern scripts to initialize without immediately crashing, even though large parts of the Web Platform are still partial or stubbed.

## JavaScript Runtime

Aurora embeds **Boa** as its JavaScript engine and exposes a live DOM/BOM bridge.

Each JavaScript node object carries a `__node_id` that points back into a Rust-side `NodeRegistry`. Methods recover the underlying Rust `NodePtr` from the registry on each call, so mutations from the parser, renderer, or JavaScript bridge remain visible through the same JS handle.

The current bridge includes partial support for:

* `document`

  * `body`, `head`, `documentElement`, `title`, `readyState`, `cookie`
  * `createElement`, `createTextNode`, `createDocumentFragment`
  * `getElementById`, `getElementsByTagName`, `getElementsByClassName`
  * `querySelector`, `querySelectorAll`
  * event listener stubs
* `Element` and `Node`

  * `tagName`, `nodeName`, `nodeType`, `id`, `className`
  * `textContent`, `innerHTML`, `innerText`, `outerHTML`
  * `children`, `childNodes`, `firstChild`, `lastChild`
  * `parentNode`, `parentElement`
  * `style`, `classList`, `dataset`, `attributes`
  * `appendChild`, `insertBefore`, `removeChild`, `replaceChild`
  * `cloneNode`, `contains`
  * `setAttribute`, `getAttribute`, `removeAttribute`, `hasAttribute`, `hasAttributes`
  * `querySelector`, `querySelectorAll`
  * `getBoundingClientRect`, `focus`, `blur`, `click`
* `window`

  * `document`, `window`, `self`, `top`, `parent`, `globalThis`
  * viewport fields such as `innerWidth`, `innerHeight`, `devicePixelRatio`, `scrollX`, `scrollY`
  * `setTimeout`, `setInterval`, `requestAnimationFrame`, `requestIdleCallback`
  * `matchMedia`, `getComputedStyle`
  * `localStorage`, `sessionStorage`
  * `location`, `history`, `navigator`, `performance`, `screen`
  * `MutationObserver`, `IntersectionObserver`, `ResizeObserver`
  * `fetch` and `XMLHttpRequest` survival stubs

The bridge currently prioritizes compatibility survival over full browser correctness. Timers, observers, storage, XHR, fetch, and several Web APIs are intentionally partial or stubbed so real-world scripts can initialize without panicking while the engine evolves.

## Rendering Path

Aurora uses a GPU-backed rendering pipeline:

1. **Event Loop**: `winit` manages the window, resizing, and user input.
2. **Scene Construction**: each frame initializes a new `vello::Scene`.
3. **Painting**: `GpuPainter` traverses the layout tree and emits vector commands into the scene.
4. **Text Shaping**: `rustybuzz` converts UTF-8 strings into positioned glyphs, sampled from a pre-baked glyph atlas texture.
5. **Rasterization**: `vello::Renderer` compiles the scene and executes GPU compute work through `wgpu`.
6. **Presentation**: the final texture is presented to the window surface.
7. **Runtime Bridge**: embedded Boa JavaScript can inspect and mutate the Rust DOM through the live DOM/BOM bridge.

## Networking and Fetching

Aurora includes intentionally small but real fetch support:

* `http://`
* `https://`
* `file://`
* basic redirects
* normal HTTPS certificate validation
* capability-gated local file access

Local `file://` fetches are only allowed when the provided identity has `workspace.read`.

## What Aurora Does Not Claim Yet

Aurora is not trying to pass as a general-purpose production browser yet. It does not currently claim:

* full HTML parsing
* broad CSS coverage
* complete layout correctness
* browser-grade JavaScript scheduling semantics
* full DOM, BOM, or Web API compliance
* full web compatibility
* spec compliance across normal browser test suites

If you want to judge it harshly, judge it as an early browser engine and runtime prototype, not as a Chrome replacement.

## Longer-Term Direction

Aurora is part of a larger sovereign computing direction. Longer-term ideas include:

* **DID-Native Identity**: identity resolution built into the browser core.
* **AT Protocol Integration**: native support for decentralized coordination.
* **Sovereign Render Path**: a GPU-accelerated rendering pipeline owned by the user.
* **Capability-Oriented Fetching**: local and remote resources mediated through explicit authority.
* **User-Owned Runtime Surface**: browser APIs shaped around user agency rather than platform capture.

## Run

```bash
cargo run
```

To fetch a page over the network:

```bash
cargo run -- http://example.com/
```

To fetch an HTTPS page:

```bash
cargo run -- https://example.com/
```

To render the bundled static Google homepage fixture:

```bash
cargo run -- --fixture google-homepage
```

To render the bundled demo fixture:

```bash
cargo run -- --fixture demo
```

To save a screenshot from the fixture:

```bash
AURORA_SCREENSHOT=/tmp/google-homepage.png cargo run -- --fixture google-homepage
```

Optional debug dumps:

```bash
cargo run -- --fixture google-homepage --debug-dom --debug-style --debug-layout
```

## Test

```bash
cargo test
```

At the time of this edit, `cargo test` passes in this directory. That matters more than any marketing sentence in this README.

## Docker

Aurora can be built as a Docker image from the parent `projects` directory because it depends on the sibling `Opus` crate:

```bash
cd ..
docker build -f Aurora/Dockerfile -t aurora .
```

From this directory, the same build is available as:

```bash
make docker-build
```

See [docs/DOCKER.md](docs/DOCKER.md) for run examples.

## Next Steps

1. Replace the narrow HTML tokenizer with a real tokenizer state machine.
2. Expand CSS parsing, inheritance, and value handling.
3. Improve layout with inline flow, wrapping, margins, and richer box behavior.
4. Add dynamic glyph atlas growth and multi-font fallback chains.
5. Turn runtime survival stubs into scheduled/evented implementations where it matters.
6. Connect JavaScript-triggered DOM mutation more deeply into style/layout invalidation.
7. Explore protocol-native identity integration once the rendering and runtime core are more stable.

