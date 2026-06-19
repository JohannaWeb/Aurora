# YouTube Hydration Investigation — why content doesn't render

*2026-06-18, branch `feature/youtube-support-fix-no-rendering`. A self-contained
write-up of a deep investigation into why real youtube.com paints only its masthead.
Companion to `docs/YOUTUBE_RENDER_CHECKDOWN.md` (the checklist) — this is the narrative,
the evidence, and the conclusion, so the reasoning isn't lost.*

---

## The question

Real `https://youtube.com` loads cleanly (EXIT 0) but paints only the masthead bar
(hamburger + icon buttons). The logo, search box, guide, and feed are blank. Why?

## TL;DR

- The blocker is **not** rendering, **not** shadow DOM, **not** the two-DOM mirror, and
  **not** a bounded bug. It is in the **JavaScript Polymer-emulation layer**.
- The masthead's own logo/search are missing because their custom elements either never
  fully materialize or are **lightweight (non–full-Polymer) components whose render path
  Aurora's Polymer-centric shim doesn't drive**.
- We **conclusively ruled out forking blitz-dom for native Shadow DOM** — it would render
  zero additional pixels. (Author-confirmed; see below.) That negative result potentially
  saved months.
- Pinpointing the actual fix is **open-ended reverse-engineering** of the interaction
  between YouTube's minified Polymer bundle and Aurora's ~1,800-line `custom_elements.js`
  + ~440-line `polymer_shim.js`. It needs sustained lifecycle instrumentation, not a patch.

---

## What we measured (the evidence trail)

All measured on real youtube.com via temporary, env-gated probes (since reverted).

### 1. The masthead renders; specific renderers collapse to 0×0
`AURORA_DEBUG_RENDER=1` layout dump:
- `ytd-masthead 1280x56` ✅ and `yt-icon-button 40x40` ✅ (buttons paint)
- `input 0x0` (search box) and `ytd-topbar-logo-renderer 0x0` (logo) — empty boxes.

### 2. The empty renderers hydrate but have no shadow content
`getComputedStyle`/`shadowRoot` probe:
- `ytd-app`: upgraded, connected, **14 shadow children → renders**.
- `ytd-topbar-logo-renderer`: `upgraded=yes` but **no shadow root, no children**.
- `ytd-searchbox`, `ytd-guide-renderer`: **MISSING entirely** (never created).

→ Not a CSS-sizing collapse (boxes are empty, not mis-sized). Not a hydration *connect*
failure.

### 3. The empty renderers have no Polymer stamping methods
Force-stamp experiment on `ytd-topbar-logo-renderer`:
```
ready=undefined  _stampTemplate=undefined  _attachDom=undefined
_enableProperties=function  _template=undefined  ce_connected=true  shadowAfter=none
```
Forcing the stamp path changed nothing (paths stayed 286). There is no stamp path to
drive — so **native shadow styling would scope content that is never produced.** Shadow
DOM is a red herring.

### 4. The prototype chains differ fundamentally
Instance prototype chains (`Object.getPrototypeOf(el)` — authoritative):
- **`ytd-app` (renders):** full 11-level Polymer mixin tower —
  `A[_attachDom] → A[ready] → Q[ready,_enableProperties] → … → q[ready,_stampTemplate] →
  … → PatchedHTMLElement`.
- **`ytd-topbar-logo-renderer` (empty):** **two levels** —
  `l[_enableProperties,_flushProperties] → l[] → PatchedHTMLElement`. The entire
  `PropertyAccessors → PropertyEffects → TemplateStamp → ElementMixin` layer is absent.

### 5. It is not a lazy-finalize trigger
Probed for Polymer's static `finalize()`/`_finalizeClass()` on these ctors:
`finalize=undefined`. There is no finalize method to call. Rejected.

### 6. A registry survey that raised more questions than it answered
Surveying Aurora's `custom_elements.js` `registry`: **1118/1118 registered ctors read as
"thin"** (no `ready`/`_stampTemplate` in the ctor prototype chain) — *including* `ytd-app`,
whose instance is demonstrably full. These two facts are contradictory: an instance cannot
be richer than the class it was upgraded from unless **working elements get their real
prototype from a path other than Aurora's registry ctor.** The registry/ctor data is
internally inconsistent, which is the signal that one-shot probing has bottomed out.

