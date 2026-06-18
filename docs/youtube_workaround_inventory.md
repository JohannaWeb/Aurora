# YouTube / Polymer / ShadyCSS Workaround Inventory

Action plan Phase 9 (Tasks 9.1–9.2). Each row is a rescue path that compensates
for a platform gap in Aurora's engine. When the platform feature in the "missing"
column lands, the workaround should be deleted (or it becomes a no-op) and the
listed regression coverage should be in place first.

Scope note: pure diagnostics (`trace`, `installGlobalErrorTracing`,
`probeCustomElementState`, `AURORA_DEBUG_*` gated logging) and genuine platform
features (the real `MutationObserver` in `src/js_v8/mutation_observer.rs`, the JS
`EventTarget`) are intentionally **not** listed — they are not workarounds.

| Workaround | File / function | Platform feature missing | Delete condition | Test coverage needed |
|------------|-----------------|--------------------------|------------------|----------------------|
| ShadyDOM event fallbacks (`__shady_addEventListener`/`__shady_dispatchEvent`/… on `Object.prototype`) | `custom_elements.js` `installShadyEventFallbacks` | Native ShadyDOM-compatible event delivery on every node | When all nodes have real `EventTarget` semantics and ShadyDOM `noPatch` mode is handled, so these stubs are never read | Event dispatch/bubbling across shadow host reaches listeners without the `__shady_*` stubs |
| Custom-element upgrade patching | `custom_elements.js` `patchHTMLElementForUpgrades`, `getDefinition`/`ensureDefinitionMetadata`/`installTemplateAccessor` | Native `customElements.define`/upgrade pipeline | When the engine upgrades custom elements natively (constructor → connected → attribute callbacks) | Define + connect + attribute-change lifecycle fires in order (existing `v8_custom_element_*` tests) |
| ShadyCSS-lite selector rewriting | `custom_elements.js` `scopeCss`/`rewriteScopedSelector`/`rewriteSelectorList` | Native Shadow DOM scoped styling (`:host`, `::slotted`) in Stylo | When Stylo resolves styles against a real shadow tree, so flattened-tree rewriting is unnecessary | `v8_shadycss_lite_rewrites_host_and_slotted_selectors`; a render test that `:host` styling matches native |
| dom-module `<style>` hoisting | `custom_elements.js` `shimDomModuleStyles`/`registerDomModule` | Native shadow-root stylesheet application | Same as ShadyCSS-lite — native shadow styling | A dom-module's styles apply to its component without being hoisted to `<head>` |
| ShadyCSS instrumentation + once-per-page warning | `custom_elements.js` `shadyCssRecord`/`shadyCssWarnOnce`; `runtime.rs` `AURORA_DEBUG_SHADYCSS` | (Diagnostic for the above; not itself a gap) | Delete together with ShadyCSS-lite | `v8_shadycss_diagnostics_are_gated_behind_debug_flag`, `v8_shadycss_emits_once_per_page_warning` |
| Polymer `this.$` id-map hooks | `custom_elements.js` `installTemplateIdAccessors`/`installInstanceTemplateIdAccessors`/`installPolymerIdMapHooks`/`rebuildPolymerIdMap`/`findStampedId` | Native shadow-root `getElementById`/`$` resolution | When `this.$.id` resolves through a real shadow tree query | Polymer component resolves `this.$.<id>` to the stamped node (existing `test_polymer_id_map` cases) |
| Polymer data-binding shim | `custom_elements.js` `installBindingHooks`/`applyStampedBindings`/`resolveBindingExpr`/`resolveBindingPath`/`parseBindingParts`; `__aurora_sweep_bindings__` | Polymer property-effects fully stamping `[[…]]`/`{{…}}` in the flattened tree | When Polymer's own property-effects run to completion (no leftover annotations) | A Polymer component with `[[prop]]` / computed bindings renders resolved text; 0 unresolved annotations document-wide |
| Polymer binding sweep call | `runner/pipeline.rs` `apply_polymer_bindings` | Same as data-binding shim — renderers stamped natively never miss hooks | Delete with the data-binding shim | Covered by the binding-shim render test above |
| `polymer_shim.js` | `js_polyfills/polymer_shim.js` (loaded last in `runtime.rs` bootstrap) | Polymer/Kevlar internals our engine doesn't provide | When the relevant Polymer internals work unaided (review per-shim) | Per-shim; needs its own breakdown before deletion |
| Prop-bag sanitizer ("STATIC not SIGNAL") | `custom_elements.js` `sanitizePropBag` | Correct prototype/own-property semantics so the callable `style`/`__shady_*` shims don't leak into Polymer prop bags | When the fallback `Object.prototype` shims (row 1) are gone, removing the leak source | A Polymer element with a `style` getter sets up props without throwing (existing `yt-attributed-string` test) |
| Layout metric heuristic fallback | `v8_post.js` `widthFallback`/`heightFallback` in `metric()` | Real Blitz layout for shadow/collapsed components | When shadow content lays out with non-zero boxes, so `__aurora_metric__` always returns a real value (Phase 8.2 wired the real path; this is the unlaid-out fallback only) | `v8_layout_accessors_read_blitz_layout` (real path); a collapsed-element case for the fallback |
| `document.elementFromPoint` stub | `v8_post.js` (`return null`) | Document-level hit-test bridge to Blitz | When wired to `BlitzDocument::hit_test_dom_node` | Point over a known element returns that element |
| `offsetTop`/`offsetLeft`/`scrollTop`/`scrollLeft` zero stubs | `js_v8/node_create.rs` `create_js_node` | Position/scroll-offset accessors reading Blitz `final_layout` | When routed through `__aurora_metric__` like width/height | Element at known position reports matching `offsetTop`/`offsetLeft` |

## Notes

- Aurora's current YouTube target is a hostile modern-web benchmark, not full
  YouTube rendering or playback. The next gate is one real content-bearing route:
  enough application data, DOM mutation, custom elements, style, and layout must
  work together to paint stable content. The dominant blocker is upstream of most
  of these rows: YouTube's closure-private **bootstrap navigation never fires**, so
  the page-content components (`ytd-browse`, `ytd-watch-flexy`) are never
  instantiated (see `docs/YOUTUBE_RENDERING_STABILIZATION_ACTION_PLAN.md` "Live
  YouTube Status"). The data-binding/stamping workarounds above only matter once
  those components exist.
- A removed rescue path worth recording: `drive_polymer_page_manager_navigation`
  (history at git `3084a1e`) drove `ytd-page-manager.updatePageData` to instantiate
  the page. It was cut in `6f137c1` and, when re-tried on 2026-06-18, made the
  visible render *worse* (swapped an empty browse page over the shell, collapsing
  paint 322→32). Do not reintroduce it without solving the empty-feed problem
  (continuation fetches / a content-rich page).
