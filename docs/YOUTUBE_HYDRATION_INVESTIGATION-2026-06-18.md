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

---

# Update 2026-06-20 — logical-root composition lands; masthead logo now paints

The ShadyDOM logical-tree composition above is implemented (branch
`feature/youtube-polymer-shim-expanded`): `adoptLogicalShadowRoot` /
`composeDetachedStamp` (`custom_elements.js`) recover a Polymer logical fragment's
host and adopt it through the native `__aurora_adoptShadowRoot` bridge
(`node_create.rs::adopt_shadow_root` → `SyntheticShadowTreeBackend::adopt_shadow_root`).

### The bug that made it look broken (fixed)
The three new regression tests (`v8_adopts_shadydom_logical_root_and_connects_lite_children`,
`v8_composes_polymer_owned_detached_stamp_into_host_root`,
`v8_tracks_fragment_owner_during_custom_element_lifecycle`) all failed the same way:
adoption succeeded and `connectedCallback` fired, but `child.isConnected` then read
**false** — the host link was being severed *after* connection.

Root cause: **`query::find_parent` was calling `clear_parent` on adopted shadow-root
fragments.** A shadow root is retained on its host via the dedicated `el.shadow_root`
field, not the light `children` list, so `find_parent`'s "self-correct" path judged the
(valid) back-pointer stale and cleared it — severing the `fragment → host` link that
`is_connected_to` and the `.host` accessor depend on. It fired on any `querySelectorAll`
that walked an ancestor chain through the fragment (`build_ancestor_chain` → `find_parent`).
`node_create::find_parent_for` already guarded shadow roots; `query::find_parent` did not.

Fix: `query::find_parent` now recognizes a shadow root / template-content fragment as a
legitimate (non-stale) child of its host via `is_retained_subtree_root` and returns it
without scanning-or-clearing. One change, all three tests green, 191 lib tests pass.

### Measured effect on real youtube.com (`AURORA_DEBUG_RENDER=1`)
- `ytd-topbar-logo-renderer` now lays out at **123×112** (was **0×0**) — the masthead
  logo stamps and paints.
- Paint paths **~286 → 471**.

### The next wall (now the dominant log noise)
With more content composing, a recurring **`MirrorIntegrity` "child mapping mismatch"**
surfaces (one legacy node, blitz child `1762` where `1761` is expected — a stable
off-by-one), repeated across every subsequent `sync_*` op on that node. This is the
Tier-2 two-DOM mirror-drift class, not the composition path. There is also a
`TypeError: Cannot read properties of null (reading '__shady_native_children')` (one
occurrence) to chase. Next content win is stabilizing the mirror under the now-larger
stamped tree.

---

# Update 2026-06-20 (later) — Stylo panic traced upstream + mirror-drift root cause localized

### Stylo panic is upstream (servo/stylo#387), mitigated locally
The `data.rs:186 ElementStyles::primary().unwrap()` on `None`, and the
`thread_state.rs` assertion cascade it triggers, are a **Stylo bug** (filed as
servo/stylo#387; nicoburns + Loirooriol confirmed it's a Blitz/Stylo issue, **fixed on
stylo `main`** but unreleased). The backtrace: an element reaches **style invalidation**
(`should_process_descendants → is_display_none → primary()`) with `ElementData`
allocated but no primary computed style. Triggered from the windowed `handle_resize →
reflow → resolve` path, so every resize/redraw re-panics and the GUI spins forever.

