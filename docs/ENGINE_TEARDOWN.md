# Aurora — A Brutal Engine Teardown

> Reviewer's framing: I was asked to look at this the way I'd look at a chunk
> of Chromium that landed without review. No diplomacy, no participation
> trophies. Everything below is backed by something in the tree, with
> file:line where it matters. Treat it as a punch list, not an insult — the
> bones are more interesting than the current state of the muscle.

Date: 2026-06-16 · Branch: `feature/continued-youtube-support` · ~37.5k LOC Rust

> **Errata (corrected):** the original draft of §3 claimed SpiderMonkey was the
> only wired engine and V8/Boa were dead code. That is backwards. The default
> build is `default = ["v8"]`; the engine modules are mutually-exclusive
> features, so in a default build **only `js_v8` compiles and only V8 is
> wired** — `js_sm`/`js_boa` aren't in the build at all. The `SmRuntime::new`
> the draft cited at `pipeline.rs:947` is inside a `#[cfg(feature="engine-sm")]`
> unit test, not the pipeline. §3, §4, §5, §10 and the appendix are corrected
> accordingly below.

---

## Implementation status — 2026-06-16

Work done this session on `feature/continued-youtube-support`. All changes are
in `src/js_polyfills/`.

### What was built

**`custom_elements.js`** — the core custom-elements shim grew substantially:

- **ShadyCSS-lite** (`shimDomModuleStyles` / `scopeCss` / `rewriteScopedSelector`):
  hoists each `<dom-module>` `<style>` into `<head>` and rewrites `:host` /
  `::slotted` / `:host-context` selectors to target Aurora's flattened light DOM.
- **Polymer data-binding shim** (`parseBindingParts` / `collectStampedBindings` /
  `applyStampedBindings` / `installBindingHooks`): after `ready()`, walks the
  stamped subtree for literal `[[prop]]` / `{{prop}}` annotations Polymer's own
  `_bindTemplate` left unreplaced, resolves them against `el.__data` / the
  element, and re-applies on every `_propertiesChanged` call.
- **`on-*` event binding** (`wireEventHandlers`): after `ready()`, walks the
  stamped subtree and wires every `on-*` attribute to the host element's instance
  method via `addEventListener`.

**`polymer_shim.js`** — new file, loads after `v8_post.js`:

| Piece | What it does |
|---|---|
| `POLYMER_PROTO` | 20+ utility methods (`fire`, `$$`, `async`, `debounce`, `set`, `notifyPath`, `push`/`pop`/`splice`, `toggleClass`, `listen`, `resolveUrl`, …) installed on every registered element prototype |
| `dom-repeat` | Clones `<template>` child once per item, substitutes `[[item.*]]` bindings statically, inserts stamped nodes as siblings, triggers custom-element upgrade on stamped descendants |
| `dom-if` | Stamps template on `if=true`, removes on `if=false`, triggers upgrade |
| `Polymer.dom()` | Full DOM-API wrapper with `observeNodes`/`unobserveNodes` via MutationObserver |
| `Polymer.RenderStatus` | `afterNextRender` / `beforeNextRender` / `whenReady` via `queueMicrotask` |
| `Polymer.Async` | `microTask`, `timeOut`, `animationFrame`, `idlePeriod` schedulers |
| `Polymer.Element` | Stub base class so `class Foo extends Polymer.Element` doesn't throw before YouTube's bundle defines the real one |
| `Polymer.LegacyElementMixin` | Stub mixin factory |
| `Polymer.mixinBehaviors` | Copies behavior properties onto target prototype |
| `Polymer.dedupingMixin` | Identity passthrough |
| `Polymer({is:…})` | Legacy factory stub for pre-class registration syntax |
| `customElements.define` wrapper | Installs all utility methods on each ctor prototype at registration time |

**`runtime.rs`** — `bootstrap_blocks` array extended from 6 to 7 to include
`polymer_shim.js` (loads last, after `v8_post.js` so `document` is live).

### What's still needed to load YouTube

These are the remaining gaps, in rough priority order:

