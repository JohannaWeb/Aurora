# Aurora – Architecture Documentation (arc42)

> **arc42** is a template for communicating and documenting software architectures.
> This document follows the arc42 structure adapted for Aurora.

---

## 1. Introduction and Goals

### 1.1 Requirements Overview

Aurora is an experimental, GPU-accelerated browser engine written in Rust. Its primary near-term benchmark is YouTube: a hostile, modern-web application that stresses application-data bootstrap, DOM mutation, custom elements, scoped style, and layout. The first milestone is not full YouTube rendering or playback; it is rendering one real content-bearing YouTube route reliably. Aurora's longer-term goal is to serve as a sovereign, AI-native, capability-gated web client as part of the Bastion stack.

### 1.2 Quality Goals

| Priority | Quality Goal | Motivation |
|----------|-------------|------------|
| 1 | **Correctness of hostile modern-web bootstrap** | YouTube requires a capable JS engine, DOM mutation path, custom elements, style, and layout working together |
| 2 | **GPU rendering performance** | Smooth, GPU-first rendering via Vello + WGPU |
| 3 | **Security via capability model** | Resources mediated through explicit `Identity` + `Capability` checks |
| 4 | **AI-native design** | First-class environment for AI agents, not a bolted-on afterthought |
| 5 | **Sovereignty** | Rust-native integration of best-in-class engines (V8, Stylo via blitz-dom, Vello) under one capability-gated, agent-controllable surface — not an opaque, telemetry-laden browser shell. Aurora is *not* an independent re-implementation of those engines. |

### 1.3 Stakeholders

| Role | Expectation |
|------|------------|
| Developer (Johanna) | Iterate quickly toward the YouTube benchmark route; maintain a clean, understandable Rust codebase |
| AI Agents | A programmable browser surface with capability-gated APIs |
| Future contributors | Clear module boundaries, documented trade-offs |

---

## 2. Architecture Constraints

- Written entirely in **Rust** (stable toolchain, 2024 edition).
- Must compile and run on Linux (primary target); macOS optional.
- Aurora integrates, rather than re-implements, the hard parts of a browser: JavaScript runs on **V8** (Chromium's engine, linked as a prebuilt library); DOM/CSS/layout come from **blitz-dom**, which uses **Stylo** (Firefox/Servo's style engine); painting is **Vello/anyrender** (Linebender). The Aurora-authored code is the integration layer, capability model, DOM bridge, and agent surface — not the engines themselves.
- JavaScript must be handled by an engine with a real JIT for YouTube-scale bundled JavaScript (hence V8).
- GPU rendering is non-negotiable — no software rasteriser fallback.

---

## 3. System Scope and Context

### 3.1 Business Context

```
┌──────────────────────────────────────────────────────────┐
│                        User / AI Agent                   │
│                (Human DID or Agent identity)             │
└────────────────────────┬─────────────────────────────────┘
                         │ CLI args / URL / fixture
                         ▼
                 ┌───────────────┐
                 │  Aurora       │
                 │  Browser      │
                 └──────┬────────┘
                        │
          ┌─────────────┼──────────────┐
          ▼             ▼              ▼
   ┌─────────────┐  ┌──────┐  ┌──────────────┐
   │ HTTP/HTTPS  │  │ file │  │  Fixture HTML │
   │   servers   │  │  FS  │  │  (bundled)   │
   └─────────────┘  └──────┘  └──────────────┘
```

### 3.2 Technical Context

Aurora fetches HTML from the network or local filesystem, parses it, runs JavaScript, lays it out using blitz-dom, paints it with blitz-paint via anyrender_vello, and presents frames via WGPU to a winit window.

---

## 4. Solution Strategy

