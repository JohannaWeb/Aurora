//! Vello GPU backend — wraps vello::Scene, used in the live window.

use peniko::{Color, Fill};
use vello::kurbo::{Affine, Rect as KRect};
use vello::Scene;

use super::commands::{BorderEdge, Bounds, RenderBackend, Rgba};

pub struct VelloBackend<'a> {
    scene: &'a mut Scene,
}

impl<'a> VelloBackend<'a> {
    pub fn new(scene: &'a mut Scene) -> Self {
        Self { scene }
    }
}

impl<'a> RenderBackend for VelloBackend<'a> {
    fn fill_rect(&mut self, bounds: Bounds, color: Rgba, opacity: f32) {
        if opacity < 0.01 || color.a == 0 {
            return;
        }
        let alpha = ((color.a as f32) * opacity).round() as u8;
        let c = Color::from_rgba8(color.r, color.g, color.b, alpha);
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            c,
            None,
            &krect(bounds),
        );
    }

    fn stroke_rect(&mut self, bounds: Bounds, border: BorderEdge, color: Rgba, opacity: f32) {
        if opacity < 0.01 || color.a == 0 {
            return;
        }
        let alpha = ((color.a as f32) * opacity).round() as u8;
        let c = Color::from_rgba8(color.r, color.g, color.b, alpha);

        let x0 = bounds.x as f64;
        let y0 = bounds.y as f64;
        let x1 = bounds.right() as f64;
        let y1 = bounds.bottom() as f64;

        // Top edge
        if border.top > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y0, x1, y0 + border.top as f64));
        }
        // Bottom edge
        if border.bottom > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y1 - border.bottom as f64, x1, y1));
        }
        // Left edge
        if border.left > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y0, x0 + border.left as f64, y1));
        }
        // Right edge
        if border.right > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x1 - border.right as f64, y0, x1, y1));
        }
    }

    fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: Rgba,
        opacity: f32,
    ) {
        if opacity < 0.01 || text.is_empty() {
            return;
        }
        let alpha = ((color.a as f32) * opacity).round() as u8;
        let c = Color::from_rgba8(color.r, color.g, color.b, alpha);
        crate::gpu_paint::text::paint_text_label(
            self.scene, text, x as f64, y as f64, font_size, c,
        );
    }

    fn draw_image(
        &mut self,
        bounds: Bounds,
        pixels: &[u8],
        img_width: u32,
        img_height: u32,
        _opacity: f32,
    ) {
        // Encode the RGBA pixel buffer as a Vello image and blit it.
        use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
        let img_data = ImageData {
            data: Blob::from(pixels.to_vec()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: img_width,
            height: img_height,
        };
        let image = ImageBrush::new(img_data);
        let scale_x = bounds.width as f64 / img_width as f64;
        let scale_y = bounds.height as f64 / img_height as f64;
        let transform = Affine::translate((bounds.x as f64, bounds.y as f64))
            * Affine::scale_non_uniform(scale_x, scale_y);
        self.scene.draw_image(image.as_ref(), transform);
    }

    fn draw_image_placeholder(&mut self, bounds: Bounds, opacity: f32) {
        self.fill_rect(bounds, Rgba::new(220, 235, 250, 255), opacity);
        self.stroke_rect(
            bounds,
            BorderEdge { top: 1.0, right: 1.0, bottom: 1.0, left: 1.0 },
            Rgba::new(100, 150, 200, 255),
            opacity,
        );
    }

    fn push_clip(&mut self, bounds: Bounds) {
        use peniko::BlendMode;
        self.scene.push_layer(
            Fill::NonZero,
            BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &krect(bounds),
        );
    }

    fn pop_clip(&mut self) {
        self.scene.pop_layer();
    }
}

fn krect(b: Bounds) -> KRect {
    KRect::new(b.x as f64, b.y as f64, b.right() as f64, b.bottom() as f64)
}
