# YouTube Render Checkdown — what's missing so YouTube renders

*Last updated 2026-06-18, branch `feature/youtube-support-fix-no-rendering`.*

A single prioritized checklist distilled from the larger docs. This is the "what's
left" view; the deep rationale lives in:

- `docs/YOUTUBE_RENDERING_STABILIZATION_ACTION_PLAN.md` — full phase-by-phase plan (source of truth)
- `docs/youtube_workaround_inventory.md` — every shim, its platform gap, and delete condition
- `docs/TEARDOWN-2026-06-18.md` — the brutal "why this is hard" review

---

## First, the thing people get wrong

There are **two different "YouTube" targets**, and they are in very different shape:

| Target | Status | Evidence |
|--------|--------|----------|
| Local fixture `fixtures/youtube/index.html` | ✅ **Renders fully** — player, title, avatar, comment all paint | `tests/screenshots/youtube.png` |
| Live `https://youtube.com` (logged-out home) | ⚠️ **Loads EXIT 0, but only masthead chrome paints** (hamburger + 3 icon circles) | Action plan "Live YouTube Status - 2026-06-18" |

So "fix my YouTube render" is really **"make live youtube.com paint content."** The
local fixture already works and is the regression anchor — don't break it.

**The headline finding:** live YouTube is *not* blocked on rendering. It is blocked on
**data**. The logged-out home feed's initial payload carries a single
`feedNudgeRenderer` and **zero** video items; the real feed needs continuation API
fetches (and likely auth) that Aurora never makes. Every paint/layout/shadow
improvement is invisible until there is content to paint.

---

## The checklist (ordered by leverage)

### Tier 0 — Get content to exist at all (the real gate)
Without this, nothing below matters; you'd be polishing an empty page.

- [ ] **Pick a content-bearing route, not the logged-out home.** The home feed has no
  inline content. Target a watch or search page whose *initial* data payload carries
  inspectable content, or accept that the home needs network continuations.
  - Done = `window.getInitialData()` (or equivalent) shows ≥1 real content item for the chosen route.
- [ ] **Make the continuation / data-fetch network calls** if staying on a feed route.
  YouTube hydrates the feed from continuation API responses Aurora does not request.
  - Anchor: fetch path in `src/fetch/`; bootstrap in `src/runner/pipeline.rs`.
  - Done = feed grid populates with >0 video items before first paint settles.
- [ ] **Do NOT reintroduce the `updatePageData` nav driver as the fix.** Measured
  2026-06-18: driving `ytd-page-manager.updatePageData` *collapses* paint 322→32 paths
  by swapping an empty browse page over the working shell. It makes render worse, not
  better, until the empty-feed problem above is solved.

### Tier 1 — Stop the engine from crashing mid-hydration
Recoverable today, but each one corrupts/rebuilds the mirror and costs content.

- [x] **The 5 caught `blitz-dom mutator.rs:807 "unreachable"` panics** during
  attribute replacement — FIXED 2026-06-18. Root cause: `sync_all_attributes` did a
  blanket clear-all-then-set-all; clearing `href` on a `<link>` whose stylesheet was
  still loading made blitz panic in `unload_stylesheet`. Fix: diff existing vs desired
  attributes and only clear genuinely-removed ones (`src/blitz_document.rs`). Verified
  on real youtube.com: **0 mutator panics** (was 5). Regression test
  `sync_all_attributes_diffs_add_update_and_remove`. One unrelated `SyncOperationFailed`
  rebuild remains (different op); not yet root-caused.
- [x] **`RefCell already mutably borrowed` abort on `textContent` rewrite** — FIXED
  2026-06-18. The `SetTextContent` dispatcher held `borrow_mut()` across the
  render-sync; now releases before syncing. Regression test
  `v8_set_text_content_on_text_node_syncs_without_reborrow_panic`.

### Tier 2 — Collapse the two-DOM mirror (root architectural cause)
The split-brain (JS mutates `Rc<RefCell<Node>>`; Stylo paints a mirrored
`BlitzDocument`) is why `MirrorIntegrityError`, the `sync_*` protocol, and snapshot
rebuilds exist at all. This is large but it deletes whole bug classes.

- [ ] **Make the Blitz/Stylo tree the single authority; JS bindings mutate it directly.**
  - Anchor: `legacy_to_blitz` / `blitz_to_legacy` + `sync_*` in `src/blitz_document.rs`;
    mutation bridge in `src/js_v8/tree/mutation.rs`.
  - Done = `validate_mirror_integrity`, the sync protocol, and the snapshot-rebuild
    fallback can be deleted rather than hardened.

