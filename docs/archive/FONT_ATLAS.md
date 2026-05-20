# Aurora Glyph Atlas System

## Architecture

Aurora uses a **unified glyph atlas** for efficient text rendering—all glyphs (0-255, ASCII + extended) packed into a single GPU texture.

### Why One Atlas?

**Memory efficiency**: 
- One packed texture instead of scattered per-glyph allocations
- Bookshelf analogy: condensed library with shared shelves beats one author per bookshelf
- Fixed 1024×1024 RGBA texture = ~4MB, contains all 256 glyphs

**Rendering efficiency**:
- Single texture bind instead of multiple draw calls
- UV lookups instead of glyph rasterization on every frame
- Deterministic memory footprint and startup behavior

### Components

#### `atlas.rs` - Core infrastructure
- `GlyphAtlas`: Packed texture + metadata
- `GlyphMetrics`: Per-glyph UV coords, offsets, advance width
- `AtlasPacker`: Guillotine bin packing algorithm

#### `font.rs` - Atlas builder
- `AtlasBuilder::build()`: Rasterizes all 256 glyphs at 32px base resolution
- `get_glyph_atlas()`: Lazy initialization via `OnceLock`
- `get_glyph_metrics()`: O(1) lookup by character

#### `gpu_paint.rs` - Rendering
- Uses atlas metrics for text layout and positioning
- Scales glyphs relative to atlas resolution (32px base)
- Applies italic/underline via rendering transforms

## Data Flow

```
1. App startup → font.rs::AtlasBuilder::build()
   ├─ Load DejaVu Sans Mono TTF
   ├─ Rasterize glyphs 0-255 at 32px
   ├─ Pack into atlas via AtlasPacker
   └─ Cache in static GLYPH_ATLAS

2. Text rendering → gpu_paint.rs::paint_text_with_opacity()
   ├─ Look up glyph metrics (O(1) HashMap)
   ├─ Scale relative to base size (32px)
   ├─ Position using advance_width, offsets
   └─ Draw via vello

3. GPU → Atlas texture bound once, all text rendered with UV coords
```

## Metrics Structure

Each glyph has:
- **Position in atlas**: (x, y)
- **Dimensions**: (width, height)
- **Baseline offsets**: (x_offset, y_offset) for proper positioning
- **Advance width**: Horizontal spacing to next glyph
- **UV bounds**: Normalized [0, 1] coordinates for texture lookup

## Scaling

Glyphs are rasterized at **32px base size**. When rendering at other sizes:

```
scale_factor = font_size / 32.0

glyph_width = metrics.width * scale_factor
glyph_height = metrics.height * scale_factor
```

This is done per-frame in `paint_text_with_opacity()`.

## Future Enhancements

### Multiple fonts in one atlas
- Pack DejaVu, Courier, serif all in one texture
- Each font claims a region of the atlas
- Metadata tracks which font, which glyphs

### Dynamic growth
- Start with common ASCII (128 chars)
- Grow atlas on-demand when Unicode beyond 0-255 is needed
- Defragmentation via periodic rebuilds

### SDF variants
- Store glyphs as Signed Distance Fields instead of bitmaps
- Enable arbitrary scaling without quality loss
- Trade: more complex shaders, post-processing

### Performance monitoring
- Track atlas hit rate (% of text using pre-baked glyphs)
- Measure first-frame cost of atlas building
- Cache perf impact vs. on-demand rasterization

## Technical Notes

### Guillotine packing
Simple row-based packing:
1. Create rows as glyphs are packed
2. Fill each row left-to-right
3. Add new row when needed
4. Works well for monospace fonts with uniform heights

### Thread safety
Atlas is built once at startup, then read-only:
- `OnceLock` handles initialization on first access
- No locks needed during rendering
- Safe for multi-threaded access

### Bitmap vs. Vector
Currently using **bitmap glyphs**:
- Simple, predictable rendering
- No shader complexity
- Can blur/pixelate at large sizes
- Scaling cost (per-frame computation)

Vector outline approach would:
- Perfect scaling to any size
- Proper glyph hinting
- Complex rasterization + packing
- Higher atlas build time

## Embedded Font

Uses **DejaVu Sans Mono** (295KB TTF):
- Comprehensive Unicode support
- Clear, legible monospace design
- No runtime font dependencies
- Embedded at compile time via `include_bytes!`

## Testing

```bash
cargo test --lib font -- --nocapture
cargo test --lib atlas -- --nocapture
```

Tests verify:
- Glyph packing correctness
- Atlas registration and lookup
- Metrics computation
