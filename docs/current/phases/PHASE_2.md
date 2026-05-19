# Phase 2 — CSS Parser, Selector Engine, and Cascade

**Status: ~Half done — cssparser in, selectors crate not wired to DOM**

## Selector engine

- [ ] Replace `Selector::parse` / `SimpleSelector::parse` (`src/css/selector.rs`) with `selectors::parser::Parser`
- [ ] Implement `selectors::Element` trait on `crate::dom::NodePtr` — the bridge that lets the crate resolve matches against the tree
- [ ] Delete `src/js_boa/selectors/query.rs` and route JS-side `querySelector*` through the same engine — two divergent parsers is a confirmed bug

## Cascade

- [x] Replace `split('}')` rule splitter with `cssparser`-driven walker
- [x] Real `!important` cascade ordering (`important: bool` per declaration)
- [x] `display: <inside>/<outside>` model with `inline-block`, `grid`, `table`, `flow-root`, `inline-flex`, `inline-grid`, `list-item`, `none`
- [x] `auto` margins — `top`/`bottom` are no longer `f32`-only
- [x] Inline-style parsing moved out of `src/js_boa/` into `src/css/`
- [x] Missing length units: `pt`, `pc`, `cm`, `mm`, `in`, `Q`, `ch`, `ex`, `vmin`, `vmax`, `svh`/`lvh`/`dvh`
- [x] External `<link rel="stylesheet">` fetching (`src/css/dom_styles.rs:collect_link_styles`)

## Property system

- [ ] Replace the 10 hardcoded inherited properties in `src/style/node.rs:apply_inherited_element_styles` with a property registry (parsing + inheritance + initial value per property)
- [ ] Replace the quadratic `var()` resolver (`stylesheet.rs:resolve_variables`) — current loop calls `find("var(")` on full string up to 100× per value
- [ ] Replace the UA stylesheet (`src/css/stylesheet.rs:23`) with a real one (Servo's or Blitz's). Remove any remaining non-standard property names.

## Performance

- [ ] Add bucketed selector matching — hash rules by rightmost simple-selector tag/ID/class. `styles_for()` is currently O(rules × elements).
- [ ] Add `calc()`, `min()`, `max()`, `clamp()` evaluation — today `style_map_resolve.rs:38,60,82` detects `calc(` and returns `auto`/`none`

## Outcome

Closes P0 #2, P1 #11 (quadratic var()), P1 #12 (unindexed matching), P1 #13 (divergent parsers), P1 #21 (style reaches into JS), P1 #22 (hardcoded inheritance).
