# Aurora Mockup Reflow Implementation Plan

Target mockup: [`docs/mockup.png`](../tests/screenshots/mockup.png)  
Mockup dimensions: `1338 x 786`

Current handoff/status: [`MOCKUP_REFLOW_STATUS.md`](MOCKUP_REFLOW_STATUS.md)

## Parsed Mockup

- [x] Treat the image as a full Aurora product render, not only webpage content.
- [x] Split the scene into two layers:
  - [x] Browser chrome: dark title bar, tabs, address bar, profile/status controls.
  - [x] Page viewport: light search homepage rendered below the chrome.
- [x] Match the chrome height at roughly `158px`.
- [x] Match the content viewport from `y ~= 158px` to the bottom of the window.
- [x] Keep the right scrollbar visible inside the content viewport.
- [x] Preserve the dark top chrome against the light page, with a hard horizontal transition.

## Browser Chrome Targets

- [x] Add browser chrome for the mockup.
- [x] Top title strip:
  - [x] Left brand block: small green/blue square icon, `AURORA`, version `0.3.1`.
  - [x] Center status text: `sovereign render path · session 0x4f:c2`.
  - [x] Right status group: `WGPU · VELLO`, window buttons.
- [x] Tab strip:
  - [x] Use flex row layout.
  - [x] Active tab has green top/accent border and brighter text.
  - [x] Inactive tabs are muted, fixed-width, clipped with ellipsis-like visual behavior where possible.
  - [x] Include compact metrics group on the right: tabs count, memory, gpu.
- [x] Address bar row:
  - [x] Back/forward/reload controls on the left.
  - [x] Large URL pill in the center.
  - [x] TLS badge, URL text, diagnostic pill `dom 412 · style 38 · layout 96`.
  - [x] Identity/account pill on the right.

## Page View Targets

- [x] Replace the existing Google clone text with Aurora-specific mockup copy:
  - [x] Top links: `mail`, `images`, `apps`, `sign in`.
  - [x] Logo: `aurora · search`.
  - [x] Search placeholder: `search the open web or paste a did:`.
  - [x] Buttons: `aurora search`, `i'm feeling sovereign`.
  - [x] Description lines:
    - [x] `static aurora fixture for homepage rendering.`
    - [x] `tokenized 412 nodes · matched 38 css rules · laid out 96 boxes.`
  - [x] Footer: `jurisdiction · sovereign`, then `advertising`, `business`, `how search works`, `privacy`, `terms`, `settings`.
- [x] Tune page vertical geometry:
  - [x] Page top starts immediately after chrome at `y ~= 158px`.
  - [x] Top links sit at about `24px` from page top.
  - [x] Logo baseline block is centered around `y ~= 280px`.
  - [x] Search box sits around `y ~= 370px`, width about `700px`, height about `52px`.
  - [x] Button row sits about `34px` below search box.
  - [x] Footer begins around `y ~= 678px`.

## Current Engine Gaps

- [x] Layout no longer has only a hard-coded `viewport_width = 1200.0` in `src/main.rs`.
- [x] Window resize in `src/window.rs` resizes the surface and rebuilds the style/layout tree.
- [x] Screenshot rendering dimensions can be controlled with `AURORA_SCREENSHOT_WIDTH` / `AURORA_SCREENSHOT_HEIGHT`.
- [x] Layout accepts a `ViewportSize` with width and height.
- [x] `layout.rs` uses height resolution for `height`, `min-height`, and `max-height`.
- [x] Positioning support appears absent; avoid relying on `position: fixed/absolute` for this mockup until implemented.

## Reflow Architecture

- [x] Introduce a `Viewport`/`ViewportSize` value with `width` and `height`.
- [x] Replace `LayoutTree::from_style_tree_with_viewport_width(...)` with a width/height-aware entry point.
- [x] Keep the old width-only constructor as a compatibility wrapper if useful for tests.
- [x] Store enough render input in the window app to rebuild layout:
  - [x] DOM tree after JS execution.
  - [x] Stylesheet.
  - [x] Base URL.
  - [x] Image cache.
- [x] Paint browser chrome outside document layout so every page gets tabs/address/status.
- [x] On `WindowEvent::Resized`, rebuild:
  - [x] `StyleTree::from_dom(...)`
  - [x] `LayoutTree::from_style_tree_with_viewport(...)`
  - [x] image cache.
- [x] Request redraw after reflow.
- [x] Make screenshot dimensions configurable from the same viewport size.

## CSS/Layout Work

- [x] Add viewport-height-aware `height`, `min-height`, and `max-height` resolution.
- [x] Apply `height_resolved(...)` in `clamp_content_height(...)`.
- [x] Add support for `min-height: 100vh` in layout.
- [x] Verify `box-sizing: border-box` still subtracts padding/border correctly.
- [x] Confirm flex row behavior for:
  - [x] `justify-content: flex-end`
  - [x] `justify-content: space-between`
  - [x] `align-items: center`
  - [x] `gap`
- [x] Confirm flex column behavior for centered hero layout.
- [x] Avoid CSS features Aurora does not support yet in the fixture:
  - [x] `position: fixed`
  - [x] `calc(...)`
  - [x] complex pseudo-elements
  - [x] grid
  - [x] media queries for the initial pass

## Fixture Implementation

- [x] Add `fixtures/aurora-search/index.html`.
- [x] Add `fixtures/aurora-search/styles.css`.
- [x] Use semantic regions:
  - [x] `.browser-shell`
  - [x] `.chrome`
  - [x] `.titlebar`
  - [x] `.tabbar`
  - [x] `.omnibar`
  - [x] `.page`
  - [x] `.topbar`
  - [x] `.hero`
  - [x] `.footer`
- [x] Prefer CSS that Aurora already supports:
  - [x] block layout
  - [x] flex layout
  - [x] fixed pixel sizes
  - [x] percentages where already supported
  - [x] margins, padding, border, background, color, font-size
- [x] Keep content within a `1338px` screenshot target first.
- [x] Add a second target around `1238px` width to reproduce the failed actual render and verify reflow.

## Verification

- [x] Add a command or Make target for rendering the fixture screenshot.
- [x] Render the fixture at `1338 x 786`.
- [x] Compare against `docs/mockup.png`.
- [x] Render at `1238 x 939` to match the failing screenshot dimensions.
- [x] Confirm the header/chrome does not wrap vertically.
- [x] Confirm the address bar remains one row.
- [x] Confirm the page hero remains centered.
- [x] Confirm footer remains near the bottom without overlapping content.
- [x] Run existing Rust tests with `cargo test`.
- [x] Add layout unit tests for viewport reflow:
  - [x] Initial layout at one width.
  - [x] Rebuilt layout at narrower width.
  - [x] Flex children participate as block-flow rows when they are `display: flex`.
  - [x] Height resolution handles `vh`.

## Suggested Order

- [x] Phase 1: Build static `fixtures/aurora-search` using only currently supported CSS.
- [x] Phase 2: Add viewport-size plumbing and screenshot size control.
- [x] Phase 3: Implement resize-triggered reflow in the window layer.
- [x] Phase 4: Add height/vh resolution in layout.
- [x] Phase 5: Tune fixture spacing against `docs/mockup.png`.
- [x] Phase 6: Add regression tests and screenshot verification notes.

## Done Criteria

- [x] `docs/mockup.png` is the visual target referenced by docs.
- [x] The Aurora search fixture renders with a stable, single-row chrome header.
- [x] Resizing the window rebuilds layout instead of stretching stale geometry.
- [x] Screenshot output can be generated at the mockup dimensions.
- [x] The old Google fixture still renders acceptably.
- [x] Tests pass.
