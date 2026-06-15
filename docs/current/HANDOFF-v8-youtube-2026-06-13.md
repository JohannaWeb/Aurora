# Handoff: V8 YouTube compatibility

**Date:** 2026-06-13 (updated)
**Status:** YouTube now boots to completion and **renders a screenshot** (clean
`EXIT=0`). It paints the page shell — masthead chrome + loading spinners/circles —
but the main content (`ytd-rich-grid`) does not stamp yet. Next frontier is
content-renderer template/data hydration, not a crash or hang.

## Resolved this session

### 1. `yt-attributed-string` "Function props must be configured as STATIC, not SIGNAL."
Root cause: our bootstrap installs a **callable** fallback `style` on
`Object.prototype` (added earlier for `yt-image`). YouTube's `setUpProps` reads
every declared prop off `rawProps`, which walks the prototype chain; for the
declared `style` prop `rawProps.style` resolved to that inherited callable and
tripped the SIGNAL-vs-STATIC check.

Fix (`src/js_polyfills/custom_elements.js`): the instance `setUpProps` hook wraps
`rawProps` in a Proxy that neutralizes any function read for a prop key
(resolving it to its unset value), while leaving genuine `Object.prototype`
builtins (`toString`, `hasOwnProperty`, …) intact. Note: the builtin allow-list
must be probed with `hasOwn.call(...)`, because a plain `{}` lookup table itself
inherits the polluted `style` getter. Regression test:
`js_v8::runtime_tests::v8_attributed_string_setup_props_tolerates_inherited_callable_style`.

### 2. Event-loop pump never advanced time (`src/runner/pipeline.rs::pump_ready_work`)
The old pump ran 8 iterations against `Instant::now()`, so any `setTimeout(_, N>0)`
never came due and time-deferred boot work stalled. Rewrote it as a small
event loop on a **virtual clock** that fast-forwards to the next scheduled timer,
throttles `requestAnimationFrame` to one batch per ~16ms virtual frame, and stops
at quiescence or a budget (`VIRTUAL_BUDGET` 2s page-time / `REAL_BUDGET` 5s wall).

### 3. O(N²) DOM operations — added parent pointers
Backtraces showed boot pegged in full-tree scans because the DOM had no parent
pointer. Fixed several layers:
- `src/js_v8/selectors/query.rs`: `query_all`/`query_first` now thread the
  ancestor chain + sibling info **top-down** through one DFS (`collect_matches`)
  instead of recomputing each node's ancestors via repeated `find_parent` scans.
- `src/dom/node.rs`: added `parent: ParentLink` (a `Weak` newtype with trivial
  `PartialEq`/`Eq`) to `ElementNode`, plus helpers `set_parent`/`clear_parent`/
  `parent_ptr`/`link_children`/`reparent_subtree`.
- `V8Runtime::new` calls `reparent_subtree` on the initial tree; the js_v8
  mutation primitives (`append/insert/remove/replace_child_ptr`, `innerHTML`,
  `replaceChildren`) maintain the pointer incrementally.
- `find_parent` treats the pointer as **authoritative** (`None` ⇒ detached, no
  scan), with a verifying scan-and-repair fallback only for a stale pointer.
- `isConnected` now walks up via `mutation::is_connected_to` (O(depth)) instead
  of `contains_ptr` scanning the whole document.

All 95 V8 tests + 15 SpiderMonkey tests pass; shared-DOM change does not affect
the default backend.

### 4. `appendChild`/`insertBefore` didn't move nodes (THE hang fix)
The `yt-icon` infinite loop (below) was an insertion-semantics bug: inserting a
node that already has a parent must first detach it from that parent (DOM move
semantics). `append_child_ptr`/`insert_before_ptr`/`replace_child_ptr` just
pushed, so `fragment.appendChild(div.firstChild)` left the node parented in both
places and `div.firstChild` never changed — spinning YouTube's
`while (el.firstChild) frag.appendChild(el.firstChild)` icon clear-loop forever.
Fixed with `detach_from_parent` (uses the new parent pointer) at the front of
each insertion primitive. Regression test:
`js_v8::runtime_tests::v8_append_child_moves_node_out_of_previous_parent`.

After this, YouTube boots to completion and renders. 96 V8 tests pass.

## Resolved (earlier this session): infinite loop in `yt-icon` rendering

After the fixes, boot reaches deep into the tree (`ytd-watch-flexy`,
`ytd-watch-next-secondary-results-renderer`, playlist panels) with no
exceptions, then hangs **deterministically** (same point in debug and release).

