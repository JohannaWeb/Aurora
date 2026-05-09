# Aurora — Principal Engineer Review

> Reviewer hat: Principal, Mozilla rendering / layout team
> Scope: end-to-end pass over `src/` as it stands today
> Tone: I'm not going to be polite. You asked for it.

---

## TL;DR

Aurora is a **demo**. It is not a browser engine in the sense the README claims, and the bundled `REFLOW_CRITICAL_NEXT.md` — also written in the voice of a Mozilla principal — wildly understates the gap. Forced synchronous layout is item #34 on the list of things that are wrong. Items #1 through #33 are the ones I'd flag in a real review.

You have, very roughly:

- A pretend HTML "parser" (90 lines, recursive, no spec algorithm, no error recovery, no foster parenting, no formatting elements, no template/iframe/foreign-content handling).
- A pretend CSS engine (descendant-only selectors, no `>`/`+`/`~`, pseudo-classes silently dropped, no `!important`, no proper cascade, seven inherited properties hardcoded, naive `split('}')` rule splitter).
- A layout tree that **clones the entire `StyleMap` (a `BTreeMap<String,String>`) into every box** and rebuilds the whole tree on every reflow.
- A "JS DOM bridge" whose fetch always rejects, whose XHR always fails with status 0, whose MutationObserver / IntersectionObserver / ResizeObserver are silent no-ops, whose Event object is literally `&[]`, and whose storage is in-memory only.
- A font path that pre-bakes **U+0000–U+00FF only**. No CJK, no emoji, no Devanagari, no Arabic, no Cyrillic, no Greek beyond the codepoints that happen to live in Latin-1. No italic. No bold. The "bold" you see in screenshots is the regular face with a bold *style declaration* — there is one TTF.
- A tokenizer that treats only `<script>` and `<style>` as raw text, so `<textarea>`, `<title>`, `<iframe>`, `<noscript>`, `<noembed>`, `<plaintext>` all parse as element soup.
- A reflow path that, on every DOM mutation, **rebuilds the StyleTree from the DOM, then rebuilds the LayoutTree from the StyleTree, then synchronously re-issues image fetches on the main thread.**

The architecture isn't merely incomplete. Several core abstractions are wrong in ways that mean the code you've written so far has to be discarded, not extended, before you can render a real page.

I'll go in severity order.

---

## P0 — Architecture-level. These are not bugs, they are wrong models.

### 1. There is no HTML parser.

`src/html/parser.rs` is 90 lines. It does no spec-compliant tokenization, no insertion modes, no foster parenting, no active formatting elements, no template handling, no error recovery, and no foreign content (SVG/MathML). `parse_element` walks a flat token list and stops at the first matching close tag — so `<div><p><div></p></div>` does not produce the DOM that any browser produces; it produces a malformed tree silently. `is_raw_text_tag` (`src/html/classify.rs:1–3`) returns true only for `script` and `style`. That means:

- `<textarea>foo<bar></textarea>` — `<bar>` becomes a child element. Wrong (textarea is RCDATA).
- `<title>foo<b>x</b></title>` — `<b>` becomes a child element. Wrong (title is RCDATA).
- `<iframe>...</iframe>` — content is parsed as DOM. Wrong (iframe is RAWTEXT).
- `<plaintext>` — does anything other than what the spec says.
- `<noscript>` — same.

The DOCTYPE is silently discarded (`src/html/tokenizer.rs:26–29`). There is no quirks-mode versus standards-mode distinction, which means later when you do try to handle real pages, half of CSS will compute differently than expected and you will not know why.

`find_tag_end` (`src/html/classify.rs:25–43`) builds a `Vec<char>` per tag — a per-tag heap allocation and per-byte UTF-8 decode. That's a hot path.

**Verdict:** rip this out, or use `html5ever`. The "from-scratch" bragging rights are not worth the bug surface. Mozilla learned this lesson by writing the spec.

### 2. There is no real CSS cascade.

