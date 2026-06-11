use peniko::Color;
use vello::Scene;

use super::display::truncate_chrome_text;
use super::text::text;
use crate::window::scene_helpers::{fill_scene_rect, stroke_scene_rect};

pub(super) fn paint_nav_and_url(scene: &mut Scene, width: u32, display_url: &str) {
    text(
        scene,
        "‹",
        16.0,
        130.0,
        24.0,
        Color::from_rgb8(150, 99, 121),
    );
    text(
        scene,
        "›",
        58.0,
        130.0,
        24.0,
        Color::from_rgb8(150, 99, 121),
    );
    text(
        scene,
        "↻",
        100.0,
        130.0,
        24.0,
        Color::from_rgb8(150, 99, 121),
    );
    let urlbar_w = (width as f64 - 390.0).max(360.0);
    fill_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(255, 249, 251),
    );
    stroke_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(227, 186, 202),
    );
    stroke_scene_rect(
        scene,
        148.0,
        130.0,
        69.0,
        30.0,
        Color::from_rgb8(226, 156, 184),
    );
    text(
        scene,
        "TLS",
        163.0,
        136.0,
        13.0,
        Color::from_rgb8(198, 87, 133),
    );
    text(
        scene,
        "/",
        231.0,
        130.0,
        24.0,
        Color::from_rgb8(188, 137, 159),
    );
    text(
        scene,
        &truncate_chrome_text(display_url, 43),
        269.0,
        135.0,
        16.0,
        Color::from_rgb8(149, 113, 129),
    );
    let diag_x = width as f64 - 610.0;
    fill_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(255, 244, 248),
    );
    stroke_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(231, 194, 209),
    );
    text(
        scene,
        "snapshot · shell 412 nodes",
        diag_x + 13.0,
        136.0,
        12.0,
        Color::from_rgb8(160, 120, 140),
    );
}
