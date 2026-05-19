# Phase 3 — Block Layout (Taffy)

**Status: In progress — Taffy wired for block/flex/grid; legacy stack still active for inline and replaced elements**

## Integration shape

- [ ] Decide between owned `TaffyTree<UserData>` vs custom `LayoutPartialTree` trait (Blitz uses `LayoutPartialTree` — recommended)
- [ ] Read [Taffy's `LayoutInput` / `LayoutOutput`](https://github.com/DioxusLabs/taffy/blob/main/src/tree/layout.rs)
- [ ] Read Taffy algorithm entry functions: [Flexbox](https://github.com/DioxusLabs/taffy/blob/main/src/compute/flexbox.rs#L166), [Block](https://github.com/DioxusLabs/taffy/blob/main/src/compute/block.rs#L244), [Grid](https://github.com/DioxusLabs/taffy/blob/main/src/compute/grid/mod.rs#L43)

## Wiring

- [x] Add `taffy` to `Cargo.toml`
- [x] Wire layout entry point (`tree.rs` → `engine.rs` → Taffy or legacy)
- [x] `display: none`, skipped children
- [x] Viewport-aware `style_to_taffy_with_viewport` (`vh`/`vw` resolve correctly)
- [x] Content-box sizing: expand width/height/min/max for Taffy border-box layout
- [x] Text intrinsic sizing via `font::measure_text`
- [x] `display: grid` mapped in `taffy_adapter.rs`
- [x] `position: absolute` / `fixed` mapped in `taffy_adapter.rs:83`
- [ ] `position: sticky` — currently falls through to static silently
- [ ] Replace `LayoutTree::from_style_tree_with_viewport` entry point — make it a call to Taffy's `compute_layout` with viewport as root `available_space`
- [ ] Replace `LayoutBox` with a typed leaf-or-branch enum that Taffy populates — remove the per-box `StyleMap` clone (`src/layout/box.rs:11`)
- [ ] Replace `find_node` / `hit_test` (`src/layout/box.rs:144–169`) with calls into the Taffy node store

## Cleanup — legacy files to delete once Taffy is the sole entry point

- [ ] Delete `src/layout/block.rs` *(still present)*
- [ ] Delete `src/layout/flex/` *(still present)*
- [ ] Delete `src/layout/construct.rs` *(still present)*
- [ ] Delete `src/layout/constraints.rs` *(still present)*
- [ ] Delete `src/layout/inline.rs` *(still present — moves to Phase 4)*
- [ ] Delete `src/layout/inline_sequence.rs` *(still present)*
- [ ] Delete `src/layout/inline_text.rs` *(still present)*
- [ ] Delete `src/layout/text_metrics.rs` *(still present)*

## Floats — requires Phase 4 (Parley) first

- [ ] Port Blitz's float glue — floats sit at the Taffy/Parley boundary and need both pieces in place

## Tests

- [ ] Port existing layout fixtures to integration tests against the Taffy-backed tree

## Outcome

Closes P0 #4 (StyleMap per box), P0 #8 (inline fragment — partially), P1 #18 (hit-test whole-tree-walk), P1 #23 (f32 layout drift — accept and document).
