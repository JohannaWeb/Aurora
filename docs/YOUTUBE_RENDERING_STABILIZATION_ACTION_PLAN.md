# Aurora YouTube Rendering Stabilization Action Plan

## Goal

Reduce Aurora's YouTube fragility by moving away from the current "legacy DOM + mirrored Blitz DOM" architecture toward one authoritative rendering DOM, while keeping the current branch working during migration. The teardown identifies the main architectural problem as split-brain behavior: JavaScript mutates the legacy DOM, while Stylo/Blitz paints a separate mirror maintained through `sync_*` hooks.

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

- Workspace is currently dirty.
- `/workspaces/Aurora` is low on disk space.
- Future checks should use `CARGO_TARGET_DIR=/tmp/aurora-target` unless the repo target directory is cleaned up.
- This document is the source of truth for stabilization progress.

## Phase 1: Add Mirror Correctness Diagnostics Before Changing Architecture

- [~] Task 1.1: Add DOM mirror invariant checks
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

- [ ] Task 1.2: Add mutation-operation logging with operation IDs
  - Target: `BlitzDocument` sync mutation hooks and snapshot-dirty call sites.
  - Add monotonically increasing operation IDs.
  - Log operation type, legacy node id, Blitz node id, parent id, child id, shadow-root involvement, and whether fallback snapshot rebuild was triggered.
  - Suggested shape: `MirrorMutationTrace { op_id, op_name, legacy_node, blitz_node, parent, child }`.
  - Tests: verify operation IDs increase and traces include enough node identity to diagnose divergence.

## Phase 2: Make Fallback Rebuilds Explicit And Measurable

- [ ] Task 2.1: Count and classify snapshot rebuilds
  - Target: `WindowInput::sync_blitz_snapshot` and all callers that mark `blitz_snapshot_dirty`.
  - Add `SnapshotRebuildReason` with `ExplicitDirty`, `MissingMapping`, `PaintFailure`, `SyncOperationFailed`, `DebugValidationFailed`, and `InitialLoad`.
  - Track `snapshot_rebuild_count`, `snapshot_rebuild_reason`, `last_rebuild_op_id`, `last_rebuild_stack/source`, and `consecutive_rebuilds`.
  - Record and log every full Blitz rebuild reason.
  - Do not silently rebuild without recording why.
  - Tests: verify dirty calls preserve reason and successful rebuild updates counters.

- [ ] Task 2.2: Fail loudly in debug mode when sync falls back too often
  - Target: snapshot rebuild accounting.
  - Add debug-only threshold controlled by `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND`.
  - Default threshold: 10 rebuilds per second.
  - In debug builds, warn or panic when exceeded with a diagnostic explaining incremental sync is incomplete.
  - Tests: verify threshold warning/panic behavior with the env var set low.

## Phase 3: Reduce Split-Brain Behavior Mutation By Mutation

- [ ] Task 3.1: Centralize DOM mutations behind one mutation API
  - Target: V8 DOM mutation bindings and shared mutation plumbing.
  - Introduce `DomMutation` variants for append, insert, remove, replace, set/remove attribute, set text, and attach shadow.
  - Route each mutation through one dispatcher instead of separately mutating legacy DOM and calling `sync_*` hooks.
  - Dispatcher applies mutation to legacy DOM, Blitz DOM, mutation observers, dirty flags, and debug validation.
  - Tests: cover each mutation variant through the dispatcher.

- [ ] Task 3.2: Make mutation application transactional
  - Target: `DomMutation` dispatcher.
  - Apply to legacy DOM and Blitz DOM before notifying observers or returning success.
  - If Blitz sync fails, rollback the legacy mutation where feasible or mark the document explicitly divergent.
  - Schedule rebuild with `SnapshotRebuildReason::SyncOperationFailed` on sync failure.
  - Notify mutation observers only after both DOMs are updated successfully.
  - Tests: verify observer delivery does not happen on failed sync and divergent state is recorded.