1. **`template.content` stamping by Polymer's own `_stampTemplate`** — our
   polyfill patches the environment, but Polymer's real `_stampTemplate` (in
   YouTube's bundle) calls `document.importNode(template.content, true)` and then
   walks the result for annotation nodes. If the walk misses nodes (e.g. because
   Aurora's `querySelectorAll` on a DocumentFragment doesn't recurse into nested
   templates), Polymer's binding infrastructure silently produces no bindings.
   Needs a targeted test: stamp a Polymer template and verify bindings fire.

2. **Property observer and computed-property callbacks** — Polymer's
   `properties: { foo: { observer: '_onFoo' } }` and `observers: ['_x(a,b)']`
   declarations are handled by Polymer's own `_propertiesChanged` once the bundle
   loads, but only if `_enableProperties()` is called. If `__dataEnabled` stays
   `false` (because `_initializeProperties` is never called, or because our
   upgrade path calls `connectedCallback` before Polymer's own `created`/`ready`
   sequence sets up the data system), observers never fire. Check the
   `__dataEnabled` / `__dataReady` flags in probe output.

3. **`<slot>` distribution** — Aurora flattens shadow to light, so `<slot>`
   elements land in the light DOM as literal `<slot>` tags. Polymer's
   `_attachDom` calls `Polymer.dom(root).appendChild(dom)` which we forward to
   native; but light-DOM children that should be distributed into a slot just sit
   before the slot element instead of inside it. For YouTube's masthead / drawer
   this causes mis-ordered DOM. A `<slot>` shim that moves light children into
   the slot's position would fix it.

4. **`document.createTreeWalker` on template content** — Polymer 2's
   `TemplateStamp._parseTemplateAnnotations` uses `createTreeWalker` to walk
   template nodes looking for binding annotations. Our `createTreeWalker` in
   `v8_post.js` exists, but verify it recurses into `DocumentFragment` children
   correctly (the `root` parameter is `template.content`, a fragment).

5. **Attribute reflection → property** — Polymer's `_attributeToProperty` syncs
   HTML attributes to properties when `attributeChangedCallback` fires. Our
   upgrade path doesn't invoke `attributeChangedCallback` when attributes are
   already set on the element at upgrade time (only future mutations go through
   `MutationObserver`). Static attributes like `disabled="[[standalone]]"` that
   were never resolved will stay as literal strings on the element without this.

6. **`Polymer.Gestures` / touch event shims** — `ytd-app` registers gesture
   listeners at boot. Without `Polymer.Gestures`, those calls throw. A stub
   `{ addListener: ()=>{}, removeListener: ()=>{} }` is enough to not crash.

7. **`iron-*` / `paper-*` element stubs** — YouTube's boot sequence touches a
   handful of `iron-iconset-svg` and `iron-meta` elements before the main bundle
   defines them. If they're not registered (or not upgraded), property accesses on
   them throw. A catch-all `rememberPending` already queues them; verify they
   actually get upgraded once their definitions arrive.

---

## TL;DR

