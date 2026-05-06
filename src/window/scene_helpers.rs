use vello::kurbo::{Affine, Rect as KRect, RoundedRect};
use vello::peniko::{Color, Fill};
use vello::Scene;

pub(super) fn fill_scene_rect(
    scene: &mut Scene,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: Color,
) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &KRect::new(x, y, x + width, y + height),
    );
}

pub(super) fn stroke_scene_rect(
    scene: &mut Scene,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    color: Color,
) {
    scene.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        color,
        None,
        &RoundedRect::from_rect(KRect::new(x, y, x + width, y + height), 0.0),
    );
}
