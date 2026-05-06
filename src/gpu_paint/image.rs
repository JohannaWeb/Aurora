use crate::layout::LayoutBox;
use crate::ImageCache;
use peniko::{Color, Fill, ImageBrush};
use vello::kurbo::{Affine, Rect as KRect};
use vello::Scene;

pub(super) fn paint_image(layout_box: &LayoutBox, scene: &mut Scene, images: &ImageCache) {
    let r = layout_box.rect();
    let k_rect = KRect::new(
        r.x as f64,
        r.y as f64,
        (r.x + r.width) as f64,
        (r.y + r.height) as f64,
    );

    if let Some(src) = layout_box.image_src() {
        if let Some(img_data) = images.get(src) {
            if img_data.width > 0 && img_data.height > 0 && r.width > 0.0 && r.height > 0.0 {
                let affine = Affine::translate((r.x as f64, r.y as f64))
                    * Affine::scale_non_uniform(
                        r.width as f64 / img_data.width as f64,
                        r.height as f64 / img_data.height as f64,
                    );
                let brush = ImageBrush::new(img_data.clone());
                scene.draw_image(brush.as_ref(), affine);
                return;
            }
        }
    }

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(200, 200, 200),
        None,
        &k_rect,
    );
}
