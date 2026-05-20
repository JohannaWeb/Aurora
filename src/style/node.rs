use crate::css::{ElementData, StyleMap, Stylesheet};
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
        element_ancestors: &[ElementData],
        inherited: InheritedStyles,
        style_ancestors: &[&StyleMap],
        siblings: &[ElementData],
        sibling_index: usize,
    ) -> Self {
        let node_borrow = node.borrow();
        match &*node_borrow {
            Node::Document { children, .. } => {
                let children_vec = children.clone();
                drop(node_borrow);
                // Collect element-only siblings for document children.
                let doc_elem_siblings = element_siblings_of(&children_vec);
                let mut elem_idx = 0usize;
                Self {
                    node,
                    styles: StyleMap::default(),
                    children: children_vec
                        .into_iter()
                        .map(|child| {
                            let is_elem = matches!(&*child.borrow(), Node::Element(_));
                            let idx = if is_elem { let i = elem_idx; elem_idx += 1; i } else { 0 };
                            Self::from_dom_node(
                                child,
                                stylesheet,
                                element_ancestors,
                                inherited.clone(),
                                style_ancestors,
                                &doc_elem_siblings,
                                idx,
                            )
                        })
                        .collect(),
                }
            }

            Node::Element(element) => {
                let current_data = ElementData {
                    tag_name: element.tag_name.clone(),
                    attributes: element.attributes.clone(),
                };
                let mut styles =
                    stylesheet.styles_for(&current_data, element_ancestors, siblings, sibling_index);
                if let Some(inline_style) = element.attributes.get("style") {
                    for (name, value) in crate::css::parse_style_text(inline_style) {
                        styles.set(name, value);
                    }
                }
                styles.resolve_vars(style_ancestors);
                apply_inherited_element_styles(&mut styles, &inherited);

                let mut next_element_ancestors = element_ancestors.to_vec();
                next_element_ancestors.push(current_data);
                let next_inherited = inherited_from_styles(&styles);
                let element_children = element.children.clone();
                drop(node_borrow);

                let mut node_to_return = Self { node, styles, children: Vec::new() };
                let mut next_style_ancestors = style_ancestors.to_vec();
                next_style_ancestors.push(&node_to_return.styles);

                // Collect element-only siblings for this element's children.
                let child_elem_siblings = element_siblings_of(&element_children);
                let mut elem_child_idx = 0usize;

                node_to_return.children = element_children
                    .into_iter()
                    .map(|child| {
                        let is_elem = matches!(&*child.borrow(), Node::Element(_));
                        let idx =
                            if is_elem { let i = elem_child_idx; elem_child_idx += 1; i } else { 0 };
                        Self::from_dom_node(
                            child,
                            stylesheet,
                            &next_element_ancestors,
                            next_inherited.clone(),
                            &next_style_ancestors,
                            &child_elem_siblings,
                            idx,
                        )
                    })
                    .collect();

                node_to_return
            }

            Node::Text(_) => {
                let mut styles = StyleMap::default();
                styles.set("display", "inline");
                apply_inherited_text_styles(&mut styles, &inherited);
                Self { node: node.clone(), styles, children: Vec::new() }
            }
        }
    }
}