Actions taken:
- **Bumped deps to the latest released:** blitz-dom/html/paint/traits `0.3.0-alpha.4 →
  alpha.5`, stylo `0.17.0 → 0.18.0` (transitive), anyrender/anyrender_vello `0.10 →
  0.11` (forced by alpha.5's `PaintScene`). Build clean, 191 lib tests green. **0.18.0
  still has the panic** (the fix is only on stylo `main`).
- **Graceful-degradation guard (Aurora-side, not a Stylo workaround):** `resolve_inner`
  and `set_viewport` (`src/blitz_document.rs`) now early-return when `!self.healthy`, so
  once the existing `consecutive_panics >= MAX_CONSECUTIVE_PANICS` breaker trips, Aurora
  stops re-driving Stylo and keeps the last good frame instead of spinning. Verified
  harmless: home feed still **471 paths / 0 panics**, watch page still **124 paths**
  (the guard never even trips there — panics are interspersed with successes so
  `consecutive` never reaches the cap; it's purely a safety net for the windowed
  resize-storm).

### Mirror-drift root cause (the thing feeding Stylo the unstyled node)
First mismatch on the logged-out home, before any cascade:
```
op=sync_insert_before  parent blitz=1773  blitz=[1769,1772] expected=[1768,1772]
```
The legacy parent's first composed child maps to blitz **1768**, but the blitz tree holds
**1769** there — two adjacent blitz ids for the same logical child slot.
`create_dom_node` (`src/blitz_document.rs`) **does** reuse by `legacy_node_key`, so a
duplicate can only arise when the "same" logical child is presented as **two different
`Rc<RefCell<Node>>` allocations** at different sync times (Polymer re-stamp / shadow
re-composition). **`legacy_node_key(node) = Rc::as_ptr(node) as usize`** keys the mirror
maps on the heap address, so it cannot recognize a re-stamped node as the same logical
node — it allocates a fresh blitz id and the old one lingers in the parent's child list.

Causal chain: pointer-based `legacy_node_key` + shadow composition (the find_parent fix,
which made re-stamping actually happen) → duplicate blitz nodes per re-composed child →
mirror child-list drift → an unstyled node reaches Stylo → upstream panic.

This is the two-DOM reconciliation project (collapse-the-mirror), not a one-liner; the
home feed renders 471 paths today, so any change here must not regress that. Two tractable
sub-directions for next session: (a) give each legacy `Node` a **stable unique id**
(monotonic counter on the node) instead of keying maps on `Rc::as_ptr`, removing the ABA
hazard and enabling logical reconciliation; (b) make the incremental `sync_insert_before`
/ `sync_replace_children` paths reconcile against `composed_children` so a re-stamped
child replaces (not duplicates) its prior blitz node.

---

# Update 2026-06-20 (3) — mirror-drift is NOT the content blocker; logical-root ADOPTION is. +116 paint paths.

Two experiments redirected the whole content effort:

1. **Stable-id refactor: aborted with evidence.** `blitz_to_legacy` holds `Rc` *clones*,
   so every mapped node is kept alive and the ABA hazard a stable id fixes basically
   cannot fire for mapped nodes. Not the bug.
2. **Fresh-rebuild experiment: clean negative.** Rebuilding the Blitz mirror from the
   *settled* legacy DOM after hydration (`try_from_dom`, gated A/B) produced the **same**
   path count as the incremental mirror (428 = 428 on home). So the mirror drift does
   **not** drop renderable content — a fresh consistent traversal recovers nothing extra.
   The dropped content simply **isn't in the composed document tree**: the logical shadow
   fragments holding it are never composed in, so neither path reaches them.

**The actual content blocker (measured, `AURORA_TRACE_CE`):** on the logged-out home,
`logical-root-adopted = 0`, `logical-root-unresolved = 78`. `adoptLogicalShadowRoot`
matched **zero** fragments to a host. `logicalRootForHost` only checks
`host.shadowRoot / __shady_shadowRoot / root / __shady.root`, and on real YouTube ShadyDOM
never exposes the logical root on the host that way — so 78 fragments full of stamped
content sat orphaned, outside the composed tree, unrenderable. (The 428 paths that did
render come from `ytd-app`'s full-Polymer `attachShadow` path plus ~29 `detached-stamp`
composes.)

**The fix (shipped, `custom_elements.js`):** the fragment already records its host in
`__aurora_fragment_owner__` (set at stamp time to `activeLifecycleHost`; the
detached-stamp path already trusted it). Added an **owner-backref fast path** to
`adoptLogicalShadowRoot` (extracted an `adoptRootToHost(root, host, via)` helper used by
both paths): if `root.__aurora_fragment_owner__` is a host that hasn't already claimed a
different root, adopt directly. Result on home:
- `logical-root-adopted 0 → 4`, `unresolved 78 → 19`.
- **Paint paths 428 → 544 (+116), deterministic across runs, 0 panics, 191 lib tests green.**

**Watch page interaction (not a regression of this fix):** the watch render is
chronically nondeterministic (0 or ~124) because it triggers the **upstream Stylo panic**
(servo/stylo#387) on the *final* one-shot resolve — verified `FailedRecoverable`,
`Unhealthy=0`, `max consecutive=2`, so it is **not** the degradation guard tripping and
**not** the owner fix (which adopts `0` on watch — those roots lack the owner backref).
Watch content awaits the released Stylo fix.

**Next levers:** the remaining 19 unresolved roots (owner backref absent — find their host
another way), and whether the owner fast path can be widened (e.g. `__dataHost` chain) to
adopt more. Mirror drift remains a secondary correctness issue (panic source, guarded),
not a content blocker.

---

# Update 2026-06-20 (4) — the universal nav blocker: ytd-app.connectedCallback fails

Followed the content thread past the data-gated home onto a **search results page**
(`/results?search_query=…`), which is a far better testbed: its initial payload carries
inline `videoRenderer` data (confirmed via curl) AND it renders **327 paths with 0
panics** — no Stylo wall, unlike watch. But the search *results* never lay out. Root
cause, the same on home and search (watch dies earlier on the Stylo panic):

**`ytd-app.connectedCallback` throws, so the app never connects** (`connected=no`,
`connectFailed=yes`), and the navigation / page-content instantiation that depends on a
completed `connectedCallback` never runs. Two errors, with stacks:

1. `TypeError: Cannot set properties of undefined (setting '_templateInfo')`
   at `q._stampTemplate (11115)` ← a nested `Q.ready` chain ← `readyUpgraded`. A
   component's `_template` resolves to **undefined** during a nested `ready()`, so
   Polymer's `_stampTemplate(undefined)` crashes. (ytd-app's own shadow stamps —
   shadowChildren≈68 — so this is a nested/sub-template, not the top template.)
2. `TypeError: Cannot read properties of undefined (reading 'addEventListener')`
   at `_.kzA (2583)` ← `_.u.attached (40235)` ← `Q.connectedCallback (11518)`. YouTube's
   `attached` reads `.addEventListener` on an undefined object — almost certainly an
   instance property / `this.$.<id>` element that the failed stamp (#1) never produced.

So #1 is the root and #2 is its consequence. Both live in YouTube's minified Polymer
(`q`/`Q` classes, the `_.kzA`/`_.u` namespace) — not patchable directly — and the nav code
that would render results sits **downstream of the throw inside YouTube's
`connectedCallback`**, so catching the error does not let nav fire. The real fix is making
Aurora's template-resolution shim (`installTemplateAccessor` / `_template` getter in
`custom_elements.js`) never hand `_stampTemplate` an undefined template for the nested
component that currently fails — i.e. resolve (or empty-fallback) that template so `ready()`
completes and `attached` finds its stamped element. That is the next concrete content
target, and the **search page is the clean rig to do it on** (real data, no Stylo panic):
`AURORA_HEADLESS=1 AURORA_DEBUG_YOUTUBE=1 cargo run -- "https://www.youtube.com/results?search_query=rust+programming"`.

Also landed this round: a real `window.visualViewport` polyfill (`v8_base.js`, built on the
real EventTarget) — it was genuinely missing (standard API, used by YouTube). It is not the
`kzA` target, so it did not unblock connect, but it is correct platform completeness and
prevents a separate future crash. Home still 544 paths; 191 lib tests green.
