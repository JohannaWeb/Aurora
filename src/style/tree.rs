use crate::css::Stylesheet;

use super::inherited::InheritedStyles;
use super::StyledNode;

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
            ),
        }
    }

    pub fn root(&self) -> &StyledNode {
        &self.root
    }
}