`src/css/selector.rs` only parses tag/`#id`/`.class` joined by whitespace. There is no child combinator (`>`), no adjacent sibling (`+`), no general sibling (`~`), no attribute selectors (`[type="text"]`), no pseudo-classes — they're stripped at parse time (`strip_pseudo_suffix`, `:139`). This means `div > p`, `a:hover`, `input[type="email"]`, `:not(.x)`, `:nth-child(2n+1)`, `::before` — all silently fail to match. The JS-side selector engine in `src/js_boa/selectors/simple.rs` is *more capable* than the cascade selector. That's not a bridge — that's two separate, divergent implementations.

`src/css/stylesheet.rs:43–44` splits the entire stylesheet on `}`. That breaks on:

- any `content: '}'` declaration
- any `data:` URL with `}` in it
- any nested rule (and you do strip `@media` first via `strip_at_rules`, but the moment someone writes `@supports` or `@container` you mis-tokenize)
- modern nesting (which you don't support, but anyone copying real CSS hits it)

Specificity calculation is fine for what's parsed. `!important` is handled by `trim_end_matches("!important")` in the value (`stylesheet.rs:151`) — meaning the precedence rule is **completely absent**. An `!important` declaration cascades exactly the same as a normal one.

Inheritance is hand-rolled for seven properties (`src/style/node.rs:140–148`). Real CSS has dozens of inherited properties (`color`, `font-*`, `text-*`, `cursor`, `direction`, `letter-spacing`, `word-spacing`, `quotes`, `list-style*`, `border-collapse`, ...). And there is no concept of `inherit` / `initial` / `unset` / `revert` keywords.

`StyleMap.display_mode` (`src/css/style_map.rs:21–28`):

```rust
Some("inline") | Some("inline-block") => DisplayMode::Inline,
```

`inline-block` is mapped to `Inline`. The `InlineBlock` variant exists in the layout module (`src/layout/box.rs:24`) but is never produced. There's no `display: grid`, no `display: table`, no `flow-root`, no `inline-flex`, no `list-item`, no `none` short-circuit at parse — these are silently swallowed.

`Margin::top` and `Margin::bottom` are `f32`; `Margin::left` and `Margin::right` are `MarginValue` (`Px` or `Auto`) (`src/css/properties.rs:43–49`). So vertical `margin-top: auto` is structurally inexpressible. That's not a feature gap — that's a data-model bug.

CSS variable resolution (`stylesheet.rs:107–132`) is a `find("var(")` loop with a 100-iteration cap. Quadratic on long stylesheets, and capped at an arbitrary number that real stylesheets can hit.

**Verdict:** the cascade is the part of a browser you cannot get away with hand-rolling. Either adopt a real CSS parser (cssparser, lightningcss) or commit to writing one with a real grammar. The current code is going to silently mis-render anything beyond your fixtures.

### 3. The reflow path is not a reflow path. It is a tree rebuild.

Look at `src/js_boa/registry.rs:107–134`:

```rust
pub(super) fn perform_sync_reflow(&self) {
    if !self.has_dirty_bits() { return; }
    ...
    let style_tree = crate::style::StyleTree::from_dom(document, &stylesheet.borrow());
    let new_layout = crate::layout::LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);
    *layout_tree.borrow_mut() = new_layout;
    self.clear_dirty_bits();
}
```

Every dirty bit triggers a full StyleTree recomputation and a full LayoutTree recomputation. The dirty-bit struct is two booleans (`registry.rs:137–141`):

```rust
struct DirtyState { style: bool, layout: bool }
```

Not per-node. Not per-subtree. Two booleans for the entire document. The `_node` parameter on `mark_style_dirty(&self, _node: &NodePtr)` is literally underscored to acknowledge that the function discards it. So the existing API is a lie — it pretends to be node-targeted and isn't. You cannot fix this incrementally; the dirty-bit infrastructure has to live on `LayoutBox` / `StyledNode`, not on a single global flag.

`pipeline.rs:23–29` and `window/input.rs:23–43` do the same thing on resize and on initial load, except `WindowInput::reflow` *also synchronously re-fetches all images on the main thread* (`input.rs:41`). Resize the window → block the UI thread on HTTP.

The author of `REFLOW_CRITICAL_NEXT.md` says "fine-grained invalidation is P1." It is P0. Without per-node dirty bits and a partial layout walk you don't have a reflow algorithm — you have `pages_load() -> repeat`.

### 4. The style and layout trees are clone-heavy by construction.

`LayoutBox` (`src/layout/box.rs:7–16`) carries:

- a `StyleMap` (which is a `BTreeMap<String, String>`)
- a tag name `String` inside `LayoutKind::Block { tag_name }` etc.

Every layout box owns its own copy of the styles. On every reflow, every `StyleMap` is cloned (`construct.rs:68`, `block.rs:97–103`, etc.) — strings and all. For a 2000-element document with 30 declarations per element that's tens of thousands of `String` clones per frame.

`StyledNode` (`src/style/node.rs`) clones `element.children` into `element_children = element.children.clone()` (`:93`) before recursing. That's a `Vec<NodePtr>` clone per element, which is reasonable, but combined with the StyleMap clone in the layout pass and the per-string cloning in property parsing, the allocator is your hot path.

`box.rs:144–154 find_node` is O(N) recursion. Every JS read of `.offsetWidth` walks the whole tree to locate the box (after rebuilding the tree). On a real page with thousands of nodes, that's a per-call O(N) layout walk on top of the O(N) reflow you just did.

**Verdict:** decide whether the LayoutBox owns or borrows styles. If it owns, intern the strings. If it borrows, lifetimes will fight you, but the perf payoff is real. Right now you have the worst of both: owned full clones and no interning.

### 5. The JS DOM bridge is going to panic on real pages.

The DOM is `Rc<RefCell<Node>>` (`src/dom/node.rs:9`). Every accessor in `js_boa/` calls `node.borrow()` or `node.borrow_mut()` and then often calls into other code that *also* needs to borrow. When real-page JS does, e.g., a `MutationObserver` callback that reads `parentElement.children` while you're in the middle of an `appendChild` mutation, you'll panic with `BorrowMutError`. The codebase has no defense against this. There is no `try_borrow`-with-deferred-fallback path, no transactional mutation queue, no generation counter.

`BoaRuntime::dispatch_event` (`runtime.rs:51–78`) calls listeners with:

```rust
let _ = listener.call(&JsValue::undefined(), &[], &mut self.context);
```

- `this` is `undefined`. Should be the event target.
- args is `&[]`. There is no `Event` object passed to the listener. Listeners that read `event.target`, `event.preventDefault()`, `event.stopPropagation()`, `event.currentTarget`, or `event.type` immediately get `undefined` and silently break.
- No capture phase, no bubbling, no path resolution. The hit-tested node fires; nobody else does.
- No `addEventListener` options (`{capture, once, passive}`).

`document.addEventListener` calls don't even appear to be wired (the listeners map is keyed by node id but there's no document-level dispatch path on click). `click` only goes to the layer node hit by `hit_test`, and `hit_test` returns the deepest node — fine — but capture/target/bubble are missing entirely.

