use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};
use crate::ImageCache;
use opus::domain::Identity;

pub struct WindowInput {
    pub dom: NodePtr,
    pub stylesheet: Stylesheet,
    pub base_url: Option<String>,
    pub identity: Identity,
    pub viewport: ViewportSize,
    pub layout: LayoutTree,
    pub images: ImageCache,
}
