use crate::dom::{ElementNode, Node};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stylesheet {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    selector: Selector,
    declarations: Vec<Declaration>,
    source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Selector {
    parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleSelector {
    tag_name: Option<String>,
    id: Option<String>,
    class_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Declaration {
    name: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleMap(BTreeMap<String, String>);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Block,
    Inline,
    None,
}

impl Stylesheet {
    pub fn parse(source: &str) -> Self {
        let mut rules = Vec::new();

        for (source_order, chunk) in source.split('}').enumerate() {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }

            let Some((selector_part, declarations_part)) = chunk.split_once('{') else {
                continue;
            };

            let selector = selector_part.trim();
            if selector.is_empty() || selector.contains(',') {
                continue;
            }

            let Some(selector) = Selector::parse(selector) else {
                continue;
            };

            let declarations = declarations_part
                .split(';')
                .filter_map(|declaration| {
                    let declaration = declaration.trim();
                    if declaration.is_empty() {
                        return None;
                    }

                    let (name, value) = declaration.split_once(':')?;
                    Some(Declaration {
                        name: name.trim().to_string(),
                        value: value.trim().to_string(),
                    })
                })
                .collect::<Vec<_>>();

            if declarations.is_empty() {
                continue;
            }

            rules.push(Rule {
                selector,
                declarations,
                source_order,
            });
        }

        Self { rules }
    }

    pub fn from_dom(document: &Node) -> Self {
        let mut source = String::new();
        collect_style_text(document, &mut source);
        Self::parse(&source)
    }

    pub fn styles_for(&self, element: &ElementNode, ancestors: &[&ElementNode]) -> StyleMap {
        let mut styles = StyleMap::default();
        let mut matching_rules = self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, ancestors))
            .collect::<Vec<_>>();

        matching_rules.sort_by_key(|rule| (rule.selector.specificity(), rule.source_order));

        for rule in matching_rules {
            for declaration in &rule.declarations {
                styles
                    .0
                    .insert(declaration.name.clone(), declaration.value.clone());
            }
        }

        styles
    }
}

impl StyleMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn display_mode(&self) -> DisplayMode {
        match self.0.get("display").map(String::as_str) {
            Some("inline") => DisplayMode::Inline,
            Some("none") => DisplayMode::None,
            _ => DisplayMode::Block,
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.insert(name.into(), value.into());
    }

    pub fn margin(&self) -> EdgeSizes {
        self.edge_sizes("margin")
    }

    pub fn padding(&self) -> EdgeSizes {
        self.edge_sizes("padding")
    }

    pub fn border_width(&self) -> EdgeSizes {
        let mut edges = parse_box_shorthand(self.get("border-width"));
        if edges == EdgeSizes::zero() {
            edges = parse_border_width_shorthand(self.get("border"));
        }
        edges.top = self.length_or("border-top-width", edges.top);
        edges.right = self.length_or("border-right-width", edges.right);
        edges.bottom = self.length_or("border-bottom-width", edges.bottom);
        edges.left = self.length_or("border-left-width", edges.left);
        edges
    }

    pub fn background_color(&self) -> Option<&str> {
        self.get("background-color").or_else(|| self.get("background"))
    }

    pub fn border_color(&self) -> Option<&str> {
        self.get("border-color")
            .or_else(|| parse_border_color_shorthand(self.get("border")))
    }

    pub fn width_px(&self) -> Option<f32> {
        self.get("width").and_then(parse_length_px)
    }

    pub fn height_px(&self) -> Option<f32> {
        self.get("height").and_then(parse_length_px)
    }

    pub fn min_width_px(&self) -> Option<f32> {
        self.get("min-width").and_then(parse_length_px)
    }

    pub fn max_width_px(&self) -> Option<f32> {
        self.get("max-width").and_then(parse_length_px)
    }

    pub fn min_height_px(&self) -> Option<f32> {
        self.get("min-height").and_then(parse_length_px)
    }

    pub fn max_height_px(&self) -> Option<f32> {
        self.get("max-height").and_then(parse_length_px)
    }

    pub fn font_size_px(&self) -> Option<f32> {
        self.get("font-size").and_then(parse_length_px)
    }

    pub fn font_weight(&self) -> &str {
        self.get("font-weight").unwrap_or("normal")
    }

    pub fn line_height_px(&self) -> Option<f32> {
        self.get("line-height").and_then(parse_length_px)
    }

    pub fn text_decoration(&self) -> Option<&str> {
        self.get("text-decoration")
    }

