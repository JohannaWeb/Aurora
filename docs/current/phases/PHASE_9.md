# Phase 9 — JS (Boa + DOM Bridge)

**Status: Foundation done — Boa runtime, DOM bindings, timers, rAF in place; spec completeness open**

Do **not** add theatrical stubs. APIs are real or absent.

## Foundation — done

- [x] Boa runtime (`BoaRuntime`, `NodeRegistry`, `execute`, script load from DOM)
- [x] DOM constructors and accessors (`createElement`, `appendChild`, `setAttribute`, style properties, layout accessors)
- [x] Globals: `window`, `document`, `navigator`, `location` (partial)
- [x] `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval`
- [x] `requestAnimationFrame` + drain in window event loop (`window/app.rs`)
- [x] XHR / fetch polyfills on capability-gated `crate::fetch` (`js_boa/network.rs`)
- [x] Dirty-bit reflow + `perform_sync_reflow` from layout accessors
- [x] `querySelector` / `querySelectorAll` (unify with Phase 2 selector engine)
- [x] `localStorage` / `sessionStorage` via `install_storage`

## Events

- [ ] `target` / `currentTarget` — currently sets `_targetId` integer on the event object (`runtime.rs:125`), not a real DOM node
- [ ] Capture phase — no `capture` option handled anywhere in event registration
- [ ] Real `addEventListener` options: `{ capture, once, passive }`
- [ ] Real scroll / resize / load / `DOMContentLoaded` events
- [ ] O(1) event dispatch — `dispatch_event` and parent walk are O(N), acknowledged in `runtime.rs:82,168`

## Observers

- [ ] Real `MutationObserver` — `observers.rs:14–16` registers `noop_native()` for observe/unobserve/disconnect
- [ ] Real `IntersectionObserver` — needs viewport + scroll integration
- [ ] Real `ResizeObserver` — needs per-node dirty bits from Phase 5

## APIs

- [ ] Real `URL` parser — `network.rs:93–94` sets `hostname`, `pathname`, `origin`, `search`, `hash` all to `''` in inline JS
- [ ] Real `window.scrollY` — `globals/core.rs:67` hardcodes `0.0`, not live scroll state
- [ ] Real `fetch` backed by Phase 7's HTTP client (with capability CORS gating)
- [ ] Fix `requestIdleCallback === setTimeout` — `timers.rs:69` registers the same `timeout_fn(false)` as `setTimeout`; real rIC defers until idle

## Outcome

Closes P0 #5 (JS DOM bridge panics), P0 #6 (theatrical observers), P1 #16 (`requestIdleCallback`), P1 #17 (`clearTimeout` after fire), P1 #19 (scroll decoupled from document).
