# Aurora — Implementation Plan

> Philosophy: use Blitz-aligned crates (html5ever, cssparser, selectors, Taffy, Parley, AnyRender) as parts of the engine, not black boxes. Aurora stays independent (Path A). Bastion identity + capability-gated fetch is the differentiator.

Each phase has its own file in [`docs/current/phases/`](phases/).

---

## Status overview

| Phase | Topic | Status |
|---|---|---|
| [0](phases/PHASE_0.md) | Foundations & reference reading | Ongoing |
| [1](phases/PHASE_1.md) | HTML parser (html5ever) | **Done ✅** |
| [2](phases/PHASE_2.md) | CSS parser, selector engine, cascade | In progress |
| [3](phases/PHASE_3.md) | Block layout (Taffy) | In progress |
| [4](phases/PHASE_4.md) | Inline layout (Parley) | Started |
| [5](phases/PHASE_5.md) | Per-node invalidation & incremental reflow | Not started |
| [6](phases/PHASE_6.md) | Rendering layer (AnyRender) | Not started |
| [7](phases/PHASE_7.md) | Networking (reqwest) | Mostly done |
| [8](phases/PHASE_8.md) | Testing & CI | Partial |
| [9](phases/PHASE_9.md) | JS — Boa + DOM bridge | Foundation done |
| [10](phases/PHASE_10.md) | Strategic direction | **Decided ✅** |

---

## Issue ↔ Phase cross-reference

| Principal-review finding | Closed by phase |
|---|---|
| P0 #1 No HTML parser | 1 |
| P0 #2 No real CSS cascade | 2 |
| P0 #3 Reflow is a tree rebuild | 5 |
| P0 #4 StyleMap cloned per box | 3 |
| P0 #5 JS DOM bridge panics | 9 |
| P0 #6 fetch / XHR / observers theatrical | 7 + 9 |
| P0 #7 Latin-1 only font path | 4 |
| P0 #8 No inline fragment model | 4 |
| P0 #9 No compositing | 6 |
| P0 #10 Synchronous serial pipeline | 5 + 7 |
| P1 #11 Quadratic var() resolver | 2 |
| P1 #12 Unindexed selector matching | 2 |
| P1 #13 Divergent selector parsers | 2 |
| P1 #14 `<style>`/`<script>` hardcoded layer | 2 |
| P1 #15 Event-loop scheduling fragile | 5 |
| P1 #16 `requestIdleCallback === setTimeout` | 9 |
| P1 #17 `clearTimeout` after fire | 9 |
| P1 #18 Hit-testing whole-tree-walk | 3 |
| P1 #19 Scroll decoupled from document | 9 |
| P1 #20 Resize re-fetches images | 5 + 7 |
| P1 #21 Style module reaches into JS module | 2 |
| P1 #22 Inheritance hardcoded | 2 |
| P1 #23 f32 layout drift | 3 (accept + document) |
| P1 #24 No visual regression diff | 8 |
| P1 #25 Misleading reflow doc | Replace after Phase 5 lands |