    pub fn opacity(&self) -> f32 {
        self.get("opacity")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn visibility(&self) -> &str {
        self.get("visibility").unwrap_or("visible")
    }

    fn edge_sizes(&self, prefix: &str) -> EdgeSizes {
        let mut edges = parse_box_shorthand(self.get(prefix));
        edges.top = self.length_or(format!("{prefix}-top").as_str(), edges.top);
        edges.right = self.length_or(format!("{prefix}-right").as_str(), edges.right);
        edges.bottom = self.length_or(format!("{prefix}-bottom").as_str(), edges.bottom);
        edges.left = self.length_or(format!("{prefix}-left").as_str(), edges.left);
        edges
    }

    fn length_or(&self, property: &str, fallback: f32) -> f32 {
        self.get(property)
            .and_then(parse_length_px)
            .unwrap_or(fallback)
    }
}

impl EdgeSizes {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

impl Selector {
    fn parse(source: &str) -> Option<Self> {
        let parts = source
            .split_whitespace()
            .map(SimpleSelector::parse)
            .collect::<Option<Vec<_>>>()?;

        if parts.is_empty() {
            return None;
        }

        Some(Self { parts })
    }

    fn matches(&self, element: &ElementNode, ancestors: &[&ElementNode]) -> bool {
        let Some((last, previous)) = self.parts.split_last() else {
            return false;
        };

        if !last.matches(element) {
            return false;
        }

        let mut search_index = ancestors.len();
        for selector in previous.iter().rev() {
            let mut matched_index = None;
            while search_index > 0 {
                search_index -= 1;
                if selector.matches(ancestors[search_index]) {
                    matched_index = Some(search_index);
                    break;
                }
            }

            if matched_index.is_none() {
                return false;
            }
        }

        true
    }

    fn specificity(&self) -> (u8, u8, u8) {
        self.parts.iter().fold((0, 0, 0), |acc, part| {
            let part_specificity = part.specificity();
            (
                acc.0 + part_specificity.0,
                acc.1 + part_specificity.1,
                acc.2 + part_specificity.2,
            )
        })
    }
}

impl SimpleSelector {
    fn parse(source: &str) -> Option<Self> {
        let mut tag_name = String::new();
        let mut id = None;
        let mut class_name = None;
        let chars = source.chars().collect::<Vec<_>>();
        let mut index = 0;

        while index < chars.len() {
            match chars[index] {
                '#' => {
                    index += 1;
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    if start == index || id.is_some() {
                        return None;
                    }
                    id = Some(chars[start..index].iter().collect());
                }
                '.' => {
                    index += 1;
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    if start == index || class_name.is_some() {
                        return None;
                    }
                    class_name = Some(chars[start..index].iter().collect());
                }
                ch if is_identifier_char(ch) => {
                    if !tag_name.is_empty() {
                        return None;
                    }
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    tag_name = chars[start..index].iter().collect();
                }
                _ => return None,
            }
        }

        if tag_name.is_empty() && id.is_none() && class_name.is_none() {
            return None;
        }

        Some(Self {
            tag_name: if tag_name.is_empty() {
                None
            } else {
                Some(tag_name)
            },
            id,
            class_name,
        })
    }

    fn matches(&self, element: &ElementNode) -> bool {
        if let Some(tag_name) = &self.tag_name {
            if &element.tag_name != tag_name {
                return false;
            }
        }

        if let Some(id) = &self.id {
            if element.attributes.get("id") != Some(id) {
                return false;
            }
        }

        if let Some(class_name) = &self.class_name {
            let Some(classes) = element.attributes.get("class") else {
                return false;
            };

            if !classes.split_whitespace().any(|candidate| candidate == class_name) {
                return false;
            }
        }

        true
    }

    fn specificity(&self) -> (u8, u8, u8) {
        (
            u8::from(self.id.is_some()),
            u8::from(self.class_name.is_some()),
            u8::from(self.tag_name.is_some()),
        )
    }
}

impl Display for Stylesheet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.rules.is_empty() {
            return writeln!(f, "(empty)");
        }

        for rule in &self.rules {
            write!(f, "{} ", rule.selector)?;
            write!(f, "{{")?;
            for (index, declaration) in rule.declarations.iter().enumerate() {
                if index > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}: {};", declaration.name, declaration.value)?;
            }
            writeln!(f, " }}")?;
        }

        Ok(())
    }
}

impl Display for Selector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, part) in self.parts.iter().enumerate() {
            if index > 0 {
                write!(f, " ")?;
            }
            write!(f, "{part}")?;
        }
        Ok(())
    }
}

impl Display for SimpleSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(tag_name) = &self.tag_name {
            write!(f, "{tag_name}")?;
        }
        if let Some(id) = &self.id {
            write!(f, "#{id}")?;
        }
        if let Some(class_name) = &self.class_name {
            write!(f, ".{class_name}")?;
        }
        Ok(())
    }
}

impl Display for StyleMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;

        for (index, (name, value)) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}: {value}")?;
        }

        write!(f, "}}")
    }
}

