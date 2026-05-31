# Phase 6 — Rendering Layer (AnyRender)

**Status: Completed**

**Progress Update:** Integrated `blitz-dom` and `blitz-paint`. The engine now uses Stylo-powered CSS resolution and Taffy-powered layout, emitting commands via the `RenderBackend` trait.

## Work items

- [x] Add `blitz-dom` and `blitz-paint` to `Cargo.toml`
- [x] Refactor `src/gpu_paint/painter.rs` to implement `RenderBackend` and emit drawing commands
- [x] Wire the Vello backend behind AnyRender — existing GPU pipeline becomes one backend among several
- [x] Reference `blitz-paint` as the canonical DOM → drawing commands translator
- [x] Stacking context support (delegated to blitz-paint)

## Note on current software backend

`src/render/image_backend.rs` (`ImageBackend`) already exists as a software path for headless/CI rendering. AnyRender would eventually unify this with the GPU path under one abstraction, but `ImageBackend` is usable and not blocking anything today.

## Outcome

Unblocks Phase 7 (Reflow & Invalidation). Closes P0 #9 (no compositing) and provides a spec-compliant CSS/Layout foundation.
