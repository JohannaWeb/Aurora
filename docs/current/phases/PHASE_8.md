# Phase 8 — Testing and CI

**Status: Partial — visual regression harness in place; WPT not started**

## Work items

- [x] `UPDATE_SNAPSHOTS=1 cargo test --test visual_regression` — update baselines
- [x] `cargo test --test visual_regression` — fails on pixel regression (1% threshold, tolerance 8 per channel)
- [x] `make all-renders` — regenerates fixture PNGs
- [x] `render_url_to_image` in `src/render/headless.rs` — headless render of any URL, saves to `tests/screenshots/`
- [x] `snapshot_wikipedia_rust` test — renders Wikipedia Rust article to `tests/screenshots/wikipedia-rust.png`
- [ ] Add an HTML5 conformance subset test runner — adopt ~200 [Web Platform Tests](https://github.com/web-platform-tests/wpt) CSS layout reftests as a gating suite
- [ ] Add a Taffy-style benchmark harness once Taffy is the sole layout entry point

## Outcome

Closes P1 #24 (no visual regression diff — partial). WPT adoption makes Phase 1–4 regressions immediately visible in CI.
