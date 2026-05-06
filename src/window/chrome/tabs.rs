use peniko::Color;
use vello::Scene;

use super::text::text;
use crate::window::scene_helpers::{fill_scene_rect, stroke_scene_rect};

pub(super) fn paint_tabs(scene: &mut Scene, width: u32) {
    fill_scene_rect(scene, 14.0, 70.0, 175.0, 40.0, Color::from_rgb8(14, 23, 23));
    stroke_scene_rect(scene, 14.0, 70.0, 175.0, 40.0, Color::from_rgb8(26, 58, 50));
    text(
        scene,
        "aurora · sove...",
        45.0,
        82.0,
        14.0,
        Color::from_rgb8(240, 245, 242),
    );
    text(
        scene,
        "atlas · font...",
        235.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    text(
        scene,
        "did:plc:k7q3...m...",
        425.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    text(
        scene,
        "bastion / opu...",
        616.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    text(
        scene,
        "loading...",
        807.0,
        82.0,
        14.0,
        Color::from_rgb8(98, 107, 117),
    );
    text(
        scene,
        "+",
        969.0,
        77.0,
        22.0,
        Color::from_rgb8(111, 120, 130),
    );
    text(
        scene,
        "5 tabs      mem 184 mb      gpu 12%",
        width as f64 - 330.0,
        82.0,
        13.0,
        Color::from_rgb8(88, 97, 107),
    );
}
