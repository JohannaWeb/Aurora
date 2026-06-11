# Analysis: Why YouTube Still Isn't Rendering (2026-06-11)

**Engine under analysis:** SpiderMonkey (default, `src/js_engine.rs:21-27`). V8 exists
as a backend but is execute-only — no DOM bridge, no timers, no events
(`src/js_v8/runtime.rs:90-133`, the methods are documented "honest no-ops"). Boa is
feature-gated and abandoned for this goal. **Every statement below was verified
against the code at `d5ff88e` unless explicitly marked inferred.**

This supersedes the analysis sections of
[POSTMORTEM-youtube-blank-page.md](POSTMORTEM-youtube-blank-page.md) (2026-06-10) and
[YOUTUBE-whats-left.md](YOUTUBE-whats-left.md). Several claims in those documents are
now stale; they are itemized below.

---

## 1. What the old docs claimed vs. what the code says today

The 2026-06-10 postmortem listed one primary cause (Polymer silently skipping
template stamping in `ytd-app`) and six contributing causes. The contributing causes
have mostly been **fixed since**, which the old doc does not reflect:

| 2026-06-10 claim | Status today | Evidence |
|---|---|---|
| No timer/microtask pump during script execution (108 s frozen boot) | **Fixed.** `pump_ready_work` runs after every script, after `DOMContentLoaded`, and after `load` (bounded at 8 tick iterations each) | `src/runner/pipeline.rs:164-182` |
| `requestIdleCallback` aliased to `setTimeout`, no `IdleDeadline` | **Fixed.** rIC is a distinct idle timer; callbacks receive a real deadline object with `didTimeout` and `timeRemaining()` | `src/js_sm/globals/timers.rs:54-99`, `src/js_sm/runtime.rs:39-49,308-317` |
| `MessagePort.postMessage` is a no-op | **Fixed.** Ports deliver to their peer via `queueMicrotask`, with `onmessage` + listener lists + `close()` semantics | `src/js_sm/globals/browser_api.rs:1260-1311` |
| `window.postMessage` is a no-op | **Still true.** | `src/js_sm/globals/core.rs:102` |
| `customElements.define` is O(document) per call (1119 defines ≈ 107 s) | **Fixed.** Defines flush a per-tag pending queue. The queue is primed once from the parsed DOM (`prime_custom_elements`) and maintained by a patched `document.createElement` and the native node-wrapper hook | `src/js_sm/globals/browser_api.rs:1869-1897,1928-1938`, `src/js_sm/document/mod.rs:249-256,290+`, `src/js_sm/document/api.rs:214-226` |
| MutationObserver skips `characterData` | Unchanged (still a scoped cut) | `src/js_sm/mutation_observer.rs` |
| `videoplayback` prefetch 403s | Unchanged, still expected (signed URLs bound to another client) | n/a |

Two larger things changed that the old docs don't know about at all:

1. **`attachShadow` now exists** and deliberately proxies onto the host element's own
   `NodePtr` — `shadowRoot.appendChild`/`innerHTML` writes land as real light-DOM
   children of the host so they can actually paint
   (`src/js_sm/document/api.rs:1246-1405`). Encapsulation is knowingly sacrificed.
2. **The render path was replaced.** First paint and *every subsequent reflow* now
   serialize the live legacy DOM back to an HTML string
   (`src/js_sm/serialization.rs`) and re-parse it into a Blitz document
   (html5ever → Stylo → Vello) — `src/runner/pipeline.rs:53-57,337-346` and
   `src/window/input.rs:71-85`. This fixes "bootstrap mutations invisible at first
   paint" but introduces new defects (§3).

## 2. Layer 1 — Hydration: the primary cause is *still unconfirmed*

The 2026-06-10 root-cause hypothesis stands: `ytd-app` upgrades and connects, but
Polymer never stamps its template, so no `ytd-page-manager`/`ytd-browse`/grid is
ever created. The decisive experiment — a live run with `AURORA_DEBUG_YOUTUBE=1`
capturing the probe — **has still never been executed**. There is no `logs/`
directory in the repo and no probe output anywhere; the probe instrumentation has
meanwhile been *expanded* (static-template chain walk, non-mutating `ctor.template`
read that undoes Polymer's `_template` memoization, dom-module fallback logging —
`src/js_sm/globals/browser_api.rs:1489-1614`) but never once run against real
YouTube. Until that happens, everything in this section is ranked inference.

### Ranked suspects in the current shim (code-reviewed 2026-06-11)

**(a) Class-style constructors are never replayed during upgrade.**
`tryUpgrade` swaps the prototype and fires `connectedCallback`, but
`shouldReplayConstructor` explicitly refuses any constructor whose source starts
with `class ` (`src/js_sm/globals/browser_api.rs:1780-1856`). Polymer's entire
instance state machine (`_initializeProperties` → `_enableProperties` → `ready()` →
`_readyClients` → `_attachDom(_stampTemplate(...))`) is rooted in constructor-time
init. An upgraded element whose constructor never ran produces *exactly* the
observed signature: connect completes, no exception, no DOM output. Whether this
fires depends on which bundle YouTube serves (see (d)). Notably the machinery to fix
it already exists: `PatchedHTMLElement` returns the top of `upgradeStack` when
invoked as a base constructor (`browser_api.rs:1372-1402`), so replaying classes via
`upgradeStack.push(el); Reflect.construct(ctor, []);` should construct *onto* the
existing element with no new infrastructure.

