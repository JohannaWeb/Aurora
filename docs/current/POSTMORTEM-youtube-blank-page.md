# Postmortem: YouTube renders a blank page

> **Superseded (2026-06-11):** several "contributing causes" below have since been
> fixed and the render path has changed. Current verified state:
> [ANALYSIS-youtube-rendering-2026-06-11.md](ANALYSIS-youtube-rendering-2026-06-11.md);
> follow-up postmortem:
> [POSTMORTEM-youtube-still-not-rendering-2026-06-11.md](POSTMORTEM-youtube-still-not-rendering-2026-06-11.md).
> Kept as the historical record.

**Date:** 2026-06-10
**Status:** Root cause narrowed to Polymer template stamping silently bailing inside `ytd-app`; contributing causes identified and listed below. Not yet fixed.
**Branch:** `javascript/move-to-spider-monkey-rust-bindings`

---

## Symptom

`cargo run --release -- https://www.youtube.com` opens a window where the
browser chrome paints fine, but the content area shows only a handful of empty
grey skeleton boxes. The page never hydrates. No errors are visible to the
user; the process doesn't crash.

Reproduced deterministically headless:

```
AURORA_DEBUG_YOUTUBE=1 AURORA_SCREENSHOT=/tmp/yt.png AURORA_SCREENSHOT_FRAMES=30 \
  ./target/release/aurora https://www.youtube.com
```

The screenshot shows the identical blank content area. At render time the live
DOM holds **412 nodes — exactly the server-sent shell HTML**. Nothing YouTube's
JS was supposed to stamp into the page ever arrived.

## What was ruled out this session

The previous compat summary blamed three missing APIs:

```
localStorage.getItem x27
localStorage.setItem x6
MutationObserver x3
```

Both were implemented today (real BTreeMap-backed `localStorage`/
`sessionStorage`; real `MutationObserver` with childList/subtree/attributes and
checkpoint-based delivery). **Verified working:** the post-fix run produces an
empty compat summary — zero missing APIs, zero tracked JS exceptions. They were
real gaps, but they were not the cause of the blank page.

## Current status

What is already in place:

- Parsed `<template>` contents survive parsing and cloning.
- `document.currentScript` is wired through the runner and SpiderMonkey bridge.
- `customElements` upgrades preserve constructor/template lookup more cleanly.
- `template.content` is now a real fragment wrapper with stable identity.
- The YouTube probe logs `ctor.template`, `app._template`, `app.root`,
  `app.shadowRoot`, and `attachShadow()` calls for `ytd-app` and
  `ytd-masthead`.

What is still missing:

- A live YouTube probe run with `AURORA_DEBUG_YOUTUBE=1` so we can see which
  branch actually fails.
- If the probe shows template lookup is fine, the remaining work is in the
  attach/stamp path (`attachShadow`, `_attachDom`, or fragment insertion).
- If the probe shows template lookup is not fine, the remaining work is in
  `dom-module` registration or the custom-elements upgrade shim.
- After hydration works, the next likely issues are boot-time scheduler/timer
  behavior and the O(document) `customElements.define` scan.

## Boot timeline (from `logs/aurora_20260610_083436.log`)

| Time | Event |
|---|---|
| t+0s | 43 scripts extracted; 10 external scripts fetched in parallel (incl. `scheduler.js`, `webcomponents-all-noPatch.js`, the kevlar app bundle) |
| t+2s → t+108s | Scripts execute. **1119 `customElements.define` calls**, each taking ~145 ms (see Contributing cause 4) |
| t+108s | `ytd-masthead` and `ytd-app` upgrade (they exist in the shell HTML). `ytd-app.connectedCallback` fires and **returns without throwing** |
| t+108s | Script phase ends. 17 promise jobs drained. Silence. |
| after window/frames tick | YouTube's scheduler *is* alive: it logs "Failed to snapshot the document" and fires 6 `googlevideo.com/videoplayback` prefetches (all HTTP 403 — signed URLs bound to another client context; expected) |
| forever | No element is ever created or inserted. No `ytd-page-manager`, no `ytd-browse`, no `ytd-rich-grid-renderer` instance. DOM stays at 412 nodes. |