## Phase 4: Shadow DOM - Replace Synthetic Behavior With A Path Toward Native Semantics

- [ ] Task 4.1: Isolate synthetic shadow behavior behind an abstraction
  - Target: shadow-root rendering and query/composed-tree helpers.
  - Introduce `ShadowTreeBackend` with `attach_shadow`, `append_shadow_child`, `composed_children`, `host_for_shadow_root`, and `is_in_shadow_tree`.
  - Implement current `data-aurora-shadow-root` behavior as `SyntheticShadowTreeBackend`.
  - Preserve current behavior; this is an abstraction step only.
  - Tests: verify synthetic backend behavior matches current behavior.

- [ ] Task 4.2: Add tests for core Shadow DOM semantics
  - Cover `attachShadow` creates a distinct shadow root.
  - Cover shadow children are not light DOM children.
  - Cover `querySelector` visibility respects shadow boundaries.
  - Cover ShadyCSS-lite `:host` and `::slotted` rewriting behavior.
  - Mark unsupported slot-like behavior and event composed path semantics as explicit TODO/ignored tests.

## Phase 5: Retire ShadyCSS-lite Gradually

- [ ] Task 5.1: Add instrumentation for ShadyCSS rewrites
  - Target: `src/js_polyfills/custom_elements.js`.
  - Log component name, original selector, rewritten selector, rules dropped, unsupported at-rules, and parse failures.
  - Gate logs behind `AURORA_DEBUG_SHADYCSS`.
  - Tests: verify logging is gated and selector rewrite diagnostics are emitted when enabled.

- [ ] Task 5.2: Add a native shadow styling disabled warning
  - Target: ShadyCSS-lite activation path.
  - Emit a once-per-page warning when YouTube or Polymer triggers ShadyCSS-lite.
  - Warning text: `Aurora is using synthetic ShadyCSS-lite rewriting. Rendering may diverge from native Shadow DOM styling.`
  - Tests: verify warning is once-per-page.

## Phase 6: Paint Failure Should Preserve Last Known-Good Frame

- [ ] Task 6.1: Distinguish painted current frame from renderer health
  - Target: `BlitzDocument::paint_to_scene` and `src/window/app.rs`.
  - Replace boolean success/health conflation with `PaintResult`.
  - Required variants: `PaintedCurrentFrame`, `PreservedLastGoodFrame`, `FailedRecoverable`, `FailedUnhealthy`.
  - Preserve current recovery behavior where possible.
  - Make it explicit when the current frame was not painted.
  - Add logging for consecutive paint failures and recovery attempts.
  - Tests: cover each result branch where practical.

- [ ] Task 6.2: Keep last known-good scene intentionally
  - Target: window content paint path and Blitz paint state.
  - Add `last_good_scene`, `last_successful_paint_time`, and `consecutive_paint_failures`.
  - On recoverable paint failure, keep displaying the previous successful scene.
  - Record the failure, mark Blitz snapshot dirty, and schedule recovery.
  - Do not silently treat a failed frame as successfully painted.
  - Tests: verify failed paint preserves previous scene and schedules recovery.

## Phase 7: Event Loop Correctness

- [ ] Task 7.1: Introduce explicit event-loop phases
  - Target: page-load pump and runtime scheduling.
  - Add `EventLoopPhase` with `RunTask`, `MicrotaskCheckpoint`, `MutationObserverDelivery`, `ResizeObserverDelivery`, `RequestAnimationFrame`, `StyleAndLayout`, `Paint`, and `IdleCallbacks`.
  - Route timers, promises/microtasks, mutation observers, rAF, style/layout, and paint through named phases.
  - Preserve current behavior initially where needed, but make ordering explicit and testable.
  - Tests: add phase-order unit coverage around the scheduler.

- [ ] Task 7.2: Add ordering tests
  - Verify promise microtasks run before rAF.
  - Verify mutation observers deliver after DOM mutations.
  - Verify rAF runs before paint.
  - Verify layout/style happens before paint.
  - Verify timers do not starve rendering.
  - Goal: prevent YouTube-specific callback-draining hacks from becoming the event-loop model.

