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
        Color::from_rgb8(255, 247, 250),
    );
    stroke_scene_rect(
        scene,
        identity_x,
        124.0,
        205.0,
        42.0,
        Color::from_rgb8(229, 188, 204),
    );
    fill_scene_rect(
        scene,
        identity_x + 11.0,
        130.0,
        30.0,
        30.0,
        Color::from_rgb8(255, 176, 205),
    );
    text(
        scene,
        "JW",
        identity_x + 15.0,
        137.0,
        12.0,
        Color::from_rgb8(116, 54, 80),
    );
    text(
        scene,
        "@johanna.aurora",
        identity_x + 51.0,
        136.0,
        12.0,
        Color::from_rgb8(116, 54, 80),
    );
}
