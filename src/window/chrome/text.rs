use peniko::{Color, Fill};
use vello::Scene;
use vello::kurbo::{Affine, Rect as KRect};

pub(super) fn text(scene: &mut Scene, value: &str, x: f64, y: f64, size: f32, color: Color) {
    let baseline_y = y + size as f64 * 0.75;
    let text_run = crate::font::layout_text_run(value, size);
    let (base_r, base_g, base_b, base_a) = color_channels(color);
    for glyph in &text_run.glyphs {
        if let Some(raster) = crate::font::rasterize_glyph(glyph.ch, size) {
            let gx = x + glyph.x as f64 + raster.x_offset as f64;
            let gy = baseline_y + glyph.y_offset as f64 + raster.y_offset as f64;
            for row in 0..raster.height {
                for col in 0..raster.width {
                    let idx = (row * raster.width + col) as usize;
                    let alpha = raster.bitmap.get(idx).copied().unwrap_or(0);
                    if alpha == 0 {
                        continue;
                    }
                    let coverage = alpha as f32 / 255.0;
                    let glyph_alpha = (base_a as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        Color::from_rgba8(base_r, base_g, base_b, glyph_alpha),
                        None,
                        &KRect::new(
                            gx + col as f64,
                            gy + row as f64,
                            gx + col as f64 + 1.0,
                            gy + row as f64 + 1.0,
                        ),
                    );
                }
            }
        }
    }
}

fn color_channels(color: Color) -> (u8, u8, u8, u8) {
    (
        (color.components[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}
