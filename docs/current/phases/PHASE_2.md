# Phase 2 — CSS Parser, Selector Engine, and Cascade

**Status: Mostly done** — major gaps closed May 2026. Remaining work is property registry, bucketed matching, and spec-complete pseudo-class coverage.

---

## Parsing pipeline

- [x] Rule block splitter — replaced by `cssparser::StyleSheetParser` + `QualifiedRuleParser::parse_prelude`. Handles all edge cases (strings with `}`, nested rules, at-rule blocks) correctly via the CSS tokenizer.
- [x] Declaration parsing — `cssparser::RuleBodyParser` + `AuroraDeclarationParser`
- [x] `!important` — `cssparser::parse_important`; stored as `Declaration.important: bool`; applied after normal declarations
- [x] External `<link rel="stylesheet">` fetching — `dom_styles.rs:collect_link_styles`
- [x] `@import` — `at_rules.rs:fetch_import_if_needed`, recursive up to depth 3
- [x] `@media` — **passthrough implemented** via `cssparser::StyleSheetParser` + `AtRuleParser`. Non-print media blocks included; `print`/`only print` dropped. `@supports` and `@layer` also passed through.
- [ ] `@media` condition evaluation — currently passes through all non-print queries without evaluating the condition (e.g. `max-width: 768px` always applies). A proper media query evaluator against the current viewport is still needed.
- [ ] `@keyframes` — stripped (no animation support yet)
- [ ] Specificity origin ordering — author vs user-agent vs inline not tracked as separate origins; all rules sorted in one list

---

## Selector engine

- [x] Tag, ID, class, attribute selectors (all operators: `=`, `~=`, `|=`, `^=`, `$=`, `*=`)
- [x] Descendant (` `) and child (`>`) combinators
- [x] Adjacent (`+`) and general sibling (`~`) combinators — **implemented** with sibling context threaded through `from_dom_node`
- [x] `:not()` with a single simple selector argument
- [x] `:root`
- [x] `:first-child`, `:last-child`, `:nth-child(an+b)`, `:first-of-type`, `:last-of-type` — **implemented** using sibling index passed from `from_dom_node`
- [x] `:is(selector-list)` — **implemented**; takes specificity of most-specific argument
- [x] `:where(selector-list)` — **implemented**; zero specificity
- [ ] State pseudo-classes — `:hover`, `:focus`, `:active`, `:visited`, `:checked`, `:disabled`, `:enabled` — always `false`; requires runtime event state
- [x] `:nth-of-type(an+b)`, `:only-child`, `:only-of-type`, `:lang()` — **implemented**
- [ ] `:empty` — needs DOM child access not available in `ElementData`; returns `false`
- [ ] `::before`, `::after` and other pseudo-elements — parsed as unknown pseudo-class, never rendered
- [x] Replace custom engine with `selectors` crate — **done**. `selectors::parser::Selector<AuroraSelectorImpl>` is now the `Selector` type. `CascadeElement` implements `selectors::Element`. Selector parsing uses `SelectorList::parse`. Matching uses `selectors::matching::matches_selector`.
- [x] Unify `src/js_boa/selectors/query.rs` with cascade engine — **done**. Both use `parse_selector_list` + `element_matches` from `selectors_impl.rs`.

---

## CSS custom properties

- [x] Custom properties stored in `Stylesheet.variables: BTreeMap<String, String>`
- [x] **Scoping bug fixed** — variables collected from all selectors, not just `:root`/`*`/`html` (`stylesheet.rs:295`)
- [x] `var()` in declaration values resolved at parse time via `resolve_variables`
- [x] Inherited custom property lookup in ancestor `StyleMap`s via `style_map_resolve.rs:resolve_vars`
- [ ] Per-element variable scoping — variables are still stored globally (last-write-wins); spec requires per-element inheritance down the tree
- [x] `var()` fallback with nested `var()` — **fixed** in `resolve_single_value`: uses depth-aware paren scanner and recursively resolves both the variable value and the fallback

---

## `calc()`, `min()`, `max()`, `clamp()`

- [x] `calc()` — **implemented** (`src/css/calc.rs`). Handles `px`, `%`, `em`, `rem`, `vw`, `vh` and all other `LengthValue` units. Supports `+`, `-`, `*`, `/` with correct precedence. Wired into `width_resolved`, `height_resolved`, `min_height_resolved`, `max_height_resolved`, `font_size_resolved`.
- [x] `min()`, `max()`, `clamp()` — **implemented** in `calc.rs`; wired into `style_map_resolve.rs` alongside `calc()`

---

## Length units

- [x] `px`, `%`, `rem`, `em`, `ch`, `ex`, `vw`, `vh`, `vmin`, `vmax`
- [x] `pt`, `pc`, `cm`, `mm`, `in`, `Q`
- [x] `svh`, `lvh`, `dvh`
- [x] `fr` (grid fractions) — parsed; resolves to 0 outside a grid container
- [x] `lh`, `rlh` (line-height relative) — approximated as `font_size` / `root_font_size`
- [ ] Container query units (`cqw`, `cqh`, etc.) — needs container size tracking

---

## Property inheritance

- [x] Expanded inherited property set — now includes `cursor`, `direction`, `letter-spacing`, `word-spacing`, `text-transform`, `text-indent`, `list-style-type`, `list-style-position`, `border-collapse`, `border-spacing`, `caption-side`, `empty-cells`, `quotes` in addition to the original 10
- [ ] Full property registry (parsing + initial value + inheritance flag per property) — still hardcoded strings, not a registry

---

## UA stylesheet

- [x] Block-level elements, headings with sizes and margins
- [x] Table display (`table`, `tr`, `td/th`, `thead/tbody/tfoot`, `col`, `colgroup`, `caption`)
- [x] List items (`ul`, `ol`, `li`, `dl`, `dt`, `dd`)
- [x] Inline elements and semantic tags
- [x] `pre` with `white-space: pre` and `font-family: monospace`
- [x] Link colours and `text-decoration: underline`
- [x] `sup { vertical-align: super; font-size: 0.75em; }` — **added**
- [x] `sub { vertical-align: sub; font-size: 0.75em; }` — **added**
- [x] `img { display: inline-block; }` — **added**
- [x] Form element styles (`input`, `button`, `select`, `textarea`) — **added** with `font-family: inherit`, `font-size: inherit`
- [x] `ol { list-style-type: decimal; }`, `ul { list-style-type: disc; }` — **added**
- [ ] `:link` in UA sheet never matches — state pseudo-classes require runtime event state

---

## Selector matching performance

- [x] Bucketed selector matching — **implemented** in `stylesheet.rs`. `Stylesheet` now holds a `HashMap<String, Vec<usize>>` index keyed by `#id`, `.class`, `tag`, or `*`. `styles_for()` queries only relevant buckets before running full selector matching.

---

## Outcome

Closes P0 #2 (no real CSS cascade), P1 #11 (quadratic var() — partially; scoping fixed, resolver unification pending), P1 #12 (unindexed selector matching — still open), P1 #13 (divergent selector parsers — still open), P1 #21 (style reaches into JS — closed), P1 #22 (hardcoded inheritance — open).
