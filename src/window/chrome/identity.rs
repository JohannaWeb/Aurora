use peniko::Color;
use vello::Scene;

use super::text::text;
use crate::window::scene_helpers::{fill_scene_rect, stroke_scene_rect};

pub(super) fn paint_identity(scene: &mut Scene, width: u32) {
    let identity_x = width as f64 - 241.0;
    fill_scene_rect(
        scene,
        identity_x,
        124.0,
        205.0,
        42.0,
        Color::from_rgb8(11, 17, 23),
    );
    stroke_scene_rect(
        scene,
        identity_x,
        124.0,
        205.0,
        42.0,
        Color::from_rgb8(38, 48, 58),
    );
    fill_scene_rect(
        scene,
        identity_x + 11.0,
        130.0,
        30.0,
        30.0,
        Color::from_rgb8(51, 209, 122),
    );
    text(
        scene,
        "JW",
        identity_x + 15.0,
        137.0,
        12.0,
        Color::from_rgb8(6, 34, 20),
    );
    text(
        scene,
        "@johanna.aurora",
        identity_x + 51.0,
        136.0,
        12.0,
        Color::from_rgb8(238, 243, 246),
    );
}