Diagnosis (via gdb SIGINT sampling + a temporary `new Error().stack` dump in
`create_js_node`):
- The hot loop is **identical across all samples**:
  `renderIcon` → `qT3.program_` (a generator) → `_.d.applyIconShape`.
- `create_js_node` is entered ~21M times in 70s, but `registered_nodes` stays
  flat at **1908** — so **no new nodes are created**. It is a *traversal* loop
  re-walking the same fixed node set, not a render explosion.

Conclusion: YouTube's `yt-icon` `applyIconShape` generator walks the DOM
(`firstChild`/`nextSibling`/`children`/attributes) expecting a condition that
never becomes true in our engine — i.e. some DOM accessor returns a subtly wrong
value that breaks the generator's exit condition.

## Current frontier: the initial navigation never fires (no content page)

The page renders its shell but the home feed is empty; live, you only see the
masthead hamburger + avatar/spinner circles.

DOM after boot: `ytd-app > ytd-page-manager` exists, plus `ytd-masthead` and
`ytd-mini-guide` — but **`ytd-browse` is defined and never instantiated**, and
`ytd-page-manager` has **0 children**. So the home page component is never built.

Post-boot probe findings (inject after `fire_load`, gated on AURORA_DEBUG_YOUTUBE):
- `window.ytInitialData` IS present and populated (`responseContext, contents,
  header, trackingParams, topbar, frameworkUpdates`) — the home feed data is
  right there.
- `ytd-app` exists; `app.data` is `undefined` (it's a computed prop, not the
  page data). Its handlers exist: `onYtNavigateStart/Finish`, `onYtNavigate`,
  `handleNavigate`, `onYtPageDataFetched`, `onYtPageManagerAttached`, …
- During the **entire** boot, YouTube's own code makes **zero** navigation
  attempts (no `navigate`/`loggingUrls`/`page-manager-attached` traces; only one
  JS exception total, and that was the injected probe). So the initial-render
  navigation simply never triggers.
- Raising the pump budgets to 30s does NOT cause it to fire — not a timing issue.
- Calling `app.handleNavigate({detail})` manually DOES drive the real nav
  (`o7.navigate`) but throws `Cannot read properties of undefined (reading
  'loggingUrls')` — a deep, obfuscated nav/VE-logging data contract.
  `onYtNavigateFinish`/`onYtPageDataFetched` called directly don't throw but
  also don't populate (they early-return on an unmatched `detail` shape).

So content rendering is blocked on **YouTube's initial navigation not firing**,
and forcing it manually requires reverse-engineering the obfuscated navigation +
VE-logging + data-binding contract layer by layer. This is a large, open-ended
effort distinct from the crisp bug-fixes above.

### 5. Event system was stubbed (now fixed for direct DOM use)
A comprehensive post-boot probe (shadow roots / template.content / importNode /
dom-repeat / MutationObserver / rAF / event delivery) established:
- ✅ Template stamping works: `ytd-app` has a populated shadow root (15 kids);
  `attachShadow`, `template.content` (DocumentFragment), and `importNode` deep
  clone all work. rAF fires. `ytInitialData` is present and full.
- ❌ **Only 7 of 1261 elements have shadow roots**; `ytd-browse`/`ytd-rich-grid`/
  `dom-repeat` count is 0 — the content page is never instantiated.
- ❌ **MutationObserver never fires** (confirmed stub).
- ❌ **Event delivery was broken**: `window/document.dispatchEvent` were no-op
  polyfill stubs, and element dispatch didn't bubble.

Fixed the event system: native `dispatchEvent` for window/document (fires id-0
listeners) in `runtime.rs`, and proper bubbling in `node_dispatch_event` (walks
ancestors via the parent pointer, then document/window id-0, respecting
`bubbles`/`cancelBubble`). Verified by `v8_dispatch_event_fires_window_and_
document_listeners` and `v8_dispatch_event_bubbles_to_ancestors_and_document`.

**But this did NOT unblock content.** A synthetic in-context probe
(`elem.dispatchEvent` / `window.dispatchEvent` with a `document` listener) still
shows `docGot=false winGot=false` *after YouTube boots*, even though the same
calls pass in an isolated runtime and the native fns still report `[native
code]`. So **ShadyDOM/Polymer re-patches event dispatch during boot** in a way
that bypasses our native path in the flat-global environment (no real
`Window`/`EventTarget` prototype chain for ShadyDOM to harvest — see the comment
near `v8_post.js:465`). YouTube's own dispatches mostly don't hit our global fn
(only `spfready`/`undefined` did during a whole boot).

