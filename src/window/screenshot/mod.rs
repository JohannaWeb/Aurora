mod chrome;
mod color;
mod layout;
mod primitives;
mod scrollbar;
mod text;

use super::BROWSER_CHROME_HEIGHT;
use super::input::{SnapshotRebuildReason, WindowInput};
use anyrender::ImageRenderer;
use anyrender_vello::VelloImageRenderer;
use image::{ImageBuffer, Rgba};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

pub(super) type ScreenshotImage = ImageBuffer<Rgba<u8>, Vec<u8>>;

pub(super) fn render_to_file(mut input: WindowInput, path: &str) {
    eprintln!("Rendering to PNG: {}", path);
    let width = env_u32("AURORA_SCREENSHOT_WIDTH")
        .or_else(|| env_u32("AURORA_VIEWPORT_WIDTH"))
        .unwrap_or(1200);
    let height = env_u32("AURORA_SCREENSHOT_HEIGHT")
        .or_else(|| env_u32("AURORA_VIEWPORT_HEIGHT"))
        .unwrap_or(1024);

    flush_ready_frame_tasks(&mut input, width, height);

    let mut img = ImageBuffer::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([255, 255, 255, 255]);
    }

    let chrome_offset = BROWSER_CHROME_HEIGHT.round() as i32;
    let rendered_with_blitz = input
        .blitz_doc
        .as_ref()
        .map(|blitz_doc| render_blitz_content(blitz_doc, width, height, &mut img, chrome_offset))
        .unwrap_or(false);
    if !rendered_with_blitz {
        let layout = input.layout.borrow();
        layout::render_layout_with_text(&layout, &mut img, 0, chrome_offset);
        scrollbar::render_scrollbars(layout.root(), &mut img, 0, chrome_offset);
    }
    chrome::render_browser_chrome(
        &mut img,
        width,
        input.base_url.as_deref().unwrap_or("aurora://local"),
        &input.dom,
        &input.identity,
    );

    if let Err(e) = img.save(path) {
        eprintln!("Failed to save screenshot: {}", e);
    } else {
        eprintln!("Screenshot saved to {}", path);
    }
}

fn render_blitz_content(
    blitz_doc: &std::rc::Rc<std::cell::RefCell<crate::blitz_document::BlitzDocument>>,
    width: u32,
    height: u32,
    img: &mut ScreenshotImage,
    chrome_offset: i32,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let content_height = height
            .saturating_sub(BROWSER_CHROME_HEIGHT.round() as u32)
            .max(1);
        let mut renderer = VelloImageRenderer::new(width, content_height);
        let mut content = Vec::new();
        renderer.render_to_vec(
            |painter| {
                let _paint_result =
                    blitz_doc
                        .borrow_mut()
                        .paint_with(painter, width, content_height);
            },
            &mut content,
        );
        blit_rgba(&content, width, content_height, img, 0, chrome_offset);
    }))
    .map(|_| true)
    .unwrap_or(false)
}

fn blit_rgba(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst: &mut ScreenshotImage,
    dst_x: i32,
    dst_y: i32,
) {
    for y in 0..src_height {
        for x in 0..src_width {
            let target_x = dst_x + x as i32;
            let target_y = dst_y + y as i32;
            if target_x < 0
                || target_y < 0
                || target_x >= dst.width() as i32
                || target_y >= dst.height() as i32
            {
                continue;
            }
            let idx = ((y * src_width + x) * 4) as usize;
            if idx + 3 < src.len() {
                dst.put_pixel(
                    target_x as u32,
                    target_y as u32,
                    Rgba([src[idx], src[idx + 1], src[idx + 2], src[idx + 3]]),
                );
            }
        }
    }
}

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse::<u32>().ok()
}

fn flush_ready_frame_tasks(input: &mut WindowInput, width: u32, height: u32) {
    let frames = env_u32("AURORA_SCREENSHOT_FRAMES").unwrap_or(4).min(60);
    for _ in 0..frames {
        let needs_reflow = {
            let Some(runtime) = input.runtime.as_mut() else {
                return;
            };
            let now = Instant::now();
            runtime.tick(now)
                | runtime.drain_animation_frame_callbacks(Instant::now())
                | runtime.take_needs_reflow()
        };
        if needs_reflow {
            if input.blitz_doc.is_none() {
                input.mark_blitz_snapshot_dirty(SnapshotRebuildReason::MissingMapping);
            }
            input.reflow(width, height);
        }
        match input.runtime.as_ref() {
            Some(runtime) if runtime.has_ready_work(Instant::now()) => {}
            _ => return,
        }
    }
}
