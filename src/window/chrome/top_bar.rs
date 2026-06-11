use peniko::Color;
use vello::Scene;

use super::text::text;
use crate::window::BROWSER_CHROME_HEIGHT;
use crate::window::scene_helpers::{fill_scene_rect, stroke_scene_rect};

pub(super) fn paint_top_bar(scene: &mut Scene, width: u32) {
    fill_scene_rect(
        scene,
        0.0,
        0.0,
        width as f64,
        BROWSER_CHROME_HEIGHT as f64,
        Color::from_rgb8(255, 241, 246),
    );
    stroke_scene_rect(
        scene,
        0.5,
        0.5,
        width as f64 - 1.0,
        BROWSER_CHROME_HEIGHT as f64 - 1.0,
        Color::from_rgb8(232, 194, 209),
    );
    fill_scene_rect(
        scene,
        15.0,
        31.0,
        18.0,
        18.0,
        Color::from_rgb8(255, 171, 204),
    );
    text(
        scene,
        "AURORA",
        43.0,
        32.0,
        14.0,
        Color::from_rgb8(105, 54, 76),
    );
    text(
        scene,
        "0.3.1",
        137.0,
        32.0,
        13.0,
        Color::from_rgb8(150, 99, 121),
    );
    text(
        scene,
        "sovereign render path · session 0x4f:c2",
        width as f64 / 2.0 - 240.0,
        32.0,
        14.0,
        Color::from_rgb8(165, 120, 139),
    );
    let engine_x = width as f64 - 193.0;
    stroke_scene_rect(
        scene,
        engine_x,
        25.0,
        148.0,
        31.0,
        Color::from_rgb8(214, 162, 186),
    );
    text(
        scene,
        "WGPU · VELLO",
        engine_x + 13.0,
        32.0,
        13.0,
        Color::from_rgb8(150, 99, 121),
    );
}