| Concern | Decision | Rationale |
|---------|----------|-----------|
| DOM & layout | **blitz-dom** | Avoids hand-rolling layout; uses Stylo (Firefox CSS engine) under the hood |
| Painting | **blitz-paint** + **anyrender_vello** | GPU-first vector rendering via Vello |
| Windowing | **winit** | Cross-platform, async-friendly event loop |
| Text | **parley** | Full text layout with shaping |
| JavaScript | **V8** (`v8` crate, default feature) | The only JS engine. JIT for YouTube-scale bundled JavaScript and the live DOM/BOM bridge. (Earlier SpiderMonkey and Boa experiments were removed; see `js_engine::JsRuntime`.) |
| Networking | Custom fetch module | Capability-gated; supports `http`, `https`, `file://`, data URLs |
| Identity | `Identity` + `Capability` structs | Fine-grained capability model for sovereign browsing |

---

## 5. Building Block View

### 5.1 Level 1 – Top-level Modules

```
┌─────────────────────────────────────────────────────────────────┐
│                          aurora (crate)                         │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │
│  │  runner  │  │  window  │  │  fetch   │  │  identity    │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘   │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │
│  │  js_v8   │  │js_engine │  │  dom     │  │blitz_document│   │
│  │ (only)   │  │ (trait)  │  │(legacy)  │  │              │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘   │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │
│  │  html    │  │  css     │  │  style   │  │  layout      │   │
│  │ (parser) │  │ (parser) │  │  (tree)  │  │  (tree)      │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘   │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                      │
│  │  render  │  │  media   │  │  atlas   │                      │
│  └──────────┘  └──────────┘  └──────────┘                      │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 Level 2 – Module Responsibilities

| Module | Responsibility |
|--------|---------------|
| `runner` | CLI parsing, startup pipeline, script fetching, fixture loading |
| `window` | winit event loop, frame rendering, scroll, input, browser chrome |
| `blitz_document` | `BlitzDocument` wrapper — owns `blitz-dom` instance, paints via `blitz-paint` + `anyrender_vello` |
| `fetch` | Capability-gated HTTP/HTTPS/file/data-URL fetching with TLS |
| `identity` | `Identity` and `Capability` types for capability-gated access |
| `js_v8` | V8 runtime, DOM/BOM bridge, `NodeRegistry`, custom-element and mutation integration — the only JS engine |
| `js_engine` | `JsRuntime` trait + `create_runtime` factory. Abstracts the engine behind a trait (originally to host SpiderMonkey/Boa alternates, now V8-only) |
| `dom` | Legacy hand-rolled DOM tree (`NodePtr`, `NodeRegistry`) — source of truth for JS and legacy tests |
| `html` | Hand-rolled HTML tokeniser/parser feeding the legacy DOM |
| `css` | CSS extraction and parsing from `<style>` tags |
| `style` | Computed style tree built from legacy DOM + Stylesheet |
| `layout` | Legacy block layout tree (used for JS accessors, hit testing, screenshots) |
| `render` | Legacy GPU painter over the legacy layout tree |
| `media` | Optional FFmpeg-backed video frame playback (`media-ffmpeg` feature) |
| `atlas` | Glyph atlas texture for legacy text rendering |
| `font` | Font metrics helpers |

---

## 6. Runtime View

### 6.1 Startup Sequence

```
main()
  │
  ├─ install_crypto_provider()          (rustls / ring)
  │
  ├─ CliOptions::from_env()
  ├─ default_identity()                 (DID + capabilities)
  │
  └─ runner::run_browser()
       │
       ├─ fetch::fetch_html()           (http / https / file / fixture)
       │
       ├─ html::Parser::parse_document()  → legacy NodePtr DOM
       │
       ├─ run_scripts()
       │    ├─ extract_scripts()
       │    ├─ fetch all external scripts in parallel (threads)
       │    ├─ V8 runtime setup with live DOM/BOM bridge
       │    └─ runtime.execute(script) for each script
       │
       ├─ Stylesheet::from_dom()  +  merge UA stylesheet
       ├─ StyleTree::from_dom()
       ├─ LayoutTree::from_style_tree_with_viewport()
       ├─ load_images() / load_svgs() / MediaCache::load()
       │
       ├─ BlitzDocument::from_html()    (blitz-dom parse + resolve)
       │
       └─ window::open()               → winit event loop
