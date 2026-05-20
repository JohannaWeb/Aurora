//! Vello GPU backend — wraps vello::Scene, used in the live window.

use peniko::{Color, Fill};
use vello::kurbo::{Affine, Rect as KRect};
use vello::Scene;

use super::commands::{BorderEdge, Bounds, Px, RenderBackend, Rgba};

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
        self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None, &krect(bounds));
    }

    fn stroke_rect(&mut self, bounds: Bounds, border: BorderEdge, color: Rgba, opacity: f32) {
        if opacity < 0.01 || color.a == 0 {
            return;
        }
        let alpha = ((color.a as f32) * opacity).round() as u8;
        let c = Color::from_rgba8(color.r, color.g, color.b, alpha);

        let x0 = bounds.x.0 as f64;
        let y0 = bounds.y.0 as f64;
        let x1 = bounds.right().0 as f64;
        let y1 = bounds.bottom().0 as f64;

        if border.top.0 > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y0, x1, y0 + border.top.0 as f64));
        }
        if border.bottom.0 > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y1 - border.bottom.0 as f64, x1, y1));
        }
        if border.left.0 > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x0, y0, x0 + border.left.0 as f64, y1));
        }
        if border.right.0 > 0.0 {
            self.scene.fill(Fill::NonZero, Affine::IDENTITY, c, None,
                &KRect::new(x1 - border.right.0 as f64, y0, x1, y1));
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
        let alpha = ((color.a as f32) * opacity).round() as u8;
        let c = Color::from_rgba8(color.r, color.g, color.b, alpha);
        crate::gpu_paint::text::paint_text_label(
            self.scene, text, x.0 as f64, y.0 as f64, font_size.0, c,
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
        use peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
        let img_data = ImageData {
            data: Blob::from(pixels.to_vec()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: img_width,
            height: img_height,
        };
        let image = ImageBrush::new(img_data);
        let scale_x = bounds.width.0 as f64 / img_width as f64;
        let scale_y = bounds.height.0 as f64 / img_height as f64;
        let transform = Affine::translate((bounds.x.0 as f64, bounds.y.0 as f64))
            * Affine::scale_non_uniform(scale_x, scale_y);
        self.scene.draw_image(image.as_ref(), transform);
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
    KRect::new(b.x.0 as f64, b.y.0 as f64, b.right().0 as f64, b.bottom().0 as f64)
}
