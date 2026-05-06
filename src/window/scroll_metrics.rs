pub(super) fn scroll_content_height(layout_box: &crate::layout::LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    (bottom - rect.y).max(rect.height)
}

fn max_box_bottom(layout_box: &crate::layout::LayoutBox) -> f32 {
    let rect = layout_box.rect();
    let mut bottom = rect.y + rect.height;
    for child in layout_box.children() {
        bottom = bottom.max(max_box_bottom(child));
    }
    bottom
}