---

## The strategic detour: "fork blitz-dom for native shadow DOM"

The teardown recommended collapsing to one DOM so shadow/stamping happen natively in Stylo.
We checked the premise and it is **false for this stack**:

- `blitz-dom 0.3.0-alpha.4` stubs Shadow DOM in its Stylo integration:
  `as_shadow_root → None`, `host() → todo!("Shadow roots not implemented")`,
  `parent_node_is_shadow_root → false`. A code comment ties it to an upstream `selectors`
  crate limitation (`OpaqueElement` can't be rehydrated).
- The blitz author (Nico Burns) confirmed directly: *"It does not currently. We need to add
  it at some point. Shouldn't be crazy hard, although there are some blockers on the Stylo
  side."*

So a fork would (a) be a months-scale effort partly upstream in Servo, and (b) render zero
additional YouTube pixels, because — per evidence #3 — the content is never stamped in the
first place. **Do not fork blitz-dom for this.**

---

## Conclusion

The chain of evidence is consistent and points one direction:

1. YouTube uses **full Polymer for some elements** (`ytd-app` → stamps → renders) and
   **lightweight/lite components for others** (`ytd-topbar-logo-renderer` → thin chain →
   never stamps).
2. Aurora's synthetic shadow is **sufficient when content stamps** (proven by `ytd-app`).
3. The masthead's missing pieces are lite components whose render path Aurora's
   Polymer-centric shim does not drive — plus some elements (`ytd-searchbox`) that are never
   created at all.
4. The exact mechanism that lets full-Polymer instances complete their prototype while lite
   ones stay thin is **not fully understood** — the registry/ctor data is contradictory —
   and resolving it requires sustained lifecycle instrumentation, not one-shot probes.

This is the ~1,800-line-emulation **"bug farm."** It is open-ended reverse-engineering of
YouTube's minified component system, not a bounded fix.

---

## Recommendation

1. **Bank the verified wins** from this session (panic fix, accessors, doc truth) — they're
   real and independent of the YouTube wall.
2. **Do not fork blitz-dom**; do not pursue native shadow as a YouTube unlock.
3. For YouTube content, choose one of:
   - **(a) A real instrumentation project** — log prototype/lifecycle state at every
     upgrade→ready→connect step across many elements to untangle the full-vs-lite divergence.
     Multi-day, in `custom_elements.js`/`polymer_shim.js`.
   - **(b) The strategic pivot** (teardown's stance) — retarget off the logged-out home to a
     watch page with inline `ytInitialPlayerResponse` data, and/or reconsider whether
     out-emulating Polymer indefinitely is the right war.

## Verified fixes shipped alongside this investigation
- 5 `blitz-dom mutator.rs:807 "unreachable"` panics on real YouTube → **0** (attribute-diff
  in `BlitzDocument::sync_all_attributes`). Regression test added.
- `offsetTop`/`offsetLeft` JS accessors wired to Blitz layout (were static `0`).
- Lib suite green (shadow `composed_children` fix).
- `docs/ARCHITECTURE.md` corrected (V8-only; no false "no-Chromium/Servo" claim).

## How to reproduce the diagnosis
```bash
AURORA_DEBUG_RENDER=1  AURORA_HEADLESS=1  cargo run -- https://youtube.com   # layout sizes + path count
AURORA_DEBUG_YOUTUBE=1 AURORA_HEADLESS=1  cargo run -- https://youtube.com   # component/probe state
# Custom-element lifecycle tracer (now permanent, gated infrastructure):
AURORA_TRACE_CE=1 AURORA_TRACE_CE_FILTER="ytd-app,ytd-topbar-logo-renderer" \
  AURORA_HEADLESS=1 cargo run -- https://youtube.com
```

---

# Update 2026-06-19 — lifecycle instrumentation finds the true root cause

Per the recommendation, we built **permanent, gated custom-element lifecycle instrumentation**
(`AURORA_TRACE_CE` / `AURORA_TRACE_CE_FILTER`, in `src/js_polyfills/custom_elements.js`,
flag wired in `src/js_v8/runtime.rs`). It traces, per element: the ctor at `define`, the ctor
at upgrade, the construct-via-`new` path, prototype-chain summaries after `setPrototypeOf`,
after construction, and after `connectedCallback`, the `connect` gate (connected? retry?),
and `_enableProperties`/`_flushProperties` timing vs. children/shadow/template. Emits
`[ce] <phase> <name>#<id> …` lines.

### What it found (the definitive chain)

1. **At `define`, `ytd-app` and `ytd-topbar-logo-renderer` are identical** — both registered
   with a thin `depth=6` ctor, no stamping methods. The registered class is *not* the divergence.
2. **The split is in the constructor.** `post-construct` shows `ytd-app`'s instance jump to
   `depth=15` (the full Polymer mixin tower) with `kids=14 template=yes`, while the logo stays
   `depth=6 kids=0`. `instIsCtorProto=false` + `ctorProtoAfter=depth=6` for both: the registered
   ctor stays thin; `ytd-app`'s constructor reaches the **real full Polymer class via its
   `super()` chain**, the logo's does not. **`ytd-topbar-logo-renderer` is genuinely a lite
   element** (no template stamping); its `connectedCallback` is its render trigger.
3. **The logo's `connectedCallback` never fires.** The connect gate shows
   `ytd-app isConnected=true` (fires, renders) but `ytd-topbar-logo-renderer isConnected=false`
   → `connect-bail-disconnected`, looping forever on the bounded retry.
4. **Why disconnected — exact mechanism.** The bail ancestry:
   `logo < div < div < div < tp-yt-app-drawer < div < #document-fragment(viaHost:false)`.
   The walk climbs into a shadow-root `#document-fragment` and dead-ends: that fragment has
   **no `parentNode` and no `.host`**. `find_parent_for` (node_create.rs) rejects the host
   because a shadow root lives in `el.shadow_root`, not the host's regular `children`, so
   `query::find_parent` returns `None`. And the fragment is a **raw ShadyDOM logical render
   root** (Polymer runs `useNativeShadow=false`), *not* registered as any element's
   `el.shadow_root`, so it has no host link at all — and it is **detached from the document**
   (a `__aurora_connect_sweep__` walk from `document.body` reaches it 0 times).

### True root cause
**The missing masthead content lives in detached ShadyDOM *logical* shadow fragments that
Aurora never composes into the connected/rendered document tree.** Therefore the elements
inside are never "connected" → their `connectedCallback` (the render trigger for lite
elements) never fires → they never render. This is a **ShadyDOM logical-tree composition**
gap, a real subsystem — not native shadow DOM, not the mirror, not a single bug.

### Fixes shipped this round (correct, low-risk, kept)
- **`ShadowRoot.host` accessor** (`src/js_v8/node_create.rs` `get_shadow_host`) — synthetic
  shadow-root fragments now expose `.host` (via `SyntheticShadowTreeBackend::host_for_shadow_root`),
  so connectivity/composed-event traversal can cross *registered* shadow boundaries. Does not
  cover raw ShadyDOM logical fragments (they aren't registered shadow roots).
- **Connect sweep** (`__aurora_connect_sweep__`, called from `apply_polymer_bindings`) — fires
  `connectedCallback` for upgraded-but-disconnected custom elements once they are reachable and
  connected, matching browser insertion semantics. No-op for the detached-fragment case, but
  correct for elements that attach after their retry window.
- **Permanent CE lifecycle instrumentation** (`AURORA_TRACE_CE`).
- All gated/zero-overhead when off; 180 lib tests green; YouTube render unchanged (286 paths).

### The remaining work (well-defined now)
Implement **ShadyDOM logical-tree composition**: when Polymer (in `useNativeShadow=false` mode)
stamps content into a logical shadow root, that fragment must be linked to its host and composed
into the rendered tree so its elements connect, fire `connectedCallback`, and paint. This is the
real YouTube-content unlock and a bounded (if non-trivial) subsystem — the instrumentation above
makes it tractable to build and verify.