Aurora is a Rust browser engine carrying **three** JavaScript engine bridges
(SpiderMonkey, V8, Boa), **three** DOM representations (a hand-rolled legacy
DOM, blitz-dom, and SpiderMonkey's own object graph), and **two-and-a-half**
HTML/CSS stacks — and it renders by **serializing the whole document to an HTML
string and reparsing it from scratch on every reflow**. The headline goal
("run YouTube") is chasing the single hardest target on the web with an
architecture that cannot survive a single animation frame at scale.

The fundamentals (capability-gated fetch, GPU-first paint via Vello, a real
JIT) are reasonable bets. The execution is drowning in duplicated bridge code
and a data model that fights itself. **~62% of the codebase (23k of 37.5k LOC)
is JS-bridge churn, and roughly half of that is wired to nothing.**

---

## 1. The fatal one: render = serialize → reparse → throw away

This is the bug that makes everything else academic.

`src/window/input.rs:76-86`:

```rust
// Re-serialize the mutated legacy DOM to HTML, then reload it into blitz_doc.
let html = crate::dom::serialize_outer_html(&self.dom);
self.blitz_doc = BlitzDocument::try_from_html(
    &html, self.base_url.as_deref(), &self.identity, content_w, content_h,
);
```

Read that again. The render path for a mutated page is:

1. Walk the **entire** legacy DOM and serialize it to an HTML string.
2. Hand that string to `blitz-html` to **parse a brand-new document from zero**.
3. Stylo re-resolves styles for **every node** in the new document.
4. The previous `BlitzDocument` — with all its resolved style, layout, and
   internal state — is **dropped on the floor**.

There is no incremental layout, no dirty-subtree invalidation, no node reuse.
`mark_needs_reflow()` is called from **19 sites** in
`src/js_sm/document/api.rs` — i.e. essentially every DOM mutation. So a script
that animates one element by touching `style.left` in a `requestAnimationFrame`
loop triggers a full document serialize + full reparse + full restyle **per
frame**. On YouTube — hundreds of thousands of nodes — this is not "slow," it
is architecturally non-viable. You will never get 60fps; you will struggle to
get 1fps once the page is real.

The architecture doc (`docs/ARCHITECTURE.md` §8.2) already flags this as
"the highest-priority technical debt." It is being undersold. This is not debt,
it is the foundation poured at a 30-degree angle. Everything built on top
inherits the tilt.

**Worse:** it means blitz-dom and JavaScript never share a live document. JS
mutates the *legacy* DOM. Blitz only ever sees a *snapshot string*. Any state
blitz-dom holds that doesn't round-trip through HTML serialization (form input
values, scroll positions, focus, canvas contents, event listeners, generated
content) is **silently destroyed on every reflow**. You are not rendering the
live document; you are rendering a photocopy of a description of it.

---

## 2. Three DOMs, and the wrong one is the source of truth

| Representation | Populated by | Role | Mutated by JS? |
|---|---|---|---|
| Legacy `dom::NodePtr` (`Rc<RefCell<Node>>`) | hand-rolled `html::Parser` | "source of truth" for JS, hit-testing, screenshots | **yes** |
| blitz-dom | `blitz-html` reparse of serialized legacy DOM | painting + nav hit-testing | no (rebuilt) |
| SpiderMonkey object graph | the bridge | what scripts actually touch | indirectly |

Pick one. A browser has exactly one DOM and everything is a view onto it.
Aurora has three and spends enormous effort keeping them approximately
consistent — and the canonical one (`Rc<RefCell<Node>>`) is the **hand-rolled**
one, not the spec-grade Stylo-backed blitz-dom you're paying for. You imported
a real layout engine (`blitz-dom` features list in `Cargo.toml` is a mile long
— floats, file_input, accessibility, parallel-construct) and then relegated it
to a read-only photocopier downstream of a toy parser.

The correct shape is the inverse: **blitz-dom is the document**, JS mutates it
through the bridge, paint reads it directly. The legacy DOM, legacy
`LayoutTree`, legacy `render`, the hand-rolled `html` parser and the hand-rolled
`css` stack should all be deleted. That is thousands of lines of liability whose
only current job is to feed a serializer.

---

## 3. Three JavaScript engine bridges in the tree, only one compiled

```
src/js_v8/    6,959 LOC   V8           ← the default, the wired one (default = ["v8"])
src/js_sm/   10,152 LOC   SpiderMonkey ← behind engine-sm; off by default
src/js_boa/   5,757 LOC   Boa          ← behind engine-boa; off by default
src/js_polyfills/  3,341 LOC  JS shims
```

The three engine directories are **mutually-exclusive Cargo features**, not
three bridges all compiled at once. `Cargo.toml` declares `default = ["v8"]`,
the `js_sm`/`js_boa`/`js_v8` modules are each `#[cfg(feature = ...)]`-gated, and
`EngineKind::default_compiled()` returns `V8` whenever the `v8` feature is on.
So a default build compiles **only `js_v8`** and `js_sm`/`js_boa` aren't in the
binary at all.