This is not a "stub being completed." This is the wrong model.

### 6. fetch / XHR / WebSocket / Worker / observers are *theatrical*.

`src/js_boa/network.rs` is 100+ lines of polyfill that:

- always rejects `fetch()` with `"Aurora: network fetch disabled in JS runtime"`.
- always fast-forwards `XMLHttpRequest` to `readyState=4, status=0` and calls `onerror`.
- defines a `URL` constructor that does not parse the URL — every field is `''`.
- defines a `URLSearchParams` parser that uses `decodeURIComponent` only on the value half (and not the key half).
- declares `WebSocket`/`Worker`/`SharedWorker` as constructors that throw.

`src/js_boa/observers.rs` makes `MutationObserver`, `IntersectionObserver`, `ResizeObserver`, `PerformanceObserver` constructors return objects with no-op `observe`/`unobserve`/`disconnect`. None of them ever fire. Pages that depend on observers (every modern lazy-loading library, every infinite scroll library, every layout-aware framework) will never call their callbacks.

`src/js_boa/storage.rs` backs `localStorage` and `sessionStorage` with the same in-process `BTreeMap`. Nothing persists. You don't even fake disk, you fake a `Map`.

The README says "the bridge currently prioritizes compatibility survival over full browser correctness." That framing is generous. **Survival** here means "the script doesn't throw on the first line." That is a low bar. The next line of any real script — `const data = await fetch(...).then(r => r.json())` — fails immediately, then either the page fallback path runs (if the author wrote one) or nothing happens.

