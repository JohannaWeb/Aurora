# SpiderMonkey/YouTube session handoff

Branch: `javascript/move-to-spider-monkey-rust-bindings`. Goal: get YouTube to
render real content under the new mozjs (SpiderMonkey) JS engine
(`src/js_sm`). Confirmed via `strings target/debug/aurora | grep js_sm\|js_boa`
that SpiderMonkey is the engine actually compiled/run — `js_boa` is gated
behind the off-by-default `engine-boa` feature. Stale `"Boa: ..."` log strings
in `src/runner/pipeline.rs` (lines ~149/160/168) are cosmetic and misleading —
safe to update/remove.

## Done this session
1. Fixed a segfault: `execute()` in `src/js_sm/runtime.rs` now holds one
   `AutoRealm` across both `evaluate_script` and `pending_exception_string`
   (previously `evaluate_script`'s internal realm guard dropped before the
   exception was read → null deref).
2. Discovered & fixed a systemic bug: `define_fn` never set `JSFUN_CONSTRUCTOR`
   (`0x400`, see `src/js_sm/utils.rs`), so none of the stub "constructors"
   (Image, CustomEvent, URL, Blob, XHR, WebSocket, observers, ...) were
   actually callable with `new`. Added `define_ctor` / `define_ctor_with_prototype`
   helpers and converted ~15 registrations to use them.
3. Added missing globals: `navigator`, `performance.timing`, `Image`,
   `Element` (with prototype), `Event`/`CustomEvent.prototype`.
4. Added `<canvas>` support in `src/js_sm/document/api.rs`
   (`create_js_node`): width/height props + `getContext('2d')` returning a
   stub `CanvasRenderingContext2D` (`build_canvas_2d_context` + helpers
   `canvas_get_context`, `canvas_to_data_url`, `canvas_measure_text`,
   `canvas_get_image_data`, `canvas_create_gradient`). `webgl*` contexts
   return `null` (no GL backend). Build is green (`cargo build`).

## Remaining known JS errors on youtube.com (as of last run, v3 log)
- `TypeError: can't access property "prototype", Aa is undefined` — a
  minified global constructor reference is undefined. Likely candidates:
  `HTMLElement`, `Node`, `EventTarget`, or `DOMException` not defined as
  globals (only instances are faked via `create_js_node`).
- `TypeError: c.initCustomEvent is not a function` — deprecated
  `CustomEvent.prototype.initCustomEvent(type, bubbles, cancelable, detail)`
  needs adding to the Event/CustomEvent prototype in
  `src/js_sm/globals/browser_api.rs`.
- `getContext` error should now be gone after this session's canvas fix —
  verify with a fresh run.

## THE BIG BLOCKER (read this first)
Even with all JS errors fixed, YouTube will likely still render blank because
its 9577 KB main app bundle (`kevlar_base_module` etc.) is **skipped** by:

```
src/runner/pipeline.rs:89-92
const MAX_SCRIPT_BYTES: usize = 256 * 1024;
const MAX_TOTAL_SCRIPT_BYTES: usize = 512 * 1024;
```

The justifying comment says "Boa has no JIT, so multi-MB bundles take minutes
to interpret" — but **SpiderMonkey has a JIT** and this comment/limit is
stale from the Boa era. Raising or removing this cap is probably the real
unlock for YouTube (and other heavy SPA sites). Worth discussing with the
user before changing — it could meaningfully affect run time/memory.

## How to test
```
cargo build && cargo run --release -- https://youtube.com
```
Screenshots land in `tests/screenshots/`; stderr shows `JS Error: ...` lines
and `Boa: skipping ... (over 256KB limit)` messages (cosmetic "Boa:" prefix
only — SpiderMonkey is what's running).