The runtime in the real pipeline is built at `src/runner/pipeline.rs:143` via
`create_runtime(EngineKind::from_env(), dom)`, which — with default features —
constructs `crate::js_v8::V8Runtime::new(...)`. Grepping `runner/`/`window/` for
`V8Runtime`/`js_v8::` returns nothing **because the wiring is deliberately
indirect**: a `create_runtime` factory returning `Box<dyn JsRuntime>`, so no
concrete engine type is named at the call site. The absence of the name is the
DI seam working, not evidence the engine is dead. (The one `SmRuntime::new`
reference in `runner/` is at `pipeline.rs:947`, inside a
`#[cfg(feature = "engine-sm")]` unit test — not the pipeline.)

The duplication critique still stands, just inverted from the first draft: the
`JsRuntime` trait was meant to make the engine swappable and instead became a
license to re-implement the entire DOM bridge three times. The commit log shows
the churn — `c90a338 move-to-spider-monkey` then `4f17366 Added more v8
features`: a migration *to* SpiderMonkey, then a migration *back* to V8 as the
default, with the abandoned bridge left in the tree. The two non-default
directories (`js_sm` + `js_boa`, ~15.9k LOC) are compiled exactly never in a
default build, yet are maintained, carried, and confuse every search in the
tree.

Decide and delete. V8 is already the default and the only JIT-backed engine
that boots YouTube here — keep `js_v8`, `rm -rf` `js_sm` and `js_boa` and their
features. Boa (no JIT) has no place on the critical path and the architecture
doc admits it "cannot run YouTube." This removes ~15.9k LOC and most of the
bridge maintenance surface at zero functional cost.

---

## 4. The NodeRegistry leaks the entire document, forever

> Corrected: the original draft cited `src/js_sm/registry.rs` (SpiderMonkey, not
> the default engine). The **identical** defect is in the actually-wired engine,
> `src/js_v8/registry.rs` — `reverse_nodes` keyed on `Rc::as_ptr(&node) as usize`
> (`registry.rs:116`), `register()` never paired with any `remove`/`unregister`.
> So everything below applies to the shipping V8 bridge; the SpiderMonkey copy
> has the same bug but isn't in the default build.

`src/js_v8/registry.rs` (and its `js_sm` twin). Two maps key the bridge:

```rust
nodes: BTreeMap<u32, NodePtr>,              // id  -> Rc clone of the node
reverse_nodes: BTreeMap<usize, u32>,        // Rc::as_ptr address -> id
js_wrappers: BTreeMap<u32, RootedTraceableBox<Heap<*mut JSObject>>>,
```

`register()` inserts a **clone of the `Rc`** and never removes it. Grep for
`nodes.remove` / `reverse_nodes.remove` / `unregister`: **none exist**. So:

- **Unbounded leak.** Every node JS has ever touched is pinned alive for the
  document's entire lifetime, even after it's detached and removed from the
  DOM. On a long-lived SPA like YouTube — which churns DOM constantly — the
  registry grows without bound. `js_wrappers` (rooted GC boxes) grows with it,
  so you are also pinning SpiderMonkey heap objects forever. This is a textbook
  steady-state memory leak on exactly the workload you're targeting.