### 6. Real MutationObserver on V8 (done)
Implemented `src/js_v8/mutation_observer.rs`: native `MutationObserver` ctor +
`observe`/`disconnect`/`takeRecords`, childList/attributes/subtree matching
(subtree via the parent pointer), records delivered when the pump calls the new
`JsRuntime::deliver_mutation_records` (default no-op for other backends). Queue
hooks added to the js_v8 mutation callbacks (append/insert/remove/replace child,
`.remove()`, set/removeAttribute). `registry.document` is now populated so
records can build target/node wrappers. Tests:
`v8_mutation_observer_reports_childlist_and_attributes`,
`v8_mutation_observer_subtree_observes_descendants`. 100 V8 tests pass.

**Did NOT unblock content.** Probe: YouTube *does* construct 3 observers and call
`observe()` 3× — but all with `childList=false attributes=false subtree=false`,
and 0 records are ever delivered during boot. Polymer-on-**ShadyDOM does its core
child tracking synchronously through its patched `appendChild` etc., not via
MutationObserver**. So MO is now correct/working but isn't the content gate.

### 7. EventTarget rework (done) — real JS event model
Replaced the stub `EventTarget` with a real one (`v8_base.js`): per-object
listener storage (`__ael`), and `dispatchEvent` runs capture/target/bubble phases
over the live path (walked via `parentNode`, extended to `document` + `window`),
honoring `bubbles`/`capture`/`once`/`stopPropagation`/`stopImmediatePropagation`.
- `window`/`document` now use the EventTarget methods (`v8_post.js`).
- Element wrappers no longer install native `add/removeEventListener/
  dispatchEvent`; `create_js_node` sets each wrapper's prototype to the DOM chain
  (`set_dom_prototype` → `HTMLElement`/`Node`/`DocumentFragment` → … →
  EventTarget). Early wrappers (document/body/head/documentElement, built before
  the JS skeletons exist) are re-linked to `HTMLElement.prototype` right after
  bootstrap.
- `fire_lifecycle_event` dispatches `DOMContentLoaded`/`load` through JS now.
- Tests: `v8_event_target_capture_once_and_remove`, plus the existing
  window/document/element dispatch + bubbling tests. **101 V8 tests pass.**

Effect on YouTube: element→document bubbling now works (verified), hydration depth
unchanged (114 vs 113 connectedCallbacks, no regression), and `ytd-watch-flexy`'s
`ready()` now progresses *further* (hits a new `this.$.X` id-map gap —
`addEventListener` of undefined — a "got further" symptom, not a regression).
**Still no content** — navigation never fires, so the EventTarget rework was
necessary infrastructure but not sufficient. (Note: the shadow-root template in
`attach_shadow` still uses the legacy registry path; secondary, pre-existing.)

## Bundle trace (2026-06-14): the complete home-load chain, mapped from kevlar.js

Captured YouTube's real scripts with the Chrome UA (`/tmp/yt_home.html`,
`/tmp/kevlar.js` = 9.8 MB app bundle, `/tmp/scheduler.js`) and traced the entire
home-page content-load path. **This is the definitive map.**

The inline HTML defines `window.getInitialData()` → returns
`{page:'browse', endpoint:{…browseEndpoint}, response:<ytInitialData>, url}`.
**Confirmed present and correct in our runtime.** The bundle reads it via
`window.getInitialData`/`getDataPromise` (NOT `ytInitialData` — that string is 0×
in kevlar).

Home-load chain (all names from kevlar.js):
1. App init runs `c.install(qJp)` (the home lifecycle; `lO$`/`rUN` is the
   experiment-on variant, `kW3`/`He$` is `/shorts`, `dgV` is `/watch`). Gated on
   `SHELL_LOAD || kevlar_fetch_initial_data_promise_client || sw_nav_preload_pbj`
   — all OFF (even on real YT), so the `else c.install(qJp)` branch is normal.
2. `qJp.initialized` → `jA_ = cUy()` (cUy = `t_q(navService, location.href, …)`,
   the data load).
3. `qJp.rendering` → waits `jA_` → `uS(c, data)` → **`c.root.loadData(data)`**
   (c.root = ytd-app).
4. `ytd-app.loadData(c)` = `this.loadDepsPromise.then(() => { … if(c.response)
   _.esx(navMgr, c.endpoint, c, 5, {}) … })`. **`_.esx(...)` is the
   navigate-with-prefetched-data call that creates `ytd-browse`.**
