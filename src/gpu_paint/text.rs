use crate::layout::LayoutBox;
use peniko::{Color, Fill};
use vello::kurbo::{Affine, Line, Rect as KRect};
use vello::Scene;

use super::color::parse_color;

pub(super) fn paint_text_label(
    scene: &mut Scene,
    text: &str,
    x: f64,
    y: f64,
    font_size: f32,
    color: Color,
) {
    let baseline_y = y + font_size as f64 * 0.75;
    paint_text_pixels(scene, text, x, baseline_y, font_size, color);
}

pub(super) fn paint_text_with_opacity(
    layout_box: &LayoutBox,
    text: &str,
    scene: &mut Scene,
    opacity: f32,
) {
    let r = layout_box.rect();
    let styles = layout_box.styles();
    let mut text_color = parse_color(styles.get("color").unwrap_or("black"));
    text_color.components[3] *= opacity;

    let font_size = zoomed_font_size(styles.font_size_px().filter(|&s| s > 0.0).unwrap_or(16.0));
    let text_run = crate::font::layout_text_run(text, font_size);
    let text_width = text_run.width as f64;
    let offset_x = match styles.text_align() {
        crate::css::TextAlign::Center => (r.width as f64 - text_width).max(0.0) / 2.0,
        crate::css::TextAlign::Right => (r.width as f64 - text_width).max(0.0),
        crate::css::TextAlign::Left => 0.0,
    };
    let baseline_y = r.y as f64 + font_size as f64 * 0.75;

    paint_shaped_run(
        scene,
        &text_run,
        r.x as f64 + offset_x,
        baseline_y,
        font_size,
        text_color,
    );
    paint_text_decoration_if_needed(
        layout_box, scene, text_width, offset_x, baseline_y, font_size, text_color,
    );
}

fn paint_text_pixels(
    scene: &mut Scene,
    text: &str,
    x: f64,
    baseline_y: f64,
    font_size: f32,
    color: Color,
) {
    let text_run = crate::font::layout_text_run(text, font_size);
    paint_shaped_run(scene, &text_run, x, baseline_y, font_size, color);
}

fn paint_shaped_run(
    scene: &mut Scene,
    text_run: &crate::font::TextRun,
    x: f64,
    baseline_y: f64,
    font_size: f32,
    color: Color,
) {
    let (base_r, base_g, base_b, base_a) = color_channels(color);

    for glyph in &text_run.glyphs {
        if let Some(raster) = crate::font::rasterize_glyph(glyph.ch, font_size) {
            let gx = x + glyph.x as f64 + raster.x_offset as f64;
            let gy = baseline_y + glyph.y_offset as f64 + raster.y_offset as f64;
            paint_raster(scene, &raster, gx, gy, base_r, base_g, base_b, base_a);
        }
    }
}

fn paint_raster(
    scene: &mut Scene,
    raster: &crate::font::RasterGlyph,
    gx: f64,
    gy: f64,
    base_r: u8,
    base_g: u8,
    base_b: u8,
    base_a: u8,
) {
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

fn paint_text_decoration_if_needed(
    layout_box: &LayoutBox,
    scene: &mut Scene,
    text_width: f64,
    offset_x: f64,
    baseline_y: f64,
    font_size: f32,
    text_color: Color,
) {
    let decoration = layout_box.styles().get("text-decoration").unwrap_or("none");
    if !decoration.contains("underline") {
        return;
    }

    let r = layout_box.rect();
    let line_y = baseline_y + font_size as f64 * 0.1;
    scene.stroke(
        &vello::kurbo::Stroke::new((font_size * 0.1).max(1.0) as f64),
        Affine::IDENTITY,
        text_color,
        None,
        &Line::new(
            (r.x as f64 + offset_x, line_y),
            (r.x as f64 + offset_x + text_width, line_y),
        ),
    );
}

fn color_channels(color: Color) -> (u8, u8, u8, u8) {
    (
        (color.components[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.components[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn zoomed_font_size(mut font_size: f32) -> f32 {
    if let Ok(zoom_str) = std::env::var("AURORA_ZOOM") {
        if let Ok(zoom) = zoom_str.parse::<f32>() {
            font_size *= zoom;
        }
    }
    font_size
}
