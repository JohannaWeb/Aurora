use crate::layout::{LayoutBox, LayoutTree};
use minifb::{Window, WindowOptions};

pub fn open(layout: &LayoutTree) {
    let root = layout.root();
    let rect = root.rect();
    let width = rect.width.ceil().max(1.0) as usize;
    let height = rect.height.ceil().max(1.0) as usize;

    eprintln!("Opening window: {}x{}", width, height);
    eprintln!("Root rect: x={} y={} w={} h={}", rect.x, rect.y, rect.width, rect.height);

    let mut window = Window::new(
        "Aurora Browser Engine",
        width,
        height,
        WindowOptions::default(),
    )
    .expect("failed to create window");

    eprintln!("Window created, rendering...");

    // Create a buffer to render into. minifb uses u32 format.
    let mut buffer = vec![0; width * height];

    // Render the layout tree into the buffer
    render_layout(layout, &mut buffer, width as u32, height as u32);

    // Display the buffer in the window
    window
        .update_with_buffer(&buffer, width, height)
        .expect("failed to update window");

    // Keep the window open until the user closes it
    while window.is_open() {
        window
            .update_with_buffer(&buffer, width, height)
            .expect("failed to update window");
    }
}

fn render_layout(layout: &LayoutTree, buffer: &mut [u32], w: u32, h: u32) {
    // Clear to a neutral background
    clear_buffer(buffer, w, h, 0xFFF8F5EB); // paper-white
    render_box(layout.root(), buffer, w, h);
}

fn render_box(b: &LayoutBox, buffer: &mut [u32], w: u32, h: u32) {
    let styles = b.styles();
    if styles.opacity() < 0.5 {
        return;
    }
    if styles.visibility() == "hidden" {
        return;
    }

    if b.is_viewport() {
        let bg = color(styles.background_color().unwrap_or("paper-white"));
        let r = b.rect();
        fill_rect(
            buffer,
            w,
            h,
            r.x.max(0.0) as u32,
            r.y.max(0.0) as u32,
            r.width as u32,
            r.height as u32,
            bg,
        );
    } else if b.is_image() {
        draw_image(b, buffer, w, h);
    } else if let Some(text) = b.text() {
        // Draw text with a simple bitmap font
        let r = b.rect();
        let text_color = color(b.styles().get("color").unwrap_or("coal"));
        draw_text_simple(buffer, w, h, r.x as u32, r.y as u32, text, text_color);
    } else {
        draw_element(b, buffer, w, h);
    }

    for child in b.children() {
        render_box(child, buffer, w, h);
    }
}

fn draw_element(b: &LayoutBox, buffer: &mut [u32], w: u32, h: u32) {
    let styles = b.styles();
    let r = b.rect();
    let border = styles.border_width();

    // Fill whole rect with border color
    if border.top > 0.0
        || border.right > 0.0
        || border.bottom > 0.0
        || border.left > 0.0
    {
        let bc = color(styles.border_color().unwrap_or("coal"));
        // Top strip
        fill_rect(
            buffer,
            w,
            h,
            r.x.max(0.0) as u32,
            r.y.max(0.0) as u32,
            r.width as u32,
            border.top.min(r.height) as u32,
            bc,
        );
        // Bottom strip
        let by = (r.y + r.height - border.bottom).max(r.y) as u32;
        fill_rect(
            buffer,
            w,
            h,
            r.x.max(0.0) as u32,
            by,
            r.width as u32,
            border.bottom.min(r.height) as u32,
            bc,
        );
        // Left strip
        fill_rect(
            buffer,
            w,
            h,
            r.x.max(0.0) as u32,
            r.y.max(0.0) as u32,
            border.left.min(r.width) as u32,
            r.height as u32,
            bc,
        );
        // Right strip
        let rx = (r.x + r.width - border.right).max(r.x) as u32;
        fill_rect(
            buffer,
            w,
            h,
            rx,
            r.y.max(0.0) as u32,
            border.right.min(r.width) as u32,
            r.height as u32,
            bc,
        );
    }

    // Fill padding+content with background color
    if let Some(bg_name) = styles.background_color() {
        let pr = b.padding_rect();
        fill_rect(
            buffer,
            w,
            h,
            pr.x.max(0.0) as u32,
            pr.y.max(0.0) as u32,
            pr.width as u32,
            pr.height as u32,
            color(bg_name),
        );
    }
}

fn draw_image(b: &LayoutBox, buffer: &mut [u32], w: u32, h: u32) {
    let styles = b.styles();
    let r = b.rect();
    let border = styles.border_width();

    if border.top > 0.0
        || border.right > 0.0
        || border.bottom > 0.0
        || border.left > 0.0
    {
        let bc = color(styles.border_color().unwrap_or("ember"));
        fill_rect(
            buffer,
            w,
            h,
            r.x.max(0.0) as u32,
            r.y.max(0.0) as u32,
            r.width as u32,
            r.height as u32,
            bc,
        );
    }

    let pr = b.padding_rect();
    let px = pr.x.max(0.0) as u32;
    let py = pr.y.max(0.0) as u32;
    let pw = pr.width as u32;
    let ph = pr.height as u32;
    let half = ph / 2;
    fill_rect(buffer, w, h, px, py, pw, half, 0xFFC0C0C8);
    fill_rect(buffer, w, h, px, py + half, pw, ph - half, 0xFF969695);
    fill_rect(buffer, w, h, px, py, pw, 4.min(ph), 0xFF00D2C8);
}

