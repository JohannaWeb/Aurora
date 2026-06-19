# Aurora YouTube Rendering Stabilization Action Plan

## Goal

Aurora targets YouTube as a hostile, modern-web integration benchmark. The first milestone is not full YouTube rendering or playback; it is proving that Aurora can bootstrap enough YouTube application data, DOM mutation, custom elements, style, and layout to render one real content-bearing route reliably.

This plan reduces Aurora's YouTube fragility by moving away from the current "legacy DOM + mirrored Blitz DOM" architecture toward one authoritative rendering DOM, while keeping the current branch working during migration. The teardown identifies the main architectural problem as split-brain behavior: JavaScript mutates the legacy DOM, while Stylo/Blitz paints a separate mirror maintained through `sync_*` hooks.

## How To Use This Checklist

- `[ ]` means not started.
- `[~]` means in progress.
- `[x]` means implemented and verified.
- Each completed item should include a terse note with key files and tests.
- At the start of each implementation chunk, mark the active task as `[~]`.
- When code and verification are complete, change it to `[x]` and add a one-line completion note.
- If verification is blocked, leave it `[~]` and add `Blocked:` with the exact reason.
- Do not mark a phase complete until every task in that phase is `[x]` or explicitly marked deferred with a reason.

## Current Notes

- Workspace currently has stabilization changes in tracked files plus untracked `scratch/` artifacts. Do not delete or rely on the scratch artifacts unless a task explicitly says so.
- Local workspace path is `/home/johanna/projects/Aurora`; older notes that mention `/workspaces/Aurora` are stale for this checkout.
- Use `CARGO_TARGET_DIR=/tmp/aurora-target` for expensive Rust checks unless the repo target directory is intentionally cleaned up.
- Current JS bridge code in this checkout is under `src/js_v8/`; do not assume SpiderMonkey-only paths when implementing mutation, event-loop, or layout-accessor work.
- This document is the source of truth for stabilization progress. Treat a reliable, real content-bearing YouTube route as the next benchmark gate; do not expand scope to full YouTube navigation, playback, or account-specific behavior until that route is proven.

## Live YouTube Status - 2026-06-18