The key observation: **this is not a stall and not a crash.** The JS event
loop, timers, promises and scheduler all run. YouTube's app simply walks past
its own rendering step without error.

## Root cause (primary)

`ytd-app` upgrades and connects, but **Polymer never stamps its template**, so
the entire component tree under it (page-manager → browse → rich-grid → items)
is never instantiated. In Polymer, template stamping is conditional and fails
*silently by design*:

```js
// polymer's _readyClients / _attachDom path, simplified
if (this._template) { this._attachDom(this._stampTemplate(this._template)); }
// no template?  no DOM, no error, carry on
```

So the exact failure signature we observe — connectedCallback completes,
nothing is added to the DOM, no exception is tracked — is Polymer concluding
that `ytd-app` has no template (or an empty one) and skipping `_attachDom`.

Why the template resolves to nothing is the open question. Aurora *does*
implement `<template>.content` (sharing the NodePtr with the template element,
`document/api.rs:165`) and `document.importNode`. The remaining suspects, in
order of likelihood:

1. **The custom-elements shim's upgrade path breaks Polymer's class-side
   template resolution.** Aurora replaces `customElements` wholesale with a
   shim (`browser_api.rs`, `install_youtube_polyfills`) that upgrades elements
   via `Object.setPrototypeOf(el, ctor.prototype); ctor.call(el)`. During
   `ctor.call(el)`, `globalThis.HTMLElement` is swapped for a dummy
   constructor, and `_initializeProperties` may be stubbed to a no-op. Polymer
   memoizes `ctor.template` on first access using `this.constructor._template`
   / dom-module lookups — if the constructor chain ran against the dummy
   HTMLElement, the static-template memoization can land on the wrong object
   or never run.
2. **`dom-module` registration never happened.** Legacy Polymer components get
   templates from `<dom-module id="ytd-app">` elements registered at script
   evaluation time. If kevlar's `dom-module` registration depends on a DOM
   feature Aurora lacks (e.g. `document.currentScript`,
   `document.head.appendChild` side effects, `template.content` cloning during
   parse), the registry comes up empty and every `ctor.template` is `null`.
3. **Shady DOM gating.** `webcomponents-all-noPatch.js` was fetched and
   executed. If its ShadyDOM/ShadyCSS layer half-initialized against Aurora's
   DOM, Polymer's `_attachDom` may route into a Shady code path that throws
   inside a `try {} catch {}` it owns.

A probe is already wired into the upgrade shim (logging `ctor.template`,
`el._template`, `el.root`, `el.__dataEnabled`, and child counts for
`ytd-app` / `ytd-masthead` right after connect). Capture a run with
`AURORA_DEBUG_YOUTUBE=1` and append the result to this document so the
template-resolution failure can be pinned down instead of inferred.

## Contributing causes (confirmed, independent of the primary one)

These don't explain the blank page by themselves, but each one is a real
defect that will bite the moment the primary cause is fixed:

1. **No timer/microtask pump during script execution.**
   `runner/pipeline.rs::run_scripts` executes all 43 scripts back-to-back and
   only drains promise jobs. `setTimeout`/`requestIdleCallback`/rAF callbacks
   scheduled during boot sit frozen for the entire 108 s until the window event
   loop starts ticking. Any code that does `setTimeout(continueBoot, 0)` waits
   ~2 minutes. The screenshot path's `flush_ready_frame_tasks` (and the window's
   `run_frame_tasks`) are the only pumps.

2. **`requestIdleCallback` is aliased to `setTimeout` with no argument**
   (`timers.rs:35`). Real rIC callbacks receive an `IdleDeadline` with
   `timeRemaining()`/`didTimeout`. YouTube's `scheduler.js` uses
   rIC/rAF/setTimeout as its task-pump tiers; a callback that does
   `deadline.timeRemaining()` throws a TypeError on undefined. (No such
   exception was tracked in this run, so YT's scheduler likely took the
   setTimeout tier — but this is a landmine.)