If the goal is "many modern scripts can initialize without crashing," fine — but it's worth being honest with yourself: the page that ran is not the page real users see, and you can't rely on these stubs to validate any other layer of the engine.

### 7. The font path can render Latin-1 only.

`src/font/atlas_builder.rs:18`:

```rust
for code in 0u32..256 {
```

That's the entire glyph atlas. Code points beyond 255 do not exist in your atlas. If you load any page with CJK, emoji, Cyrillic beyond the basic block, Devanagari, Arabic, Hebrew, Thai, math symbols, or any pictographic glyphs, you get nothing. The fallback is "the glyph isn't in the atlas, so don't draw it."

There's exactly one font file (`fonts/default.ttf`) and one face. No font fallback chain. No bold variant, no italic variant — `font-style: italic` and `font-weight: bold` change nothing about which glyph gets drawn, only which color/style flag is in the StyleMap.

`src/font/shape.rs` zips rustybuzz `glyph_infos` with `text.chars()` by index:

```rust
for (i, (_info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
    let ch = text_chars.get(i).copied().unwrap_or(' ');
```

This is wrong. Shaping produces one glyph per *cluster* of input characters in general — ligatures decrease glyph count, decompositions increase it, RTL reorders. The `cluster` field on `glyph_info` is what tells you which input char range produced this glyph. By indexing with `i` into `text_chars` you're misaligning glyphs and characters every time the shaper does anything non-trivial. Today this looks fine because the font is Latin-only and the text is left-to-right. The first time you load Arabic or any ligature-heavy font, the rendered text is garbage.

Atlas glyphs are all baked at a single size (`ATLAS_BASE_SIZE = 32.0`). At other sizes you sample the 32-px atlas. Subpixel positioning, hinting at small sizes, signed distance fields — none of it.

**Verdict:** the text path is the second hardest part of a browser after the cascade. You haven't started it.

### 8. Inline layout has no fragment model.

`src/layout/inline.rs` and `inline_text.rs` walk children in a single pass, accumulating `line_x`, `line_y`, `line_height` as mutable cursors. There is no notion of an inline formatting context, no line box, no fragment tree, no concept of break opportunities other than ASCII whitespace. This means:

- `word-break`, `overflow-wrap`, `hyphens`: not supported, not even data-modeled.
- BiDi: not supported. The Unicode Bidi Algorithm doesn't exist here.
- vertical-align: not supported.
- `text-indent`: not supported.
- `<br>`: I don't see it being lowered into a forced line break in the inline pass.
- Nested inlines (`<span><b>x</b></span>`): the inner box is *measured* by re-laying-out via `from_styled_node` (`inline.rs:60–79`), and if it doesn't fit, **it is laid out a second time** at a new line. Three nested inlines that don't fit cause exponential re-layout in the worst case.

When anything but trivial English text needs to wrap, you'll have to throw this away and adopt a real line-breaking algorithm (Knuth-Plass at the high end; iterative best-fit at the low end). What you have is the simplest possible algorithm and it doesn't generalize.

### 9. There is no compositing or paint optimization.

`src/window/app.rs:72–125` rebuilds a fresh `vello::Scene` every frame and walks the entire layout tree. There is no display list cache, no layer tree, no scroll layer separated from content layer (well, there's a `push_layer` for scroll translation, but it's a single layer for the whole page), no transform-only update path. Animating a `transform: translateX` would re-layout, re-paint, and re-tessellate the whole document.