fn clear_buffer(buffer: &mut [u32], _w: u32, _h: u32, color: u32) {
    for pixel in buffer.iter_mut() {
        *pixel = color;
    }
}

fn fill_rect(buffer: &mut [u32], cw: u32, ch: u32, x: u32, y: u32, w: u32, h: u32, color: u32) {
    if w == 0 || h == 0 || x >= cw || y >= ch {
        return;
    }
    let x_end = (x + w).min(cw);
    let y_end = (y + h).min(ch);
    for row in y..y_end {
        for col in x..x_end {
            let idx = (row * cw + col) as usize;
            if idx < buffer.len() {
                buffer[idx] = color;
            }
        }
    }
}

fn draw_text_simple(buffer: &mut [u32], w: u32, h: u32, mut x: u32, y: u32, text: &str, color: u32) {
    let char_width = 6;
    let char_height = 10;

    for ch in text.chars() {
        if ch == '\n' {
            // Wrap to next line (not implemented)
            continue;
        }

        if x + char_width > w {
            // Skip chars that overflow
            continue;
        }

        if y + char_height > h {
            break; // Stop if we've gone past the bottom
        }

        // Draw simple character using a bitmap pattern
        draw_char(buffer, w, h, x, y, ch, color, char_width, char_height);
        x += char_width + 1;
    }
}

fn draw_char(buffer: &mut [u32], w: u32, h: u32, x: u32, y: u32, ch: char, color: u32, _cw: u32, _ch_h: u32) {
    // Simple bitmap patterns for common characters (4x6 or 5x8 approximate)
    let pattern = match ch {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b11100, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b01110, 0b00001, 0b10001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        'a' => [0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111],
        'b' => [0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b11110],
        'c' => [0b00000, 0b01110, 0b10000, 0b10000, 0b10000, 0b01110],
        'd' => [0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b01111],
        'e' => [0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
        'f' => [0b00110, 0b01000, 0b11110, 0b01000, 0b01000, 0b01000],
        'g' => [0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b01110],
        'h' => [0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001],
        'i' => [0b00100, 0b00000, 0b00100, 0b00100, 0b00100, 0b00110],
        'j' => [0b00100, 0b00000, 0b00100, 0b00100, 0b10100, 0b01000],
        'k' => [0b10000, 0b10000, 0b10010, 0b11100, 0b10010, 0b10001],
        'l' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00110],
        'm' => [0b00000, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101],
        'n' => [0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001],
        'o' => [0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        'p' => [0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000],
        'q' => [0b00000, 0b01111, 0b10001, 0b01111, 0b00001, 0b00001],
        'r' => [0b00000, 0b11011, 0b10110, 0b10000, 0b10000, 0b10000],
        's' => [0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110],
        't' => [0b00100, 0b11110, 0b00100, 0b00100, 0b00100, 0b00010],
        'u' => [0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b01111],
        'v' => [0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'w' => [0b00000, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'x' => [0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001],
        'y' => [0b00000, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110],
        'z' => [0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111],
        '0' => [0b01110, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00110, 0b00001, 0b10001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b10001, 0b01110],
        '6' => [0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000],
        '8' => [0b01110, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00000],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b01000],
        ':' => [0b00000, 0b00100, 0b00000, 0b00100, 0b00000, 0b00000],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        '#' => [0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b00000],
        '(' => [0b00010, 0b00100, 0b00100, 0b00100, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00100, 0b00100, 0b00100, 0b01000],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '?' => [0b01110, 0b00001, 0b00110, 0b00100, 0b00000, 0b00100],
        '+' => [0b00000, 0b00100, 0b01110, 0b00100, 0b00000, 0b00000],
        '=' => [0b00000, 0b01111, 0b00000, 0b01111, 0b00000, 0b00000],
        '/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000],
        '"' => [0b01010, 0b01010, 0b00000, 0b00000, 0b00000, 0b00000],
        '\'' => [0b00100, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        '-' => [0b00000, 0b00000, 0b01111, 0b00000, 0b00000, 0b00000],
        _ => [0b11111, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001], // ? symbol as fallback
    };

    // Draw the pattern as a 5-bit wide bitmap
    for (row, &bits) in pattern.iter().enumerate() {
        let py = y + row as u32;
        if py >= h {
            break;
        }
        for col in 0..5 {
            if (bits & (1 << (4 - col))) != 0 {
                let px = x + col;
                if px < w {
                    let idx = (py * w + px) as usize;
                    if idx < buffer.len() {
                        buffer[idx] = color;
                    }
                }
            }
        }
    }
}

fn color(name: &str) -> u32 {
    // minifb uses ARGB format: 0xAARRGGBB
    match name.trim() {
        "aurora-cyan" => 0xFF00D2C8,
        "mist" => 0xFFDCE6EB,
        "sand" => 0xFFE6D2AF,
        "ember" => 0xFFD26428,
        "haze" => 0xFFBEC8D7,
        "flare" => 0xFFF0A03C,
        "paper-white" => 0xFFF8F5EB,
        "pine" => 0xFF286446,
        "coal" => 0xFF282828,
        "slate" => 0xFF646E7D,
        "gold" => 0xFFCAA832,
        "cyan" => 0xFF00C8DC,
        "blue" => 0xFF3C64DC,
        "red" => 0xFFD23232,
        "gray" | "grey" => 0xFF969696,
        "green" => 0xFF3CB450,
        "white" => 0xFFFFFFFF,
        "black" => 0xFF000000,
        _ => 0xFFB4B4B4, // visible fallback
    }
}