```

### 6.2 Frame Render Loop

```
winit: RedrawRequested
  │
  ├─ run_frame_tasks()
  │    ├─ runtime.tick()                    (fire due setTimeout/setInterval)
  │    ├─ runtime.drain_animation_frame_callbacks()  (requestAnimationFrame)
  │    ├─ runtime.take_needs_reflow()
  │    └─ media.update()                    (decode next video frame)
  │
  ├─ [if needs_reflow] perform_sync_reflow()
  │    ├─ legacy LayoutTree reflow
  │    └─ BlitzDocument::resolve()
  │
  └─ render()
       ├─ Scene::new()                      (vello scene)
       ├─ paint_content_layer()
       │    └─ BlitzDocument::paint_to_scene()
       │         ├─ inner.resolve()          (blitz-dom re-resolve)
       │         ├─ VelloScenePainter::new(scene)
       │         └─ blitz_paint::paint_scene()
       ├─ paint_browser_chrome_scene()      (URL bar, chrome)
       └─ vello Renderer::render_to_texture()  → WGPU present
```

### 6.3 Click / Event Dispatch

```
WindowEvent::MouseInput (left press)
  │
  ├─ BlitzDocument::hit_test_anchor()    → navigation (uses blitz-dom coords)
  │    └─ [if href found] navigate_to() → reload pipeline
  │
  └─ legacy LayoutTree::hit_test()      → JS event dispatch
       └─ JS runtime dispatch_event(node, "click")
```

---

## 7. Deployment View

```
┌──────────────────────────────────────────────┐
│  Host OS (Linux, X11 / Wayland)              │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │  Aurora process                        │  │
│  │                                        │  │
│  │  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │ winit    │  │ WGPU / GPU       │   │  │
│  │  │ event    │  │ render context   │   │  │
│  │  │ loop     │  └──────────────────┘   │  │
│  │  └──────────┘                         │  │
│  │                                        │  │
│  │  ┌──────────────────────────────────┐  │  │
│  │  │ V8 JavaScript runtime            │  │  │
│  │  └──────────────────────────────────┘  │  │
│  │                                        │  │
│  │  ┌────────────┐  ┌─────────────────┐  │  │
│  │  │ reqwest /  │  │ blitz-dom /     │  │  │
│  │  │ rustls     │  │ blitz-paint     │  │  │
│  │  └────────────┘  └─────────────────┘  │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  GPU (Vulkan / Metal / DX12 via WGPU)        │
└──────────────────────────────────────────────┘
```

Also distributable as a **Docker** image (headless / screenshot mode via `AURORA_SCREENSHOT`).

---

## 8. Cross-cutting Concepts

### 8.1 Identity and Capability Model

Every resource access is mediated through an `Identity`:

```rust
struct Identity {
    did: String,           // e.g. "did:human:johanna"
    name: String,
    kind: IdentityKind,    // Human | Agent
    default_capabilities: Vec<Capability>,
}

enum Capability {
    NetworkAccess,   // required for http:// / https://
    ReadWorkspace,   // required for file://
}
```

Capability checks happen at the fetch boundary. JS-initiated fetch inherits the session identity.

### 8.2 Dual DOM (Current Trade-off)

Aurora currently maintains two parallel document representations:

| | Legacy DOM (`dom::NodePtr`) | Blitz DOM (`blitz-dom`) |
|-|----------------------------|-------------------------|
| **Source of truth for** | JavaScript bridge, legacy tests, hit testing for JS events | Painting, navigation hit testing |
| **Populated by** | Hand-rolled `html::Parser` | `blitz-html::HtmlDocument` |
| **Layout engine** | Hand-rolled `layout::LayoutTree` | Stylo (via blitz-dom) |
| **Mutation by JS** | Direct mutation via the JS bridge | Incremental sync where supported; explicit snapshot rebuild on failed sync |

> **Known issue**: Aurora still maintains a legacy DOM and a Blitz DOM. Incremental sync handles many JS mutations, but failed or unsupported sync paths still fall back to full snapshot rebuilds. Collapsing toward one authoritative rendering DOM remains the highest-priority technical debt.

### 8.3 JavaScript Engine Strategy

```
JsRuntime trait
     │
     └─── V8 runtime                ← the only implementation; JIT-backed, YouTube benchmark target