3. **`MessagePort.postMessage` is a no-op** (`browser_api.rs:878`), and so is
   `window.postMessage` (`core.rs:102`), while the `MessageChannel` constructor
   exists and hands out ports. Any library that feature-detects MessageChannel
   and uses it as its flush mechanism (older React scheduler, some Promise
   polyfills) will enqueue work that never runs, silently. YouTube's current
   scheduler.js doesn't use it (verified by fetching the bundle), but the
   half-implemented surface is worse than its absence: feature detection
   passes, behaviour doesn't.

4. **`customElements.define` is O(whole document) per call.** The shim runs
   `document.querySelectorAll(name)` over the full document for every define to
   upgrade existing elements. 1119 defines × ~145 ms ≈ **107 of the 108 seconds
   of boot**. Real browsers parse YouTube's bundle in ~2 s. Fix: maintain one
   pass-deferred upgrade queue, or index parsed-but-undefined custom tags once
   and look up by name instead of re-querying the tree.

5. **MutationObserver v1 skips `characterData`.** Acceptable scope cut for
   Polymer's childList usage, but `webcomponents-all-noPatch.js` contains a
   Promise polyfill whose microtask scheduler is literally
   `new MutationObserver(flush).observe(textNode, {characterData: true})`.
   SpiderMonkey's native Promise means that path is probably dormant, but if
   anything in the bundle picks the polyfill Promise, its `.then` callbacks
   will never fire on Aurora.

6. **`videoplayback` prefetches 403.** The signed googlevideo URLs embedded in
   the page are bound to the requesting client (IP/UA/cookies). Aurora's
   refetch doesn't carry the same fingerprint → 403. Harmless for the blank
   page (it's a video prefetch), but worth knowing it's expected, not a bug.

## Why it looks like "nothing is happening"

Three separate design choices compound into total silence:

- Polymer treats "no template" as a valid configuration (no error).
- Aurora's `clear_pending_exception` swallows callback exceptions into the
  compat tracker (logged once per unique message) — fine, but combined with
  the above there was nothing to track.
- `[yt-fetch]`/`[yt-xhr]` traces are gated behind `AURORA_DEBUG_YOUTUBE=1`,
  while `[yt-life]` traces are unconditional — so the default log shows
  lifecycle events and then silence, which reads like a hang when it's
  actually a silent no-op.

## Verification artifacts

- `logs/aurora_20260610_081909.log` — user's windowed run (post-fix, blank)
- `logs/aurora_20260610_083436.log` — instrumented headless repro
- `/tmp/yt_test.png` — headless screenshot reproducing the blank content area
- Compat summary post-fix: empty (was: localStorage ×33, MutationObserver ×3)

## Recommended next steps (ordered)

1. Run the existing template probe for `ytd-app` and `ytd-masthead`, then
   record which template-resolution path is actually failing.
2. Fix the confirmed template path. The likely fixes are preserving
   class-side `template` resolution through the upgrade shim or implementing
   the `dom-module` behavior YouTube expects.
3. Remove the O(document) scan from `customElements.define`. It is currently
   dominating boot time and will continue to make the page feel broken even
   after hydration starts working.
4. Add a bounded timer/microtask pump between script executions in
   `run_scripts`, then make `requestIdleCallback` pass a real `IdleDeadline`
   object.
5. Either implement `MessagePort` delivery on the same checkpoint as promise
   jobs or remove the `MessageChannel` constructor so feature detection fails
   honestly.
6. After hydration works, expect the next wall at `youtubei/v1/browse`
   (innertube data fetches). The homepage does embed `ytInitialData` inline
   (verified, 2 occurrences in the served HTML), so first paint may not need
   it.

---

*Appendix pending: captured probe output for `ctor.template` / `el._template`
/ `el.root` on `ytd-app` and `ytd-masthead`.*
