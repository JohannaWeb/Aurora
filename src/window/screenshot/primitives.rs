use super::ScreenshotImage;
use image::Rgba;

pub(super) fn draw_border(
    img: &mut ScreenshotImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: Rgba<u8>,
) {
    let (width, height) = img.dimensions();

    for px in x..=(x + w).min(width - 1) {
        if y < height {
            img.put_pixel(px, y, color);
        }
        if y + h < height {
            img.put_pixel(px, y + h, color);
        }
    }

    for py in y..=(y + h).min(height - 1) {
        if x < width {
            img.put_pixel(x, py, color);
        }
        if x + w < width {
            img.put_pixel(x + w, py, color);
        }
    }
}

pub(super) fn draw_rect(
    img: &mut ScreenshotImage,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: Rgba<u8>,
) {
    let (width, height) = img.dimensions();
    for py in y..=(y + h).min(height - 1) {
        for px in x..=(x + w).min(width - 1) {
            if px < width && py < height {
                img.put_pixel(px, py, color);
            }
        }
    }
}
