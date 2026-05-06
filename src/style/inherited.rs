#[derive(Default, Clone)]
pub(super) struct InheritedStyles {
    pub(super) color: Option<String>,
    pub(super) font_size: Option<String>,
    pub(super) font_weight: Option<String>,
    pub(super) line_height: Option<String>,
    pub(super) visibility: Option<String>,
    pub(super) text_decoration: Option<String>,
    pub(super) font_style: Option<String>,
    pub(super) white_space: Option<String>,
}