### Tier 3 — Missing platform features content relies on
These don't block the masthead bar, but they ARE why the masthead's own logo and
search box are invisible, and a real feed/watch page exercises them further.

- [ ] **Incomplete Polymer prototype on nested custom-element upgrade — THE root blocker.**
  Pinned down 2026-06-18 via a force-stamp experiment (`__aurora_force_stamp__`, since
  reverted). Probing `ytd-topbar-logo-renderer` after load:
  `ready=undefined _stampTemplate=undefined _attachDom=undefined _enableProperties=function
  _template=undefined ce_connected=true shadowAfter=none`. The element upgrades and connects,
  but its prototype has only the low-level `PropertiesMixin` (`_enableProperties`) — it is
  **missing the entire `ElementMixin`/`PropertyEffects`/`TemplateStamp` layer** (`ready`,
  `_stampTemplate`, `_attachDom`, `_template`). So there is no stamp path to drive: nothing
  produces shadow content. `ytd-app` (which *does* have the full mixin) stamps and renders
  fine — proving synthetic shadow is sufficient *when content stamps*. Some renderers
  (`ytd-searchbox`, `ytd-guide-renderer`) are `MISSING` entirely (never created).
  - This is a **JS-shim** bug in how Aurora links/applies the Polymer class prototype chain
    during custom-element upgrade (`custom_elements.js`), NOT a renderer/Stylo/shadow issue.
  - **Native Stylo shadow DOM would fix none of this** (verified: forcing the path left paths
    at 286, unchanged) — you cannot scope content that was never stamped. Shadow DOM is a
    red herring for YouTube; do not fork blitz-dom for it.
  - Done = nested renderers upgrade with the full Polymer prototype, `ready()`/`_stampTemplate`
    exist, and they stamp content that paints.
  - **Localized further 2026-06-18 (two-upgrade-path split):** a survey of all registered
    custom elements found **1118/1118 have a "thin" ctor** in Aurora's `custom_elements.js`
    `registry` (no `ready`/`_stampTemplate` anywhere in the ctor prototype chain) — yet
    `ytd-app`'s *instance* has the full 11-level Polymer mixin tower and renders. Both can
    only be true if working elements are upgraded by **Polymer's native machinery** (real
    class) while elements like `ytd-topbar-logo-renderer` go through **Aurora's registry
    upgrade** (`Object.setPrototypeOf(el, ctor.prototype)` with the thin registry ctor) and
    end up unable to stamp. So the lever is: **why do some elements get native Polymer
    upgrade and others Aurora's thin path, and can Aurora's path preserve the real class's
    full prototype chain?** `polymer_shim.js` `installPolymerMethods` only adds Polymer's
    legacy *data* API (`set`/`get`/`notifyPath`/`splice`/`fire`), never the stamping methods,
    so Aurora-upgraded elements have no stamp path at all. NOTE: residual uncertainty about
    exactly what `registry` stores (Aurora wrapper ctor vs YouTube's real class) — resolve
    that first. This is genuinely the 1800-line-emulation "bug farm"; the fix is a real
    project in the CE upgrade/registry layer, not a one-liner.
  - **Finalize hypothesis tested and rejected 2026-06-18:** probed whether calling the
    class's static `finalize()`/`_finalizeClass()` would install the stamping mixins —
    **no such method exists** on these ctors (`finalize=undefined`). So it's not a
    lazy-finalize trigger problem either. The *authoritative* signal is the instance
    prototype chain (`Object.getPrototypeOf(el)`): `ytd-app`'s instance genuinely has the
    full Polymer tower and renders; `ytd-topbar-logo-renderer`'s instance is genuinely
    thin (`l → l → PatchedHTMLElement`). Best current read: the logo renderer's real
    registered class **is** a lightweight/lite component (not full Polymer), so it never
    stamps a template — its content arrives via a render path Aurora's Polymer-centric
    shim doesn't drive. Residual unknown: *how* full-Polymer instances like `ytd-app` get
    their complete prototype when their registry ctor reads as thin — the registry/ctor
    data is internally contradictory, so the exact prototype-completion mechanism needs
    **sustained lifecycle instrumentation** (log prototype state at every upgrade/ready/
    connect step), not more one-shot probes. **Conclusion: this is open-ended emulation
    reverse-engineering — the bug farm — not a bounded fix.**
- [ ] **`getComputedStyle` returns stubbed values** (probe showed `display=?`/`width=?`).
  Separate gap; wire to Blitz computed style. Not the cause of the empty renderers above.

### Note on "collapse to one DOM" / native shadow (investigated 2026-06-18)
The teardown's "collapse to one DOM so shadow/stamping happen natively in Stylo" rests on a
false premise: **blitz-dom 0.3 has Shadow DOM stubbed** (`stylo.rs`: `as_shadow_root → None`,
`host() → todo!("Shadow roots not implemented")`), confirmed directly by the blitz author
(Nico Burns: "It does not currently. We need to add it… blockers on the Stylo side" — the
`OpaqueElement` can't be rehydrated, a `selectors`-crate limitation). Collapsing the two DOMs
is still worthwhile to delete the mirror-drift bug class (where the `sync_*` panics lived),
but it does **not** grant native shadow scoping and does **not** address the stamping blocker
above. The YouTube-critical path is JS hydration (prototype/stamping) + feed data, not the DOM
architecture.
- [x] **`document.elementFromPoint`** — already wired natively to the Blitz hit-test
  (`runtime.rs::element_from_point` → `registry.hit_test`). Test
  `v8_element_from_point_hits_blitz_layout`. (The `v8_post.js` `return null` is only a
  fallback when no native impl is present.)
- [x] **`offsetTop` / `offsetLeft`** — wired 2026-06-18. The native `__aurora_metric__`
  bridge already mapped `offsetTop→y`/`offsetLeft→x`; v8_post.js now installs the
  getters (was static `0`). Test `v8_offset_position_accessors_read_blitz_layout`.
  `scrollTop`/`scrollLeft` intentionally stay settable `0` data properties (element
  scroll offset, not box position).
- [ ] **Retire the ~13 Polymer/ShadyDOM shims as the platform catches up.** Each has an
  explicit delete-condition in `docs/youtube_workaround_inventory.md`. Every shim that
  becomes load-bearing is a permanent tax on Google's frontend changes.

### Tier 4 — Test & doc health (so progress is measurable)
- [x] **Lib test suite green** — 180 pass. The `dom::shadow` + cascading
  `blitz_document` failures were one real bug: `composed_children` returned the shadow
  root's children instead of the shadow root node, which the Blitz mirror needs. Fixed
  in `src/dom/shadow.rs` (host composes to `[shadow_root]`); the rest was mutex-poison
  cascade.
- [x] **ARCHITECTURE.md corrected** — was claiming SpiderMonkey/Boa engines and
  "no Chromium/Servo dependency." Now states the truth: V8-only (prebuilt Chromium JS
  engine), Stylo via blitz-dom, Vello; sovereignty redefined as integration + capability
  gating, not engine independence.
- [ ] **Visual regression flakiness.** Static fixtures (demo/demo-narrow/hydrated-smoke)
  had stale baselines from the branch's font-rendering rewrite — regenerated, now pass.
  `wikipedia-rust` is genuinely **non-deterministic** (~8% run-to-run) from async image
  loading: the headless render (`src/render/headless.rs`) paints immediately after
  `try_from_dom` without waiting for resource fetches to settle. blitz exposes
  `has_pending_critical_resources()` + `poll()` for a deterministic settle loop, but
  critical-resource tracking doesn't cover images, so a full fix needs image-load
  awareness. Not YouTube-blocking.
- [ ] **Add a real-YouTube smoke gate** distinct from the authored fixture. Today the
  fixture matches the shims, which proves the shims, not the engine.

---

## One-line status

*Updated 2026-06-20, branch `feature/youtube-polymer-shim-expanded`.*

Local fixture: **renders.** Live YouTube: **masthead bar + icon buttons paint; the
topbar logo now stamps and lays out at 123×112** (was 0×0) after the ShadyDOM
logical-tree composition landed — paint paths **~286 → 471**. Feed is still empty (needs
network continuation). Engine stability: the 5 per-load mutator panics remain gone, but
the now-larger stamped tree exposes a recurring **Tier-2 `MirrorIntegrity` "child mapping
mismatch"** (stable off-by-one, blitz `1762` vs expected `1761`) that floods the log on
every `sync_*`. Next *render* win is **stabilizing the two-DOM mirror** under the larger
tree; next *content* win is still **Tier 0 data**. See the 2026-06-20 update in
`docs/YOUTUBE_HYDRATION_INVESTIGATION-2026-06-18.md` for the root-cause writeup
(`query::find_parent` was severing adopted shadow-root host links).