There is no z-index ordering beyond paint order. Real CSS stacking contexts don't exist. `position: absolute|fixed|sticky` aren't supported (and aren't even parsed).

### 10. The pipeline is synchronous and serial.

`src/runner/pipeline.rs:14–54`:

```
load_html  // sync HTTP on the main thread
parse      // single-threaded
extract_scripts
run_scripts // synchronous, no defer/async, no script-blocking-style awareness
build stylesheet // sync, including @import fetches
compute style tree // single pass
build layout tree  // single pass
load images        // sync HTTP, on main thread, before paint
open window        // GPU init
```

There is no progressive parsing, no progressive rendering, no preload scanner, no off-main-thread fetch, no off-main-thread script compilation. The browser blocks on each phase. For a real page, the user waits for all images to download before the first frame paints.

`async`/`defer` script attributes are ignored. Scripts run in source order, blocking, on the main thread, before any paint.

---

## P1 — Things that are merely badly implemented.

### 11. CSS variable resolution is quadratic.

`stylesheet.rs:107–132`. Each `find("var(")` is a linear scan from byte 0 of `result`. Each replacement reshuffles the string. On a stylesheet with N variable references, this is O(N²) per declaration, capped at 100 iterations. Real stylesheets can have hundreds of var references.

### 12. Selector matching is unindexed.

`stylesheet.rs:88–93` filters every rule for every element:

```rust
let mut matching_rules = self.rules.iter()
    .filter(|rule| rule.selector.matches(element, ancestors))
    .collect::<Vec<_>>();
```

For R rules and N elements, this is O(R·N) selector-matches. Real engines bucket selectors by rightmost simple selector — by tag, ID, and class name — and only check rules whose rightmost simple-selector hash matches. Without that bucketing, a 10k-rule stylesheet (Bootstrap, Tailwind) on a 2k-node document is 20M comparisons per style recalc. You will see seconds-per-frame.

### 13. There are two divergent selector parsers and two entity decoders.

- Cascade selectors (`src/css/selector.rs`) and JS selectors (`src/js_boa/selectors/simple.rs`) parse different grammars.
- Entity decoding lives in `src/html/text.rs` (used by the tokenizer) **and** `src/layout/inline_text.rs:35–48` (a separate hand-rolled `decode_entities` with its own subset of entities). The layout layer should not be entity-decoding text — that's the parser's job. If a `&nbsp;` appears in text content, the two paths disagree.

### 14. `<style>` and `<script>` are dropped at the layout level instead of the style level.

`src/layout/construct.rs:25–29`:

```rust
if node.tag_name() == Some("style".to_string())
    || node.tag_name() == Some("script".to_string())
{
    return None;
}
```

Two `String::clone` allocations per call to a hot recursive function, to do something that should be a `display: none` rule. The user-agent stylesheet at `src/css/stylesheet.rs:31` already declares `style, script, ... { display: none; }` — so this layout-level check is redundant *and* wrong: if a real page sets `style { display: block; content: attr(...) }` (legal in CSS), the override never wins because the layout layer hardcodes the suppression.

### 15. The event loop scheduling is fragile.

`src/window/app_handler.rs:89–94, 151–159`:

```rust
fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
    if self.has_animation_frame_callbacks() || self.timer_is_due() {
        self.request_redraw();
    }
    self.schedule_next_frame(event_loop);
}
```

The redraw-versus-tick logic is correct but fragile: if a frame callback enqueues another frame callback (the standard rAF idiom for animations), `has_animation_frame_callbacks` is true forever, the control flow stays at `Poll`, and the engine spins at unbounded fps until the loop empties. There's no minimum frame interval, no vsync coupling beyond `PresentMode::Fifo`, no budget on frame-task work (timer ready limit is 100 callbacks per `ready_timers` call — no equivalent for rAF).

`drain_microtasks` (`runtime.rs:181–200`) caps at 1000 outer iterations but drains the entire microtask queue per iteration. A tight Promise chain inside a microtask will starve everything else. The outer cap is arbitrary and the inner loop is unbounded.

### 16. `requestIdleCallback === setTimeout`.

`src/js_boa/globals/timers.rs:69–73` aliases `requestIdleCallback` to the timer constructor with default delay 0. That's not idle. That's eager. The whole point of rIC is to defer until idle — by aliasing it to "fire as soon as possible" you've inverted the contract.

### 17. `clearTimeout` after fire.

`ready_timers` (`runtime.rs:163–179`) drains all timers, fires the ready ones, and pushes the rest back. A `clearTimeout(id)` issued *during* a callback can't see the in-flight entry because it's already been moved out. Probably benign for one-shots; it matters for intervals.

### 18. Hit-testing is whole-tree-walk per click.

`src/layout/box.rs:156–169 hit_test` recurses every layout box per click. `src/window/app_handler.rs:121–137 handle_click` does one of these per left-mouse-down. Not a problem at fixture sizes; quadratically problematic at real-page sizes when combined with reflow rebuild on every state change. Real engines build a coarse spatial index (R-tree or quad-tree) over the painted boxes.

### 19. Scroll is decoupled from the document.

`scroll_y` lives on `AuroraApp` (`window/app.rs:20`). The JS-visible `window.scrollY` is captured at runtime construction time and not updated when arrow keys scroll. There is no `scroll` event dispatched. Scroll-aware sticky headers, scroll-spy navigation, and infinite-scroll libraries get nothing.

### 20. Resize triggers a full pipeline rerun including image refetch.

`src/window/input.rs:23–43`:

```rust
self.images = crate::load_images(layout.root(), self.base_url.as_deref(), &self.identity);
```

Resizing the window re-issues image HTTP fetches synchronously on the UI thread. Drag the corner of the window and the engine performs N HTTP GETs per resize event. Add a `Cache-Control: no-cache` to the test page and you have a self-DoS.

### 21. CSS resolution mid-layout reaches into an unrelated module.

`src/style/node.rs:83`:

```rust
for (name, value) in crate::js_boa::parse_style_text(inline_style) {
    styles.set(name, value);
}
```

The style module reaches into the JS-bridge module to parse inline `style="..."` attributes. The dependency direction is upside-down. The parser ought to live in `css/`; the JS bridge consumes it. As written, removing the JS module breaks the cascade.

### 22. Inheritance is a hardcoded list.

`src/style/node.rs:140–148, 177–188` hardcodes the inheritance set. Real engines drive this off the property registry. If you ever want `cursor` or `direction` or `letter-spacing` to inherit, you'll add a line here. And another. And another. Better: declare each property's inheritance behavior alongside its parser.

### 23. Layout uses `f32` arithmetic without snapping.

Many engines use `LayoutUnit` (a fixed-point integer) to avoid accumulating float error in nested margins / borders / paddings. You're using `f32` everywhere. Long pages will drift; subpixel positions will compound. This is a "you'll see a 1-pixel hairline at scroll position 4096" problem.

### 24. Tests are golden-image renders without diffing.

`make all-renders` regenerates the screenshots in `tests/screenshots/`. `make screenshot` writes a single screenshot. There is no automated pixel-diff or perceptual-diff comparing the output against a known-good baseline. The `cargo test` step does not gate on visual regressions; the README's claim that "`cargo test` passing matters more than any marketing sentence" is true, but `cargo test` here doesn't actually catch rendering regressions — it catches Rust unit-test regressions in your fixture code paths.

### 25. The reflow analysis doc is, frankly, misleading.

`docs/REFLOW_CRITICAL_NEXT.md` is written in the voice of a Mozilla principal and it lists "forced synchronous layout" as P0 and "fine-grained invalidation" as P1. That ordering is upside down. Without fine-grained invalidation, sync layout can't ship — every `getBoundingClientRect()` call from JS triggers a full document rebuild + image refetch. **That** is the death spiral. The document also tells you not to write your own CSS parser. You already are: `src/css/` is your own CSS parser. So the doc isn't internally consistent with the codebase it describes.

---

## P2 — Smaller-but-nontrivial issues.

- `src/css/length.rs` parses `px`, `%`, `rem`, `em`, `vw`, `vh` only. No `pt`, `pc`, `cm`, `mm`, `in`, `Q`, `ch`, `ex`, `vmin`, `vmax`, `svh`/`lvh`/`dvh`. No `calc()`. No `min()` / `max()` / `clamp()`. No unitless zero in many contexts (handled, barely, by the special-case `value == "0"`). No negative length acceptance check.
- `src/dom/ops.rs find_node_by_id` is O(N) tree walk. `getElementById` is supposed to be O(1) — every browser maintains an ID map.
- `src/dom/node.rs Node::Text(String)` represents text as one big String. Concatenated text nodes (two adjacent text fragments after a removeChild) aren't normalized.
- The user-agent stylesheet (`src/css/stylesheet.rs:21–33`) is one inlined string with about 30 declarations. The real HTML5 UA sheet is ~600 lines. Many tags will get default `display: block` from the fallback in `StyleMap.display_mode`, which happens to be what you want for many of them, but you're also defaulting `<a>` to inline only via your UA sheet — fine. `<table>`, `<tr>`, `<td>`, `<th>`, `<caption>`, `<thead>`, `<tbody>`, `<tfoot>`, `<col>`, `<colgroup>` all fall through to `Block`. So tables don't lay out as tables.
- `src/runner/pipeline.rs:42 let _ = crate::font::get_glyph_metrics('A');` is a side-effecting call that exists to warm up a `OnceLock`. Either the warming should happen in font module init, or the warm-up should be commented as such. As written it looks like dead code.
- `src/css/stylesheet.rs:23–24` UA sheet declares colors `accent`, `rust`, `coal`, `ink` — these are not CSS named colors. Either you have a custom palette resolver (I didn't grep deep enough to find one), or they fail to parse and fall back to the foreground color, in which case you're shipping an invalid UA stylesheet.
- `src/js_boa/network.rs:65–69` constructor `URL`: every property is the empty string. Calling `new URL("https://x.com/y").pathname` returns `''`. There are libraries that depend on URL parsing for routing (e.g., react-router doing `new URL(window.location.href).pathname`); they will silently take the wrong path.
- `src/js_boa/storage.rs` has no quota, no `storage` event, no JSON-vs-string discrimination, no `length` updating (it's set to 0 once at init and never updated — `:78`).
- `src/js_boa/runtime.rs:51–78 dispatch_event` re-borrows `nodes` to find the node id by linear scan (`:55–58`). Every event dispatch is O(N) over registered nodes. Maintain a reverse map keyed by `Rc::as_ptr(&node)`.
- `src/js_boa/runtime.rs:117–139 drain_animation_frame_callbacks` collects callbacks into a Vec then drops the borrow. Good. But it does not handle callbacks enqueued *during* a callback — those land in the freshly-borrowed `animation_frames` vec on the next frame, which is the right behavior, but it means `requestAnimationFrame(loop)` always gives you a one-frame gap, not a same-frame chain. That will surprise people porting code that expects rAF to run nested.
- `src/runner/scripts.rs` (not read but inferable from its callers): scripts are extracted by tree-walk, fetched if external, and concatenated into one big eval. There's no script element associated with the running script, no `document.currentScript`, no individual error reporting, no `src` re-entry guard.

---

## What the codebase does well

To be balanced — these are real:

- **Module structure is clean.** 180 source files, sensible boundaries, very few cross-module hot pairs. The `js_boa::accessors` split into `family/identity/layout/objects/text_html/style_class` is pleasant. Most files are under 200 lines. That's discipline.
- **The fetch layer is real.** TLS, redirects, capability gating, file:// fallback. It's narrow but correct on the happy path. It doesn't pretend.
- **Layout/style/DOM separation is conceptually right.** The pipeline `DOM → Style → Layout → Paint` is the standard browser pipeline. The pieces are wrong; the seams are well-placed.
- **GPU compositor via Vello is a reasonable bet.** You don't have to build your own compute rasterizer to ship.
- **Tests exist.** Unit tests in `src/layout/tests/`, `src/atlas/tests.rs`, `src/css/`, `src/paint/tests.rs`, etc. Many of them.
- **Identity / capability gating is interesting and unique.** The Bastion / Opus integration giving fetches an `Identity` with `NetworkAccess` / `ReadWorkspace` capabilities is a genuinely new angle. If the longer-term direction is "user-owned runtime surface," this is the right load-bearing decision.

The architecture's *bones* are okay. The flesh on the bones is wrong in many places.

---

## The "won't load any real page" reasoning chain

Pick a reasonably-stripped page from the wild — say, a mostly-static blog post.

1. The HTML parser silently produces a misshapen DOM the moment it hits a malformed tag, an unclosed `<p>`, or a `<table>` whose `<tr>` isn't wrapped in `<tbody>`. ➜ visible content shifts.
2. The cascade silently drops `>`, `+`, `~`, `[attr=val]`, every pseudo-class, every pseudo-element, and `!important`. ➜ most of the styling is wrong.
3. `display: grid`, `position: absolute|fixed|sticky`, `display: table*` are absent. ➜ most layouts are wrong.
4. `inline-block` is mapped to `inline`. ➜ navbars and badges are wrong.
5. Inline layout has no real fragmentation. ➜ wrapping looks wrong, line heights mis-stack, RTL and BiDi fail.
6. Atlas is Latin-1. ➜ any non-Latin glyphs vanish.
7. Shaper-glyph alignment is by index. ➜ ligature-heavy fonts mis-render.
8. Storage doesn't persist. ➜ login session lost on every reload.
9. fetch / XHR always fail. ➜ data-driven content empty.
10. Observers never fire. ➜ lazy-loaded images never load, sticky-on-scroll headers never stick.
11. `event` object is undefined inside listeners. ➜ click handlers that read `event.target` throw `Cannot read property 'target' of undefined` and the page goes silent.
12. Resize re-runs HTTP for every image. ➜ user resizes, the engine self-DoSes.

Any one of these makes the page wrong. All of them together make the page **invisibly** wrong — it loads, it renders something, the something is not the page.

---

## What I would actually do

If I owned this and needed it to render real pages within a year, in priority order:

1. **Replace the HTML parser.** Either adopt `html5ever` or commit to writing an HTML5-spec parser with insertion modes. Don't try to incrementally patch the current 90 lines.
2. **Replace the CSS parser and selector engine.** Use `cssparser` + a real selector matcher, or write one with the right grammar. Add the bucket-by-rightmost-simple-selector index. This unblocks selectors and the cascade.
3. **Make invalidation per-node.** Dirty bits on `LayoutNode` and `StyledNode`. Mark ancestors dirty on mutation. Walk only dirty subtrees in style recompute. *Then* talk about sync layout from JS. (`REFLOW_CRITICAL_NEXT.md` has this as P1; promote it to P0.)
4. **Stop owning `StyleMap` per `LayoutBox`.** Either use `Rc<StyleMap>` for sharing, or make `LayoutBox` borrow-from-StyledNode. Combined with #3 this kills most of the per-frame allocation.
5. **Pick a text/Unicode story.** Drop the prebuilt 256-codepoint atlas. Use a dynamic atlas with per-shape, per-size glyphs, fed by rustybuzz cluster info (not by character index). Add at least a fallback chain (default + emoji + CJK).
6. **Decide what "JS support" actually means.** Three honest options:
   (a) Drop JS entirely and ship a static renderer with extreme honesty about scope. This is a real product — `dillo` and `netsurf` exist for a reason.
   (b) Make a small set of APIs *real* (fetch, MutationObserver, basic Event objects, persistent storage) and remove the rest. Document what works and what doesn't. Test against pages that only use what works.
   (c) Commit to running real JS. Then dispatch real `Event` objects, implement bubble/capture, fire scroll/resize/load/DOMContentLoaded, persist storage, support fetch via the existing real fetch layer, give `URL` a real parser. This is months of work.
   The current "survive but lie" mode is the worst of the three because it makes the engine look like it works.
7. **Add visual regression diffing in CI.** Pixel-diff against `tests/screenshots/*.png`. Make `make all-renders` the *update* command, and a separate `make check-renders` the gate.
8. **Move image fetch off the resize/reflow path.** Off-thread, request-once, observed by an image-load callback that triggers an invalidation, not a synchronous block.
9. **Fix `inline-block` mapping and InlineBlock layout.** This is a one-day fix in `style_map.rs` and a few lines of layout code. Low-hanging.
10. **Either restore selectors > + ~ in the cascade or remove `descendant` from the README's claim list.** Right now the README ships claims that the code does not honor.

---

## Closing

The code is lovingly written. The module structure is one of the better Rust browser-engine layouts I've seen in a small project. The ambition is real and the bones are sound.

But the engine is, today, **a fixture renderer with a JavaScript-shaped object next to it**. The fixtures pass because the fixtures were written to fit what the engine can do. Real pages are full of things that the engine cannot represent — unsupported display modes, unsupported selectors, untokenizable text, ligatured shaping, persistent storage, event objects, observers, second-byte Unicode. Each of those, individually, is fixable. Collectively they mean that the next milestone — "render any non-fixture page correctly" — is not three weeks of polish. It's a structural rewrite of three or four modules.

The README's last line is right: "If you want to judge it harshly, judge it as an early browser engine and runtime prototype, not as a Chrome replacement." Judged as that, what I'd say is: it's a prototype that has demonstrated the pipeline shape. The next phase isn't more features — it's replacing the prototype implementations behind each pipeline stage with ones that survive contact with the web.

Don't extend. Replace. Then extend.

— Principal review, end.
