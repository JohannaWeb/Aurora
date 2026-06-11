use peniko::Color;
use vello::Scene;

use super::text::text;
use crate::window::scene_helpers::{fill_scene_rect, stroke_scene_rect};

pub(super) fn paint_tabs(scene: &mut Scene, width: u32) {
    fill_scene_rect(
        scene,
        14.0,
        70.0,
        175.0,
        40.0,
        Color::from_rgb8(255, 227, 238),
    );
    stroke_scene_rect(
        scene,
        14.0,
        70.0,
        175.0,
        40.0,
        Color::from_rgb8(225, 164, 189),
    );
    text(
        scene,
        "aurora · sove...",
        45.0,
        82.0,
        14.0,
        Color::from_rgb8(110, 60, 81),
    );
    text(
        scene,
        "atlas · font...",
        235.0,
        82.0,
        14.0,
        Color::from_rgb8(165, 120, 139),
    );
    text(
        scene,
        "did:plc:k7q3...m...",
        425.0,
        82.0,
        14.0,
        Color::from_rgb8(165, 120, 139),
    );
    text(
        scene,
        "bastion / opu...",
        616.0,
        82.0,
        14.0,
        Color::from_rgb8(165, 120, 139),
    );
    text(
        scene,
        "loading...",
        807.0,
        82.0,
        14.0,
        Color::from_rgb8(165, 120, 139),
    );
    text(
        scene,
        "+",
        969.0,
        77.0,
        22.0,
        Color::from_rgb8(176, 128, 148),
    );
    text(
        scene,
        "5 tabs      mem 184 mb      gpu 12%",
        width as f64 - 330.0,
        82.0,
        13.0,
        Color::from_rgb8(164, 117, 137),
    );
}