5. `loadDepsPromise = _.TrF([_.U9(), pageManagerAttachedPromise.promise])`.
   `_.U9()` **resolves immediately**; `pageManagerAttachedPromise` resolves in
   `onYtPageManagerAttached` when `_.Qc(event).id === "page-manager"` (fired when
   the page-manager attaches).

What I verified at runtime (probe, now removed):
- `window.getInitialData()` returns the correct `{page,endpoint,response,url}`.
- `app.loadData(d)` exists and runs **without error**, but no page appears.
- `app.handleNavigate({command:d.endpoint})` (with the *real* endpoint) **no
  longer throws `loggingUrls`** — earlier failures were from a hand-built endpoint
  and from passing `{detail}` instead of `{command}`.
- Resolving `app.pageManagerAttachedPromise.resolve()` did **not** make the page
  appear.
- Microtasks DO drain (verified with a scratch test:
  `Promise.resolve().then().then()` reaches 2).

So the data path and the entry function (`loadData`) work, but the
promise/lifecycle chain that ends in `_.esx` doesn't complete. The lifecycle
plugins (qJp's `initialized`/`rendering`) run via a **`T2` state machine**
(`T2.prototype.transition` runs plugin phase callbacks on state transitions like
`application_navigating`). The natural chain never starts because that initial
state transition never fires — and `getInitialData` is still defined after boot
(the bootstrap would set it to `void 0` once consumed), proving the consume-step
never ran.

**Remaining unknowns (where to resume):**
- Why the `T2` lifecycle state machine's initial transition (which would run
  `qJp.rendering` → `loadData` → `_.esx`) never fires. This is the natural
  trigger; everything downstream of it works or is reachable.
- Why manually calling `loadData` + resolving `pageManagerAttachedPromise`
  doesn't complete `loadDepsPromise.then` → `_.esx`. Likely the live
  `this.loadDepsPromise` is the combined `_.TrF` and our `.resolve()` on
  `pageManagerAttachedPromise` doesn't propagate through YouTube's custom `_.Ht`
  promise type, OR `_.esx`/`navMgr` (closure-private, `_` not global) needs more.
- Concrete next experiment: after `loadData`, dispatch the real
  `yt-page-manager-attached` event the way the page-manager does (so
  `onYtPageManagerAttached` resolves the gate through the same object), and pump;
  failing that, find what advances the `T2` state machine on app startup.

All `_`-namespaced internals (`_.esx`, `_.Mt`, `_.TrF`, the nav manager) are
closure-private — not reachable from the global scope — so driving them requires
either matching the natural lifecycle trigger or patching inside the bundle.

## Navigation trace (2026-06-14): data path works, page creation is closure-private

Drove the real handler sequence post-boot (gated on AURORA_DEBUG_YOUTUBE):
- Dumped `ytd-app` handler sources. `onYtPageDataFetched(c, e)` sets
  `this.data = e.pageData` and `this.dataUpdatePromise = ...updatePageData(data)`.
  It takes **two args** (event, detail) — earlier one-arg `{detail}` calls were
  why it looked like a no-op.
- `app.onYtPageDataFetched(evt, { pageData: window.ytInitialData })` **works**:
  `app.data` becomes the object and `dataUpdatePromise` is created. **The data
  path is fine.**
- `onYtNavigateFinish(c, e)` just does `this.dataUpdatePromise.then(() => Wy8(...))`
  (Wy8 = render). But after both handlers + pump, page-manager still has **0
  children** and no `ytd-browse`. Reason: `updatePageData` updates an *existing*
  page; the page itself is created by the navigation (`o7.navigate`), which never
  ran.
- `handleNavigate(c)` = `_.Mt().resolve(_.qg).navigate(c.command, ...)`. It needs
  `c.command` (a nav command) and triggers a *fresh* navigation. `o7.navigate`
  throws `Cannot read 'loggingUrls' of undefined` deep inside (`fKH@3213`), an
  opaque internal VE/logging contract.
- The navigation manager (`o7 = _.Mt().resolve(_.qg)`) is **closure-private**:
  `_` is not reachable from the global scope, so it can't be introspected or
  driven correctly from outside.
- The app DOES register listeners for `yt-navigate`/`yt-navigate-finish`/
  `yt-page-data-fetched`/`yt-navigate-start` (the EventTarget rework made these
  real — confirmed via `app.__ael` keys). But YouTube **never fires its own
  initial navigation** during boot.