/// Collect `ElementData` for every Element child (ignoring Text nodes).
fn element_siblings_of(children: &[crate::dom::NodePtr]) -> Vec<ElementData> {
    children
        .iter()
        .filter_map(|child| {
            if let Node::Element(el) = &*child.borrow() {
                Some(ElementData { tag_name: el.tag_name.clone(), attributes: el.attributes.clone() })
            } else {
                None
            }
        })
        .collect()
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
    apply_if_missing(styles, "font-style", &inherited.font_style);
    apply_if_missing(styles, "font-family", &inherited.font_family);
    apply_if_missing(styles, "line-height", &inherited.line_height);
    apply_if_missing(styles, "text-align", &inherited.text_align);
    apply_if_missing(styles, "text-decoration", &inherited.text_decoration);
    apply_if_missing(styles, "visibility", &inherited.visibility);
    apply_if_missing(styles, "white-space", &inherited.white_space);
    apply_if_missing(styles, "cursor", &inherited.cursor);
    apply_if_missing(styles, "direction", &inherited.direction);
    apply_if_missing(styles, "letter-spacing", &inherited.letter_spacing);
    apply_if_missing(styles, "word-spacing", &inherited.word_spacing);
    apply_if_missing(styles, "text-transform", &inherited.text_transform);
    apply_if_missing(styles, "text-indent", &inherited.text_indent);
    apply_if_missing(styles, "list-style-type", &inherited.list_style_type);
    apply_if_missing(styles, "list-style-position", &inherited.list_style_position);
    apply_if_missing(styles, "border-collapse", &inherited.border_collapse);
    apply_if_missing(styles, "border-spacing", &inherited.border_spacing);
    apply_if_missing(styles, "caption-side", &inherited.caption_side);
    apply_if_missing(styles, "empty-cells", &inherited.empty_cells);
    apply_if_missing(styles, "quotes", &inherited.quotes);
}

fn apply_inherited_text_styles(styles: &mut StyleMap, inherited: &InheritedStyles) {
    if let Some(v) = &inherited.color { styles.set("color", v); }
    if let Some(v) = &inherited.font_size { styles.set("font-size", v); }
    if let Some(v) = &inherited.font_weight { styles.set("font-weight", v); }
    if let Some(v) = &inherited.font_style { styles.set("font-style", v); }
    if let Some(v) = &inherited.font_family { styles.set("font-family", v); }
    if let Some(v) = &inherited.line_height { styles.set("line-height", v); }
    if let Some(v) = &inherited.text_align { styles.set("text-align", v); }
    if let Some(v) = &inherited.text_decoration { styles.set("text-decoration", v); }
    if let Some(v) = &inherited.visibility { styles.set("visibility", v); }
    if let Some(v) = &inherited.white_space { styles.set("white-space", v); }
}

fn inherited_from_styles(styles: &StyleMap) -> InheritedStyles {
    InheritedStyles {
        color: styles.get("color").map(ToOwned::to_owned),
        font_size: styles.get("font-size").map(ToOwned::to_owned),
        font_weight: styles.get("font-weight").map(ToOwned::to_owned),
        font_style: styles.get("font-style").map(ToOwned::to_owned),
        font_family: styles.get("font-family").map(ToOwned::to_owned),
        line_height: styles.get("line-height").map(ToOwned::to_owned),
        text_align: styles.get("text-align").map(ToOwned::to_owned),
        text_decoration: styles.get("text-decoration").map(ToOwned::to_owned),
        visibility: styles.get("visibility").map(ToOwned::to_owned),
        white_space: styles.get("white-space").map(ToOwned::to_owned),
        cursor: styles.get("cursor").map(ToOwned::to_owned),
        direction: styles.get("direction").map(ToOwned::to_owned),
        letter_spacing: styles.get("letter-spacing").map(ToOwned::to_owned),
        word_spacing: styles.get("word-spacing").map(ToOwned::to_owned),
        text_transform: styles.get("text-transform").map(ToOwned::to_owned),
        text_indent: styles.get("text-indent").map(ToOwned::to_owned),
        list_style_type: styles.get("list-style-type").map(ToOwned::to_owned),
        list_style_position: styles.get("list-style-position").map(ToOwned::to_owned),
        border_collapse: styles.get("border-collapse").map(ToOwned::to_owned),
        border_spacing: styles.get("border-spacing").map(ToOwned::to_owned),
        caption_side: styles.get("caption-side").map(ToOwned::to_owned),
        empty_cells: styles.get("empty-cells").map(ToOwned::to_owned),
        quotes: styles.get("quotes").map(ToOwned::to_owned),
    }
}
