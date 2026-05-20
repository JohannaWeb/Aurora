# Phase 4 — Inline Layout (Parley)

**Status: Started — `parley` dep in Cargo.toml, `parley_text.rs` scaffold exists but is not called from the layout path**

## Reading

- [ ] Read [Parley `line_break.rs`](https://github.com/linebender/parley/blob/main/parley/src/layout/line_break.rs)
- [ ] Read [Blitz's `inline.rs` glue](https://github.com/DioxusLabs/blitz/blob/main/packages/blitz-dom/src/layout/inline.rs) — canonical Parley + Taffy + DOM integration

## Integration

- [x] Add `parley` to `Cargo.toml`
- [ ] Wire `layout_text_with_parley` (`src/layout/parley_text.rs:19`) into the layout entry path — currently dead code
- [ ] Establish one `parley::Layout` per inline formatting context (IFC). Detect IFC boundaries during the layout pass.
- [ ] Build a `parley::FontContext` shared per document — replace `src/font/resources.rs` single-OnceLock face with a real fallback chain
- [ ] Hook Taffy's measure function for inline children into Parley: Taffy calls a closure → Parley layout → `LayoutOutput`
- [ ] Watch for `parley_core` crate split (track Linebender Zulip) and migrate when available

## Cleanup — files to delete once Parley drives inline

- [ ] Delete `src/layout/inline.rs` *(active, drives production)*
- [ ] Delete `src/layout/inline_sequence.rs` *(active)*
- [ ] Delete `src/layout/inline_text.rs` *(active)*
- [ ] Delete `src/layout/text_metrics.rs` *(active)*
- [ ] Delete `src/font/shape.rs` — per-char rustybuzz shaping, wrong at cluster level *(still present)*
- [ ] Delete Latin-1 prebuilt atlas path in `src/font/atlas_builder.rs` — Parley feeds glyphs per-shape/size

## Tests

- [ ] Load text in Cyrillic, Greek, Arabic RTL, Devanagari, CJK — all must render (none work today)

## Outcome

Closes P0 #7 (Latin-1 atlas), P0 #8 (no fragment model), shaper-glyph alignment bugs. Removes the entire `src/layout/inline*.rs` stack.
