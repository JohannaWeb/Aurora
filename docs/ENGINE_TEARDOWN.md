# Aurora — A Brutal Engine Teardown

> Reviewer's framing: I was asked to look at this the way I'd look at a chunk
> of Chromium that landed without review. No diplomacy, no participation
> trophies. Everything below is backed by something in the tree, with
> file:line where it matters. Treat it as a punch list, not an insult — the
> bones are more interesting than the current state of the muscle.

Date: 2026-06-16 · Branch: `feature/continued-youtube-support` · ~37.5k LOC Rust

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

## 3. Three JavaScript engines, two of them wired to nothing

```
src/js_sm/   10,152 LOC   SpiderMonkey   ← the only one runner actually calls
src/js_v8/    6,959 LOC   V8             ← not referenced from runner/ or window/
src/js_boa/   5,757 LOC   Boa            ← not referenced from runner/ or window/
src/js_polyfills/  3,341 LOC  JS shims
```

The only runtime constructed in the real pipeline is
`crate::js_sm::SmRuntime::new(...)` at `src/runner/pipeline.rs:947`. Grepping
`runner/` and `window/` for `V8Runtime`, `BoaRuntime`, `js_v8::`, `js_boa::`
returns **nothing**. So ~12,700 lines of `registry.rs` / `runtime.rs` /
`mutation_observer.rs` / `capture.rs` / `node_create.rs` — triplicated, file
for file — exist to be compiled (behind features), maintained, and to confuse
every search you ever run in this tree. The `JsRuntime` trait was supposed to
make the engine swappable; instead it became a license to re-implement the
entire DOM bridge three times.

The commit log tells the story: `b0dee4f Spider monkey`,
`c90a338 move-to-spider-monkey`, then `4f17366 Added more v8 features`. You
*migrated* to SpiderMonkey and then kept growing a V8 bridge anyway. Decide.
For a JIT-backed YouTube target, SpiderMonkey or V8 — **one** — and delete the
other two directories outright. Boa (no JIT) has no place on the critical path
and the architecture doc admits it "cannot run YouTube."

This single decision removes ~12k LOC and roughly a third of the maintenance
surface at zero functional cost, because the deleted code runs in production
exactly never.

---

## 4. The NodeRegistry leaks the entire document, forever

`src/js_sm/registry.rs`. Two maps key the bridge:

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

## 5. 391 `unsafe`, 261 of it in the SpiderMonkey bridge

| Dir | `unsafe` |
|---|---|
| `src/js_sm/` | 261 |
| `src/js_v8/` | 91 |
| `src/stylo_bridge/` | 20 |
| `src/js_boa/` | 19 |

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
2. **Delete two JS engines.** Keep SpiderMonkey (or V8 — pick one), `rm -rf`
   the other two bridge directories and their features. ~12k LOC gone, zero
   functional loss. (§3)
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
  js_sm .................... 10,152   (the only one wired to runner)
  js_v8 ....................  6,959   (dead: unreferenced in runner/window)
  js_boa ...................  5,757   (dead: unreferenced in runner/window)
JS polyfills (.js) .........  3,341
Largest file: src/js_sm/document/api.rs ... 4,253 LOC, 102 unsafe blocks
unsafe blocks .............. 391  (js_sm 261)
unwrap() ................... 381
expect() ...................  39
panic!/unreachable!/todo! ..  12
.clone() ...................  559
#[test] fns ................ 155  (28 files)
Cargo.lock packages ........ 685
```

Sources: every figure above is reproducible from the tree at commit `3084a1e`.
The render-by-reserialize path is `src/window/input.rs:76-86`; the single wired
runtime is `src/runner/pipeline.rs:947`; the leaking registry is
`src/js_sm/registry.rs`.
