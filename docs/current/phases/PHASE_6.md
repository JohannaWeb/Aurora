# Phase 6 — Rendering Layer (AnyRender)

**Status: Not started**

Confirmed in code: `src/gpu_paint/painter.rs` calls `vello::Scene` directly throughout. No `anyrender` in `Cargo.toml`.

## Work items

- [ ] Add `anyrender` and `anyrender_vello` to `Cargo.toml` (check current crate name — may have changed)
- [ ] Refactor `src/gpu_paint/painter.rs` to emit `anyrender` drawing commands instead of calling `vello::Scene` directly
- [ ] Wire the Vello backend behind AnyRender — existing GPU pipeline becomes one backend among several
- [ ] Reference [`blitz-paint`](https://github.com/DioxusLabs/blitz/tree/main/packages/blitz-paint) as the canonical DOM → drawing commands translator
- [ ] Add stacking context support: real CSS painting order, `z-index`, `position: relative` painting promotion, transforms

## Note on current software backend

`src/render/image_backend.rs` (`ImageBackend`) already exists as a software path for headless/CI rendering. AnyRender would eventually unify this with the GPU path under one abstraction, but `ImageBackend` is usable and not blocking anything today.

## Outcome

Sets up Phase 8 (visual regression with GPU backend). Unblocks `transform`-only repaint paths. Closes P0 #9 (no compositing).
