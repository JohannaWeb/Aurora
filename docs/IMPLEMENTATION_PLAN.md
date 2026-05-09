# Aurora — Implementation Plan

> Anchor: every phase below maps to a specific recommendation Nico Burns (maintainer of [Blitz](https://github.com/DioxusLabs/blitz) and [Taffy](https://github.com/DioxusLabs/taffy)) gave directly. The order is ordered by dependency and by what unblocks correctness gains for the engine fastest.
>
> Companion document: [PRINCIPAL_REVIEW.md](PRINCIPAL_REVIEW.md). Each P0/P1 finding in that review maps to one or more phases in this plan.
>
> Philosophy (Nico's, adopted): _"If there's a crate that implements a subsystem in a way that can be used standalone then we'll use it. But we'll also extend it / contribute to it / treat it like part of our engine rather than treating it like a black box. If there's not then we'll build our own."_

---
F
## Phase 0 — Foundations and reference reading

Before changing any code, internalise the prior art Nico pointed at. The whole plan below lifts the same crates Blitz uses; reading their codebases is the fastest path to "doing it the way it's done."

### Reference codebases — read top-to-bottom before touching anything

- [ ] Read [Blitz](https://github.com/DioxusLabs/blitz) — first-principles browser engine, no JS, same scope as Aurora. Pay particular attention to:
  - [ ] `blitz-dom` — DOM representation
  - [ ] `blitz-html` — html5ever integration
  - [ ] `blitz-paint` — DOM → drawing commands
  - [ ] `blitz-net` — networking (~300 LoC, Nico's reference for "you don't need much on top of reqwest")
  - [ ] Blitz's inline.rs (Taffy ↔ Parley glue)
- [ ] Read [Takumi](https://github.com/kane50613/takumi) — JSX → image renderer using the same Blitz/Taffy/Parley stack but simpler (no incremental updates, no real-time). This is the smallest reference for "how the pieces fit when you don't need a reflow loop."
- [ ] Skim [Stylo](https://searchfox.org/mozilla-central/source/servo/components/style) (Firefox/Servo/Blitz style system) and [vizia_style](https://github.com/vizia/vizia/tree/main/crates/vizia_style) — Stylo is "the big complex production-ready one"; vizia_style is "much smaller and more manageable" with non-standard properties but real selector resolution.
- [ ] Skim [lightningcss](https://github.com/parcel-bundler/lightningcss) parser layer — completest CSS parsing setup outside Stylo.

### Community / learning

- [ ] Join the [Linebender Zulip](https://xi.zulipchat.com). Vello and Parley live there. _"A great place to learn about the nitty-gritty details of those things."_
- [ ] Read Parley's [`line_break.rs`](https://github.com/linebender/parley/blob/main/parley/src/layout/line_break.rs) once. Hard but understandable; the itemisation/text-analysis/shaping layer beneath it is harder.

### Decisions to lock in before Phase 1

- [ ] **JS strategy decision.** Pick one, write it down, stop pretending:
  - (a) Drop JS entirely. Static renderer only. Honest and shippable. Closest to Blitz's current scope.
  - (b) Real but narrow: a small set of APIs with real implementations, rest removed.
  - (c) Full survival: every Web API real or absent. Years of work.
  - **Recommendation:** start at (a) so the engine can be made correct, then bolt on (b) selectively when each subsystem (DOM, layout, fetch) is real enough to support a non-stub implementation.
- [ ] Decide whether Aurora continues as its own engine, **or** whether the right move is contributing to Blitz directly. Nico explicitly invited it: _"Or even better, come and help me build Blitz!"_ — this is worth weighing before re-implementing every crate Blitz already integrates.

---

## Phase 1 — Replace the HTML parser with `html5ever`

> Nico: _"For parsing you should definitely look into html5ever. It has a tokenizer and a 'tree builder' (that fixes up the tree according the rules of the html5ever algorithm, and it pushes parsed content into a user-defined tree structure!"_

This deletes `src/html/` (~5 files, ~400 LoC of fragile state machine) and replaces it with a spec-compliant parser that already handles foster parenting, formatting elements, foreign content, RAWTEXT/RCDATA, DOCTYPE quirks mode, error recovery, and template/iframe content.

### Work items

- [x] Add `html5ever` and `markup5ever` to `Cargo.toml`.
- [x] Implement the `TreeSink` trait on a thin adapter over `crate::dom::NodePtr`. The tree-builder pushes nodes into your own tree shape — you don't have to adopt their tree.
- [x] Replace `Parser::parse_document` (`src/html/parser.rs`) with `html5ever::parse_document(sink, ParseOpts::default()).from_utf8().read_from(&mut bytes)?`.
- [x] Delete `src/html/parser.rs`, `tokenizer.rs`, `tag_parsing.rs`, `tokens.rs`, `text.rs`, `classify.rs`. Keep only the test fixtures (move to integration tests against the new sink).
- [x] Move the duplicate `decode_entities` out of `src/layout/inline_text.rs:35–48` — html5ever decodes entities at the tokenizer level, so the layout layer must not decode again.
- [x] Add a quirks-mode flag to the document, plumbed from html5ever's `quirks_mode`. Required for correct cascade and width calculation later.
- [x] **Test:** find any HTML5 conformance suite snippet that tests foster parenting (`<table>X<td>` style), formatting-element reconstruction (the "adoption agency"), and `<textarea>` RCDATA. Verify the tree shape matches a real browser.

### Outcomes

Eliminates P0 #1 from the principal review. Closes the "DOCTYPE silently dropped," "raw-text only for `script`/`style`," and "no error recovery" findings. Buys you `<title>`, `<textarea>`, `<noscript>`, `<iframe>`, `<plaintext>`, MathML, SVG inline content, and templates for free.

---

## Phase 2 — Replace the CSS parser, the selector engine, and the cascade

> Nico: _"The cssparser is a small crate that does low-level parsing (tokenization and also parsing of the syntax (so all you have to parse are the values))."_
> _"The selectors crate parses selectors, and can also resolve selectors against a user-defined tree."_
> _"Several people have built full style systems (of varying complexity) on top of these crates."_

This deletes `src/css/` selector parsing, `src/css/stylesheet.rs` rule splitter, the variable resolver, and the inline-style parser. It replaces them with [`cssparser`](https://crates.io/crates/cssparser) + [`selectors`](https://crates.io/crates/selectors), and a real cascade.

### Choose your reference style system

- [ ] Pick a tier based on the long-term ambition:
  - [ ] **Tier A:** Stylo — production, highest ceiling, hardest to land. Used by Firefox/Servo/Blitz.
  - [ ] **Tier B:** vizia_style — small, manageable, real selector resolution, non-standard properties. Best learning target.
  - [ ] **Tier C:** Takumi's style system — simplest, real selector resolution, less optimised.
  - **Recommended starting point:** Tier C → Tier B → Tier A. Blitz is on Stylo — if the long-term plan is Blitz-shaped, target Stylo.

### Work items

- [x] Add `cssparser` and `selectors` (and optionally `lightningcss` if you want a richer prebuilt parser) to `Cargo.toml`.
- [x] Replace `Stylesheet::parse` and `Stylesheet::do_parse` (`src/css/stylesheet.rs:35–73`) with a `cssparser::Parser`-driven walker that produces a `Vec<Rule>` with proper at-rule handling. Delete the `split('}')` rule splitter — it is wrong on `content: '}'`, `data:` URLs, and any nested at-rule.
- [ ] Replace `Selector::parse` and `SimpleSelector::parse` (`src/css/selector.rs`) with a `selectors::parser::Parser` and an `Impl` trait that maps your `dom::ElementNode` into the `selectors` crate's `Element` trait. This gives you `>`, `+`, `~`, attribute selectors, pseudo-classes, `:not()`, and `:is()` for free.
- [ ] Implement the `selectors::Element` trait on `crate::dom::NodePtr`. This is the bridge that lets the selectors crate _resolve_ matches against your tree, not just parse selectors.
- [ ] Delete `src/js_boa/selectors/simple.rs` and route JS-side `querySelector*` through the same engine. Two divergent selector parsers is a bug.
- [x] Implement real `!important` cascade ordering. Today `stylesheet.rs:151` does `trim_end_matches("!important")` and treats the declaration as normal. Move this to a per-declaration `important: bool` and apply origin-and-importance ordering per the [CSS Cascade and Inheritance spec](https://www.w3.org/TR/css-cascade-4/#cascade).
- [ ] Replace the seven hardcoded inherited properties (`src/style/node.rs:140–148`) with a property registry that declares, per property, parsing + inheritance + initial value. (Stylo/vizia_style both demonstrate this pattern.)
- [ ] Replace the quadratic `var()` resolver (`stylesheet.rs:107–132`) with cssparser's nested parsing or a resolver that walks the AST once.
- [x] Replace the `display_mode` mapping (`src/css/style_map.rs:21–28`) with a real `display: <inside>/<outside>` model. Restore `inline-block` to its own variant and add `grid`, `table`, `flow-root`, `inline-flex`, `inline-grid`, `list-item`, `none`.
- [x] Replace `Margin` (`src/css/properties.rs:43–49`) so `top` and `bottom` can be `auto`. Today they are `f32` and `auto` is structurally inexpressible.
- [x] Move inline-style parsing out of `src/js_boa/` and into `src/css/`. The cascade currently calls `crate::js_boa::parse_style_text` (`src/style/node.rs:83`) — wrong direction.
- [ ] Replace the user-agent stylesheet (`src/css/stylesheet.rs:21–33`) with a real one. Use [Servo's UA stylesheet](https://github.com/servo/servo/blob/main/resources/user-agent.css) or Blitz's as a starting point. Includes correct `display` for table elements, lists, forms, and replaced elements. Remove the made-up colour names (`accent`, `rust`, `coal`, `ink`).
- [ ] Add bucketed selector matching: hash rules by rightmost simple-selector tag/ID/class. Required to make `Stylesheet::styles_for` (`stylesheet.rs:88–93`) something other than O(R·N).
- [ ] Add `calc()`, `min()`, `max()`, `clamp()` parsing once cssparser is in.
- [x] Add the missing length units in `src/css/length.rs`: `pt`, `pc`, `cm`, `mm`, `in`, `Q`, `ch`, `ex`, `vmin`, `vmax`, `svh`/`lvh`/`dvh`.

### Outcomes

Closes P0 #2 of the review. Closes P1 #11 (quadratic `var()`), P1 #12 (unindexed selector matching), P1 #13 (divergent selector parsers), P1 #14 (`<style>`/`<script>` hardcoded layer), and most of P2 length-unit gaps.

---

## Phase 3 — Replace block-level layout with Taffy

> Nico: _"Taffy implements 'block level' layout (block, flexbox, css grid, etc). Each layout algorithm in Taffy is defined as a function that operates on a single container of that layout type and can be used standalone."_

This deletes `src/layout/block.rs`, `src/layout/flex/*`, `src/layout/construct.rs`, `src/layout/constraints.rs`, `src/layout/box.rs` (rect-only retained), and replaces them with Taffy. CSS Grid, which Aurora has not started, lands for free.

### Work items

- [x] Add `taffy` to `Cargo.toml`.
- [ ] Read [Taffy's `LayoutInput` / `LayoutOutput`](https://github.com/DioxusLabs/taffy/blob/main/src/tree/layout.rs). _"I'd also encourage you to look at the LayoutInput and LayoutOutput types that form a key part of the interface that all of the layout algorithms conform to."_
- [ ] Read each algorithm's entry function in turn:
  - [ ] [Flexbox](https://github.com/DioxusLabs/taffy/blob/main/src/compute/flexbox.rs#L166)
  - [ ] [Block](https://github.com/DioxusLabs/taffy/blob/main/src/compute/block.rs#L244)
  - [ ] [Grid](https://github.com/DioxusLabs/taffy/blob/main/src/compute/grid/mod.rs#L43)
- [ ] Decide between the two integration shapes:
  - [ ] **Owned tree** — store nodes in `taffy::TaffyTree<UserData>`. Simpler. Used by many small consumers.
  - [ ] **Custom `LayoutPartialTree`** — implement Taffy's `LayoutPartialTree` / `TraversePartialTree` traits on your own DOM/Style tree. This is what Blitz does. More code, but no double-storage of the tree shape. **Recommended** to match Blitz's approach.
- [x] Map `crate::style::StyledNode` → `taffy::Style` per element. Encapsulate this in a `style_to_taffy` adapter. CSS values that don't translate (e.g. `box-sizing`, `min/max width`) become Taffy's equivalents.
- [ ] Replace the entry point. `LayoutTree::from_style_tree_with_viewport` becomes a call to Taffy's `compute_layout` with the viewport size as the root's `available_space`.
- [ ] Delete `src/layout/block.rs`, `src/layout/flex/*`, `src/layout/construct.rs`. Keep `src/layout/rect.rs` for screen-space rects.
- [ ] Replace `LayoutBox` with a typed leaf-or-branch enum that Taffy populates. The current per-box owned `StyleMap` clone (`src/layout/box.rs:11`) goes away — Taffy does not own styles, it borrows them through the trait.
- [ ] Wire `position: absolute|fixed|sticky` through Taffy (Taffy supports these natively).
- [ ] Wire `display: grid` through Taffy. Closes a gap the principal review flagged.
- [ ] Replace ad-hoc `find_node`/`hit_test` (`src/layout/box.rs:144–169`) with calls into the Taffy node store.
- [ ] **Test:** port the existing layout fixtures to integration tests against the Taffy-backed tree. Compare positions to within ±0.5 px of the previous output where it was correct, accept new (correct) positions where the old engine was wrong.

### Floats — uses Taffy + Parley together

> Nico: _"Taffy and Parley also co-operate (with glue code in Blitz) to implement Floats."_

- [ ] After Phase 4 (Parley) lands, port Blitz's float glue. Floats sit at the boundary between block-level layout (Taffy) and inline layout (Parley) and require both pieces to be in place.

### Outcomes

Closes P0 #4 (StyleMap-cloned-into-every-box), P0 #8 (no fragment-based inline — partially; finished in Phase 4), the `inline-block` mismapping, missing `position`/`grid`/`table`, the inline-layout exponential re-layout case (`src/layout/inline.rs:60–79`).

---

## Phase 4 — Replace inline layout with Parley

> Nico: _"Parley is a text-layout library that implements / can be used to implement inline layout (which is essentially fancy text layout). The actual layout logic is in line_break.rs and is relatively understandable, but it depends on an 'itemization'/'text analysis'/'shaping' stage that is kinda complex and requires a lot of detailed knowledge about unicode to understand."_
> _"The code that Blitz uses to integrate Parley with Taffy is in inline.rs. There is one Parley Layout per inline formatting context."_

This deletes `src/layout/inline.rs`, `inline_sequence.rs`, `inline_text.rs`, `text_metrics.rs`, and most of `src/font/`. Parley owns shaping, line breaking, BiDi, font fallback, glyph clusters — all the things Aurora currently does wrong.

### Work items

- [ ] Add `parley` to `Cargo.toml`.
- [ ] Read [Parley `line_break.rs`](https://github.com/linebender/parley/blob/main/parley/src/layout/line_break.rs).
- [ ] Read [Blitz's `inline.rs` glue](https://github.com/DioxusLabs/blitz/blob/main/packages/blitz-dom/src/layout/inline.rs). This is the canonical "Parley + Taffy + DOM" integration.
- [ ] Establish "one Parley `Layout` per inline formatting context." An IFC is a contiguous run of inline-level children of a block container. Detect IFC boundaries during the style/layout pass and build a Parley layout for each.
- [ ] Build a `parley::FontContext` shared per document. Replace `src/font/resources.rs`'s single-OnceLock face with a real fallback chain (system fonts + bundled fallback).
- [ ] Delete the Latin-1 prebuilt atlas (`src/font/atlas_builder.rs:18`). Parley feeds glyphs to the renderer per-shape, per-size — atlas is dynamic, downstream.
- [ ] Delete `src/font/shape.rs`. Its `text.chars().enumerate()` zip with rustybuzz glyphs is wrong (cluster-by-cluster shaping is what Parley does).
- [ ] Hook Taffy's "measure function" for inline children into Parley: when Taffy needs to measure an inline-formatting context, it calls a closure you provide that runs Parley's layout against the constraint and returns `LayoutOutput`.
- [ ] Watch for the upcoming `parley_core` crate Nico mentioned: _"We may soon be splitting out a 'parley_core' crate that would allow you to implement your own layout while using Parley's logic for that core complex bit if you wanted to."_ — track Linebender Zulip / GitHub for this and migrate when available.
- [ ] **Test:** load text in scripts beyond Latin-1 (Cyrillic, Greek, Arabic RTL, Devanagari, CJK). Each must render. Aurora today renders none of them.

### Outcomes

Closes P0 #7 (Latin-1 atlas), P0 #8 (no fragment model), P0's "shaper-glyph alignment by index" (because Parley uses cluster info), and removes the entire `src/layout/inline*.rs` stack.

---

## Phase 5 — Per-node invalidation and incremental reflow

> The principal review's structural P0: today `BoaRuntime`/`registry.rs` rebuilds the StyleTree and LayoutTree on every dirty bit. There are two booleans for the entire document. Forced sync layout from JS cannot ship safely without this work first.

Taffy already has dirty-tracking (`Dirty` flag per node, `mark_dirty` walks ancestors). Use it.

### Work items

- [ ] Replace the global `DirtyState { style: bool, layout: bool }` (`src/js_boa/registry.rs:137–141`) with per-node dirty bits stored on the StyledNode / Taffy node.
- [ ] On any DOM mutation (`appendChild`, `setAttribute`, `style.X = ...`, etc.), mark the affected node and its ancestors dirty up to the nearest BFC/IFC boundary.
- [ ] Style-only changes (a CSS value that doesn't affect layout — `color`, `background-color`, `visibility`, `text-decoration`) skip layout entirely.
- [ ] Layout-affecting changes invalidate Taffy's cache for that subtree.
- [ ] Implement `flush_pending_layout()` and call it from `getBoundingClientRect`, `offsetWidth`, `offsetHeight`, `getComputedStyle`, `clientWidth`, `clientHeight`, `scrollWidth`, `scrollHeight`. The implementations in `src/js_boa/accessors/layout.rs:11, 28, 51` already call `perform_sync_reflow` — make sure that path actually does incremental work.
- [ ] Stop synchronously refetching images on resize (`src/window/input.rs:41`). Issue image fetches when the layout discovers a new `<img src>`, store them off-thread, repaint when they arrive.
- [ ] Add a generation counter on the document so JS RefCell callbacks can detect a re-entrant mutation and defer instead of panic.

### Outcomes

Closes P0 #3 (reflow path is a tree rebuild), P1 #20 (resize re-issues HTTP), and makes the `REFLOW_CRITICAL_NEXT.md` "forced sync layout" item ship-able.

---

## Phase 6 — Replace the rendering layer with AnyRender

> Nico: _"You may find Blitz's implementation useful. Blitz uses the AnyRender abstraction (which we created). It operates in terms of 'drawing commands' like 'draw glyph', 'fill rect', etc. The blitz-paint crate translates our (styled and layouted) DOM representation into AnyRender drawing commands. And then there are AnyRender backends for 2D canvas crates like vello and skia for drawing to screen."_

Aurora already uses Vello directly via `gpu_paint`. AnyRender doesn't replace Vello — it wraps it, so you can also output to Skia, to a software rasteriser for tests, or to PDF for printing.

### Work items

- [ ] Add `anyrender` and `anyrender_vello` (or whichever backend crates exist) to `Cargo.toml`.
- [ ] Refactor `src/gpu_paint/painter.rs` to emit `anyrender` drawing commands instead of calling `vello::Scene` directly.
- [ ] Wire the Vello backend behind AnyRender — your existing GPU pipeline becomes one backend among several.
- [ ] Add a software / image backend so visual regression tests can run without a GPU.
- [ ] Reference [`blitz-paint`](https://github.com/DioxusLabs/blitz/tree/main/packages/blitz-paint) — it is already the "DOM → drawing commands" translator. You may be able to lift it directly.
- [ ] Add stacking context support: real CSS painting order with z-index, `position: relative` painting promotion, transforms.

### Outcomes

Sets up Phase 8 (visual regression) and unblocks `transform`-only repaint paths later.

---

## Phase 7 — Networking via reqwest or ureq

> Nico: _"You'll likely find you don't need too much on top of a general purpose HTTP client like reqwest or ureq. The blitz-net crate is only ~300 LoC."_

Aurora's `src/fetch/` is already real HTTPS + capability-gated — keep the capability layer, replace the bespoke HTTP transport.

### Work items

- [ ] Choose `reqwest` (async, more features) or `ureq` (sync, simpler). For an event-loop engine, `reqwest` integrates better.
- [ ] Read [`blitz-net`](https://github.com/DioxusLabs/blitz/tree/main/packages/blitz-net) end-to-end. It is the reference for "minimum glue from HTTP client to engine."
- [ ] Replace `src/fetch/http.rs`, `chunked.rs`, `tls.rs`, `redirects.rs` with the chosen client. Keep `capability.rs` and `resolve.rs` — those are Aurora-specific and still load-bearing.
- [ ] Run image fetches off the layout thread (channels back to the event loop).
- [ ] Run script fetches off the parser thread (preload scanner pattern).

### Outcomes

Closes the synchronous-fetch-on-main-thread issues throughout the principal review (P0 #10, P1 #20, "synchronous network on main thread").

---

## Phase 8 — Testing and CI: visual regression that actually gates

The current `make all-renders` regenerates baselines. Nothing fails when a render changes.

### Work items

- [ ] Split `make all-renders` into:
  - [ ] `make update-snapshots` — explicitly regenerate baselines.
  - [ ] `make check-snapshots` — pixel-or-perceptual-diff against the baselines, fail on regression. Wire into `cargo test`.
- [ ] Add an HTML5 conformance subset test runner. The [Web Platform Tests](https://github.com/web-platform-tests/wpt) repo has many small CSS layout tests with reference renders ("reftests"). Adopting even 200 of them tells you immediately when a Phase 1–4 change regresses something.
- [ ] Add a Taffy-style benchmark harness once Taffy lands so layout perf changes are visible in CI.

---

## Phase 9 — JS strategy follow-through (only if option (b) or (c) was picked in Phase 0)

If the JS decision is "keep, but make real for a narrow surface," pick the surface and make it real. Do **not** continue with stubs.

### Real, not stubbed

- [ ] Real `Event` object passed to listeners — `target`, `currentTarget`, `type`, `preventDefault`, `stopPropagation`, `bubbles`, `defaultPrevented`. Today `dispatch_event` (`src/js_boa/runtime.rs:51–78`) calls listeners with `&[]`.
- [ ] Capture phase + bubbling phase. Today only the hit-tested target fires.
- [ ] Real `addEventListener` options (`{capture, once, passive}`).
- [ ] Real `URL` parser. Today `src/js_boa/network.rs:65–69` returns `''` for every field.
- [ ] Real `MutationObserver`. The DOM mutation code paths must enqueue records; the runtime tick must drain them. Today `src/js_boa/observers.rs` is no-op.
- [ ] Real `IntersectionObserver` — needs viewport + scroll integration.
- [ ] Real `ResizeObserver` — needs the per-node dirty-bit Phase 5.
- [ ] Real `fetch` backed by Phase 7's HTTP client (with CORS gating off whatever capability model Aurora's identity layer ships).
- [ ] Real persistence for `localStorage` (file-backed, capability-gated under `workspace.read`/`write`).
- [ ] Reverse map keyed by `Rc::as_ptr(&node)` so `dispatch_event` is O(1), not O(N) over registered nodes (`runtime.rs:55–58`).
- [ ] Real `window.scrollY` getter that observes the live scroll state (today it's a snapshot from runtime construction).
- [ ] Real scroll/resize/load/DOMContentLoaded events.
- [ ] Fix `requestIdleCallback === setTimeout` in `src/js_boa/globals/timers.rs:69–73`. Real rIC defers until idle; today it's eager.

### Or: option (a) — drop JS entirely

- [ ] Delete `src/js_boa/` (~50 files).
- [ ] Delete the `boa_engine` / `boa_gc` deps.
- [ ] Update README to reflect the static-renderer scope.
- [ ] Optional: gain the Blitz alignment. Static renderer is exactly Blitz's current shape — easier to port content / collaborate.

---

## Phase 10 — Decide where Aurora goes

Three honest paths. Pick one and update the README.

- [ ] **Path A — continue Aurora as an independent engine** built on the Blitz-aligned crate stack (html5ever / cssparser / selectors / Taffy / Parley / AnyRender). Distinct angle: Bastion identity / capability model. Real, hard work; clear differentiation.
- [ ] **Path B — converge with Blitz.** Contribute the Aurora-specific bits (capability gating, identity-bound fetch, sovereign-runtime ideas) upstream. Drop the duplicated infrastructure. Nico's offer: _"come and help me build Blitz."_
- [ ] **Path C — narrow Aurora's scope dramatically.** Static-only, no JS, fixture-renderer for the Bastion stack. Honest README. Useful tool, not a browser.

The principal review's verdict was: _"Don't extend. Replace. Then extend."_ Phases 1–7 are the replace. Phase 10 is the question of what you extend toward.

---

## Issue ↔ Phase cross-reference

| Principal-review finding | Closed by phase |
| --- | --- |
| P0 #1 No HTML parser | Phase 1 |
| P0 #2 No real CSS cascade | Phase 2 |
| P0 #3 Reflow is a tree rebuild | Phase 5 (depends on Phase 3) |
| P0 #4 StyleMap cloned per box | Phase 3 |
| P0 #5 JS DOM bridge will panic | Phase 9 (or removed in Phase 0(a)) |
| P0 #6 fetch / XHR / observers theatrical | Phase 7 + Phase 9 |
| P0 #7 Latin-1 only font path | Phase 4 |
| P0 #8 No inline fragment model | Phase 4 |
| P0 #9 No compositing | Phase 6 |
| P0 #10 Synchronous serial pipeline | Phase 5 + Phase 7 |
| P1 #11 Quadratic var() resolver | Phase 2 |
| P1 #12 Unindexed selector matching | Phase 2 |
| P1 #13 Divergent selector parsers | Phase 2 |
| P1 #14 `<style>`/`<script>` hardcoded layer | Phase 2 |
| P1 #15 Event-loop scheduling fragile | Phase 5 |
| P1 #16 `requestIdleCallback === setTimeout` | Phase 9 |
| P1 #17 `clearTimeout` after fire | Phase 9 |
| P1 #18 Hit-testing whole-tree-walk | Phase 3 |
| P1 #19 Scroll decoupled from document | Phase 9 |
| P1 #20 Resize re-fetches images | Phase 5 + Phase 7 |
| P1 #21 Style module reaches into JS module | Phase 2 |
| P1 #22 Inheritance hardcoded | Phase 2 |
| P1 #23 f32 layout drift | Phase 3 (Taffy uses f32 too — accept and document, or PR a fixed-point mode) |
| P1 #24 No visual regression diff | Phase 8 |
| P1 #25 Misleading reflow doc | Replace `REFLOW_CRITICAL_NEXT.md` after Phase 5 lands |
