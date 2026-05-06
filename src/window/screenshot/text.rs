use super::ScreenshotImage;
use image::Rgba;

pub(super) fn render_text_simple(
    img: &mut ScreenshotImage,
    text: &str,
    x: i32,
    y: i32,
    color: Rgba<u8>,
    font_size: u32,
) {
    let font_size = font_size as f32;
    let text_run = crate::font::layout_text_run(text, font_size);
    let baseline_y = y as f32 + font_size * 0.75;

    for glyph in &text_run.glyphs {
        if glyph.ch != '\n' {
            draw_glyph_bitmap(
                img,
                glyph.ch,
                x as f32 + glyph.x,
                baseline_y + glyph.y_offset,
                font_size / 32.0,
                color,
            );
        }
    }
}

fn draw_glyph_bitmap(
    img: &mut ScreenshotImage,
    ch: char,
    x: f32,
    y: f32,
    scale: f32,
    color: Rgba<u8>,
) {
    let (width, height) = img.dimensions();
    let Some(metrics) = crate::font::get_glyph_metrics(ch) else {
        return;
    };
    if metrics.width == 0 || metrics.height == 0 {
        return;
    }

    let (atlas, atlas_width, _) = crate::font::get_atlas_texture();
    let scale = scale.max(0.1);
    let draw_origin_x = x + metrics.x_offset as f32 * scale;
    let draw_origin_y = y + metrics.y_offset as f32 * scale;
    let scaled_width = (metrics.width as f32 * scale).ceil().max(1.0) as i32;
    let scaled_height = (metrics.height as f32 * scale).ceil().max(1.0) as i32;

    for dy in 0..scaled_height {
        for dx in 0..scaled_width {
            let src_x = ((dx as f32) / scale).floor() as u32;
            let src_y = ((dy as f32) / scale).floor() as u32;
            if src_x >= metrics.width || src_y >= metrics.height {
                continue;
            }

            let atlas_idx =
                (((metrics.y + src_y) * atlas_width + metrics.x + src_x) * 4 + 3) as usize;
            let alpha = atlas.get(atlas_idx).copied().unwrap_or(0);
            if alpha == 0 {
                continue;
            }

            let draw_x = draw_origin_x.round() as i32 + dx;
            let draw_y = draw_origin_y.round() as i32 + dy;
            if draw_x < 0 || draw_y < 0 || (draw_x as u32) >= width || (draw_y as u32) >= height {
                continue;
            }

            let dst = img.get_pixel_mut(draw_x as u32, draw_y as u32);
            let coverage = alpha as f32 / 255.0;
            let inv = 1.0 - coverage;
            dst.0[0] = (color.0[0] as f32 * coverage + dst.0[0] as f32 * inv).round() as u8;
            dst.0[1] = (color.0[1] as f32 * coverage + dst.0[1] as f32 * inv).round() as u8;
            dst.0[2] = (color.0[2] as f32 * coverage + dst.0[2] as f32 * inv).round() as u8;
            dst.0[3] = 255;
        }
    }
}