- Fixed a fatal `RefCell already mutably borrowed` abort that crashed `cargo run -- https://youtube.com` during Polymer hydration. Root cause: the `DomMutation::SetTextContent` dispatcher arm in `src/js_v8/tree/mutation.rs` held `node.borrow_mut()` across the render-sync call, and `BlitzDocument::sync_text_node` re-borrows the node via `parent_ptr`/`is_shadow_root_node`. YouTube triggered it immediately because Polymer rewrites `textContent` on text nodes. Fix: apply the structural mutation under the borrow, release it, then sync (matching the `SetAttribute` arm's existing pattern). Regression test: `v8_set_text_content_on_text_node_syncs_without_reborrow_panic` in `src/js_v8/runtime_tests.rs`.
- After the fix YouTube loads through the full custom-element upgrade pipeline and renders a screenshot (`AURORA_SCREENSHOT=... cargo run -- https://youtube.com`), reproducibly, EXIT 0.
- Remaining render gap (not a crash): only the masthead chrome paints (hamburger + three icon-button circles); no search box, logo, guide, or feed. There are also 5 caught (recoverable) `blitz-dom mutator.rs:807 "unreachable"` panics during attribute replacement that each force a `SyncOperationFailed` snapshot rebuild.
- 2026-06-18 investigation outcome (masthead teardown + "make it render"): measured with `AURORA_DEBUG_RENDER=1` — without the nav driver the page paints 322 paths (masthead renders); driving `ytd-page-manager.updatePageData` collapses it to 32 paths because it swaps in an empty browse page over the shell (the masthead stays connected in the DOM — it is a paint collapse, not a teardown). The nav-driver re-add was therefore reverted; `src/runner/pipeline.rs` is back to its pre-session state. Probing `window.getInitialData()` showed the logged-out home feed grid contains a single `feedNudgeRenderer` and **zero** video items — the real feed needs continuation API fetches (and likely auth) Aurora doesn't make. So for the logged-out home the masthead is essentially all the renderable content. The next benchmark should be a real content-bearing route whose initial data carries inspectable content, not a claim that the YouTube homepage or playback is supported.

## Status Snapshot - 2026-06-17

- Task 1.1 is complete and verified in `src/blitz_document.rs`: `MirrorIntegrityError`, `validate_mirror_integrity`, mapped node checks, debug validation calls after active `sync_*` hooks, focused positive mutation tests, shadow-root coverage, and one negative corrupted-map test are present.
- Task 1.2 is complete: sync hooks record monotonically increasing `MirrorMutationTrace` entries, and snapshot rebuilds now capture the last mirror op id.
- Task 2.1 is complete: `WindowInput` tracks explicit `SnapshotRebuildReason`, caller source, rebuild counts, consecutive rebuilds, last reason, and last mirror op id. Bare `mark_blitz_snapshot_dirty()` calls have been removed.
- Task 2.2 is complete: debug builds track rebuilds in a one-second rolling window and honor `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND`, including explicit `panic:<n>` mode.
- Paint state now returns `PaintResult` from `BlitzDocument::paint_to_scene` and `paint_with`; the live window schedules a paint-failure snapshot rebuild only for explicit failed-frame results and preserves a same-size last-known-good content scene for recoverable failures.
- JS DOM mutation bindings are collapsed behind `DomMutation`: core child-list methods, variadic insertion helpers, public set/remove attribute calls, `textContent`/`innerHTML`/`replaceChildren`, NamedNodeMap helpers, property-backed attributes, and `attachShadow` now dispatch through `src/js_v8/tree/mutation.rs`. Failed Blitz syncs suppress observer delivery and schedule `SnapshotRebuildReason::SyncOperationFailed` for the next snapshot rebuild.
- `src/window/app_handler.rs` already uses Blitz hit testing first for navigation and JS event dispatch, with legacy layout only as a fallback. Do not regress this when auditing layout authority.
- Task 4.1 is complete: synthetic shadow behavior is isolated behind `ShadowTreeBackend`/`SyntheticShadowTreeBackend` in `src/dom/shadow.rs`. `BlitzDocument` shadow predicates and the V8 `attach_shadow` binding delegate to it; the Blitz synthetic `<div data-aurora-shadow-root>` mirror stays in `sync_attach_shadow_root`.
- Task 4.2 is complete: core Shadow DOM semantics are covered by JS-level tests in `src/js_v8/runtime_tests.rs` (distinct shadow root, shadow vs light children, `querySelector` boundary visibility, ShadyCSS-lite `:host`/`::slotted` rewriting), plus two `#[ignore]`d TODO tests for slot distribution and shadow-crossing `composedPath`. The pure ShadyCSS rewriters are now exposed on `globalThis.__aurora_shadycss__` (no behavior change) for deterministic testing and Phase 5 reuse.
- Known harness limitation surfaced during 4.2: `<style>` is dropped from `<template>` `innerHTML` parsing, and `querySelectorAll` on a detached template-content fragment returns nothing when a render document is attached. This blocks end-to-end dom-module ShadyCSS hoist testing; revisit if Phase 5 needs the full pipeline.
- Phase 5 is complete: ShadyCSS rewrite instrumentation (Task 5.1) is gated behind `AURORA_DEBUG_SHADYCSS` and buffered on `globalThis.__aurora_shadycss__.diagnostics`; the once-per-page synthetic-styling warning (Task 5.2) fires from `scopeCss` and is counted at `__aurora_shadycss__.warningCount`.
- Phase 7 is complete: `src/runner/event_loop.rs` defines `EventLoopPhase` + canonical `TURN_ORDER`, and `pump_ready_work` drives turns through `run_event_loop_turn`. Ordering invariants are unit-tested; YouTube render is unchanged (322 paths).
- Phase 8 is complete. Task 8.1 audited the accessors (table filled in); Task 8.2 wired `getBoundingClientRect`/`getClientRects` and the box metrics to Blitz/Stylo `final_layout` via `BlitzDocument::dom_node_layout_metrics` + a native `__aurora_metric__` bridge, with `v8_post.js` preferring the real value and falling back to the old heuristic only for unlaid-out elements. No YouTube regression (322 paths). Remaining follow-ups: `offsetTop/Left`/`scrollTop/Left` still stubbed, and `document.elementFromPoint` still returns `null` (needs a document-level hit-test bridge).
- Phase 9 is complete: `docs/youtube_workaround_inventory.md` lists every rescue path with a platform gap, delete condition, and needed test coverage.
- **All 14 execution-order items / Phases 1–9 are now complete.** Remaining open threads are follow-ups noted inline, not plan tasks: (a) the upstream YouTube navigation/bootstrap blocker (content components never instantiate — the real gate on rendering YouTube content); (b) deferred layout accessors (`offsetTop/Left`, `elementFromPoint`); (c) wiring the shadow backend's `composed_children`/`is_in_shadow_tree`/`append_shadow_child` into call sites as native shadow semantics tighten. The diagnostics, measurable fallbacks, centralized mutation/transactional path, shadow abstraction, event-loop phases, and Blitz-authoritative layout reads built across this plan are the foundation for tackling the navigation blocker next.

## Current Code Anchors

These are starting points for future implementation work. Re-check them before editing because line numbers will move.

| Concern | Current anchor | Notes |
|---------|----------------|-------|
| Mirror maps and validation | `src/blitz_document.rs` | Owns `legacy_to_blitz`, `blitz_to_legacy`, `MirrorIntegrityError`, and all render-document `sync_*` hooks. |
| Snapshot rebuilds | `src/window/input.rs` | `blitz_snapshot_dirty` carries a `SnapshotRebuildReason`, caller source, counters, and last mirror operation id. |
| Paint result state | `src/blitz_document.rs`, `src/window/app.rs`, `src/window/screenshot/mod.rs`, `src/runner/pipeline.rs`, `src/window/chrome/dioxus_chrome.rs`, `src/render/headless.rs` | `paint_to_scene` and `paint_with` return `PaintResult`; live content paint preserves a same-size last-known-good scene for recoverable failures. |
| V8 mutation bridge | `src/js_v8/node_create.rs`, `src/js_v8/registry.rs`, `src/js_v8/tree/mutation.rs` | Core child-list methods, variadic insertion helpers, public set/remove attribute calls, `textContent`/`innerHTML`/`replaceChildren`, NamedNodeMap helpers, property-backed attributes, and `attachShadow` use `DomMutation`; sync failures now surface `SnapshotRebuildReason::SyncOperationFailed` through `WindowInput`. |
| Shadow DOM backend | `src/dom/shadow.rs`, `src/blitz_document.rs`, `src/js_v8/node_create.rs` | `ShadowTreeBackend`/`SyntheticShadowTreeBackend` own the `#document-fragment` shadow model, boundary predicates, and composed-tree flattening. Blitz predicates and the V8 `attach_shadow` binding delegate here; the Blitz synthetic mirror node lives in `sync_attach_shadow_root`. |
| JS selector/query bridge | `src/js_v8/registry.rs`, `src/blitz_document.rs` | Query helpers already prefer the Blitz document when a render document is attached. |
| JS layout accessors | `src/js_v8/node_create.rs`, `src/js_polyfills/v8_post.js` | Native accessors and JS fallbacks both need classification before moving layout authority. |
| Window hit testing and scroll | `src/window/app_handler.rs` | Uses Blitz document height and hit testing first, with legacy layout fallback. |
| Event-loop pump | `src/window/app.rs`, `src/runner/pipeline.rs`, `src/js_v8/runtime.rs` | Timers and rAF are drained opportunistically; explicit phases do not exist yet. |

## Cross-Cutting Invariants

- A JS-visible DOM mutation must not be considered successful until the renderer mutation path has either succeeded or recorded a specific divergence/rebuild reason.
- Mutation observers should describe the DOM state that JS can observe after the mutation, not an intermediate state between legacy and Blitz documents.
- Query, hit testing, layout reads, and paint should agree on the same rendered tree in normal Blitz mode. Legacy layout may remain as a fallback, test fixture, or debug path, but not as the authority for visible pixels.
- Any YouTube-specific rescue path must be attached to a platform gap and a delete condition before it grows further.
- Diagnostic-only work should compile out of release builds or stay low overhead enough to keep normal browsing behavior unchanged.

## Definition Of Done For Stabilization Tasks

- Code compiles with the relevant feature set used by the touched module.
- New diagnostics include enough node/operation identity to reproduce the failure without screenshots.
- Any fallback that rebuilds or preserves stale renderer state records a machine-readable reason.
- Tests are focused on the changed behavior. If a test cannot be added because the local infrastructure is missing, document the exact blocker under the task instead of silently marking it complete.
- Release behavior is unchanged for diagnostic-only phases unless the task explicitly says otherwise.

## Suggested Verification Commands

Use the narrowest check that covers the changed surface first, then broaden only when the patch touches shared runtime behavior.

```bash
CARGO_TARGET_DIR=/tmp/aurora-target cargo test blitz_document
CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::runtime_tests
CARGO_TARGET_DIR=/tmp/aurora-target cargo test --test visual_regression
CARGO_TARGET_DIR=/tmp/aurora-target cargo test
```

If a command fails for an environmental reason, record the exact command and failure under the active task.

## Phase 1: Add Mirror Correctness Diagnostics Before Changing Architecture

- [x] Task 1.1: Add DOM mirror invariant checks
  - Target: `src/blitz_document.rs`.
  - Add debug-only `validate_mirror_integrity(&self) -> Result<(), MirrorIntegrityError>`.
  - Verify `legacy_to_blitz` and `blitz_to_legacy` are symmetric.
  - Verify every mapped legacy node and Blitz node still exists.
  - Verify mapped parent/child relationships are consistent where possible.
  - Verify mapped text content and element attributes match.
  - Verify shadow-root synthetic nodes are linked to the correct host.
  - Call after `sync_append_child`, `sync_insert_before`, `sync_remove_child`, `sync_replace_child`, `sync_replace_children`, `sync_set_attribute`, `sync_remove_attribute`, `sync_set_text`, and `sync_attach_shadow` in debug builds.
  - Return structured errors with operation context, divergent legacy node, and Blitz node.
  - Tests: add focused regression tests if BlitzDocument-level tests support this path.
  - Prompt: implement diagnostics only; do not remove the legacy DOM yet.
  - Done: implemented in `src/blitz_document.rs` as `MirrorIntegrityError`, `validate_mirror_integrity`, `validate_mapped_node_state`, and `debug_validate_mirror_after`.
  - Done: added focused tests for initial snapshots, append/remove/attribute/text sync, shadow-root sync, and corrupted reverse-map detection.
  - Done: fixed debug-build custom `data-aurora-*` attribute validation by using dynamic `LocalName` construction instead of atom-only `local_name!`.
  - Done: fixed `src/dom/node.rs` parent-link maintenance so template content and shadow-root fragments are linked to their owning element during `link_children` and `reparent_subtree`.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test blitz_document` passes.

- [x] Task 1.2: Add mutation-operation logging with operation IDs
  - Target: `BlitzDocument` sync mutation hooks and snapshot-dirty call sites.
  - Add monotonically increasing operation IDs.
  - Log operation type, legacy node id, Blitz node id, parent id, child id, shadow-root involvement, and whether fallback snapshot rebuild was triggered.
  - Suggested shape: `MirrorMutationTrace { op_id, op_name, legacy_node, blitz_node, parent, child, result, failure }`.
  - Store the latest successful/failed mutation op id on `BlitzDocument` so `WindowInput` can attach it to rebuild reasons in Phase 2.
  - Include success/failure and failure class: missing mapping, Stylo panic, unsupported mutation shape, or validation failure.
  - Gate verbose logs behind `AURORA_DEBUG_MIRROR_MUTATIONS`; always keep counters and last-op metadata available for debug assertions.
  - Tests: verify operation IDs increase and traces include enough node identity to diagnose divergence.
  - Acceptance: a failed `sync_*` call can be correlated with the following `blitz_snapshot_dirty` rebuild or with a debug validation error.
  - Do not log whole HTML, text bodies, or attribute maps by default. Node identity and short attribute names are enough for first-pass diagnosis.
  - Done: added `MirrorMutationTrace`, `MirrorMutationResult`, and `MirrorMutationFailure` in `src/blitz_document.rs`.
  - Done: all active `sync_*` hooks record traces for success, missing mappings, sync failures, and debug validation failures.
  - Done: verbose trace logging is gated by `AURORA_DEBUG_MIRROR_MUTATIONS`.
  - Done: added tests for monotonic operation IDs and missing-mapping failure traces.
  - Done: snapshot rebuild tracking in Task 2.1 now records the last mirror op id.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test blitz_document` passes.

## Phase 2: Make Fallback Rebuilds Explicit And Measurable

- [x] Task 2.1: Count and classify snapshot rebuilds
  - Target: `WindowInput::sync_blitz_snapshot` and all callers that mark `blitz_snapshot_dirty`.
  - Add `SnapshotRebuildReason` with `ExplicitDirty`, `MissingMapping`, `PaintFailure`, `SyncOperationFailed`, `DebugValidationFailed`, and `InitialLoad`.
  - Track `snapshot_rebuild_count`, `snapshot_rebuild_reason`, `last_rebuild_op_id`, `last_rebuild_stack/source`, and `consecutive_rebuilds`.
  - Record and log every full Blitz rebuild reason.
  - Do not silently rebuild without recording why.
  - Replace bare `mark_blitz_snapshot_dirty()` with `mark_blitz_snapshot_dirty(reason: SnapshotRebuildReason)`.
  - Add narrowly named helpers for common callers, for example `mark_blitz_snapshot_dirty_for_paint_failure()` only if that keeps call sites readable.
  - `WindowInput::mark_dirty()` should pass `ExplicitDirty` until the DOM mutation dispatcher can provide more specific reasons.
  - `WindowInput::sync_blitz_snapshot()` should consume the pending reason on success and preserve it on failure so repeated rebuild attempts remain diagnosable.
  - Tests: verify dirty calls preserve reason and successful rebuild updates counters.
  - Acceptance: `rg "mark_blitz_snapshot_dirty\\(" src` shows no call site that can omit a reason.
  - Done: added `SnapshotRebuildReason` and reasoned dirty marking in `src/window/input.rs`.
  - Done: all call sites pass explicit reasons: `ExplicitDirty`, `InitialLoad`, `MissingMapping`, or `PaintFailure`.
  - Done: rebuilds record total count, consecutive count, last reason, caller source, and last mirror mutation op id.
  - Done: rebuild failure preserves pending reason/source for retry.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test window::input::tests` passes.
  - Verified: `rg "mark_blitz_snapshot_dirty\\(" src` shows no reasonless call sites.

- [x] Task 2.2: Fail loudly in debug mode when sync falls back too often
  - Target: snapshot rebuild accounting.
  - Add debug-only threshold controlled by `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND`.
  - Default threshold: 10 rebuilds per second.
  - In debug builds, warn or panic when exceeded with a diagnostic explaining incremental sync is incomplete.
  - Prefer warning by default and panic only when `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND=panic:<n>` or a similarly explicit mode is set. This keeps normal debug runs usable while still enabling strict CI/local probes.
  - Tests: verify threshold warning/panic behavior with the env var set low.
  - Done: added debug-only rolling one-second rebuild tracking in `src/window/input.rs`.
  - Done: unset env defaults to warning after 10 rebuilds per second; `0`/`off` disables; `panic:<n>` switches to panic mode.
  - Done: threshold diagnostics include rebuild count, threshold, reason, and last mirror op id.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test window::input::tests` passes.

## Phase 3: Reduce Split-Brain Behavior Mutation By Mutation

- [x] Task 3.1: Centralize DOM mutations behind one mutation API
  - Target: V8 DOM mutation bindings in `src/js_v8/` and shared mutation plumbing.
  - Introduce `DomMutation` variants for append, insert, remove, replace, set/remove attribute, set text, and attach shadow.
  - Route each mutation through one dispatcher instead of separately mutating legacy DOM and calling `sync_*` hooks.
  - Dispatcher applies mutation to legacy DOM, Blitz DOM, mutation observers, dirty flags, and debug validation.
  - The first patch may wrap existing behavior without changing semantics. Do not combine this with shadow semantics or layout accessor work.
  - Current high-risk call sites include `append_child`, `remove_child`, `insert_before`, `replace_child`, `remove_node`, attribute setters/removers, `textContent`, `innerHTML`, `replaceChildren`, and `attachShadow` in `src/js_v8/node_create.rs`.
  - Preserve existing custom-element tracking hooks while moving mutation mechanics; those hooks are a separate platform-gap cleanup.
  - Tests: cover each mutation variant through the dispatcher.
  - Acceptance: common mutation methods in `src/js_v8/node_create.rs` no longer directly interleave legacy mutation and render-document sync calls; they delegate to one dispatcher path.
  - Done: introduced `DomMutation`/`apply_dom_mutation` for `appendChild`, `prepend`, `insertBefore`, `removeChild`, `replaceChild`, `remove()`, `setAttribute`, `removeAttribute`, `textContent`, `innerHTML`, `replaceChildren`, and `attachShadow`. Dispatcher owns render sync, legacy mutation, observer queueing, dirty marking, and render-sync success reporting for those variants.
  - Done: migrated the remaining variadic insertion helpers in `src/js_v8/node_create.rs` to the dispatcher, including `append_children`, `prepend_children`, `insert_relative_to_self`, `insert_nodes_at_position`, and `replace_with`.
  - Verified so far: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::tree::mutation::tests` and `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::runtime_tests` pass.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::tree::mutation::tests` and `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::runtime_tests` pass after the final insertion-helper migration.

- [x] Task 3.2: Make mutation application transactional
  - Target: `DomMutation` dispatcher.
  - Apply to legacy DOM and Blitz DOM before notifying observers or returning success.
  - If Blitz sync fails, rollback the legacy mutation where feasible or mark the document explicitly divergent.
  - Schedule rebuild with `SnapshotRebuildReason::SyncOperationFailed` on sync failure.
  - Notify mutation observers only after both DOMs are updated successfully.
  - Tests: verify observer delivery does not happen on failed sync and divergent state is recorded.
  - Done: dispatcher suppresses observer queueing when Blitz sync fails, records the failure in the mirror trace, and schedules `SnapshotRebuildReason::SyncOperationFailed` so `WindowInput` can rebuild on the next reflow.
  - Done: `WindowInput::reflow` now consumes pending snapshot-rebuild reasons from the runtime before snapshot sync.
  - Done: runtime regression covers a detached-node attribute mutation that fails render sync, delivers no observer records, and leaves `SnapshotRebuildReason::SyncOperationFailed` queued for rebuild.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::tree::mutation::tests`, `CARGO_TARGET_DIR=/tmp/aurora-target cargo test window::input::tests`, and `CARGO_TARGET_DIR=/tmp/aurora-target cargo test js_v8::runtime_tests` pass.

## Phase 4: Shadow DOM - Replace Synthetic Behavior With A Path Toward Native Semantics

- [x] Task 4.1: Isolate synthetic shadow behavior behind an abstraction
  - Target: shadow-root rendering and query/composed-tree helpers.
  - Introduce `ShadowTreeBackend` with `attach_shadow`, `append_shadow_child`, `composed_children`, `host_for_shadow_root`, and `is_in_shadow_tree`.
  - Implement current `data-aurora-shadow-root` behavior as `SyntheticShadowTreeBackend`.
  - Preserve current behavior; this is an abstraction step only.
  - Tests: verify synthetic backend behavior matches current behavior.
  - Done: added `src/dom/shadow.rs` defining `ShadowTreeBackend` and `SyntheticShadowTreeBackend`, re-exported from `src/dom/mod.rs`. The backend owns the `#document-fragment` shadow-root model, host↔root linking, shadow-boundary predicates, and composed-tree flattening (light children followed by the shadow root).
  - Done: `BlitzDocument::is_shadow_root_node`/`nearest_shadow_root` now delegate to the backend (single source of truth), and the V8 `attach_shadow` binding uses `SyntheticShadowTreeBackend::attach_shadow` for the legacy DOM link instead of inlining `get_or_insert`/`set_parent`. The Blitz synthetic `<div data-aurora-shadow-root>` mirror stays in `sync_attach_shadow_root`.
  - Done: `append_shadow_child`, `composed_children`, `host_for_shadow_root`, and `is_in_shadow_tree` are the Task 4.2 migration surface (test-only callers for now); marked `#[allow(dead_code)]` with an explanatory comment so the abstraction surface is intentional rather than warning noise.
  - Done: added 5 focused backend tests covering attach idempotency/linking, shadow-child append + parenting, host resolution (roots only), nearest-root walk, and composed-children ordering.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test dom::shadow`, `cargo test blitz_document`, and `cargo test js_v8::runtime_tests` pass; `cargo build` is clean of new warnings.

- [x] Task 4.2: Add tests for core Shadow DOM semantics
  - Cover `attachShadow` creates a distinct shadow root.
  - Cover shadow children are not light DOM children.
  - Cover `querySelector` visibility respects shadow boundaries.
  - Cover ShadyCSS-lite `:host` and `::slotted` rewriting behavior.
  - Mark unsupported slot-like behavior and event composed path semantics as explicit TODO/ignored tests.
  - Done: added JS-level tests in `src/js_v8/runtime_tests.rs`: `v8_attach_shadow_creates_distinct_shadow_root` (distinct object, host↔root back-refs, nodeType 11, mode), `v8_shadow_children_are_not_light_dom_children` (shadow child absent from `host.childNodes`, present in `root.childNodes`), and `v8_query_selector_respects_shadow_boundary` (with a render document attached: `document`/`host` queries see only light matches, `root` sees only shadow matches).
  - Done: `v8_shadycss_lite_rewrites_host_and_slotted_selectors` verifies `:host`→tag, `:host(sel)`→tagsel, `::slotted(sel)`→`tag sel`, component-internal scoping, and that global selectors (`:root`) are left unscoped.
  - Done: added two `#[ignore]`d TODO tests documenting unsupported native semantics — `v8_shadow_slot_distribution_assigns_light_children_to_slots` and `v8_composed_path_includes_shadow_root_between_child_and_host` — each with an ignore reason describing the gap.
  - Production seam: exposed the pure ShadyCSS-lite rewriters on a namespaced `globalThis.__aurora_shadycss__` hook in `src/js_polyfills/custom_elements.js` (no runtime behavior change; the live hoist path still calls them directly). This made `:host`/`::slotted` rewriting testable deterministically and is reusable by Phase 5 Task 5.1 instrumentation.
  - Blocker note (does not block this task): the end-to-end dom-module → hoisted scoped `<style>` path is not exercisable in the test harness because `<style>` is dropped from `<template>` `innerHTML` parsing (template content came back as `DIV` only), and `querySelectorAll` on a detached template-content fragment returns nothing once a render document is attached. Rewriting behavior is therefore covered via the pure-function hook instead of the full pipeline.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test --lib js_v8::runtime_tests` passes (45 passed, 2 ignored).

## Phase 5: Retire ShadyCSS-lite Gradually

- [x] Task 5.1: Add instrumentation for ShadyCSS rewrites
  - Target: `src/js_polyfills/custom_elements.js`.
  - Log component name, original selector, rewritten selector, rules dropped, unsupported at-rules, and parse failures.
  - Gate logs behind `AURORA_DEBUG_SHADYCSS`.
  - Tests: verify logging is gated and selector rewrite diagnostics are emitted when enabled.
  - Done: added `shadyCssRecord`/`shadyCssDebugEnabled` gated behind `globalThis.__aurora_debug_shadycss__`, wired from the `AURORA_DEBUG_SHADYCSS` env var in `src/js_v8/runtime.rs` (mirroring `AURORA_DEBUG_YOUTUBE`). `rewriteSelectorList` records `selector` (from→to) diffs; `scopeCss` records `at-rule-passthrough` for unscoped `@`-rules; `shimDomModuleStyles` records `parse-failure` and `dropped`. Each entry also `console.log`s `[shadycss] …` when enabled, and is buffered on `globalThis.__aurora_shadycss__.diagnostics`.
  - Done: test `v8_shadycss_diagnostics_are_gated_behind_debug_flag` verifies zero diagnostics when off, and `selector` + `at-rule-passthrough` records (with component/from/to) when on.

- [x] Task 5.2: Add a native shadow styling disabled warning
  - Target: ShadyCSS-lite activation path.
  - Emit a once-per-page warning when YouTube or Polymer triggers ShadyCSS-lite.
  - Warning text: `Aurora is using synthetic ShadyCSS-lite rewriting. Rendering may diverge from native Shadow DOM styling.`
  - Tests: verify warning is once-per-page.
  - Done: `shadyCssWarnOnce()` fires the exact warning text via `console.warn` (falling back to `console.log`) the first time `scopeCss` runs, regardless of the debug flag; subsequent calls are no-ops. Exposed `globalThis.__aurora_shadycss__.warningCount`.
  - Done: test `v8_shadycss_emits_once_per_page_warning` runs the rewriter three times and asserts `warningCount` goes 0 → 1.
  - Verified: `CARGO_TARGET_DIR=/tmp/aurora-target cargo test --lib js_v8::runtime_tests::v8_shadycss` (3 passed). Note: `v8_drains_raf_scheduled_from_timer_then_zero_timer` is a pre-existing flaky timer test (passes ~1/3), unrelated to these JS-only changes.

## Phase 6: Paint Failure Should Preserve Last Known-Good Frame

- [x] Task 6.1: Distinguish painted current frame from renderer health
  - Target: `BlitzDocument::paint_to_scene` and `src/window/app.rs`.
  - Replace boolean success/health conflation with `PaintResult`.
  - Required variants: `PaintedCurrentFrame`, `PreservedLastGoodFrame`, `FailedRecoverable`, `FailedUnhealthy`.
  - Preserve current recovery behavior where possible.
  - Make it explicit when the current frame was not painted.
  - Add logging for consecutive paint failures and recovery attempts.
  - Audit all current paint call sites before changing signatures: content paint, screenshot capture, hydrated first-paint debug path, Dioxus chrome paint, and headless rendering.
  - Tests: cover each result branch where practical.
  - Completed 2026-06-17: added `PaintResult`, migrated content/debug/chrome/screenshot/headless paint call sites, preserved current recovery behavior, and covered success plus recoverable/unhealthy failure transitions in `src/blitz_document.rs`.

- [x] Task 6.2: Keep last known-good scene intentionally
  - Target: window content paint path and Blitz paint state.
  - Add `last_good_scene`, `last_successful_paint_time`, and `consecutive_paint_failures`.
  - On recoverable paint failure, keep displaying the previous successful scene.
  - Record the failure, mark Blitz snapshot dirty, and schedule recovery.
  - Do not silently treat a failed frame as successfully painted.
  - Tests: verify failed paint preserves previous scene and schedules recovery.
  - Completed 2026-06-17: added app-level `LastGoodSceneState`, preserves same-size content scenes only for `FailedRecoverable`, keeps marking snapshot dirty/reflow for recovery, and covers success, preservation, size mismatch, and unhealthy failure cases in `src/window/app.rs`.

## Phase 7: Event Loop Correctness

- [x] Task 7.1: Introduce explicit event-loop phases
  - Target: page-load pump and runtime scheduling.
  - Add `EventLoopPhase` with `RunTask`, `MicrotaskCheckpoint`, `MutationObserverDelivery`, `ResizeObserverDelivery`, `RequestAnimationFrame`, `StyleAndLayout`, `Paint`, and `IdleCallbacks`.
  - Route timers, promises/microtasks, mutation observers, rAF, style/layout, and paint through named phases.
  - Preserve current behavior initially where needed, but make ordering explicit and testable.
  - Tests: add phase-order unit coverage around the scheduler.
  - Done: added `src/runner/event_loop.rs` with `EventLoopPhase` (all 8 variants), a canonical `TURN_ORDER`, and `run_event_loop_turn` which drives the phases in fixed order and returns the phases that did work. `pump_ready_work` in `src/runner/pipeline.rs` now runs each turn through `run_event_loop_turn`, dispatching `RunTask`→`tick`, `MutationObserverDelivery`→`deliver_mutation_records`, `RequestAnimationFrame`→`drain_animation_frame_callbacks`. `MicrotaskCheckpoint` is implicit (V8 drains microtasks per task); `ResizeObserverDelivery`/`StyleAndLayout`/`Paint`/`IdleCallbacks` are explicit no-ops in the headless pump (window-loop concerns / unsupported APIs) so the full turn ordering stays visible.
  - Done: this moved `MutationObserverDelivery` ahead of `RequestAnimationFrame` (canonical microtask-checkpoint ordering). Verified no YouTube hydration regression: `AURORA_DEBUG_RENDER=1` still reports `nodes=1382 … paths=322` (identical to pre-refactor).

- [x] Task 7.2: Add ordering tests
  - Verify promise microtasks run before rAF.
  - Verify mutation observers deliver after DOM mutations.
  - Verify rAF runs before paint.
  - Verify layout/style happens before paint.
  - Verify timers do not starve rendering.
  - Goal: prevent YouTube-specific callback-draining hacks from becoming the event-loop model.
  - Done: `turn_order_matches_html_event_loop_invariants` asserts RunTask→Microtask→MutationObserverDelivery all precede RequestAnimationFrame, and rAF < StyleAndLayout < Paint, with IdleCallbacks last. `run_event_loop_turn_invokes_every_phase_in_canonical_order` and `run_event_loop_turn_reports_only_phases_that_did_work_in_order` verify the driver runs phases in `TURN_ORDER` and that running a task does not skip the rendering phases in the same turn (timers don't starve rendering). All 3 pass (`cargo test --lib runner::event_loop`).

## Phase 8: Delete Legacy Layout As Authority

- [x] Task 8.1: Find all JS layout accessors using legacy layout
  - Audit `offsetWidth`, `offsetHeight`, `clientWidth`, `clientHeight`, `scrollWidth`, `scrollHeight`, `getBoundingClientRect`, `elementFromPoint`, and hit testing.
  - Classify each path as reads Blitz/Stylo layout, reads legacy layout, placeholder, stub, or incorrect.
  - Output: code-level report with file/function and classification.
  - Include `src/window/app_handler.rs` hit testing and scroll-height paths in the audit; they still reference legacy layout.
  - Include `src/js_v8/registry.rs` shared-state layout access in the audit; it is the likely bridge for JS layout accessors.
  - Include `src/js_polyfills/v8_post.js`; it defines JS-side metric fallbacks and `document.elementFromPoint = function() { return null; }`.
  - Do not begin Task 8.2 until this inventory exists. The risk here is accidentally making JS accessors read a newer tree than input dispatch or screenshots.
  - Done (2026-06-18): audited every accessor below; none of the JS box-metric accessors read Blitz/Stylo layout today — they are static `0` stubs overridden by a JS heuristic. Hit testing and document height already read Blitz. Inventory table filled in; no code changed (Task 8.2 will).

### Layout Accessor Audit

Completed inventory. "Source of truth" is what the accessor returns *today*; classification is the action-plan bucket; required fix is for Task 8.2.

| API/path | File/function | Current source of truth | Classification | Required fix | Regression test |
|----------|---------------|-------------------------|----------------|--------------|-----------------|
| `offsetWidth` / `offsetHeight` | `src/js_v8/node_create.rs` (`create_js_node` sets `0.0`), `src/js_polyfills/v8_post.js` (`metric()` getter) | Native wrapper seeds `0`; v8_post.js redefines the prop with a getter returning a stored value **or**, for connected custom elements, a heuristic (`widthFallback`→parent `clientWidth`/`offsetWidth` else `innerWidth`; `heightFallback`→`innerHeight`) | Stub + JS heuristic (not real layout) | Read the element's Blitz `final_layout` border-box size | Element with known Blitz size reports matching `offsetWidth/Height` |
| `clientWidth` / `clientHeight` | same as above | same heuristic getter | Stub + JS heuristic | Read Blitz `final_layout` content-box (minus borders/scrollbars) | as above |
| `scrollWidth` / `scrollHeight` | same as above | same heuristic getter | Stub + JS heuristic | Read Blitz scrollable content size | as above |
| `offsetTop` / `offsetLeft` / `scrollTop` / `scrollLeft` | `src/js_v8/node_create.rs` (`create_js_node` sets `0.0`) | Static `0`, no JS override | Stub | Read Blitz `final_layout` position / scroll offset | as above |
| `getBoundingClientRect` (element) | `src/js_v8/node_create.rs` (`get_bounding_client_rect`) | All-zero rect | Stub | Build rect from Blitz `final_layout` (viewport-relative, incl. scroll) | element rect matches Blitz box |
| `getClientRects` | `src/js_v8/node_create.rs` (`get_client_rects`) | Empty array | Stub | Return `[getBoundingClientRect()]` for laid-out boxes | non-empty for a laid-out element |
| `getBoundingClientRect` (Range) | `src/js_polyfills/v8_post.js` (Range shim, ~line 61) | All-zero rect | Stub | Low priority; Range geometry rarely used | n/a |
| `document.elementFromPoint` | `src/js_polyfills/v8_post.js` (~line 523) | Returns `null` | Stub | Wire to Blitz `hit_test_dom_node` via a native bridge | point over known element returns it |
| Window click hit testing | `src/window/app_handler.rs` (`hit_test_dom_node`) | `BlitzDocument::hit_test_dom_node` | Reads Blitz/Stylo | Preserve (already authoritative) | click maps to expected node |
| Scroll bounds / document height | `src/window/app_handler.rs` (`document_height`) | `BlitzDocument::document_height` | Reads Blitz/Stylo | Preserve | doc height matches Blitz |
| Screenshot scrollbar content height | `src/window/scroll_metrics.rs` (`scroll_content_height`) | legacy `LayoutBox` | Reads legacy layout | Switch to Blitz document height, or document why legacy is acceptable for the scrollbar overlay | scrollbar size matches Blitz |

- [x] Task 8.2: Move layout reads to Blitz/Stylo
  - Refactor JS layout accessors to read from the Blitz/Stylo layout data that produced current pixels.
  - Remove dependence on legacy layout state for these accessors in normal Blitz mode.
  - Tests: verify layout accessors match visible Blitz layout in normal mode.
  - Done: added `BlitzDocument::dom_node_layout_metrics` (border-box size + document-relative position from `final_layout`, summing taffy locations up the box tree) and `LayoutMetrics`; exposed via `NodeRegistry::layout_metrics`. `getBoundingClientRect`/`getClientRects` now build their rect from Blitz layout, and a native `__aurora_metric__(name)` bridge feeds the box metrics. `v8_post.js`'s `metric()` getters now prefer the real Blitz value, falling back to the prior heuristic only when the element is unlaid-out/collapsed (so YouTube's collapsed components are unaffected).
  - Done: tests `v8_layout_accessors_read_blitz_layout` (a `200x50` div reports `200|50` via both `getBoundingClientRect` and `__aurora_metric__`) and `v8_layout_accessors_zero_without_render_document` (no render doc → 0). Full runtime suite green (50 passed).
  - Verified: no YouTube regression — `AURORA_DEBUG_RENDER=1` still reports `nodes=1383 … paths=322`.
  - Deferred: `offsetTop`/`offsetLeft`/`scrollTop`/`scrollLeft` still read their `0` instance stubs (not yet routed through `__aurora_metric__`); `document.elementFromPoint` still returns `null` (needs a document-level hit-test bridge). Both are noted in the audit table as remaining follow-ups.

## Phase 9: Remove YouTube-Specific Rescue Code After Platform Fixes

- [x] Task 9.1: Inventory YouTube-specific code
  - Find YouTube-specific, Polymer-specific, ShadyCSS-specific, and component rescue paths.
  - Group by file, function, trigger condition, and platform feature each workaround compensates for.
  - Output: inventory suitable for linking to deletion conditions.
  - Done: inventoried the rescue paths across `custom_elements.js`, `polymer_shim.js`, `v8_post.js`, `node_create.rs`, and `runner/pipeline.rs`; captured in `docs/youtube_workaround_inventory.md`. Diagnostics and genuine platform features (real MutationObserver, EventTarget) are explicitly excluded.

- [x] Task 9.2: Attach each workaround to a platform gap
  - Create `docs/youtube_workaround_inventory.md`.
  - Include table columns: Workaround, File, Platform feature missing, Delete condition, Test coverage needed.
  - Tests/docs: each workaround should have a concrete delete condition and required regression coverage.
  - Done: `docs/youtube_workaround_inventory.md` has the full table (13 rows: ShadyDOM event fallbacks, CE upgrade patching, ShadyCSS-lite rewriting + dom-module hoisting + instrumentation, Polymer `$` id-map, data-binding shim + sweep, `polymer_shim.js`, prop-bag sanitizer, layout metric heuristic, `elementFromPoint` stub, offset/scroll position stubs) each with a delete condition and needed test coverage. Notes capture the upstream navigation blocker and the removed `drive_polymer_page_manager_navigation` path.

## Suggested Execution Order

- [x] 1. Mirror diagnostics
- [x] 2. Snapshot rebuild reasons
- [x] 3. PaintResult enum
- [x] 4. Last-known-good frame preservation
- [x] 5. Central DomMutation dispatcher
- [x] 6. Transactional mutation application
- [x] 7. ShadowTreeBackend abstraction
- [x] 8. Shadow DOM tests
- [x] 9. ShadyCSS instrumentation
- [x] 10. EventLoopPhase enum
- [x] 11. Event-loop ordering tests
- [x] 12. Layout accessor audit
- [x] 13. Move layout reads to Blitz/Stylo
- [x] 14. YouTube workaround inventory

## First Implementation Prompt

You are working on Aurora branch `feature/youtube-support-fix-no-rendering`.

Start with diagnostics, not architecture changes. In the current checkout, the validator is already implemented; finish the verification gap before broadening the architecture change.

Add focused tests for the debug-only mirror integrity validator in `src/blitz_document.rs`.

Requirements:

- Add at least one test that builds a `BlitzDocument` from a small legacy DOM and verifies `validate_mirror_integrity()` returns `Ok(())`.
- Add mutation-path coverage for append, remove, attribute update, text update, and shadow-root attachment if the existing test helpers can create those DOM shapes cheaply.
- Add one negative test only if it can corrupt the mirror through test-only APIs without exposing new release-build mutation surface.
- Do not change runtime behavior in release builds.
- If negative corruption requires invasive visibility changes, document that blocker under Task 1.1 and proceed to Task 1.2.

Do not attempt to remove the legacy DOM yet. The goal of this patch is to expose divergence clearly before larger refactors.

## Second Implementation Prompt

Add explicit `SnapshotRebuildReason` tracking.

Requirements:

- Define a `SnapshotRebuildReason` enum.
- Track why `blitz_snapshot_dirty` was set.
- Replace every bare `mark_blitz_snapshot_dirty()` call with a call that supplies a reason.
- Update `WindowInput::sync_blitz_snapshot` so every full Blitz rebuild records and logs its reason.
- Add counters for total rebuilds, consecutive rebuilds, and last rebuild reason.
- Preserve the pending reason if a rebuild fails and the previous renderer snapshot is kept.
- Include the most recent mirror mutation operation id once Task 1.2 exists; until then, leave a clear `None`/`unknown` field rather than inventing correlation.
- Add a debug-only excessive rebuild warning controlled by `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND`.
- Do not change rendering behavior yet.
- Verification: `rg "mark_blitz_snapshot_dirty\\(" src` should show no reasonless call sites.

## Third Implementation Prompt

Refactor `BlitzDocument` painting result state.

Requirements:

- Replace boolean paint success/health conflation with a `PaintResult` enum:
  - `PaintedCurrentFrame`
  - `PreservedLastGoodFrame`
  - `FailedRecoverable`
  - `FailedUnhealthy`
- Update `BlitzDocument::paint_to_scene` and `src/window/app.rs` call sites.
- Preserve current recovery behavior where possible.
- Make it explicit when the current frame was not painted.
- Add logging for consecutive paint failures and recovery attempts.
- Do not implement last-known-good scene storage in the same patch unless the `PaintResult` call-site migration is already small and fully tested; that is Task 6.2.

This gives Codex a clean path: observe divergence first, make fallbacks measurable, then refactor toward one authoritative DOM.
