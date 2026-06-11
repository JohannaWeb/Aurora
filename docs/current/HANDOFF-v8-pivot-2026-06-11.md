# Handoff: V8 pivot — build-system done, js_v8 itself doesn't compile

**Date:** 2026-06-11
**Status:** Open. Decision needed before continuing.
**Context:** Johanna decided to pivot Aurora's active JS engine from
SpiderMonkey to V8 (decision recorded in agent memory as
`project_v8_pivot_decision`, confirmed three times — do not re-litigate the
choice itself). This doc covers what was done tonight and the blocker hit at
the end of the session.

---

## What's done (verified working)

Made `engine-spidermonkey` (mozjs) and `v8` mutually-exclusive, optional Cargo
features so the binary can be built with either engine but not both (they
can't be statically linked together — duplicate `v8::internal::*` /
`diplomat_free` symbols).

- `Cargo.toml`: `mozjs` is now `optional = true`, gated behind a new
  `engine-spidermonkey` feature (in `default`). `v8` feature unchanged.
- `src/lib.rs` / `src/main.rs`: added a `compile_error!` guard if both
  features are enabled together; `mod js_sm` is now
  `#[cfg(feature = "engine-spidermonkey")]`.
- `src/js_engine.rs`: `EngineKind::from_env()` / new `default_engine()` fall
  back to V8 when SpiderMonkey isn't compiled in, and recognize
  `AURORA_JS_ENGINE=spidermonkey|sm` only when it is. The `create_runtime`
  match arm for `SpiderMonkey` is now feature-gated like the Boa/V8 arms
  already were.
- Consolidated three divergent copies of `serialize_outer_html`
  (`js_sm/serialization.rs`, a `js_boa` one, and a wrong reference from
  `js_v8`) into a single engine-agnostic `crate::dom::serialize_html`,
  re-exported as `crate::dom::serialize_outer_html`. Updated all four call
  sites (`js_sm/document/api.rs`, `runner/pipeline.rs`, `window/input.rs`,
  `js_v8/node_create.rs`).
- Also added `__shady_attachShadow` to `install_element_methods` in
  `js_sm/globals/browser_api.rs`, aliasing `attachShadow`'s implementation
  (`element_attach_shadow`, made `pub(in crate::js_sm)` and re-exported from
  `js_sm/document/mod.rs`). This was a separate small fix requested earlier in
  the session, unrelated to the V8 work but included in the same uncommitted
  diff.

**Verified:** default build (`cargo check`, SpiderMonkey) is clean — exit 0,
only the 2 pre-existing unrelated warnings. **Nothing has been committed
yet** — all of the above is uncommitted working-tree changes.

---

## The blocker: `src/js_v8/` doesn't compile — 423 errors

Ran `cargo check --no-default-features --features v8`. Result: **423 errors**
(cargo reports "321 previous errors" because some are deduplicated/grouped,
but per-file the counts sum to 423), almost all in:

| file | lines | errors |
|---|---|---|
| `src/js_v8/runtime.rs` | 1019 | 220 |
| `src/js_v8/node_create.rs` | 639 | ~124 |
| `src/js_v8/style_class/style.rs` | 323 | ~41 |
| `src/js_v8/style_class/classlist.rs` | 198 | ~38 |

Error breakdown across the module: 194× `E0308` (type mismatch), 61×
`E0631` (closure/fn-trait mismatch), 28× `E0277`, 27× `E0599` (missing
method), 5× `E0433`, 4× `E0593`, 2× `E0271`.

### Root cause

`Cargo.toml` pins `v8 = "150.0.0"` (the rusty_v8 crate), added in today's
commit `c6331b3` alongside `js_v8` itself. **rusty_v8 150.x ships a
brand-new, very recent "Pin"-based scope API** (`PinnedRef<'_, HandleScope<'_>>`,
`PinScope`, the `v8::scope!`/`v8::scope_with_context!`/`v8::tc_scope!` macros,
typestate `ScopeStorage` → `Pin<&mut ScopeStorage<T>>` → `init()` →
`PinnedRef`). This is documented at length in `v8-150.0.0/src/scope.rs` (a
~150-line module doc with its own tutorial).

