# Phase 5 — Per-Node Invalidation and Incremental Reflow

**Status: Not started**

Confirmed in code: `src/js_boa/registry.rs:176` has `struct DirtyState { style: bool, layout: bool }` — two booleans for the entire document. Every JS mutation triggers a full tree rebuild.

## Work items

- [ ] Replace global `DirtyState { style: bool, layout: bool }` (`src/js_boa/registry.rs:137–141`) with per-node dirty bits on `StyledNode` / Taffy node
- [ ] On DOM mutation (`appendChild`, `setAttribute`, `style.X = ...`), mark affected node and ancestors dirty up to nearest BFC/IFC boundary
- [ ] Style-only changes (`color`, `background-color`, `visibility`, `text-decoration`) skip layout entirely
- [ ] Layout-affecting changes invalidate Taffy's cache for that subtree only
- [ ] Implement `flush_pending_layout()` and call it from `getBoundingClientRect`, `offsetWidth`, `offsetHeight`, `getComputedStyle`, `clientWidth`, `clientHeight`, `scrollWidth`, `scrollHeight`
- [ ] Stop synchronously re-fetching images on resize (`src/window/input.rs:54–55`) — issue fetches when layout discovers a new `<img src>`, store off-thread, repaint on arrival
- [ ] Add a generation counter on the document so JS `RefCell` callbacks can detect re-entrant mutation and defer instead of panic

## Outcome

Closes P0 #3 (reflow is a tree rebuild), P1 #15 (event-loop scheduling fragile), P1 #20 (resize re-fetches images). Makes forced sync layout from JS safe to ship.
