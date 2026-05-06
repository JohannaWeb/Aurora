use crate::css::{StyleMap, Stylesheet};
use crate::dom::Node;

use super::inherited::InheritedStyles;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledNode {
    pub node: crate::dom::NodePtr,
    pub styles: StyleMap,
    pub children: Vec<StyledNode>,
}

impl StyledNode {
    pub fn styles(&self) -> &StyleMap {
        &self.styles
    }

    pub fn children(&self) -> &[StyledNode] {
        &self.children
    }

    pub fn tag_name(&self) -> Option<String> {
        let node = self.node.borrow();
        if let Node::Element(el) = &*node {
            Some(el.tag_name.clone())
        } else {
            None
        }
    }

    pub fn text(&self) -> Option<String> {
        let node = self.node.borrow();
        if let Node::Text(text) = &*node {
            Some(text.clone())
        } else {
            None
        }
    }

    pub fn attribute(&self, name: &str) -> Option<String> {
        match &*self.node.borrow() {
            Node::Element(el) => el.attributes.get(name).cloned(),
            _ => None,
        }
    }

    pub(super) fn from_dom_node(
        node: crate::dom::NodePtr,
        stylesheet: &Stylesheet,
        element_ancestors: &[crate::css::ElementData],
        inherited: InheritedStyles,
        style_ancestors: &[&StyleMap],
    ) -> Self {
        let node_borrow = node.borrow();
        match &*node_borrow {
            Node::Document { children } => {
                let children_vec = children.clone();
                drop(node_borrow);
                Self {
                    node,
                    styles: StyleMap::default(),
                    children: children_vec
                        .into_iter()
                        .map(|child| {
                            Self::from_dom_node(
                                child,
                                stylesheet,
                                element_ancestors,
                                inherited.clone(),
                                style_ancestors,
                            )
                        })
                        .collect(),
                }
            }
            Node::Element(element) => {
                let current_data = crate::css::ElementData {
                    tag_name: element.tag_name.clone(),
                    attributes: element.attributes.clone(),
                };
                let mut styles = stylesheet.styles_for(&current_data, element_ancestors);
                styles.resolve_vars(style_ancestors);
                apply_inherited_element_styles(&mut styles, &inherited);

                let mut next_element_ancestors = element_ancestors.to_vec();
                next_element_ancestors.push(current_data);
                let next_inherited = inherited_from_styles(&styles);
                let element_children = element.children.clone();
                drop(node_borrow);

                let mut node_to_return = Self {
                    node,
                    styles,
                    children: Vec::new(),
                };
                let mut next_style_ancestors = style_ancestors.to_vec();
                next_style_ancestors.push(&node_to_return.styles);
                node_to_return.children = element_children
                    .into_iter()
                    .map(|child| {
                        Self::from_dom_node(
                            child,
                            stylesheet,
                            &next_element_ancestors,
                            next_inherited.clone(),
                            &next_style_ancestors,
                        )
                    })
                    .collect();

                node_to_return
            }
            Node::Text(_) => {
                let mut styles = StyleMap::default();
                styles.set("display", "inline");
                apply_inherited_text_styles(&mut styles, &inherited);
                Self {
                    node: node.clone(),
                    styles,
                    children: Vec::new(),
                }
            }
        }
    }
}

fn apply_if_missing(styles: &mut StyleMap, property: &str, value: &Option<String>) {
    if styles.get(property).is_none() {
        if let Some(value) = value {
            styles.set(property, value);
        }
    }
}

fn apply_inherited_element_styles(styles: &mut StyleMap, inherited: &InheritedStyles) {
    apply_if_missing(styles, "color", &inherited.color);
    apply_if_missing(styles, "font-size", &inherited.font_size);
    apply_if_missing(styles, "font-weight", &inherited.font_weight);
    apply_if_missing(styles, "line-height", &inherited.line_height);
    apply_if_missing(styles, "visibility", &inherited.visibility);
    apply_if_missing(styles, "text-decoration", &inherited.text_decoration);
    apply_if_missing(styles, "white-space", &inherited.white_space);
}

fn apply_inherited_text_styles(styles: &mut StyleMap, inherited: &InheritedStyles) {
    if let Some(color) = &inherited.color {
        styles.set("color", color);
    }
    if let Some(font_size) = &inherited.font_size {
        styles.set("font-size", font_size);
    }
    if let Some(font_weight) = &inherited.font_weight {
        styles.set("font-weight", font_weight);
    }
    if let Some(line_height) = &inherited.line_height {
        styles.set("line-height", line_height);
    }
    if let Some(visibility) = &inherited.visibility {
        styles.set("visibility", visibility);
    }
    if let Some(text_decoration) = &inherited.text_decoration {
        styles.set("text-decoration", text_decoration);
    }
    if let Some(font_style) = &inherited.font_style {
        styles.set("font-style", font_style);
    }
    if let Some(white_space) = &inherited.white_space {
        styles.set("white-space", white_space);
    }
}

fn inherited_from_styles(styles: &StyleMap) -> InheritedStyles {
    InheritedStyles {
        color: styles.get("color").map(ToOwned::to_owned),
        font_size: styles.get("font-size").map(ToOwned::to_owned),
        font_weight: styles.get("font-weight").map(ToOwned::to_owned),
        line_height: styles.get("line-height").map(ToOwned::to_owned),
        visibility: styles.get("visibility").map(ToOwned::to_owned),
        text_decoration: styles.get("text-decoration").map(ToOwned::to_owned),
        font_style: styles.get("font-style").map(ToOwned::to_owned),
        white_space: styles.get("white-space").map(ToOwned::to_owned),
    }
}
