# Critical Implementation Plan: Reflow Next Steps

> Author: Principal Engineer, Mozilla Rendering Team
> Review context: Aurora reflow implementation phase 1 complete

---

## Executive Summary

The timer-driven reflow foundation is in place and the synthetic fixtures pass. This is good — you have a working frame loop. But you're not done. The current implementation is a **proof of concept**, not a production rendering engine. Several critical pieces are missing or need hardening before Aurora can handle real-world web content.

---

## P0: Must Fix Before Shipping Any Real Page

### 1. Forced Synchronous Layout

**Current state:** `offsetWidth`, `offsetHeight`, `getBoundingClientRect()` return stale or zero values.

**Why it matters:** Almost every non-trivial JS page reads layout during execution. jQuery's `.width()`, React's measurement hooks, infinite scroll implementations — they all force layout. If you don't support this, the page either breaks silently or you get the dreaded "forced synchronous layout" performance death spiral where every frame triggers full recalc.

**Required work:**
- [ ] Implement `flush_pending_layout()` that runs reflow synchronously
- [ ] Call it from `getBoundingClientRect()`, `offsetWidth`, `offsetHeight`, `getComputedStyle()`
- [ ] Store pending JS call, run reflow at top of call stack, then resume JS (avoids `RefCell` panic)
- [ ] Handle edge case: forced layout inside `setTimeout` callback triggered by mutation (re-entrancy)

### 2. Input-Triggered Reflow

**Current state:** No user input triggers script execution.

**Why it matters:** Click handlers, form submissions, focus events — they all run JS that mutates DOM.

**Required work:**
- [ ] Wire up `WindowEvent::MouseInput` to dispatch to JS event handlers
- [ ] Wire up `WindowEvent::KeyboardInput` for key events
- [ ] Ensure event handlers can mutate DOM and trigger the reflow path
- [ ] Test: click button that adds class to container, expect layout change

---

## P1: Should Fix Before "Real Page" Testing

### 3. Fine-Grained Invalidation (Performance)

**Current state:** Coarse invalidation — any mutation dirties everything.

**Why it matters:** For a 200-node page, full rebuild is fine. For a 2000-node page, you're burning milliseconds. Real pages have thousands of nodes.

**Required work:**
- [ ] Add dirty bits to each `LayoutNode`: `needs_style_recalc`, `needs_layout`
- [ ] Propagate dirtiness up the tree on mutation (mark ancestors as dirty)
- [ ] In `LayoutTree::build()`, skip subtrees where both bits are clean
- [ ] Add benchmarks to measure improvement

### 4. Timer Precision and Drift

**Current state:** `setTimeout(fn, 0)` likely runs immediately.

**Why it matters:** Per the REFLOW_ANALYSIS.md, zero-delay timers must still go through the queue. Misordering breaks Promise resolution order.

**Required work:**
- [ ] Ensure all timers go through `tick()` regardless of delay
- [ ] Add a "budget" to prevent infinite timer loops (max ~100 callbacks per frame)
- [ ] Test: chain of `setTimeout(fn, 0)` must not starve rAF

### 5. Layout Box Reuse

**Current state:** Every reflow allocates fresh `LayoutBox` structs.

**Why it matters:** Allocation churn in a 60fps loop causes GC pauses (even in Rust, the allocator fights).

**Required work:**
- [ ] Add a `LayoutBoxPool` that recycles boxes between reflows
- [ ] Only allocate for new nodes; reuse boxes where DOM node is same

---

## P2: Nice to Have Before Production

### 6. CSS Transitions and Animations

**Current state:** CSS transitions are not implemented.

**Why it matters:** Half of modern web pages use `transition` or `@keyframes`.

**Required work:**
- [ ] Track transition state per element
- [ ] Interpolate computed values over time
- [ ] Trigger repaint on each frame of transition

### 7. Element.getBoundingClientRect() Recalculation

**Current state:** Returns stale geometry.

**Why it matters:** Mobile scroll libraries, floating headers, anything that tracks element position.

**Required work:**
- [ ] After reflow, walk the layout tree and compute accurate rects
- [ ] Cache rects per-element until next reflow

### 8. Memory Pressure Handling

**Current state:** No cleanup of timer IDs, layout boxes, or style computations.

**Why it matters:** Long-running sessions will leak.

**Required work:**
- [ ] Implement `clearTimeout`/`clearInterval` that removes IDs from queue
- [ ] Prune dead timer IDs on each tick
- [ ] Consider soft limits on layout box count

---

## Recommended Order

```
Phase 1 (This week):
  - [ ] P0 #1: Forced synchronous layout
  - [ ] P0 #2: Input-triggered reflow

Phase 2 (Next week):
  - [ ] P1 #3: Fine-grained invalidation
  - [ ] P1 #4: Timer precision fix

Phase 3 (When needed):
  - [ ] P1 #5: Layout box reuse
  - [ ] P2 #6-8: CSS animations, getBCR, memory
```

---

## What NOT to Do Yet

- **Don't implement WebGL rendering.** The 2D path works. Don't add complexity.
- **Don't add WebSocket or fetch streaming.** The fetch infrastructure is fine for initial pages.
- **Don't attempt shadow DOM or custom elements.** These are enormous.
- **Don't write your own CSS parser.** You're using a library. Don't touch it.

---

## The Real Test

Once P0 is complete, try loading a simple real page:
- A static HTML page with a click handler
- Or a page with one `setTimeout` that adds content

Not Google. Not Facebook. Not any page with analytics or ads. Start small.

If that works, you've got a rendering engine. If it panics on the `RefCell` borrow, go back and fix the re-entrancy path — that's the hard part, and there's no way around it.