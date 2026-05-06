use crate::css::JustifyContent;

pub(super) fn justify_start_and_spacing(
    justify: JustifyContent,
    origin: f32,
    free_space: f32,
    gap: f32,
    item_count: usize,
) -> (f32, f32) {
    match justify {
        JustifyContent::FlexEnd => (origin + free_space, gap),
        JustifyContent::Center => (origin + free_space / 2.0, gap),
        JustifyContent::SpaceBetween => {
            let spacing = if item_count > 1 {
                free_space / (item_count as f32 - 1.0)
            } else {
                0.0
            };
            (origin, spacing)
        }
        JustifyContent::SpaceAround => {
            let spacing = if item_count > 0 {
                free_space / item_count as f32
            } else {
                0.0
            };
            (origin + spacing / 2.0, spacing)
        }
        _ => (origin, gap),
    }
}