**Precise boundary:** page creation (`ytd-browse`) goes through YouTube's
closure-private navigation manager with an opaque internal data contract, AND the
bootstrap that would fire the initial navigation never runs. Both block content,
and both are inside the minified bundle we can't reach from outside.

Realistic paths from here: (1) capture the multi-MB minified bundle and read the
init/navigation path to learn what fires the first nav and what `o7.navigate`'s
contract is (painful but definitive); (2) accept that full desktop-Polymer
YouTube content is out of reach for now and validate the rendering pipeline on a
lighter target. The shell renders; the data path works; the gate is YouTube's
encapsulated navigation kickoff.

## (Earlier framing) navigation never fires

NOTE — a probe overturned the earlier "ShadyDOM patches the prototype chain"
theory. In-context findings:
- `window.ShadyDOM.inUse === true` but **`ShadyDOM.noPatch === true`** — ShadyDOM
  does NOT patch the DOM/event prototypes. So a `Node`/`Element` prototype-chain
  rework would NOT help; that lead is a dead end.
- `window === globalThis`, `window.dispatchEvent === __shady_native_dispatchEvent`
  and both report `[native code]`, AND `window.addEventListener` is still our
  native (it registers: `add_event_listener` fires for test events).
- BUT `window.dispatchEvent(new CustomEvent('zz'))` **does not call our
  `dispatch_event_global`** (only 3 global dispatches in the whole boot, for
  `spfready` and `undefined`). So late in boot `globalThis.dispatchEvent` is
  reassigned to a different native-looking function that drops events. A fresh
  `window.addEventListener` + `window.dispatchEvent` round-trip fails even though
  the same pattern passes in an isolated runtime.
- YouTube dispatches mostly on **elements** (Polymer `fire()`), not on window.
  Element dispatch (`node_dispatch_event`) is per-wrapper native and not globally
  reassignable, and now bubbles via parent pointers — but an earlier probe showed
  element→document delivery also failing, likely because `event.bubbles` isn't
  read from YouTube's (possibly redefined) event objects.

So the event layer has at least two distinct faults: (a) global `dispatchEvent`
reassigned/neutered post-boot; (b) element-dispatch bubbling possibly not reading
`bubbles`/`type` from YouTube's event objects. Both must work for Polymer's
navigation events to reach their listeners and fire the initial navigation.

Investigated target (2): instrumented `node_dispatch_event`. Findings:
- Only **31 events reach our element dispatch in the entire boot** — YouTube's
  event system is overwhelmingly **self-contained** (Polymer/ShadyDOM route
  events internally on their own objects, not through our native dispatch).
- All 31 are `ctor=CustomEvent` with own keys `[detail, __composed, target]` and
  **no readable `type`** (`event.type` reads `undefined`, `has_own_property('type')`
  is false). So our dispatch can't match listeners by type for them.
- Made our `Event` constructor robust (always init `type`/`bubbles`/`composed` on
  `this`, set `cancelBubble` from `stopPropagation`) — kept as a correctness fix,
  but it did NOT change YouTube's events: they don't chain through our `Event`
  polyfill. They're fully YouTube/Polymer-internal and opaque to us.

**Conclusion (architectural boundary):** YouTube content rendering is gated on
deep Polymer/ShadyDOM event-model integration. The custom elements need real
`EventTarget` semantics that Polymer's internal event routing expects; our flat,
per-wrapper DOM doesn't provide that, and YouTube's events bypass our native
dispatch entirely. This is a substantial architectural effort (real EventTarget +
Polymer event-flow compatibility), not a bounded fix — confirmed now from three
angles (events, MutationObserver-not-used, navigation-never-fires).

Realistic next direction (larger): give DOM nodes real `EventTarget` behavior
that Polymer/ShadyDOM's event system composes with — i.e. event listeners stored
per-node and a dispatch that Polymer's `fire()`/`__shady` paths route through —
OR pursue a fundamentally different strategy (e.g. server-rendered HTML snapshot,
or a lighter target than the full desktop Polymer app). Ties into the PAGE_TOKEN /
data-injection note in [[v8-youtube-hydration-status]].

## Verification
- `cargo test --no-default-features --features v8` → 95 pass.
- `cargo test js_sm::runtime_tests` (default backend) → 15 pass.
- Live probe: `AURORA_DEBUG_YOUTUBE=1 AURORA_HEADLESS=1 AURORA_SCREENSHOT=/tmp/yt.png ./target/debug/aurora https://www.youtube.com/`
  (hangs in the icon loop; no screenshot yet).
