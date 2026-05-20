# Reflow Analysis for Aurora

> Reviewer: principal SWE, Blink rendering. Twelve years on the document lifecycle, layout invalidation, and "why is this page jank" investigations. Not a fan of cleverness in hot paths.

This document is a design review, not an implementation plan. The goal is to get the architectural decisions right *before* the first line of reflow code lands, because retrofitting an invalidation model onto a rendering engine that wasn't built for one is one of the most expensive mistakes you can make. I have personally watched a team eat 18 months trying to bolt a proper invalidation system onto a renderer that assumed one-shot layout. Don't do that.

---

## 1. What "reflow" actually is

In a real engine, reflow is not a function. It's a *phase* of the document lifecycle. In Blink the phases are roughly:

```
Style recalc  →  Layout  →  Compositing inputs  →  Paint  →  Composite
   (dirty bits propagate forward; nothing runs unless something upstream is dirty)
   ```

   What makes this work is not the recompute logic. The recompute logic is mechanical. What makes it work is:

   1. **Dirty bits.** Every node knows whether *it* is invalid and whether any *descendant* is invalid. `NeedsStyleRecalc` and `ChildNeedsStyleRecalc` are the two bits that matter for style; the analogous pair exists for layout. When you mutate the DOM, you walk *up* setting `ChildNeedsX` and *down* setting `NeedsX` only on the actually-affected subtree. Reflow then walks the tree and **skips any subtree where both bits are clean**. That's the whole performance story. Without dirty bits you re-layout the world on every keystroke and you ship a 4 fps browser.

   2. **A scheduler that batches.** JS does not synchronously trigger layout. JS dirties nodes; layout runs once, lazily, when someone *asks for a measurement* (forced layout) or when the next frame is about to paint. Aurora has no scheduler today, so this is greenfield — get it right the first time.

   3. **A boundary between "DOM mutated" and "layout was recomputed".** Most JS APIs only need the *DOM* to be current. A small set (`offsetHeight`, `getBoundingClientRect`, `getComputedStyle`, etc.) need *layout* to be current and force a synchronous flush. This boundary is where 80% of web perf bugs live.

   If you only take one thing from this doc: **the hard part of reflow is invalidation tracking, not recomputation.** Recomputation is just "run the existing pipeline again."

   ---

   ## 2. Where Aurora is today

   I read [main.rs:208-251](../src/main.rs#L208-L251) carefully. The current pipeline is:

   ```rust
   let dom = parse_html(...);
   run_all_scripts_synchronously(&dom);   // boa runs here, top-to-bottom, then exits
   let stylesheet = Stylesheet::from_dom(&dom, ...);
   let style_tree = StyleTree::from_dom(&dom, &stylesheet);
   let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, viewport);
   window::open(&layout, &image_cache);    // event loop starts AFTER layout is frozen
   ```

   This is a **batch compiler**, not a browser. Three concrete observations:

   - The JS engine ([js_boa.rs](../src/js_boa.rs)) already implements `appendChild`, `setAttribute`, `innerHTML` setter, `createElement`, `removeChild` — the mutation surface is real. But none of those mutations have anywhere to go. They write into the `Rc<RefCell<Node>>` DOM and nothing else observes them.
   - `setTimeout` and `requestAnimationFrame` are *registered* on globalThis ([js_boa.rs:303,321](../src/js_boa.rs#L303-L321)) but I'd bet money they're no-ops or stubs. Boa has no native event loop. There is no tick.
   - `LayoutTree` is built once from `&StyleTree` and handed to `window::open` as `&'a LayoutTree` ([window.rs:42,392](../src/window.rs#L42)). That `'a` lifetime is going to fight you the moment you try to rebuild layout from inside the event loop. Plan to break it.

   The good news: the static pipeline is clean. `StyleTree::from_dom` and `LayoutTree::from_style_tree_with_viewport_width` are pure functions of (DOM, stylesheet, viewport). That means a brute-force "rebuild everything on any change" reflow is a 50-line patch. That's also a trap — see §5.

   ---

   ## 3. The four architectural decisions you must make first

   Pick these now. Each one is a one-way door.

   ### 3.1 Where does the event loop live?

   You have two options:

   **(A) winit drives everything.** `RedrawRequested` is the heartbeat. JS callbacks, timers, and reflow all run inside `window_event`. This is what Servo does. Pros: single-threaded, no synchronization, debuggable. Cons: long-running JS blocks the UI thread (same as every browser, actually — accept it).

   **(B) JS runs on its own thread, posts mutations to the render thread.** This is what Chromium does at scale (renderer process / compositor process), but it's enormous overkill for Aurora and Boa isn't `Send` anyway.

   **Recommendation: (A).** Don't even think about (B) until Aurora has a reason to scale. The whole rendering crate is `!Send` because of `Rc<RefCell<Node>>` — you'd have to rewrite the DOM to make (B) work. Not worth it.

   ### 3.2 Coarse or fine invalidation?

   **Coarse:** one global `dirty: bool`. Any DOM mutation sets it. Next frame: rebuild StyleTree + LayoutTree from scratch. ~50 LOC.

   **Fine:** dirty bits per node, partial recompute, subtree-scoped invalidation. This is what real browsers do. Order of magnitude more code; hard to get right; mandatory if you ever want to render anything large at interactive framerates.

   **Recommendation: ship coarse first, then evolve.** Aurora's pages are small. A full rebuild of StyleTree+LayoutTree on a 200-node Google homepage is going to be sub-millisecond. The reason to do fine invalidation is performance, and you don't have a performance problem yet. **But** — and this matters — design the *API* such that fine invalidation can be slotted in without changing callers. Specifically:

   ```rust
   // Good: opaque, can grow into per-subtree invalidation
   document.invalidate_style(node);
   document.invalidate_layout(node);

   // Bad: leaks the coarse model into every call site
   document.dirty = true;
   ```

   If JS code calls `invalidate_layout(specific_node)` everywhere, the day you implement real subtree invalidation, you flip the implementation and the call sites don't change. If JS code sets a global flag, you'll have to rewrite every call site later.

   ### 3.3 Synchronous or asynchronous reflow?

   When JS does `element.style.width = "100px"; element.offsetWidth`, the second access *must* return the post-mutation width. That's a forced synchronous layout. Every browser hates them, every browser supports them.

   The decision: do you support forced layout from day one, or do you defer it?

   **Recommendation: defer.** Most JS that pages run does not do forced layout. `offsetWidth` and friends can return stale values for now (or NaN, or 0 — be loud about it). Add forced layout when a real page demands it. The reason to defer: forced layout requires re-entrant reflow (running layout from inside a JS call), which means your reflow code can't hold any borrows it isn't prepared to drop. That's a real constraint and `RefCell` will not be your friend. Punt until you have a use case.

   ### 3.4 What triggers a frame?

   Three triggers, in order of importance:

   1. **Timer fires** (`setTimeout`/`setInterval` callback ran and dirtied something).
   2. **`requestAnimationFrame` callback** ran.
   3. **User input** caused a script to run (later — input dispatch is its own can of worms).

   For (1) and (2) you need a real timer queue. Boa doesn't give you one. You'll write it: a min-heap of `(deadline_instant, callback_id)`, ticked from the winit event loop. winit has `ControlFlow::WaitUntil` — use it to wake the event loop at the next deadline so you don't busy-spin.

   ---

   ## 4. Phased implementation plan

   Five phases, each independently shippable. **Do not skip ahead.** Each phase builds the testbed for the next.

   ### Phase 0: Make the pipeline rebuildable (no JS yet)

   Before any JS work: prove that you can rebuild StyleTree + LayoutTree from a mutated DOM and re-paint, *without* tearing down the window. Today the layout is built once in `main.rs` and borrowed for the lifetime of `AuroraApp`. Break that.

   - Move ownership of `LayoutTree` *into* `AuroraApp`. Replace `&'a LayoutTree` with `LayoutTree` ([window.rs:392](../src/window.rs#L392)).
   - Add a method `AuroraApp::rebuild_from_dom(&mut self)` that re-runs `StyleTree::from_dom` and `LayoutTree::from_style_tree_with_viewport_width`.
   - Add a debug keybind (`R` for "reload") that calls it. Mutate the DOM by hand in a test fixture. Confirm the screen updates.

   This is the riskiest plumbing change because of the lifetimes. Get it landed and stable before introducing JS into the loop. **If you cannot do this cleanly, every phase below will be worse.**

   ### Phase 1: Microtask queue and timer queue

   In `js_boa.rs`, replace the stub `setTimeout`/`requestAnimationFrame` with real implementations that:

   - Push `(deadline, JsFunction)` onto a queue owned by the runtime.
   - Return an opaque ID.
   - Are drained by a new `BoaRuntime::tick(now: Instant)` method that runs every ready callback.

   `requestAnimationFrame` callbacks fire once, before the next paint. They go in a separate queue that's drained inside the frame, after timers, before reflow.

   The microtask queue is the queue Promises use. Boa has `Context::run_jobs()` — call it after every JS entry point to flush microtasks. Don't try to be smart about this.

   ### Phase 2: Invalidation API

   Add to `dom.rs` (or a new `document.rs`):

   ```rust
   pub struct Document {
       root: NodePtr,
           style_dirty: bool,
               layout_dirty: bool,
               }

               impl Document {
                   pub fn mark_style_dirty(&mut self, _node: &NodePtr) { self.style_dirty = true; }
                       pub fn mark_layout_dirty(&mut self, _node: &NodePtr) { self.layout_dirty = true; }
                       }
                       ```

                       Note the unused `_node` parameter. That's the seam for future fine invalidation. Call these from every JS DOM-mutation binding in `js_boa.rs` — `appendChild`, `setAttribute`, `innerHTML` setter, `removeChild`, `style.x = y`, etc. Audit them all. Missing one is a "ghost layout" bug that takes a week to find.

                       A reasonable rule of thumb:
                       - Anything that changes the tree shape → `mark_layout_dirty` (which implies style too).a
                       - Anything that changes attributes or inline style → `mark_style_dirty`.
                       - Text content changes → both.

                       Don't try to be precise. Coarse is fine.

                       ### Phase 3: The frame loop

                       Inside `AuroraApp`, on every `RedrawRequested` (or on a `WaitUntil` wake-up):

                       ```
                       1. now = Instant::now()
                       2. runtime.tick(now)                          // fire ready timers, run their callbacks
                       3. runtime.run_microtasks()                   // drain Promise queue
                       4. runtime.drain_animation_frame_callbacks()  // rAF
                       5. if document.layout_dirty || document.style_dirty:
                              rebuild StyleTree + LayoutTree
                                     document.clear_dirty_bits()
                                     6. paint
                                     7. schedule next wakeup at the earliest timer deadline (ControlFlow::WaitUntil)
                                     ```

                                     This is the heart of the engine. Keep it ten lines. Resist the urge to add hooks.

                                     ### Phase 4: Validate against a real page

                                     Pick *one* page that exercises the loop. Not Google — Google is a tarpit. Pick something like a static page with a single `setTimeout` that adds a `<p>` after 1 second. If that works, try a `setInterval` that updates a counter. Then try a `requestAnimationFrame` loop that translates an element. *Then* go look at a real page.

                                     Premature contact with google.com will demoralize you. The page issues hundreds of fetches and runs MB of obfuscated JS that assumes APIs you don't have. It is not a useful test.

                                     ---

                                     ## 5. Pitfalls (the stuff that will actually bite you)

                                     **The `RefCell` panic loop.** The DOM is `Rc<RefCell<Node>>`. JS bindings borrow nodes mutably to mutate them. Reflow code borrows the same nodes immutably to read them. If reflow runs *while* a JS borrow is live (re-entrancy), you get a runtime panic. The fix: **never run reflow from inside a JS callback.** Reflow only runs at the top of the frame, when no JS is on the stack. This is non-negotiable — design the API so the only entry point is `Document::flush_pending_layout()` and only call it from the frame loop.

                                     **Forgotten invalidation.** Every JS binding that mutates the DOM must call `mark_*_dirty`. There are a lot of them in `js_boa.rs`. Add a `#[must_use]`-style discipline: mutation methods return a "you must invalidate" token, or run a CI lint. I have seen "ghost layout" bugs eat sprints.

                                     **Style recalc cost.** A full `StyleTree::from_dom` rebuild walks the DOM, re-matches every selector against every rule. On the kind of CSS Google ships (thousands of rules), this is the expensive operation, not layout. If frame times get ugly, that's where to look first — not at layout, not at paint.

                                     **Timer drift.** `setTimeout(fn, 0)` does not mean "run on next tick." It means "run after at least 0ms, at the next opportunity." If you treat zero-delay timers as "run immediately" you'll re-enter JS in surprising places. Always queue, always tick from the frame boundary.

                                     **The "infinite reflow" loop.** A `requestAnimationFrame` callback that mutates the DOM dirties layout. Reflow runs. Paint happens. Next frame, rAF fires again, dirties again. This is fine — that's exactly how animation works. But: a `setTimeout(fn, 0)` callback that *also* schedules another `setTimeout(fn, 0)` will starve rAF if you drain timers without a budget. Cap timer drains per frame (say, 100 callbacks max).

                                     **Layout stability.** Aurora's layout currently allocates fresh layout boxes on every build. That's fine for correctness. It will become a perf issue much later. Don't optimize it now. The right answer is incremental layout (reusing boxes whose inputs didn't change) and that is a months-long project. Out of scope.

                                     ---

                                     ## 6. What I would explicitly NOT do

                                     - **Don't build a "virtual DOM" or diffing layer.** You have a real DOM. Mutate it. The whole point of dirty bits is to avoid diffing.
                                     - **Don't try to make reflow incremental yet.** Coarse rebuild. Profile. If frame times are bad, *then* localize.
                                     - **Don't implement `MutationObserver`.** It's a JS-visible API for observing DOM mutations. Aurora has no use case for it. Skip it for two years.
                                     - **Don't add a separate compositor thread.** Vello already runs paint on the GPU. The compositing model in Chromium exists for reasons that don't apply to a single-window engine.
                                     - **Don't try to implement forced synchronous layout (§3.3) until you have a real page that needs it.** It's the single biggest source of complexity in the document lifecycle.
                                     - **Don't put style invalidation logic in the JS bindings file.** `js_boa.rs` is already 3,300 lines. The bindings should call `document.mark_*_dirty()` and stay dumb. The invalidation logic lives in `dom.rs` / a new `document.rs`.

                                     ---

                                     ## 7. Estimate

                                     For a competent Rust engineer who already knows this codebase:

                                     - Phase 0 (rebuildable pipeline, lifetime surgery): **2-3 days**. The lifetime work is the unknown — could be half a day, could be a week if `LayoutTree` borrows from `StyleTree` in ways that resist re-creation.
                                     - Phase 1 (timer + microtask queues): **2 days**.
                                     - Phase 2 (invalidation API + binding audit): **1-2 days**. The audit is the long pole.
                                     - Phase 3 (frame loop): **1 day** if Phase 0-2 are clean, **a week** if they aren't.
                                     - Phase 4 (validation): **2-3 days** to get a real page working end-to-end and shake out the inevitable re-entrancy bugs.

                                     **Total: 8-13 days for a working dynamic layer.** That gets Aurora to "renders pages with `setTimeout`-driven content updates correctly." It does *not* get Aurora to "renders Google." That's still gated on external CSS, image fetching, and the fact that Google's JS will hit a hundred unimplemented APIs the moment it runs.

                                     But it is the unlock. After this, "render google" becomes a JS surface-area problem, not an architecture problem. And surface-area problems can be ground down. Architecture problems compound.

                                     ---

                                     ## 8. One last thing

                                     The reason most browser-engine side projects die is not that the layout code was wrong. It's that the author tried to render google.com on day one, failed, and concluded the project was hopeless. Aurora has the opposite problem on its hands now — the static rendering is genuinely good for what it is, and the temptation will be to stretch it further. Don't. Build the dynamic layer on a synthetic test page first. The day you point a working frame loop at a real page and watch a `setTimeout` fire and the DOM update on screen is the day Aurora becomes a browser engine instead of a renderer.

                                     Good luck. Page me on the lifetime work in Phase 0 if it gets weird; that's the only part of this that I'd expect to surprise you.
                                     