## Phase 8: Delete Legacy Layout As Authority

- [ ] Task 8.1: Find all JS layout accessors using legacy layout
  - Audit `offsetWidth`, `offsetHeight`, `clientWidth`, `clientHeight`, `scrollWidth`, `scrollHeight`, `getBoundingClientRect`, `elementFromPoint`, and hit testing.
  - Classify each path as reads Blitz/Stylo layout, reads legacy layout, placeholder, stub, or incorrect.
  - Output: code-level report with file/function and classification.

- [ ] Task 8.2: Move layout reads to Blitz/Stylo
  - Refactor JS layout accessors to read from the Blitz/Stylo layout data that produced current pixels.
  - Remove dependence on legacy layout state for these accessors in normal Blitz mode.
  - Tests: verify layout accessors match visible Blitz layout in normal mode.

## Phase 9: Remove YouTube-Specific Rescue Code After Platform Fixes

- [ ] Task 9.1: Inventory YouTube-specific code
  - Find YouTube-specific, Polymer-specific, ShadyCSS-specific, and component rescue paths.
  - Group by file, function, trigger condition, and platform feature each workaround compensates for.
  - Output: inventory suitable for linking to deletion conditions.

- [ ] Task 9.2: Attach each workaround to a platform gap
  - Create `docs/youtube_workaround_inventory.md`.
  - Include table columns: Workaround, File, Platform feature missing, Delete condition, Test coverage needed.
  - Tests/docs: each workaround should have a concrete delete condition and required regression coverage.

## Suggested Execution Order

- [ ] 1. Mirror diagnostics
- [ ] 2. Snapshot rebuild reasons
- [ ] 3. PaintResult enum
- [ ] 4. Last-known-good frame preservation
- [ ] 5. Central DomMutation dispatcher
- [ ] 6. Transactional mutation application
- [ ] 7. ShadowTreeBackend abstraction
- [ ] 8. Shadow DOM tests
- [ ] 9. ShadyCSS instrumentation
- [ ] 10. EventLoopPhase enum
- [ ] 11. Event-loop ordering tests
- [ ] 12. Layout accessor audit
- [ ] 13. Move layout reads to Blitz/Stylo
- [ ] 14. YouTube workaround inventory

## First Implementation Prompt

You are working on Aurora branch `feature/youtube-support-fix-no-rendering`.

Start with diagnostics, not architecture changes.

Implement a debug-only mirror integrity validator in `src/blitz_document.rs`.

Requirements:

- Verify `legacy_to_blitz` and `blitz_to_legacy` are symmetric.
- Verify every mapped legacy node and Blitz node still exists.
- Verify mapped parent/child relationships are consistent where possible.
- Verify mapped text content matches.
- Verify mapped element attributes match.
- Include shadow-root synthetic nodes in validation.
- Return structured errors with operation context.
- Call the validator after every `sync_*` mutation method in `debug_assertions` builds.
- Do not change runtime behavior in release builds.
- Add logs that identify the operation name and node ids when validation fails.
- Add focused regression tests if the test infrastructure already supports BlitzDocument-level tests.

Do not attempt to remove the legacy DOM yet. The goal of this patch is to expose divergence clearly before larger refactors.

## Second Implementation Prompt

Add explicit `SnapshotRebuildReason` tracking.

Requirements:

- Define a `SnapshotRebuildReason` enum.
- Track why `blitz_snapshot_dirty` was set.
- Update `WindowInput::sync_blitz_snapshot` so every full Blitz rebuild records and logs its reason.
- Add counters for total rebuilds, consecutive rebuilds, and last rebuild reason.
- Add a debug-only excessive rebuild warning controlled by `AURORA_DEBUG_MAX_BLITZ_REBUILDS_PER_SECOND`.
- Do not change rendering behavior yet.

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

This gives Codex a clean path: observe divergence first, make fallbacks measurable, then refactor toward one authoritative DOM.