**(b) Template resolution through the shim's accessor.**
`installTemplateAccessor` now defers to inherited static `template` getters and only
falls back to the `dom-module` registry (`browser_api.rs:1674-1753`), and it avoids
poisoning Polymer's own-property `_template` cache — this is much better than what
the 2026-06-10 doc described. Remaining risk: the accessor is only installed when
the ctor has an *own* `template` descriptor or *no* inherited one; combined with
(a), a never-constructed instance may still resolve `null`.

**(c) dom-module registration depends on upgrade order.**
`registerDomModule` only runs when a `dom-module` element passes through
`tryUpgrade` (`browser_api.rs:1852-1854`), which requires YouTube's bundle to have
called `customElements.define('dom-module', …)` *and* the element to be in the
pending queue. Runtime-created modules are caught by the patched `createElement`
(installed before scripts run, `document/mod.rs:249-254`), so the path is plausible
but unproven.

**(d) Which bundle does Aurora even get?**
Aurora's UA is `Aurora/0.1 (...)` (`src/fetch/http.rs:15-16`). YouTube serves
unknown UAs the legacy-browser variant — consistent with the 2026-06-10 boot log
showing `webcomponents-all-noPatch.js` and the ES5 custom-elements adapter. If
that's still true, constructors are ES5 functions, (a) doesn't fire, and the suspect
list inverts toward (b)/(c) plus the ES5 adapter's interaction with the shimmed
registry (the adapter wraps defines with `Reflect.construct(HTMLElement, …)`, which
the `upgradeStack` patch was built for). **The probe decides this in one run.**

Minor confirmed nits in the same area: `whenDefined` permanently re-wraps
`customElements.define` per waiter (`browser_api.rs:1943-1951`) — a growing closure
chain, functional but O(waiters) per define; rIC's `didTimeout` is true whenever a
timeout *option* was passed, not when expiry actually caused the call
(`src/js_sm/runtime.rs:310`).

## 3. Layer 2 — Rendering: confirmed defects in the new Blitz round-trip

Even if hydration completed perfectly today, the page would render wrong. These are
verified directly in the serializer, independent of any YouTube run:

1. **`<style>`/`<script>` contents are HTML-escaped.** `serialize_node` escapes
   *every* text node (`src/js_sm/serialization.rs:36-38,62-67`), but style/script
   are rawtext elements — entities are **not** decoded when Blitz re-parses them. A
   selector `ytd-app > #content` round-trips as `ytd-app &gt; #content` and the rule
   is dropped by Stylo. YouTube is styled almost entirely from JS-injected `<style>`
   elements; this silently destroys a large fraction of its CSS on every reflow.
2. **No doctype is emitted** (the `Node` enum has no doctype variant —
   `src/dom/node.rs`), so Blitz/Stylo parses the serialized page in **quirks mode**.
3. **Whole-document re-parse per dirty frame.** `WindowInput::reflow` re-serializes
   and re-parses the entire DOM into a fresh `BlitzDocument` on every reflow
   (`src/window/input.rs:71-85`). For a hydrated YouTube DOM (tens of thousands of
   nodes, multi-MB HTML) this is O(page) per frame; the app will appear frozen even
   with correct hydration. Acceptable scaffolding for now, but it must not survive
   contact with a working YouTube.
4. **Shadow flattening side effects.** Because shadow content lands in light DOM
   (§1), `:host`, `::slotted`, and scoping can't apply; ShadyCSS may also take wrong
   branches because `CSS.supports` always returns `false`
   (`browser_api.rs:1967-1973`) while the actual renderer (Stylo) supports custom
   properties natively.

## 4. Layer 3 — Platform gaps that gate the *next* milestones

Not blockers for first hydration, but each is load-bearing for what follows:
`window.postMessage` no-op (`core.rs:102`); `Event`/`CustomEvent` polyfills are
plain-object factories with no EventTarget integration
(`browser_api.rs:1221-1243`); MSE/`HTMLMediaElement` surface is shape-only with no
bytes reaching a decoder (`browser_api.rs:1977+`, by design per its comment);
`document.write` is a no-op (`document/mod.rs:213`).

## 5. Recommended sequence (ordered, each step falsifiable)

1. **Run the probe.** `AURORA_DEBUG_YOUTUBE=1` per
   [YOUTUBE-whats-left.md](YOUTUBE-whats-left.md). One run distinguishes §2(a)
   vs (b)/(c)/(d). Nothing else in §2 should be "fixed" before this.
2. If class constructors are being skipped: replay them with
   `Reflect.construct(ctor, [])` under `upgradeStack` (infrastructure already
   present, §2(a)).
3. Fix the serializer: emit rawtext children of `style`/`script` unescaped and
   prepend `<!DOCTYPE html>` (§3.1, §3.2). Small, testable, needed regardless of
   what the probe says.
4. Re-run the probe; append output to the 2026-06-11 postmortem.
5. Only then: incremental Blitz updates instead of whole-document re-parse (§3.3).
6. After first hydrated paint: expect the next wall at `youtubei/v1/browse`
   (innertube), softened by inline `ytInitialData` (verified present in the shell
   HTML on 2026-06-10).
