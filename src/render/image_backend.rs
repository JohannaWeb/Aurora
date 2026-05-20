//! Software image backend — draws to an image::RgbaImage.
//! Used for headless rendering in CI and visual regression tests.

use image::{ImageBuffer, Rgba as ImgRgba, RgbaImage};

use super::commands::{BorderEdge, Bounds, Px, RenderBackend, Rgba};

pub struct ImageBackend {
    pub image: RgbaImage,
}

impl ImageBackend {
    pub fn new(width: u32, height: u32) -> Self {
        let image = ImageBuffer::from_pixel(width, height, ImgRgba([255, 255, 255, 255]));
        Self { image }
    }

    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    pub fn save(&self, path: &str) -> Result<(), image::ImageError> {
        self.image.save(path)
    }

    fn blend_pixel(&mut self, px: u32, py: u32, color: Rgba, opacity: f32) {
        let (w, h) = self.image.dimensions();
        if px >= w || py >= h {
            return;
        }
        let alpha = (color.a as f32 / 255.0) * opacity;
        if alpha < 0.004 {
            return;
        }
        let dst = self.image.get_pixel_mut(px, py);
        let inv = 1.0 - alpha;
        dst.0[0] = (color.r as f32 * alpha + dst.0[0] as f32 * inv).round() as u8;
        dst.0[1] = (color.g as f32 * alpha + dst.0[1] as f32 * inv).round() as u8;
        dst.0[2] = (color.b as f32 * alpha + dst.0[2] as f32 * inv).round() as u8;
        dst.0[3] = 255;
    }
}

impl RenderBackend for ImageBackend {
    fn fill_rect(&mut self, bounds: Bounds, color: Rgba, opacity: f32) {
        if opacity < 0.01 || color.a == 0 {
            return;
        }
        let (w, h) = self.image.dimensions();
        let x0 = bounds.x.0.max(0.0) as u32;
        let y0 = bounds.y.0.max(0.0) as u32;
        let x1 = bounds.right().0.min(w as f32) as u32;
        let y1 = bounds.bottom().0.min(h as f32) as u32;

        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px, py, color, opacity);
            }
        }
    }

    fn stroke_rect(&mut self, bounds: Bounds, border: BorderEdge, color: Rgba, opacity: f32) {
        if opacity < 0.01 || color.a == 0 {
            return;
        }
        let x0 = bounds.x;
        let y0 = bounds.y;
        let x1 = bounds.right();
        let y1 = bounds.bottom();

        if border.top.0 > 0.0 {
            self.fill_rect(Bounds::new(x0, y0, bounds.width, border.top), color, opacity);
        }
        if border.bottom.0 > 0.0 {
            self.fill_rect(
                Bounds::new(x0, Px(y1.0 - border.bottom.0), bounds.width, border.bottom),
                color, opacity,
            );
        }
        if border.left.0 > 0.0 {
            self.fill_rect(Bounds::new(x0, y0, border.left, bounds.height), color, opacity);
        }
        if border.right.0 > 0.0 {
            self.fill_rect(
                Bounds::new(Px(x1.0 - border.right.0), y0, border.right, bounds.height),
                color, opacity,
            );
        }
    }

    fn draw_text(
        &mut self,
        text: &str,
        x: Px,
        y: Px,
        font_size: Px,
        color: Rgba,
        opacity: f32,
    ) {
        if opacity < 0.01 || text.is_empty() {
            return;
        }
        let x = x.0;
        let y = y.0;
        let font_size = font_size.0;

        let text_run = crate::font::layout_text_run(text, font_size);
        let baseline_y = y + font_size * 0.75;

        let (atlas, atlas_width, _) = crate::font::get_atlas_texture();
        let (img_w, img_h) = self.image.dimensions();

        for glyph in &text_run.glyphs {
            if glyph.ch == '\n' {
                continue;
            }
            let Some(metrics) = crate::font::get_glyph_metrics(glyph.ch) else {
                continue;
            };
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }

            let scale = (font_size / 32.0).max(0.1);
            let gx = x + glyph.x + metrics.x_offset as f32 * scale;
            let gy = baseline_y + glyph.y_offset + metrics.y_offset as f32 * scale;
            let sw = (metrics.width as f32 * scale).ceil().max(1.0) as u32;
            let sh = (metrics.height as f32 * scale).ceil().max(1.0) as u32;

            for dy in 0..sh {
                for dx in 0..sw {
                    let src_x = ((dx as f32) / scale) as u32;
                    let src_y = ((dy as f32) / scale) as u32;
                    if src_x >= metrics.width || src_y >= metrics.height {
                        continue;
                    }
                    let ai = ((metrics.y + src_y) * atlas_width + (metrics.x + src_x)) * 4 + 3;
                    let alpha = atlas.get(ai as usize).copied().unwrap_or(0);
                    if alpha == 0 {
                        continue;
                    }
                    let px = (gx.round() as i32 + dx as i32) as u32;
                    let py = (gy.round() as i32 + dy as i32) as u32;
                    if px < img_w && py < img_h {
                        let glyph_color = Rgba::new(
                            color.r, color.g, color.b,
                            ((alpha as f32 / 255.0) * color.a as f32).round() as u8,
                        );
                        self.blend_pixel(px, py, glyph_color, opacity);
                    }
                }
            }
        }
    }

    fn draw_image(
        &mut self,
        bounds: Bounds,
        pixels: &[u8],
        img_width: u32,
        img_height: u32,
        opacity: f32,
    ) {
        if opacity < 0.01 {
            return;
        }
        let (w, h) = self.image.dimensions();
        let bw = bounds.width.0 as u32;
        let bh = bounds.height.0 as u32;
        let scale_x = img_width as f32 / bounds.width.0;
        let scale_y = img_height as f32 / bounds.height.0;

        for dy in 0..bh {
            for dx in 0..bw {
                let dst_x = bounds.x.0 as u32 + dx;
                let dst_y = bounds.y.0 as u32 + dy;
                if dst_x >= w || dst_y >= h {
                    continue;
                }
                let src_x = ((dx as f32 * scale_x) as u32).min(img_width - 1);
                let src_y = ((dy as f32 * scale_y) as u32).min(img_height - 1);
                let i = ((src_y * img_width + src_x) * 4) as usize;
                if i + 3 < pixels.len() {
                    let color = Rgba::new(pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]);
                    self.blend_pixel(dst_x, dst_y, color, opacity);
                }
            }
        }
    }

    fn draw_image_placeholder(&mut self, bounds: Bounds, opacity: f32) {
        self.fill_rect(bounds, Rgba::new(220, 235, 250, 255), opacity);
        self.stroke_rect(
            bounds,
            BorderEdge { top: Px(1.0), right: Px(1.0), bottom: Px(1.0), left: Px(1.0) },
            Rgba::new(100, 150, 200, 255),
            opacity,
        );
    }

    fn push_clip(&mut self, _bounds: Bounds) {}
    fn pop_clip(&mut self) {}
}
