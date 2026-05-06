use crate::gpu_paint::GpuPainter;
use peniko::Color;
use vello::Scene;

pub(super) fn text(scene: &mut Scene, value: &str, x: f64, y: f64, size: f32, color: Color) {
    GpuPainter::paint_text_label(scene, value, x, y, size, color);
}