- **Address-reuse aliasing hazard.** `reverse_nodes` is keyed on
  `Rc::as_ptr(&node) as usize`, a raw heap address. Today the leak accidentally
  protects you (the `Rc` is never freed, so the address can't be recycled). The
  day you fix the leak — and you must — any new node allocated at a freed node's
  old address will collide in `reverse_nodes` and resolve to a **stale id
  pointing at the wrong node**. You've coupled a correctness landmine to the
  leak so that fixing one arms the other. Key node identity on a monotonic id
  you mint, never on a pointer value.

---

## 5. 391 `unsafe` — 261 in the dormant SM bridge, 91 in the wired V8 one

> Corrected framing: the bulk of the `unsafe` (261 blocks, the 4,253-line
> `api.rs`) is in `src/js_sm/`, which is **not the default build**. The wired
> engine, `src/js_v8/`, carries 91 `unsafe` blocks of its own — a smaller but
> still-unaudited GC/handle frontier (V8 `Local`/`HandleScope` rather than
> SpiderMonkey rooting). The argument below is the same; for the shipping
> engine, point it at `js_v8`. The `js_sm` numbers describe code that costs
> review attention while running in production exactly never (see §3).

| Dir | `unsafe` | In default build? |
|---|---|---|
| `src/js_sm/` | 261 | no (`engine-sm`) |
| `src/js_v8/` | 91 | **yes** (default) |
| `src/stylo_bridge/` | 20 | no (dead) |
| `src/js_boa/` | 19 | no (`engine-boa`) |

`src/js_sm/document/api.rs` is **4,253 lines** with **102 `unsafe` blocks** in
that one file. This is the GC-rooting frontier: every one of those blocks is a
place where a missed `rooted!` or a JSObject pointer held across a function that
can trigger GC is a use-after-free that SpiderMonkey will not catch for you. A
4k-line file is not reviewable; nobody can hold the rooting invariants of 102
unsafe blocks in their head. This file *will* have GC-safety bugs, and they will
present as non-deterministic crashes that take days to bisect.

This isn't an argument against `unsafe` — embedding SpiderMonkey requires it.
It's an argument that the unsafe surface must be **small, wrapped, and
audited**. Right now it's large, raw, and spread across the biggest file in the
repo. Concentrate every JSObject/Heap/rooting interaction into a thin, tested
`gc`-safe wrapper module measured in hundreds of lines, and make the other 3,900
lines of `api.rs` call *safe* functions.

---

## 6. 381 `unwrap()` on a remote-input engine

A browser's entire job is to ingest hostile bytes from the network and not fall
over. 381 `unwrap()` + 39 `expect()` is 420 places a malformed response, a
weird header, a truncated body, or a surprising CSS token can convert into a
process abort. The architecture doc waves at "112+ unwrap in network/parse
paths"; the real count across the tree is 3.4× that. Every one on a
fetch/parse/style path is a remote DoS. `src/fetch/http.rs:40` will
`expect("failed to build HTTP client")` — panic the browser because a client
builder hiccuped. These need to become typed errors that surface as "page
failed to load," not `SIGABRT`.

---

## 7. Compatibility-by-polyfill is technical debt with a JS extension

3,341 lines of `src/js_polyfills/` (`v8_base.js`, `v8_post.js`,
`css_stub.js`, `trusted_types.js`, ...) exist to let real-world scripts
"initialise without panicking." The README is admirably honest that the bridge
"prioritises compatibility survival over full correctness." But understand what
that buys: scripts that *detect* a half-implemented API behave **worse** than
scripts that detect a *missing* one. Feature detection (`if (window.X)`) is the
whole web's compatibility model. A stub that exists but lies makes YouTube take
the fast path into code that assumes the API actually works, and then fails
somewhere deep and unattributable. "Survival stubs" defer the crash; they don't
remove it, and they move it somewhere harder to debug. Each stub needs a tracked
"make real or remove" decision, not permanent residency.

---

## 8. Redundant layers you're paying to maintain

- **HTML parsing, ×3 conceptually.** Hand-rolled `src/html/parser.rs` (the
  actual source of truth), plus `html5ever`/`markup5ever` in `Cargo.toml`, plus
  `blitz-html`. The hand-rolled one — explicitly "misses many real-world
  constructs" — is the one feeding your canonical DOM. You're parsing YouTube's
  HTML with the weakest of the three parsers you ship.
- **CSS, ×2.** Hand-rolled `src/css/` (2,606 LOC: `ast.rs`, `selectors_impl.rs`,
  `shorthand.rs`, `calc.rs`...) *and* Stylo via blitz-dom *and* `cssparser`/
  `selectors` crates. Stylo is the most battle-tested CSS engine on earth. Why
  is there a hand-rolled selector matcher next to it?
- **Layout, ×2.** Legacy `src/layout/` (3,907 LOC, block+flex+inline) used for
  "JS accessors, hit testing, screenshots," parallel to Stylo/Taffy in
  blitz-dom. Two layout engines that can and will disagree on coordinates —
  §8.2/§11 of the arch doc already calls out hit-testing divergence.
- **`src/stylo_bridge/`** — 825 LOC the arch doc itself labels dead code
  ("Not compiled; confuses searches"). Delete it. Today.

Every one of these duplications is a place two implementations silently diverge.
A browser is hard enough with **one** of each.

---

## 9. Repo hygiene

- `logs/` (28K), `managed_context/`, `test_suite_analysis/`, `scratch/` are
  **tracked in git**. `scratch/debug_layout.rs`, `Cargo.toml.bak`,
  `Dockerfile.dockerignore` — scratch and backup files do not belong in version
  control. `.gitignore` them.
- 685 entries in `Cargo.lock`. For a 37k-LOC project, the dependency graph is
  enormous (blitz + Stylo + mozjs + wgpu + vello + reqwest each drag a forest).
  Not wrong, but every one is attack surface and build time; the V8/Boa deletion
  in §3 trims it.
- 155 `#[test]` functions across 28 files is respectable for the size — the one
  genuinely healthy number in this report. Keep that culture; point it at the
  rooting wrapper from §5.

---

## 10. What I'd actually do (in order)

1. **Stop serializing-and-reparsing.** Make blitz-dom *the* DOM. Route JS
   mutations through blitz-dom's node API; paint reads it directly. This is the
   whole ballgame — nothing else matters until the document stops being
   rebuilt from a string every frame. (Kills §1 and §2 together.)
2. **Delete two JS engines.** V8 is already the default and only-compiled
   engine — keep `js_v8`, `rm -rf` `js_sm` and `js_boa` and their features.
   ~15.9k LOC gone, zero functional loss (neither is in a default build). (§3)
3. **Delete the legacy DOM/layout/parser/CSS stacks** once #1 lands. The
   hand-rolled `html`, `css`, `layout`, `render`, `style`, `stylo_bridge`
   modules are all downstream of the serialize hack and die with it. (§2, §8)
4. **Rebuild the bridge on a monotonic node-id + a small audited GC wrapper.**
   Fix the registry leak and the pointer-keyed identity at the same time; the
   safe wrapper shrinks the 261-block unsafe surface to something reviewable.
   (§4, §5)
5. **Triage `unwrap()` on fetch/parse/style paths into typed errors.** A
   browser does not abort on bad input. (§6)
6. **Then, and only then, chase YouTube.** Right now you're optimizing the
   hardest site on the web on top of a render path that can't survive it.

The honest version of the roadmap: items 1–4 are not on the current roadmap at
all, and they gate every item that is. "Render YouTube" is step 1 in the README;
in reality it's step 6 here, and steps 1–5 are invisible because they're
architecture, not features.

---

## Appendix — measurements

```
Total Rust LOC ............. 37,520
JS bridge LOC (sm+v8+boa) .. 22,868   (61% of tree)
  js_v8 ....................  6,959   (the wired engine; default = ["v8"])
  js_sm .................... 10,152   (dormant: behind engine-sm, off by default)
  js_boa ...................  5,757   (dormant: behind engine-boa, off by default)
JS polyfills (.js) .........  3,341
Largest file: src/js_sm/document/api.rs ... 4,253 LOC, 102 unsafe blocks (dormant engine)
unsafe blocks .............. 391  (js_sm 261)
unwrap() ................... 381
expect() ...................  39
panic!/unreachable!/todo! ..  12
.clone() ...................  559
#[test] fns ................ 155  (28 files)
Cargo.lock packages ........ 685
```

Sources: every figure above is reproducible from the tree at commit `3084a1e`.
The render-by-reserialize path is `src/window/input.rs:76-86`; the wired runtime
is built at `src/runner/pipeline.rs:143` via `create_runtime(EngineKind::from_env(), …)`
→ V8 by default (`Cargo.toml: default = ["v8"]`); the leaking registry is
`src/js_v8/registry.rs` (and its dormant `src/js_sm/registry.rs` twin). The
`SmRuntime::new` at `pipeline.rs:947` is a `#[cfg(feature="engine-sm")]` test,
not the pipeline.