The existing `js_v8` code was **never compiled** — it mixes:
- old-style direct `&mut HandleScope<'_>` parameters (doesn't exist in 150),
- the *new* `v8::scope!`/`tc_scope!`/`scope_with_context!` macros (these
  *do* exist in 150 — so whoever/whatever wrote this had partial awareness of
  the new API),
- `ObjectTemplate::set_accessor_with_data` /
  `set_accessor_with_data_setter` (these methods don't exist in 150 *or*
  130 — only `set_accessor`, `set_accessor_with_setter`,
  `set_accessor_with_configuration` exist),
- calls to `Rc<NodeRegistry>::mark_layout_dirty` /
  `mark_style_dirty`, which were never added to `src/js_v8/registry.rs`
  (it only has `take_needs_reflow`/`clear_dirty_bits`/`has_dirty_bits`,
  backed by a private `DirtyState { style, layout }` — adding the two
  setter methods is trivial, ~10 lines, but doesn't move the needle on the
  other 400 errors).

It's effectively unvalidated scaffolding, not a working bridge with a few
broken call sites.

### Downgrade experiment (tried, reverted, no working-tree changes left)

Tried pinning `v8 = "130"` (last version confirmed to predate the Pin
redesign — `&mut HandleScope` works, no `PinnedRef`/`scope!` macros exist).
Result: 227 errors (down from 423), but **168 of those are cascading
"cannot find `scope`/`scope_with_context`/`tc_scope` in `v8`"** — because the
code *also* relies on the new macros that don't exist pre-150. So v130 isn't
simply "the version this was written for" either; it's a different incoherent
mix.

Tried `v8 = "140"`: fails earlier — a transitive dep (`temporal_rs`) doesn't
build at all in this toolchain, unrelated to our code. Not viable without
further investigation.

**Net: there is no version of the `v8` crate the existing `src/js_v8` code
actually compiles against.** Cargo.toml/Cargo.lock are back to the original
`150.0.0` state (no diff beyond the legitimate feature-gating changes above).

---

## Decision needed before continuing

Getting `--features v8` to compile is **not a bug-fix pass** — it's
effectively writing the V8 embedding layer (`runtime.rs` especially, 1019
lines / 220 errors) from scratch against rusty_v8 150's Pin/typestate scope
API, which is unusual and not well-represented in common Rust patterns. This
is a multi-session rewrite, not a quick follow-up.

Two real options for next session:

1. **Rewrite forward against rusty_v8 150** (current pin). Read
   `v8-150.0.0/src/scope.rs`'s module doc carefully first (it's short and
   explains the `scope!`/`PinnedRef`/`PinScope` patterns), then rebuild
   `runtime.rs` function-by-function. `node_create.rs` and
   `style_class/*.rs` likely follow the same patterns once `runtime.rs`
   establishes the idioms. Keeps the newest V8.

2. **Pick a different/older `v8` version** with a stable, conventional
   `&mut HandleScope` API (no Pin macros) and rewrite `js_v8` against *that*
   instead — possibly less exotic/more documented, but still a rewrite (227+
   errors at v130, and v130's own ecosystem compat isn't fully verified
   either — v140 already broke on a transitive dep).

Either way, expect to treat `src/js_v8/` as "design and build the V8 bridge"
rather than "fix it" — `js_sm`'s structure (registry, node_create, tree/,
selectors/, style_class/) is a reasonable reference for *what* needs to
exist; the V8-specific scope/handle plumbing is what's unsolved.

---

## Task tracker state

- **#1** (make mozjs optional/mutually exclusive with v8): code complete,
  default build verified. Should be marked done once this is committed.
- **#2** (verify `--features v8` builds/links): blocked on the above —
  can't even compile yet, let alone link.
- **#3** (survey js_v8 DOM bridge completeness vs js_sm): superseded —
  the bridge isn't "incomplete", it's non-compiling scaffolding; survey
  folds into whichever rewrite option is chosen.

## Also pending from earlier in the session

- The `__shady_attachShadow` one-liner (done, see above) and the
  `this.

  empty-id-map` investigation in SpiderMonkey/Polymer hydration (diagnosed —
  `node.attributes`/NamedNodeMap not implemented in `js_sm`'s DOM bridge —
  but unconfirmed as root cause, and now secondary since SpiderMonkey work is
  paused per the V8 pivot).

## Nothing committed

All changes described above (feature-gating + `__shady_attachShadow` +
`serialize_outer_html` consolidation) are uncommitted working-tree changes.
`git status` / `git diff` will show the full set when you're back.
