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
        Color::from_rgb8(199, 206, 212),
    );
    text(
        scene,
        "›",
        58.0,
        130.0,
        24.0,
        Color::from_rgb8(199, 206, 212),
    );
    text(
        scene,
        "↻",
        100.0,
        130.0,
        24.0,
        Color::from_rgb8(199, 206, 212),
    );
    let urlbar_w = (width as f64 - 390.0).max(360.0);
    fill_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(11, 17, 23),
    );
    stroke_scene_rect(
        scene,
        135.0,
        124.0,
        urlbar_w,
        42.0,
        Color::from_rgb8(38, 48, 58),
    );
    stroke_scene_rect(
        scene,
        148.0,
        130.0,
        69.0,
        30.0,
        Color::from_rgb8(36, 79, 61),
    );
    text(
        scene,
        "TLS",
        163.0,
        136.0,
        13.0,
        Color::from_rgb8(65, 204, 120),
    );
    text(scene, "/", 231.0, 130.0, 24.0, Color::from_rgb8(40, 49, 58));
    text(
        scene,
        &truncate_chrome_text(display_url, 43),
        269.0,
        135.0,
        16.0,
        Color::from_rgb8(122, 130, 139),
    );
    let diag_x = width as f64 - 610.0;
    fill_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(18, 24, 33),
    );
    stroke_scene_rect(
        scene,
        diag_x,
        128.0,
        355.0,
        32.0,
        Color::from_rgb8(29, 38, 48),
    );
    text(
        scene,
        "dom 412 · style 38 · layout 96",
        diag_x + 13.0,
        136.0,
        12.0,
        Color::from_rgb8(112, 121, 132),
    );
}
