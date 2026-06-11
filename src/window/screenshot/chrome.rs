use super::ScreenshotImage;
use super::primitives::{draw_border, draw_rect};
use super::text::render_text_simple;
use crate::window::BROWSER_CHROME_HEIGHT;
use crate::window::chrome::{chrome_display_url, truncate_chrome_text};
use image::Rgba;

pub(super) fn render_browser_chrome(img: &mut ScreenshotImage, width: u32, url: &str) {
    let display_url = chrome_display_url(url);
    let chrome_h = BROWSER_CHROME_HEIGHT.round() as u32;
    draw_rect(img, 0, 0, width, chrome_h, Rgba([255, 241, 246, 255]));
    draw_border(
        img,
        0,
        0,
        width.saturating_sub(1),
        chrome_h,
        Rgba([232, 194, 209, 255]),
    );
    render_header(img, width);
    render_tabs(img, width);
    render_urlbar(img, width, &display_url);
    render_identity(img, width);
}

fn render_header(img: &mut ScreenshotImage, width: u32) {
    draw_rect(img, 15, 31, 18, 18, Rgba([255, 171, 204, 255]));
    text(img, "AURORA", 43, 32, Rgba([105, 54, 76, 255]), 14);
    text(img, "0.3.1", 137, 32, Rgba([150, 99, 121, 255]), 13);
    text(
        img,
        "sovereign render path · session 0x4f:c2",
        (width as i32 / 2) - 240,
        32,
        Rgba([165, 120, 139, 255]),
        14,
    );
    draw_rect(
        img,
        width.saturating_sub(193),
        25,
        148,
        31,
        Rgba([255, 241, 246, 255]),
    );
    draw_border(
        img,
        width.saturating_sub(193),
        25,
        148,
        31,
        Rgba([214, 162, 186, 255]),
    );
    text(
        img,
        "WGPU · VELLO",
        width.saturating_sub(180) as i32,
        32,
        Rgba([150, 99, 121, 255]),
        13,
    );
}

fn render_tabs(img: &mut ScreenshotImage, width: u32) {
    draw_rect(img, 14, 70, 175, 40, Rgba([255, 227, 238, 255]));
    draw_border(img, 14, 70, 175, 40, Rgba([225, 164, 189, 255]));
    text(
        img,
        "aurora · sove...",
        45,
        82,
        Rgba([110, 60, 81, 255]),
        14,
    );
    text(
        img,
        "atlas · font...",
        235,
        82,
        Rgba([165, 120, 139, 255]),
        14,
    );
    text(
        img,
        "did:plc:k7q3...m...",
        425,
        82,
        Rgba([165, 120, 139, 255]),
        14,
    );
    text(
        img,
        "bastion / opu...",
        616,
        82,
        Rgba([165, 120, 139, 255]),
        14,
    );
    text(img, "loading...", 807, 82, Rgba([165, 120, 139, 255]), 14);
    text(img, "+", 969, 77, Rgba([176, 128, 148, 255]), 22);
    text(
        img,
        "5 tabs      mem 184 mb      gpu 12%",
        width.saturating_sub(330) as i32,
        82,
        Rgba([164, 117, 137, 255]),
        13,
    );
}

fn render_urlbar(img: &mut ScreenshotImage, width: u32, display_url: &str) {
    text(img, "‹", 16, 130, Rgba([150, 99, 121, 255]), 24);
    text(img, "›", 58, 130, Rgba([150, 99, 121, 255]), 24);
    text(img, "↻", 100, 130, Rgba([150, 99, 121, 255]), 24);
    let urlbar_w = width.saturating_sub(390).max(360);
    draw_rect(img, 135, 124, urlbar_w, 42, Rgba([255, 249, 251, 255]));
    draw_border(img, 135, 124, urlbar_w, 42, Rgba([227, 186, 202, 255]));
    draw_rect(img, 148, 130, 69, 30, Rgba([255, 247, 250, 255]));
    draw_border(img, 148, 130, 69, 30, Rgba([226, 156, 184, 255]));
    text(img, "TLS", 163, 136, Rgba([198, 87, 133, 255]), 13);
    text(img, "/", 231, 130, Rgba([188, 137, 159, 255]), 24);
    text(
        img,
        &truncate_chrome_text(display_url, 43),
        269,
        135,
        Rgba([149, 113, 129, 255]),
        16,
    );
    let diag_x = width.saturating_sub(610);
    draw_rect(img, diag_x, 128, 355, 32, Rgba([255, 244, 248, 255]));
    draw_border(img, diag_x, 128, 355, 32, Rgba([231, 194, 209, 255]));
    text(
        img,
        "snapshot · shell 412 nodes",
        diag_x as i32 + 13,
        136,
        Rgba([160, 120, 140, 255]),
        12,
    );
}

fn render_identity(img: &mut ScreenshotImage, width: u32) {
    let identity_x = width.saturating_sub(241);
    draw_rect(img, identity_x, 124, 205, 42, Rgba([255, 247, 250, 255]));
    draw_border(img, identity_x, 124, 205, 42, Rgba([229, 188, 204, 255]));
    draw_rect(
        img,
        identity_x + 11,
        130,
        30,
        30,
        Rgba([255, 176, 205, 255]),
    );
    text(
        img,
        "JW",
        identity_x as i32 + 15,
        137,
        Rgba([116, 54, 80, 255]),
        12,
    );
    text(
        img,
        "@johanna.aurora",
        identity_x as i32 + 51,
        136,
        Rgba([116, 54, 80, 255]),
        12,
    );
}

fn text(img: &mut ScreenshotImage, value: &str, x: i32, y: i32, color: Rgba<u8>, size: u32) {
    render_text_simple(img, value, x, y, color, size);
}