fn collect_style_text(node: &Node, output: &mut String) {
    match node {
        Node::Document { children } => {
            for child in children {
                collect_style_text(child, output);
            }
        }
        Node::Element(element) => {
            if element.tag_name == "style" {
                for child in &element.children {
                    if let Node::Text(text) = child {
                        output.push_str(text);
                        output.push('\n');
                    }
                }
            }

            for child in &element.children {
                collect_style_text(child, output);
            }
        }
        Node::Text(_) => {}
    }
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

fn parse_box_shorthand(value: Option<&str>) -> EdgeSizes {
    let Some(value) = value else {
        return EdgeSizes::zero();
    };

    let parts = value
        .split_whitespace()
        .filter_map(parse_length_px)
        .collect::<Vec<_>>();

    match parts.as_slice() {
        [all] => EdgeSizes {
            top: *all,
            right: *all,
            bottom: *all,
            left: *all,
        },
        [vertical, horizontal] => EdgeSizes {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => EdgeSizes {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left] => EdgeSizes {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        _ => EdgeSizes::zero(),
    }
}

fn parse_length_px(value: &str) -> Option<f32> {
    let value = value.trim();
    if value == "0" {
        return Some(0.0);
    }

    value.strip_suffix("px")?.trim().parse::<f32>().ok()
}

fn parse_border_width_shorthand(value: Option<&str>) -> EdgeSizes {
    let Some(value) = value else {
        return EdgeSizes::zero();
    };

    let widths = value
        .split_whitespace()
        .filter_map(parse_length_px)
        .collect::<Vec<_>>();

    match widths.first().copied() {
        Some(width) => EdgeSizes {
            top: width,
            right: width,
            bottom: width,
            left: width,
        },
        None => EdgeSizes::zero(),
    }
}

fn parse_border_color_shorthand(value: Option<&str>) -> Option<&str> {
    let value = value?;
    value
        .split_whitespace()
        .find(|part| parse_length_px(part).is_none() && *part != "solid")
}

#[cfg(test)]
mod tests {
    use super::{DisplayMode, EdgeSizes, Stylesheet};
    use crate::dom::Node;
    use std::collections::BTreeMap;

    #[test]
    fn parses_simple_tag_rules() {
        let stylesheet = Stylesheet::parse("h1 { display: block; color: red; } p { display: inline; }");
        let h1 = Node::element("h1", vec![]);
        let p = Node::element("p", vec![]);

        let h1_styles = match h1 {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(h1_styles.display_mode(), DisplayMode::Block);
        assert_eq!(p_styles.display_mode(), DisplayMode::Inline);
        assert_eq!(h1_styles.to_string(), "{color: red, display: block}");
    }

    #[test]
    fn matches_class_and_id_selectors_with_specificity() {
        let stylesheet = Stylesheet::parse(
            "p { color: gray; } .lead { color: blue; } #hero { color: red; } p.lead { display: inline; }",
        );

        let mut attributes = BTreeMap::new();
        attributes.insert("class".to_string(), "lead featured".to_string());
        attributes.insert("id".to_string(), "hero".to_string());

        let element = match Node::element_with_attributes("p", attributes, vec![]) {
            Node::Element(element) => element,
            _ => unreachable!(),
        };

        let styles = stylesheet.styles_for(&element, &[]);
        assert_eq!(styles.to_string(), "{color: red, display: inline}");
    }

    #[test]
    fn matches_descendant_selectors() {
        let stylesheet = Stylesheet::parse("article .lead { color: green; } body article p.lead { display: inline; }");

        let mut body_attributes = BTreeMap::new();
        body_attributes.insert("class".to_string(), "page".to_string());
        let body = match Node::element_with_attributes("body", body_attributes, vec![]) {
            Node::Element(element) => element,
            _ => unreachable!(),
        };
        let article = match Node::element("article", vec![]) {
            Node::Element(element) => element,
            _ => unreachable!(),
        };

        let mut p_attributes = BTreeMap::new();
        p_attributes.insert("class".to_string(), "lead".to_string());
        let p = match Node::element_with_attributes("p", p_attributes, vec![]) {
            Node::Element(element) => element,
            _ => unreachable!(),
        };

        let styles = stylesheet.styles_for(&p, &[&body, &article]);
        assert_eq!(styles.to_string(), "{color: green, display: inline}");
    }

    #[test]
    fn extracts_styles_from_dom_style_elements() {
        let dom = Node::document(vec![Node::element(
            "html",
            vec![Node::element(
                "head",
                vec![Node::element(
                    "style",
                    vec![Node::text("p { display: none; color: gray; }")],
                )],
            )],
        )]);

        let stylesheet = Stylesheet::from_dom(&dom);
        let p = Node::element("p", vec![]);
        let styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(styles.display_mode(), DisplayMode::None);
        assert_eq!(styles.to_string(), "{color: gray, display: none}");
    }

    #[test]
    fn parses_margin_and_padding_lengths() {
        let stylesheet = Stylesheet::parse(
            "div { margin: 8px 12px; padding: 4px; padding-left: 10px; margin-bottom: 16px; }",
        );
        let div = Node::element("div", vec![]);
        let styles = match div {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(
            styles.margin(),
            EdgeSizes {
                top: 8.0,
                right: 12.0,
                bottom: 16.0,
                left: 12.0,
            }
        );
        assert_eq!(
            styles.padding(),
            EdgeSizes {
                top: 4.0,
                right: 4.0,
                bottom: 4.0,
                left: 10.0,
            }
        );
    }

    #[test]
    fn parses_background_and_border_helpers() {
        let stylesheet = Stylesheet::parse(
            "div { background-color: sand; border: 2px solid ember; border-left-width: 4px; }",
        );
        let div = Node::element("div", vec![]);
        let styles = match div {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(styles.background_color(), Some("sand"));
        assert_eq!(styles.border_color(), Some("ember"));
        assert_eq!(
            styles.border_width(),
            EdgeSizes {
                top: 2.0,
                right: 2.0,
                bottom: 2.0,
                left: 4.0,
            }
        );
    }

    #[test]
    fn parses_fixed_width_and_height() {
        let stylesheet = Stylesheet::parse("div { width: 120px; height: 48px; }");
        let div = Node::element("div", vec![]);
        let styles = match div {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(styles.width_px(), Some(120.0));
        assert_eq!(styles.height_px(), Some(48.0));
    }

    #[test]
    fn parses_min_and_max_sizes() {
        let stylesheet = Stylesheet::parse(
            "div { min-width: 80px; max-width: 140px; min-height: 24px; max-height: 72px; }",
        );
        let div = Node::element("div", vec![]);
        let styles = match div {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(styles.min_width_px(), Some(80.0));
        assert_eq!(styles.max_width_px(), Some(140.0));
        assert_eq!(styles.min_height_px(), Some(24.0));
        assert_eq!(styles.max_height_px(), Some(72.0));
    }

    #[test]
    fn parses_font_size() {
        let stylesheet = Stylesheet::parse("h1 { font-size: 24px; } p { font-size: 14px; }");
        let h1 = Node::element("h1", vec![]);
        let p = Node::element("p", vec![]);

        let h1_styles = match h1 {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(h1_styles.font_size_px(), Some(24.0));
        assert_eq!(p_styles.font_size_px(), Some(14.0));
    }

    #[test]
    fn parses_font_weight() {
        let stylesheet = Stylesheet::parse("strong { font-weight: bold; } .light { font-weight: 300; } p {}");
        let strong = Node::element("strong", vec![]);
        let light = Node::element("div", vec![]);
        let p = Node::element("p", vec![]);

        let strong_styles = match strong {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let light_styles = match light {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(strong_styles.font_weight(), "bold");
        assert_eq!(light_styles.font_weight(), "normal");
        assert_eq!(p_styles.font_weight(), "normal");
    }

    #[test]
    fn parses_line_height() {
        let stylesheet = Stylesheet::parse("p { line-height: 20px; }");
        let p = Node::element("p", vec![]);
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(p_styles.line_height_px(), Some(20.0));
    }

    #[test]
    fn parses_text_decoration() {
        let stylesheet = Stylesheet::parse("a { text-decoration: underline; } p {}");
        let a = Node::element("a", vec![]);
        let p = Node::element("p", vec![]);

        let a_styles = match a {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(a_styles.text_decoration(), Some("underline"));
        assert_eq!(p_styles.text_decoration(), None);
    }

    #[test]
    fn parses_opacity() {
        let stylesheet = Stylesheet::parse("div { opacity: 0.5; } .transparent { opacity: 0; }");
        let div = Node::element("div", vec![]);
        let mut transparent_attrs = BTreeMap::new();
        transparent_attrs.insert("class".to_string(), "transparent".to_string());
        let transparent = Node::element_with_attributes("div", transparent_attrs, vec![]);

        let div_styles = match div {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let transparent_styles = match transparent {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert!((div_styles.opacity() - 0.5).abs() < 0.001);
        assert!((transparent_styles.opacity() - 0.0).abs() < 0.001);
    }

    #[test]
    fn parses_visibility() {
        let stylesheet = Stylesheet::parse("h1 { visibility: hidden; } p {}");
        let h1 = Node::element("h1", vec![]);
        let p = Node::element("p", vec![]);

        let h1_styles = match h1 {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };
        let p_styles = match p {
            Node::Element(element) => stylesheet.styles_for(&element, &[]),
            _ => unreachable!(),
        };

        assert_eq!(h1_styles.visibility(), "hidden");
        assert_eq!(p_styles.visibility(), "visible");
    }
}
