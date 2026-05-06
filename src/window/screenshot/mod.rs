mod chrome;
mod color;
mod layout;
mod primitives;
mod scrollbar;
mod text;

use super::input::WindowInput;
use super::BROWSER_CHROME_HEIGHT;
use image::{ImageBuffer, Rgba};

pub(super) type ScreenshotImage = ImageBuffer<Rgba<u8>, Vec<u8>>;

pub(super) fn render_to_file(input: &WindowInput, path: &str) {
    eprintln!("Rendering to PNG: {}", path);
    let width = env_u32("AURORA_SCREENSHOT_WIDTH")
        .or_else(|| env_u32("AURORA_VIEWPORT_WIDTH"))
        .unwrap_or(1200);
    let height = env_u32("AURORA_SCREENSHOT_HEIGHT")
        .or_else(|| env_u32("AURORA_VIEWPORT_HEIGHT"))
        .unwrap_or(1024);

    let mut img = ImageBuffer::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([255, 255, 255, 255]);
    }

    let chrome_offset = BROWSER_CHROME_HEIGHT.round() as i32;
    layout::render_layout_with_text(&input.layout, &mut img, 0, chrome_offset);
    scrollbar::render_scrollbars(input.layout.root(), &mut img, 0, chrome_offset);
    chrome::render_browser_chrome(
        &mut img,
        width,
        input.base_url.as_deref().unwrap_or("aurora://local"),
    );

    if let Err(e) = img.save(path) {
        eprintln!("Failed to save screenshot: {}", e);
    } else {
        eprintln!("Screenshot saved to {}", path);
    }
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse::<u32>().ok()
}
