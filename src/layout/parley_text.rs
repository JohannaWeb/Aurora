use std::cell::RefCell;

use parley::layout::Alignment;
use parley::style::{FontStack, FontWeight, StyleProperty};
use parley::{FontContext, LayoutContext};

use crate::css::{EdgeSizes, Margin, StyleMap, TextAlign, WhiteSpace};

use super::{LayoutBox, LayoutKind, Rect};

/// Shared FontContext per thread. Expensive to build; reused across layout calls.
/// In Phase 5 this should move to a per-document resource.
thread_local! {
    static FONT_CTX: RefCell<FontContext> = RefCell::new(FontContext::default());
}

/// Layout a text string into one or more LayoutBoxes, one per wrapped line.
/// Replaces `layout_text_fragments` from inline_text.rs.
pub fn layout_text_with_parley(
    node: Option<crate::dom::NodePtr>,
    text: &str,
    styles: &StyleMap,
    x: f32,
    y: f32,
    available_width: f32,
) -> Vec<LayoutBox> {
    let text = normalize_whitespace(text, styles.white_space());
    if text.is_empty() {
        return vec![];
    }

    let font_size = styles.font_size_px().unwrap_or(16.0);
    let max_width = if styles.white_space() == WhiteSpace::NoWrap {
        None
    } else {
        Some(available_width)
    };

    let alignment = match styles.text_align() {
        TextAlign::Center => Alignment::Middle,
        TextAlign::Right => Alignment::End,
        _ => Alignment::Start,
    };

    let is_bold = styles.is_bold();
    let is_italic = styles.is_italic();

    FONT_CTX.with(|font_cx| {
        let mut font_cx = font_cx.borrow_mut();
        let mut layout_cx: LayoutContext<peniko::Color> = LayoutContext::new();

        let mut builder =
            layout_cx.ranged_builder(&mut font_cx, &text, 1.0 /* scale factor */);

        builder.push_default(&StyleProperty::FontSize(font_size));
        builder.push_default(&StyleProperty::LineHeight(1.2));
        builder.push_default(&StyleProperty::FontStack(FontStack::Source("system-ui")));

        if is_bold {
            builder.push_default(&StyleProperty::FontWeight(FontWeight::BOLD));
        }
        if is_italic {
            builder.push_default(&StyleProperty::FontStyle(
                parley::style::FontStyle::Italic,
            ));
        }

        // Apply text color as brush.
        if let Some(color_str) = styles.get("color") {
            if let Some(color) = parse_peniko_color(color_str) {
                builder.push_default(&StyleProperty::Brush(color));
            }
        }

        let mut layout = builder.build(&text);

        // Break into lines.
        layout.break_all_lines(max_width);
        layout.align(max_width, alignment);

        // Convert each line to a LayoutBox.
        let mut boxes = Vec::new();
        let mut cursor_y = y;

        for line in layout.lines() {
            let metrics = line.metrics();
            let line_height = metrics.line_height;
            let line_width = line
                .items()
                .fold(0.0_f32, |acc, item| {
                    use parley::layout::PositionedLayoutItem;
                    match item {
                        PositionedLayoutItem::GlyphRun(run) => acc + run.advance(),
                        PositionedLayoutItem::InlineBox(b) => acc + b.width,
                    }
                })
                .min(available_width);

            // Collect the text content for this line.
            let line_text = collect_line_text(&line, &text);

            boxes.push(LayoutBox {
                node: node.clone(),
                kind: LayoutKind::Text { text: line_text },
                rect: Rect {
                    x,
                    y: cursor_y,
                    width: line_width,
                    height: line_height,
                },
                styles: styles.clone(),
                margin: Margin::zero(),
                border: EdgeSizes::zero(),
                padding: EdgeSizes::zero(),
                children: Vec::new(),
            });

            cursor_y += line_height;
        }

        boxes
    })
}

/// Measure the width of a text string using Parley.
/// Replaces the rough `estimate_text_width` heuristic.
pub fn measure_text_width(text: &str, styles: &StyleMap) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let font_size = styles.font_size_px().unwrap_or(16.0);

    FONT_CTX.with(|font_cx| {
        let mut font_cx = font_cx.borrow_mut();
        let mut layout_cx: LayoutContext<peniko::Color> = LayoutContext::new();
        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0);
        builder.push_default(&StyleProperty::FontSize(font_size));
        builder.push_default(&StyleProperty::FontStack(FontStack::Source("system-ui")));
        let mut layout = builder.build(text);
        layout.break_all_lines(None); // No wrapping for measurement.
        layout
            .lines()
            .next()
            .map(|line| {
                line.items().fold(0.0_f32, |acc, item| {
                    use parley::layout::PositionedLayoutItem;
                    match item {
                        PositionedLayoutItem::GlyphRun(run) => acc + run.advance(),
                        PositionedLayoutItem::InlineBox(b) => acc + b.width,
                    }
                })
            })
            .unwrap_or(0.0)
    })
}

/// Collect the visible text content of a Parley line.
fn collect_line_text(line: &parley::layout::Line<peniko::Color>, full_text: &str) -> String {
    use parley::layout::PositionedLayoutItem;
    let mut result = String::new();
    for item in line.items() {
        if let PositionedLayoutItem::GlyphRun(run) = item {
            let range = run.run().text_range();
            if let Some(slice) = full_text.get(range) {
                result.push_str(slice);
            }
        }
    }
    result
}

/// Normalize whitespace per CSS white-space property.
fn normalize_whitespace(text: &str, white_space: WhiteSpace) -> String {
    match white_space {
        WhiteSpace::NoWrap | WhiteSpace::Normal => {
            let words: Vec<&str> = text.split_whitespace().collect();
            words.join(" ")
        }
        WhiteSpace::Pre => text.to_string(),
    }
}

/// Parse a CSS color string to `peniko::Color`.
fn parse_peniko_color(s: &str) -> Option<peniko::Color> {
    let s = s.trim().to_lowercase();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            return Some(peniko::Color::from_rgba8(
                ((n >> 16) & 0xFF) as u8,
                ((n >> 8) & 0xFF) as u8,
                (n & 0xFF) as u8,
                255,
            ));
        }
        if hex.len() == 3 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let r = ((n >> 8) & 0xF) as u8 * 17;
            let g = ((n >> 4) & 0xF) as u8 * 17;
            let b = (n & 0xF) as u8 * 17;
            return Some(peniko::Color::from_rgba8(r, g, b, 255));
        }
    }
    match s.as_str() {
        "black" => Some(peniko::Color::BLACK),
        "white" => Some(peniko::Color::WHITE),
        "red" => Some(peniko::Color::from_rgba8(255, 0, 0, 255)),
        "green" => Some(peniko::Color::from_rgba8(0, 128, 0, 255)),
        "blue" => Some(peniko::Color::from_rgba8(0, 0, 255, 255)),
        "gray" | "grey" => Some(peniko::Color::from_rgba8(128, 128, 128, 255)),
        _ => None,
    }
}