```

The `JsRuntime` trait keeps the rest of the codebase engine-agnostic. It was introduced to host SpiderMonkey and Boa alternates; those were removed (V8 won — see ADR-002), but the trait boundary remains so a future engine swap stays localized.

### 8.4 Script Loading

External scripts are fetched in **parallel** (one thread per script) but **executed in order** to preserve script ordering semantics. A per-script size limit (16 MB) and total limit (32 MB) protect against runaway bundles.

### 8.5 Networking

```
fetch::fetch_bytes(url, identity)
  │
  ├─ "http://" / "https://"  → reqwest (blocking, rustls TLS)
  ├─ "file://"               → std::fs (requires Capability::ReadWorkspace)
  ├─ "data:"                 → inline decode
  └─ else                    → FetchError::UnsupportedScheme
```

An in-process `NET_CACHE` (BTreeMap behind a Mutex) deduplicates repeated fetches within a session.

---

## 9. Architecture Decisions

### ADR-001: Use blitz-dom instead of hand-rolling layout
- **Status**: Adopted
- **Context**: Full layout (inline flow, Flexbox, Grid, Stylo CSS) is enormous scope.
- **Decision**: Use `blitz-dom` which embeds Stylo and handles layout.
- **Consequences**: Faster path to correct rendering; introduces dual-DOM complexity while legacy DOM coexists.

### ADR-002: JIT JavaScript engine for the YouTube benchmark
- **Status**: Adopted
- **Context**: Boa (pure Rust) lacks a JIT and cannot run YouTube or modern bundled JS. SpiderMonkey was prototyped but could not be statically linked alongside V8 (duplicate-symbol conflict) and was abandoned.
- **Decision**: Use V8 as the sole runtime. Keep the `JsRuntime` trait boundary so an alternate engine could be reintroduced behind it, but ship only V8.
- **Consequences**: YouTube-scale JavaScript is tractable; Aurora depends on a prebuilt V8 binary, and "no-Chromium" is explicitly *not* claimed (see Quality Goal 5).

### ADR-003: Capability-gated fetch
- **Status**: Adopted
- **Context**: A sovereign browser should not silently access local files or arbitrary network resources.
- **Decision**: All fetch calls pass an `Identity`; `file://` requires `Capability::ReadWorkspace`.
- **Consequences**: Explicit authority model; groundwork for AI-agent-safe browsing.

### ADR-004: GPU-first rendering (Vello + WGPU)
- **Status**: Adopted
- **Context**: Software rasterisers are a dead end for a modern browser.
- **Decision**: Use Vello (vector compute rasteriser) over WGPU; no CPU fallback.
- **Consequences**: High performance; requires a capable GPU; narrows supported deployment targets slightly.

---

## 10. Quality Requirements

| Requirement | Measure |
|-------------|---------|
| YouTube benchmark route paints real content | Aurora bootstraps enough application data, DOM mutation, custom elements, style, and layout to render one content-bearing route reliably |
| Real TLS validation | `reqwest` with `rustls` and default cert store; no `danger_accept_invalid_certs` |
| Capability enforcement | `file://` fetch rejected if `ReadWorkspace` not in identity |
| Test suite passes | `cargo test` green |
| Headless / CI mode | `AURORA_HEADLESS=1` or `AURORA_SCREENSHOT=path` skips display requirement |

---

## 11. Risks and Technical Debt

| Risk / Debt | Severity | Notes |
|-------------|----------|-------|
| Dual DOM snapshot rebuild fallback | **High** | Unsupported or failed incremental sync paths still rebuild the render snapshot and can diverge from JS-visible DOM behavior |
| No full HTML parser | **Medium** | Hand-rolled tokeniser misses many real-world constructs |
| Partial CSS coverage | **Medium** | Many properties and values not yet handled |
| 112+ `.unwrap()` calls in network/parse paths | **Medium** | Each is a potential remote DoS / crash |
| Legacy `LayoutTree` used for JS event hit testing | **Low-Medium** | Can diverge from blitz-dom painted positions |
| `stylo_bridge/` directory (dead code) | **Low** | Not compiled; confuses searches |
