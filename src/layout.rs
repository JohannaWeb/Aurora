use crate::css::{DisplayMode, EdgeSizes, StyleMap};
use crate::style::{StyleTree, StyledNode};
use std::fmt::{self, Display, Formatter};

#[allow(dead_code)]
const DEFAULT_VIEWPORT_WIDTH: f32 = 800.0;
const BLOCK_VERTICAL_PADDING: f32 = 8.0;
const INLINE_BOX_HEIGHT: f32 = 20.0;
const TEXT_CHAR_WIDTH: f32 = 7.0;
const TEXT_LINE_HEIGHT: f32 = 18.0;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTree {
    root: LayoutBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutBox {
    kind: LayoutKind,
    rect: Rect,
    styles: StyleMap,
    z_index: i32,
    margin: EdgeSizes,
    border: EdgeSizes,
    padding: EdgeSizes,
    children: Vec<LayoutBox>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LayoutKind {
    Viewport,
    Block { tag_name: String },
    Inline { tag_name: String },
    Flex { tag_name: String },
    Image {
        alt: Option<String>,
        src: Option<String>,
        display_mode: DisplayMode,
    },
    Text { text: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutTree {
    #[allow(dead_code)]
    pub fn from_style_tree(style_tree: &StyleTree) -> Self {
        Self::from_style_tree_with_viewport_width(style_tree, DEFAULT_VIEWPORT_WIDTH)
    }

    pub fn from_style_tree_with_viewport_width(
        style_tree: &StyleTree,
        viewport_width: f32,
    ) -> Self {
        let mut root = LayoutBox::layout_root(style_tree.root(), viewport_width)
            .expect("style tree root must produce a viewport");
        
        let viewport_rect = Rect { x: 0.0, y: 0.0, width: viewport_width, height: root.rect.height };
        root.layout_positioned_elements(viewport_rect, viewport_rect);

        Self { root }
    }

    pub fn root(&self) -> &LayoutBox {
        &self.root
    }
}

impl LayoutBox {
    fn layout_root(node: &StyledNode, viewport_width: f32) -> Option<Self> {
        let mut root = Self::from_styled_node(node, 0.0, 0.0, viewport_width)?;
        root.rect.width = viewport_width;
        Some(root)
    }

    fn from_styled_node(node: &StyledNode, x: f32, y: f32, available_width: f32) -> Option<Self> {
        if node.tag_name() == Some("style") {
            return None;
        }

        match node.tag_name() {
            None if node.text().is_none() => Some(Self::layout_container(
                LayoutKind::Viewport,
                StyleMap::default(),
                EdgeSizes::zero(),
                EdgeSizes::zero(),
                EdgeSizes::zero(),
                node.children(),
                x,
                y,
                available_width,
            )),
            Some(tag_name) => Self::from_element(tag_name, node, x, y, available_width),
            None => Some(Self::layout_text(node.text().unwrap_or_default(), node.styles().clone(), x, y)),
        }
    }

    fn from_element(
        tag_name: &str,
        node: &StyledNode,
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Option<Self> {
        let styles = node.styles().clone();
        let display_mode = if styles.get("display").is_none() && is_inline_by_default(tag_name) {
            DisplayMode::Inline
        } else {
            styles.display_mode()
        };

        match display_mode {
            DisplayMode::None => None,
            mode if tag_name == "img" => Some(Self::layout_image(
                node,
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                x,
                y,
                available_width,
                mode,
            )),
            DisplayMode::Block => Some(Self::layout_container(
                LayoutKind::Block {
                    tag_name: tag_name.to_string(),
                },
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
            DisplayMode::Inline => Some(Self::layout_inline(
                tag_name,
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
            DisplayMode::Flex => Some(Self::layout_flex(
                tag_name,
                styles,
                node.styles().margin(),
                node.styles().border_width(),
                node.styles().padding(),
                node.children(),
                x,
                y,
                available_width,
            )),
        }
    }

    fn layout_image(
        node: &StyledNode,
        styles: StyleMap,
        margin: EdgeSizes,
        border: EdgeSizes,
        padding: EdgeSizes,
        x: f32,
        y: f32,
        available_width: f32,
        display_mode: DisplayMode,
    ) -> Self {
        let rect_x = x + margin.left;
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let width_hint = node
            .attribute("width")
            .and_then(parse_html_length_px)
            .unwrap_or(160.0);
        let height_hint = node
            .attribute("height")
            .and_then(parse_html_length_px)
            .unwrap_or(96.0);
        let content_width = clamp_content_width(&styles, width_hint, available_rect_width);
        let content_height = clamp_content_height(&styles, height_hint);

        Self {
            kind: LayoutKind::Image {
                alt: node.attribute("alt").map(ToOwned::to_owned),
                src: node.attribute("src").map(ToOwned::to_owned),
                display_mode,
            },
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width),
                height: content_height + padding.vertical() + border.vertical(),
            },
            z_index: node.styles().z_index(),
            styles,
            margin,
            border,
            padding,
            children: Vec::new(),
        }
    }

    fn layout_container(
        kind: LayoutKind,
        styles: StyleMap,
        margin: EdgeSizes,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let rect_x = x + margin.left;
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width =
            (available_rect_width - padding.horizontal() - border.horizontal()).max(0.0);
        let content_width = clamp_content_width(&styles, default_content_width, default_content_width);
        let rect_width =
            (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width);
        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;
        let mut layout_children = Vec::new();
        let mut cursor_y = content_y;
        let mut previous_block_bottom_margin: f32 = 0.0;
        let mut previous_was_block = false;

        for child in children {
            let child_margin = child.styles().margin();
            let child_is_positioned = child.styles().position() == "absolute" || child.styles().position() == "fixed";

            if let Some(layout_child) =
                Self::from_styled_node(child, content_x, cursor_y, content_width)
            {
                if !child_is_positioned {
                    let child_is_block = child
                        .tag_name()
                        .map(|_| child.styles().display_mode() == DisplayMode::Block)
                        .unwrap_or(false);
                    let collapse_overlap = if previous_was_block && child_is_block {
                        previous_block_bottom_margin.min(child_margin.top)
                    } else {
                        0.0
                    };

                    // Re-layout child with correct y if there's overlap (minor optimization: we already laid it out)
                    // For now, just adjust the rect and cursor
                    let mut final_child = layout_child;
                    final_child.rect.y -= collapse_overlap;

                    cursor_y += final_child.total_height() - collapse_overlap;
                    previous_block_bottom_margin = if child_is_block {
                        final_child.margin.bottom
                    } else {
                        0.0
                    };
                    previous_was_block = child_is_block;
                    layout_children.push(final_child);
                } else {
                    layout_children.push(layout_child);
                }
            }
        }

        let content_height = cursor_y - content_y;
        let inner_height = if layout_children.is_empty() {
            BLOCK_VERTICAL_PADDING
        } else {
            content_height + BLOCK_VERTICAL_PADDING
        };
        let resolved_content_height = clamp_content_height(&styles, inner_height);

        Self {
            kind,
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: rect_width,
                height: border.top + padding.top + resolved_content_height + padding.bottom + border.bottom,
            },
            z_index: styles.z_index(),
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_flex(
        tag_name: &str,
        styles: StyleMap,
        margin: EdgeSizes,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let rect_x = x + margin.left;
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width = (available_rect_width - padding.horizontal() - border.horizontal()).max(0.0);
        let content_width = clamp_content_width(&styles, default_content_width, default_content_width);
        let rect_width = (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width);
        
        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;
        
        let is_column = styles.flex_direction() == "column";
        let gap = styles.gap_px();
        
        let mut total_grow = 0.0;
        let mut total_natural_size = 0.0;
        let mut child_natural_sizes = Vec::new();
        
        // 1. Initial measure
        for child in children {
            let child_is_positioned = child.styles().position() == "absolute" || child.styles().position() == "fixed";
            if child_is_positioned {
                child_natural_sizes.push(0.0);
                continue;
            }

            if is_column {
                // Column: measure natural height
                if let Some(layout_child) = Self::from_styled_node(child, content_x, content_y, content_width) {
                    let h = layout_child.total_height();
                    total_natural_size += h;
                    child_natural_sizes.push(h);
                    total_grow += child.styles().flex_grow();
                } else {
                    child_natural_sizes.push(0.0);
                }
            } else {
                // Row: measure natural width
                if let Some(layout_child) = Self::from_styled_node(child, content_x, content_y, content_width) {
                    let w = layout_child.total_width();
                    total_natural_size += w;
                    child_natural_sizes.push(w);
                    total_grow += child.styles().flex_grow();
                } else {
                    child_natural_sizes.push(0.0);
                }
            }
        }

        // Add gaps to total natural size
        if child_natural_sizes.len() > 1 {
            total_natural_size += gap * (child_natural_sizes.len() - 1) as f32;
        }

        // 2. Space distribution and final layout
        let extra_space = if is_column {
            // Usually flex columns have auto height, but if height is fixed:
            (styles.height_px().unwrap_or(total_natural_size) - total_natural_size).max(0.0)
        } else {
            (content_width - total_natural_size).max(0.0)
        };

        let mut layout_children = Vec::new();
        let mut cursor_main = if is_column { content_y } else { content_x };
        
        // Apply justify-content: flex-start (simple)
        // If center or space-between, we adjust start cursor or add spacing
        let justify = styles.justify_content();
        if justify == "center" && extra_space > 0.0 && total_grow == 0.0 {
            cursor_main += extra_space / 2.0;
        }

        for (index, child) in children.iter().enumerate() {
            let child_is_positioned = child.styles().position() == "absolute" || child.styles().position() == "fixed";
            let natural_size = child_natural_sizes[index];
            let grow = child.styles().flex_grow();
            let added_size = if total_grow > 0.0 {
                (grow / total_grow) * extra_space
            } else {
                0.0
            };
            let final_size = natural_size + added_size;

            if is_column {
                if let Some(mut layout_child) = Self::from_styled_node(child, content_x, cursor_main, content_width) {
                    if !child_is_positioned {
                        layout_child.rect.height = (final_size - layout_child.margin.vertical() - layout_child.border.vertical() - layout_child.padding.vertical()).max(0.0);
                        cursor_main += final_child_size(&layout_child, is_column) + gap;
                        
                        apply_align_items(&mut layout_child, styles.align_items(), content_x, content_width, false);
                    }
                    layout_children.push(layout_child);
                }
            } else {
                let child_available_width = if total_grow > 0.0 { final_size - child.styles().margin().horizontal() } else { content_width };
                if let Some(mut layout_child) = Self::from_styled_node(child, cursor_main, content_y, child_available_width) {
                    if !child_is_positioned {
                        layout_child.rect.width = (final_size - layout_child.margin.horizontal() - layout_child.border.horizontal() - layout_child.padding.horizontal()).max(0.0);
                        cursor_main += final_child_size(&layout_child, is_column) + gap;
                    }
                    layout_children.push(layout_child);
                }
            }
        }
        
        // Finalize height for row
        let total_content_height = if is_column {
            (cursor_main - content_y).max(0.0)
        } else {
            let h = layout_children.iter().filter(|c| c.styles().position() == "static").map(|c| c.total_height()).fold(0.0, f32::max);
            // Now we can align row items
            for child in &mut layout_children {
                if child.styles().position() == "static" {
                    apply_align_items(child, styles.align_items(), content_y, h, true);
                }
            }
            h
        };
        
        let resolved_content_height = clamp_content_height(&styles, total_content_height.max(INLINE_BOX_HEIGHT));

        Self {
            kind: LayoutKind::Flex {
                tag_name: tag_name.to_string(),
            },
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: rect_width,
                height: border.top + padding.top + resolved_content_height + padding.bottom + border.bottom,
            },
            z_index: styles.z_index(),
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_inline(
        tag_name: &str,
        styles: StyleMap,
        margin: EdgeSizes,
        border: EdgeSizes,
        padding: EdgeSizes,
        children: &[StyledNode],
        x: f32,
        y: f32,
        available_width: f32,
    ) -> Self {
        let rect_x = x + margin.left;
        let rect_y = y + margin.top;
        let available_rect_width = (available_width - margin.horizontal()).max(0.0);
        let default_content_width =
            (available_rect_width - padding.horizontal() - border.horizontal()).max(TEXT_CHAR_WIDTH);
        let content_width = clamp_content_width(&styles, default_content_width, default_content_width);
        let max_rect_width =
            (content_width + padding.horizontal() + border.horizontal()).min(available_rect_width);
        let content_x = rect_x + border.left + padding.left;
        let content_y = rect_y + border.top + padding.top;
        let mut layout_children = Vec::new();
        let mut line_x = content_x;
        let mut line_y = content_y;
        let mut line_height: f32 = 0.0;
        let mut max_line_width: f32 = 0.0;

        for child in children {
            if let Some(text) = child.text() {
                let fragments = Self::layout_text_fragments(
                    text,
                    child.styles().clone(),
                    content_x,
                    content_width,
                    &mut line_x,
                    &mut line_y,
                    &mut line_height,
                    &mut max_line_width,
                );
                layout_children.extend(fragments);
                continue;
            }

            let child_is_positioned = child.styles().position() == "absolute" || child.styles().position() == "fixed";

            let remaining_width = (content_width - (line_x - content_x)).max(TEXT_CHAR_WIDTH);
            if let Some(mut layout_child) =
                Self::from_styled_node(child, line_x, line_y, remaining_width)
            {
                if !child_is_positioned {
                    if line_x > content_x && layout_child.total_width() > remaining_width {
                        max_line_width = max_line_width.max(line_x - content_x);
                        line_y += line_height.max(TEXT_LINE_HEIGHT);
                        line_x = content_x;
                        line_height = 0.0;

                        if let Some(reflowed_child) =
                            Self::from_styled_node(child, line_x, line_y, content_width)
                        {
                            layout_child = reflowed_child;
                        }
                    }

                    line_x += layout_child.total_width();
                    line_height = line_height.max(layout_child.total_height());
                    max_line_width = max_line_width.max(line_x - content_x);
                    layout_children.push(layout_child);
                } else {
                    layout_children.push(layout_child);
                }
            }
        }

        let content_used_width = if layout_children.is_empty() {
            content_width.min(120.0)
        } else {
            max_line_width.max((line_x - content_x).min(content_width))
        };
        let total_content_height = if layout_children.is_empty() {
            INLINE_BOX_HEIGHT
        } else {
            (line_y - content_y) + line_height.max(INLINE_BOX_HEIGHT)
        };
        let resolved_content_height = clamp_content_height(&styles, total_content_height);

        Self {
            kind: LayoutKind::Inline {
                tag_name: tag_name.to_string(),
            },
            rect: Rect {
                x: rect_x,
                y: rect_y,
                width: (content_used_width + padding.horizontal() + border.horizontal()).min(max_rect_width),
                height: resolved_content_height + padding.vertical() + border.vertical(),
            },
            z_index: styles.z_index(),
            styles,
            margin,
            border,
            padding,
            children: layout_children,
        }
    }

    fn layout_text(text: &str, styles: StyleMap, x: f32, y: f32) -> Self {
        let char_width = char_width_from_styles(&styles);
        let line_height = line_height_from_styles(&styles);

        Self {
            kind: LayoutKind::Text {
                text: text.to_string(),
            },
            rect: Rect {
                x,
                y,
                width: text.chars().count() as f32 * char_width,
                height: line_height,
            },
            z_index: styles.z_index(),
            styles,
            margin: EdgeSizes::zero(),
            border: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            children: Vec::new(),
        }
    }

    fn layout_text_fragments(
        text: &str,
        styles: StyleMap,
        x: f32,
        available_width: f32,
        line_x: &mut f32,
        line_y: &mut f32,
        line_height: &mut f32,
        max_line_width: &mut f32,
    ) -> Vec<Self> {
        let mut fragments = Vec::new();
        let words = text.split_whitespace().collect::<Vec<_>>();

        if words.is_empty() {
            return fragments;
        }

        let char_width = char_width_from_styles(&styles);
        let base_line_height = line_height_from_styles(&styles);
        let mut current_line = String::new();

        for word in words {
            let candidate = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{current_line} {word}")
            };
            let candidate_width = candidate.chars().count() as f32 * char_width;
            let used_width = *line_x - x;
            let remaining_width = (available_width - used_width).max(char_width);

            if !current_line.is_empty() && candidate_width > remaining_width {
                let fragment = Self::layout_text(&current_line, styles.clone(), *line_x, *line_y);
                *line_x += fragment.rect.width;
                *line_height = (*line_height).max(fragment.rect.height);
                *max_line_width = (*max_line_width).max(*line_x - x);
                fragments.push(fragment);

                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
                current_line = word.to_string();
            } else if current_line.is_empty() && candidate_width > remaining_width && *line_x > x {
                *max_line_width = (*max_line_width).max(*line_x - x);
                *line_y += (*line_height).max(base_line_height);
                *line_x = x;
                *line_height = 0.0;
                current_line = word.to_string();
            } else {
                current_line = candidate;
            }
        }

        if !current_line.is_empty() {
            let fragment = Self::layout_text(&current_line, styles, *line_x, *line_y);
            *line_x += fragment.rect.width;
            *line_height = (*line_height).max(fragment.rect.height);
            *max_line_width = (*max_line_width).max(*line_x - x);
            fragments.push(fragment);
        }

        fragments
    }

    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match &self.kind {
            LayoutKind::Viewport => {
                writeln!(f, "{indent}viewport {}", self.rect)?;
            }
            LayoutKind::Block { tag_name } => {
                writeln!(
                    f,
                    "{indent}block<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Inline { tag_name } => {
                writeln!(
                    f,
                    "{indent}inline<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Flex { tag_name } => {
                writeln!(
                    f,
                    "{indent}flex<{tag_name}> {} {}",
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Image { alt, src, display_mode } => {
                let kind = if *display_mode == DisplayMode::Inline {
                    "inline"
                } else {
                    "block"
                };
                writeln!(
                    f,
                    "{indent}{kind}<img alt={:?} src={:?}> {} {}",
                    alt,
                    src,
                    format_styles(&self.styles),
                    self.rect
                )?;
            }
            LayoutKind::Text { text } => {
                writeln!(f, "{indent}text(\"{text}\") {}", self.rect)?;
            }
        }

        for child in &self.children {
            child.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn total_width(&self) -> f32 {
        self.margin.left + self.rect.width + self.margin.right
    }

    pub fn total_height(&self) -> f32 {
        self.margin.top + self.rect.height + self.margin.bottom
    }

    #[allow(dead_code)]
    pub fn padding(&self) -> EdgeSizes {
        self.padding
    }

    #[allow(dead_code)]
    pub fn content_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + self.border.left + self.padding.left,
            y: self.rect.y + self.border.top + self.padding.top,
            width: (self.rect.width - self.border.horizontal() - self.padding.horizontal()).max(0.0),
            height: (self.rect.height - self.border.vertical() - self.padding.vertical()).max(0.0),
        }
    }

    pub fn padding_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + self.border.left,
            y: self.rect.y + self.border.top,
            width: (self.rect.width - self.border.horizontal()).max(0.0),
            height: (self.rect.height - self.border.vertical()).max(0.0),
        }
    }

    pub fn styles(&self) -> &StyleMap {
        &self.styles
    }

    pub fn children(&self) -> &[LayoutBox] {
        &self.children
    }

    pub fn z_index(&self) -> i32 {
        self.z_index
    }

    pub fn tag_name(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Block { tag_name } | LayoutKind::Inline { tag_name } | LayoutKind::Flex { tag_name } => Some(tag_name),
            LayoutKind::Image { .. } => Some("img"),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Text { text } => Some(text),
            _ => None,
        }
    }

    pub fn is_viewport(&self) -> bool {
        matches!(self.kind, LayoutKind::Viewport)
    }

    pub fn image_alt(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Image { alt, .. } => alt.as_deref(),
            _ => None,
        }
    }

    pub fn image_src(&self) -> Option<&str> {
        match &self.kind {
            LayoutKind::Image { src, .. } => src.as_deref(),
            _ => None,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, LayoutKind::Image { .. })
    }

    pub fn layout_positioned_elements(
        &mut self,
        containing_block_rect: Rect,
        viewport_rect: Rect,
    ) {
        let is_positioned_ancestor = self.styles.position() != "static" || self.is_viewport();
        let next_containing_block = if is_positioned_ancestor {
            self.padding_rect()
        } else {
            containing_block_rect
        };

        for child in &mut self.children {
            let position = child.styles.position();
            if position == "absolute" || position == "fixed" {
                let cb = if position == "fixed" { viewport_rect } else { next_containing_block };
                
                if let Some(top) = child.styles.top_px() {
                    child.rect.y = cb.y + top;
                } else if let Some(bottom) = child.styles.bottom_px() {
                    child.rect.y = (cb.y + cb.height - child.rect.height - bottom).max(cb.y);
                }

                if let Some(left) = child.styles.left_px() {
                    child.rect.x = cb.x + left;
                } else if let Some(right) = child.styles.right_px() {
                    child.rect.x = (cb.x + cb.width - child.rect.width - right).max(cb.x);
                }
            }
            
            child.layout_positioned_elements(next_containing_block, viewport_rect);
        }
    }
}

fn char_width_from_styles(styles: &StyleMap) -> f32 {
    // Default font-size assumed 16px → char_width = font_size / 2.0
    let base_width = styles.font_size_px().map(|s| s / 2.0).unwrap_or(TEXT_CHAR_WIDTH);

    // Apply font-weight multiplier
    if styles.font_weight() == "bold" || styles.font_weight() == "700" {
        base_width * 1.1
    } else {
        base_width
    }
}

fn final_child_size(layout_box: &LayoutBox, is_column: bool) -> f32 {
    if is_column {
        layout_box.total_height()
    } else {
        layout_box.total_width()
    }
}

fn apply_align_items(
    layout_box: &mut LayoutBox,
    align: &str,
    container_start: f32,
    container_size: f32,
    is_vertical: bool,
) {
    let child_size = if is_vertical {
        layout_box.total_height()
    } else {
        layout_box.total_width()
    };
    let extra_space = container_size - child_size;

    if is_vertical {
        match align {
            "center" => {
                layout_box.rect.y = container_start + extra_space / 2.0;
            }
            "flex-end" | "end" => {
                layout_box.rect.y = container_start + extra_space;
            }
            "stretch" => {
                layout_box.rect.height = (container_size - layout_box.margin.vertical() - layout_box.border.vertical() - layout_box.padding.vertical()).max(0.0);
            }
            _ => { /* flex-start */ }
        }
    } else {
        match align {
            "center" => {
                layout_box.rect.x = container_start + extra_space / 2.0;
            }
            "flex-end" | "end" => {
                layout_box.rect.x = container_start + extra_space;
            }
            "stretch" => {
                layout_box.rect.width = (container_size - layout_box.margin.horizontal() - layout_box.border.horizontal() - layout_box.padding.horizontal()).max(0.0);
            }
            _ => { /* flex-start */ }
        }
    }
}

fn line_height_from_styles(styles: &StyleMap) -> f32 {
    styles.line_height_px()
        .or_else(|| styles.font_size_px().map(|s| s * 1.2))
        .unwrap_or(TEXT_LINE_HEIGHT)
}

impl Display for LayoutTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.root.fmt_with_indent(f, 0)
    }
}

impl Display for Rect {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[x: {:.0}, y: {:.0}, w: {:.0}, h: {:.0}]",
            self.x, self.y, self.width, self.height
        )
    }
}

fn format_styles(styles: &StyleMap) -> String {
    if styles.is_empty() {
        "{}".to_string()
    } else {
        format!("{styles}")
    }
}

fn clamp_content_width(styles: &StyleMap, candidate_width: f32, available_width: f32) -> f32 {
    let mut width = styles.width_px().unwrap_or(candidate_width);

    // If border-box, the specified width includes padding and border
    if styles.box_sizing() == "border-box" {
        let padding = styles.padding();
        let border = styles.border_width();
        width = (width - padding.horizontal() - border.horizontal()).max(0.0);
    }

    if let Some(min_width) = styles.min_width_px() {
        let mut min = min_width;
        if styles.box_sizing() == "border-box" {
            let padding = styles.padding();
            let border = styles.border_width();
            min = (min - padding.horizontal() - border.horizontal()).max(0.0);
        }
        width = width.max(min);
    }
    if let Some(max_width) = styles.max_width_px() {
        let mut max = max_width;
        if styles.box_sizing() == "border-box" {
            let padding = styles.padding();
            let border = styles.border_width();
            max = (max - padding.horizontal() - border.horizontal()).max(0.0);
        }
        width = width.min(max);
    }
    width.min(available_width).max(0.0)
}

fn clamp_content_height(styles: &StyleMap, candidate_height: f32) -> f32 {
    let mut height = styles.height_px().unwrap_or(candidate_height);

    if styles.box_sizing() == "border-box" {
        let padding = styles.padding();
        let border = styles.border_width();
        height = (height - padding.vertical() - border.vertical()).max(0.0);
    }

    if let Some(min_height) = styles.min_height_px() {
        let mut min = min_height;
        if styles.box_sizing() == "border-box" {
            let padding = styles.padding();
            let border = styles.border_width();
            min = (min - padding.vertical() - border.vertical()).max(0.0);
        }
        height = height.max(min);
    }
    if let Some(max_height) = styles.max_height_px() {
        let mut max = max_height;
        if styles.box_sizing() == "border-box" {
            let padding = styles.padding();
            let border = styles.border_width();
            max = (max - padding.vertical() - border.vertical()).max(0.0);
        }
        height = height.min(max);
    }
    height.max(0.0)
}

fn parse_html_length_px(value: &str) -> Option<f32> {
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .parse::<f32>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::LayoutTree;
    use crate::css::Stylesheet;
    use crate::dom::Node;
    use crate::style::StyleTree;

    #[test]
    fn builds_layout_boxes_with_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hello")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; color: blue; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert!(rendered.contains("viewport [x: 0, y: 0, w: 800"));
        assert!(rendered.contains("block<body> {} [x: 0, y: 0, w: 800"));
        assert!(rendered.contains("inline<p> {color: blue, display: inline}"));
        assert!(rendered.contains("text(\"Hello\") [x: 0, y: 0, w: 35, h: 18]"));
    }

    #[test]
    fn stacks_block_children_vertically() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("One")]),
                Node::element("section", vec![Node::text("Two")]),
            ],
        )]);
        let stylesheet = Stylesheet::parse("");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {} [x: 0, y: 0, w: 800, h: 26]"));
        assert!(rendered.contains("block<section> {} [x: 0, y: 26, w: 800, h: 26]"));
    }

    #[test]
    fn wraps_inline_text_across_multiple_lines() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("alpha beta gamma delta epsilon zeta")],
            )],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 96.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("inline<p> {display: inline}"));
        assert!(rendered.contains("text(\"alpha beta\") [x: 0, y: 0, w: 70, h: 18]"));
        assert!(rendered.contains("text(\"gamma delta\") [x: 0, y: 18, w: 77, h: 18]"));
        assert!(rendered.contains("text(\"epsilon zeta\") [x: 0, y: 36, w: 84, h: 18]"));
    }

    #[test]
    fn wraps_inline_children_when_the_row_fills() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "span",
                vec![
                    Node::element("em", vec![Node::text("hello")]),
                    Node::element("strong", vec![Node::text("world")]),
                ],
            )],
        )]);
        let stylesheet =
            Stylesheet::parse("span { display: inline; } em { display: inline; } strong { display: inline; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 72.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("inline<em> {display: inline} [x: 0, y: 0, w: 35, h: 20]"));
        assert!(rendered.contains("inline<strong> {display: inline}"));
    }

    #[test]
    fn applies_margin_and_padding_to_block_layout() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Box")])],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { margin: 10px 12px; padding: 4px 6px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {margin: 10px 12px, padding: 4px 6px} [x: 12, y: 10, w: 176, h: 34]"));
        assert!(rendered.contains("text(\"Box\") [x: 18, y: 14, w: 21, h: 18]"));
    }

    #[test]
    fn includes_border_width_in_box_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Border")])],
        )]);
        let stylesheet =
            Stylesheet::parse("section { border: 4px solid ember; padding: 6px; width: 80px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 220.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {border: 4px solid ember, padding: 6px, width: 80px} [x: 0, y: 0, w: 100, h: 46]"));
        assert!(rendered.contains("text(\"Border\") [x: 10, y: 10, w: 42, h: 18]"));
    }

    #[test]
    fn applies_fixed_width_and_height_to_block_boxes() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Sized")])],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { width: 120px; height: 48px; padding: 4px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 300.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {height: 48px, padding: 4px, width: 120px} [x: 0, y: 0, w: 128, h: 56]"));
        assert!(rendered.contains("text(\"Sized\") [x: 4, y: 4, w: 35, h: 18]"));
    }

    #[test]
    fn constrains_inline_wrapping_with_fixed_width() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("one two three four five")],
            )],
        )]);
        let stylesheet = Stylesheet::parse("p { display: inline; width: 64px; padding: 4px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("inline<p> {display: inline, padding: 4px, width: 64px}"));
        assert!(rendered.contains("text(\"one two\") [x: 4, y: 4, w: 49, h: 18]"));
        assert!(rendered.contains("text(\"three\")"));
        assert!(rendered.contains("text(\"four five\")"));
    }

    #[test]
    fn clamps_block_width_and_height_with_min_and_max() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("Min")]),
                Node::element("article", vec![Node::text("Max")]),
            ],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { width: 40px; min-width: 80px; height: 12px; min-height: 24px; padding: 4px; } article { width: 180px; max-width: 96px; height: 120px; max-height: 40px; padding: 4px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {height: 12px, min-height: 24px, min-width: 80px, padding: 4px, width: 40px} [x: 0, y: 0, w: 88, h: 32]"));
        assert!(rendered.contains("block<article> {height: 120px, max-height: 40px, max-width: 96px, padding: 4px, width: 180px} [x: 0, y: 32, w: 104, h: 48]"));
    }

    #[test]
    fn collapses_vertical_margins_between_block_siblings() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![
                Node::element("section", vec![Node::text("One")]),
                Node::element("section", vec![Node::text("Two")]),
            ],
        )]);
        let stylesheet =
            Stylesheet::parse("section { margin-top: 12px; margin-bottom: 18px; padding: 4px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 12, w: 240, h: 34]"));
        assert!(rendered.contains("block<section> {margin-bottom: 18px, margin-top: 12px, padding: 4px} [x: 0, y: 64, w: 240, h: 34]"));
    }

    #[test]
    fn clamps_inline_width_before_wrapping() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "p",
                vec![Node::text("one two three four five")],
            )],
        )]);
        let stylesheet = Stylesheet::parse(
            "p { display: inline; width: 140px; max-width: 64px; min-height: 60px; padding: 4px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("inline<p> {display: inline, max-width: 64px, min-height: 60px, padding: 4px, width: 140px}"));
        assert!(rendered.contains("text(\"three\")"));
        assert!(rendered.contains("text(\"four five\")"));
    }

    #[test]
    fn omits_nodes_with_display_none() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("p", vec![Node::text("Hidden")])],
        )]);
        let stylesheet = Stylesheet::parse("p { display: none; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree(&style_tree);
        let rendered = layout.to_string();

        assert!(!rendered.contains("<p>"));
        assert!(!rendered.contains("Hidden"));
    }

    #[test]
    fn includes_border_width_in_inline_box_geometry() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("span", vec![Node::text("Hi")])],
        )]);
        let stylesheet =
            Stylesheet::parse("span { display: inline; border: 4px solid ember; padding: 2px; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();

        assert!(rendered.contains(
            "inline<span> {border: 4px solid ember, display: inline, padding: 2px} [x: 0, y: 0, w: 26, h: 32]"
        ));
        assert!(rendered.contains("text(\"Hi\") [x: 6, y: 6, w: 14, h: 18]"));
    }

    #[test]
    fn lays_out_images_with_attributes_as_replaced_boxes() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element_with_attributes(
                "img",
                [
                    ("alt".to_string(), "grumpy cat".to_string()),
                    ("src".to_string(), "cat.txt".to_string()),
                    ("width".to_string(), "120".to_string()),
                    ("height".to_string(), "80".to_string()),
                ]
                .into_iter()
                .collect(),
                Vec::new(),
            )],
        )]);
        let stylesheet =
            Stylesheet::parse("img { display: inline; padding: 4px; border: 2px solid ember; }");
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);

        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 240.0);
        let rendered = layout.to_string();

        assert!(rendered.contains(
            "inline<img alt=Some(\"grumpy cat\") src=Some(\"cat.txt\")> {border: 2px solid ember, display: inline, padding: 4px} [x: 0, y: 0, w: 132, h: 92]"
        ));
    }

    #[test]
    fn applies_border_box_sizing_correctly() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element("section", vec![Node::text("Sized")])],
        )]);
        let stylesheet = Stylesheet::parse(
            "section { box-sizing: border-box; width: 100px; height: 50px; padding: 10px; border: 5px solid ember; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();

        assert!(rendered.contains("block<section>"));
        assert!(rendered.contains("box-sizing: border-box"));
        assert!(rendered.contains("width: 100px"));
        assert!(rendered.contains("[x: 0, y: 0, w: 100, h: 50]"));
        assert!(rendered.contains("text(\"Sized\") [x: 15, y: 15, w: 35, h: 18]"));
    }

    #[test]
    fn positions_absolute_elements_relative_to_nearest_positioned_ancestor() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "div",
                vec![Node::element("span", vec![Node::text("Abs")])],
            )],
        )]);
        let stylesheet = Stylesheet::parse(
            "div { position: relative; margin: 20px; width: 100px; height: 100px; } span { position: absolute; top: 10px; left: 10px; width: 30px; height: 30px; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();
        println!("DEBUG POSITIONED:\n{}", rendered);

        assert!(rendered.contains("block<div"));
        assert!(rendered.contains("position: relative"));
        assert!(rendered.contains("[x: 20, y: 20, w: 100, h: 100]"));
        assert!(rendered.contains("inline<span>"));
        assert!(rendered.contains("position: absolute"));
        assert!(rendered.contains("[x: 30, y: 30, w: 30, h: 30]"));
    }

    #[test]
    fn lays_out_flex_row_with_gap_and_growth() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "div",
                vec![
                    Node::element("span", vec![Node::text("A")]),
                    Node::element("span", vec![Node::text("B")]),
                ],
            )],
        )]);
        let stylesheet = Stylesheet::parse(
            "div { display: flex; flex-direction: row; gap: 10px; width: 100px; } span { flex-grow: 1; }",
        );
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport_width(&style_tree, 200.0);
        let rendered = layout.to_string();
        println!("DEBUG FLEX:\n{}", rendered);

        assert!(rendered.contains("flex<div"));
        assert!(rendered.contains("display: flex"));
        assert!(rendered.contains("flex-direction: row"));
        assert!(rendered.contains("gap: 10px"));
        assert!(rendered.contains("[x: 0, y: 0, w: 100"));
        
        assert!(rendered.contains("inline<span> {flex-grow: 1} [x: 0, y: 0, w: 45"));
        assert!(rendered.contains("inline<span> {flex-grow: 1} [x: 55, y: 0, w: 45"));
    }
}
