use crate::css::Stylesheet;

use super::StyledNode;
use super::inherited::InheritedStyles;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleTree {
    root: StyledNode,
}

impl StyleTree {
    pub fn from_dom(document: &crate::dom::NodePtr, stylesheet: &Stylesheet) -> Self {
        Self {
            root: StyledNode::from_dom_node(
                std::rc::Rc::clone(document),
                stylesheet,
                &[],
                InheritedStyles::default(),
                &[],
                &[],
                0,
            ),
        }
    }

    pub fn root(&self) -> &StyledNode {
        &self.root
    }
}
