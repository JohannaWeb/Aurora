// Import DOM node types
// RUST FUNDAMENTAL: This module works at the boundary between parsed DOM data and computed CSS data,
// so it imports the DOM shapes it needs to inspect while matching selectors and collecting styles.
use crate::dom::{Node, NodePtr};
// Import BTreeMap for sorted attribute storage
use std::collections::BTreeMap;
// Import Display traits for formatting
use std::fmt::{self, Display, Formatter};

// Data structure representing an HTML element for CSS selector matching
// RUST FUNDAMENTAL: Derive macros generate common trait implementations automatically.
// `Debug` gives developer-oriented formatting, `Clone` enables explicit duplication,
// `PartialEq` and `Eq` make equality comparisons possible, and `Default` provides a conventional empty value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementData {
    // The element's tag name (e.g., "div", "p")
    // RUST FUNDAMENTAL: `String` is owned UTF-8 text.
    // Storing ownership here means selector-matching code does not have to worry about borrowed lifetimes for tag names.
    pub tag_name: String,

    // Map of element attributes (id, class, data-*, etc.)
    // RUST FUNDAMENTAL: `BTreeMap` provides deterministic key-ordered iteration.
    // That is often useful in parser or engine code because output stays stable across runs.
    pub attributes: BTreeMap<String, String>,
}

// CSS stylesheet containing rules and CSS custom properties
// RUST FUNDAMENTAL: Making a struct `pub` exposes the type itself.
// Individual `pub` fields then allow direct field access from outside the module with dot syntax.
pub struct Stylesheet {
    // Vector of CSS rules with selectors and declarations
    // RUST FUNDAMENTAL: `Vec<T>` is Rust's standard growable contiguous collection.
    // It is a natural fit when order matters and you expect to append items over time, as with stylesheet rules.
    pub rules: Vec<Rule>,

    // Map of CSS custom properties like --name: value
    // RUST FUNDAMENTAL: A map is a good representation for CSS custom properties because lookup is by name.
    // The ordered map choice also keeps iteration stable for debugging and later processing.
    pub variables: BTreeMap<String, String>,
}

// Stylesheet implementation
impl Stylesheet {
    // Merge another stylesheet's rules and variables into this one
    pub fn merge(&mut self, other: Stylesheet) {
        // Extend rules vector with rules from other stylesheet
        // RUST FUNDAMENTAL: `.extend(...)` appends all items from another iterable into the existing collection.
        self.rules.extend(other.rules);
        // Extend variables map with variables from other stylesheet
        // RUST FUNDAMENTAL: For maps, later inserted keys replace earlier ones, so merge order affects the final value.
        self.variables.extend(other.variables);
    }

    // Create default browser user-agent stylesheet with sensible defaults
    pub fn user_agent_stylesheet() -> Self {
        // Parse built-in CSS rules that browsers apply by default
        Self::parse(
            // CSS string with default display modes for common tags
            "a, abbr, b, bdo, big, br, cite, code, dfn, em, i, img, input, kbd, label, map, object, output, q, samp, select, small, span, strong, sub, sup, textarea, time, tt, var { display: inline; } \
             b, strong { font-weight: bold; color: accent; } \
             i, em { font-style: italic; color: rust; } \
             h1 { font-size: 32px; font-weight: bold; text-align: center; color: coal; } \
             h2 { font-size: 24px; font-weight: bold; color: ink; } \
             h3 { font-size: 18px; font-weight: bold; color: ink; } \
             li { display: block; margin: 4px 0; } \
             div, section, article { display: block; } \
             head, style, script, link, meta, title, noscript, template { display: none; }"
        )
    }

    // Parse CSS stylesheet source string into rules and variables
    pub fn parse(source: &str) -> Self {
        Self::do_parse(source, None)
    }

    // Internal parser that optionally resolves @import rules via fetch
    fn do_parse(source: &str, fetch_ctx: Option<(&str, &opus::domain::Identity)>) -> Self {
        // Initialize result vectors
        let mut rules = Vec::new();
        // Map for CSS custom properties (variables)
        // RUST FUNDAMENTAL: Mutable local variables like these are common in parser-style code that accumulates results incrementally.
        let mut variables = BTreeMap::new();

        // Remove @media, @keyframes, @font-face and other at-rules first;
        // when a fetch context is available, @import URLs are fetched and inlined.
        let stripped = strip_at_rules(source, fetch_ctx, 0);

        // Split on '}' to separate rule blocks, tracking source order
        // RUST FUNDAMENTAL: `.enumerate()` works on any iterator and gives each item a monotonically increasing index.
        for (source_order, chunk) in stripped.split('}').enumerate() {
            // Trim whitespace from chunk
            let chunk = chunk.trim();
            // Skip empty chunks
            if chunk.is_empty() {
                continue;
            }

            // Split selectors and declarations on '{'
            // RUST FUNDAMENTAL: `split_once` is useful when you want at most one split and a pair of borrowed substrings.
            let Some((selector_part, declarations_part)) = chunk.split_once('{') else {
                // Skip malformed rules
                continue;
            };

            // Trim selector part
            let selector_part = selector_part.trim();
            // Parse declarations (property: value pairs)
            let declarations = declarations_part
                // Split on ';' to get individual declarations
                .split(';')
                // Parse each declaration, filtering out empty ones
                // RUST FUNDAMENTAL: `.filter_map(...)` combines transformation and conditional omission into one iterator step.
                .filter_map(|declaration| {
                    // Trim declaration
                    let declaration = declaration.trim();
                    // Skip empty declarations
                    if declaration.is_empty() {
                        return None;
                    }

                    // Split on ':' to separate property name and value
                    // RUST FUNDAMENTAL: The `?` here works because the closure returns `Option<Declaration>`.
                    // If `split_once` fails, the closure yields `None` for this declaration.
                    let (name, value) = declaration.split_once(':')?;
                    // Trim and keep property name
                    let name = name.trim().to_string();
                    // Trim value and remove !important flag
                    let value = value
                        .trim()
                        .trim_end_matches("!important")
                        .trim()
                        .to_string();

                    // Store CSS custom properties (var definitions like --color: blue)
                    // RUST FUNDAMENTAL: String methods like `starts_with` let parser code express tiny lexical rules directly.
                    if name.starts_with("--") && matches!(selector_part, ":root" | "*" | "html") {
                        variables.insert(name.clone(), value.clone());
                    }

                    // Return parsed declaration
                    Some(Declaration { name, value })
                })
                // Collect all declarations into vec
                // RUST FUNDAMENTAL: Type-directed `collect::<Vec<_>>()` tells Rust exactly which collection to build from the iterator.
                .collect::<Vec<_>>();

            // Skip rules with no declarations
            if declarations.is_empty() {
                continue;
            }

            // Handle comma-separated selectors (e.g., "h1, h2, h3 { ... }")
            for selector_str in selector_part.split(',') {
                // Trim each selector
                let selector_str = selector_str.trim();
                // Skip empty selectors
                if selector_str.is_empty() {
                    continue;
                }

                // Parse selector into selector object
                if let Some(selector) = Selector::parse(selector_str) {
                    // Create rule with selector and declarations
                    rules.push(Rule {
                        selector,
                        // Clone declarations for each selector
                        // RUST FUNDAMENTAL: Because one comma-separated block can produce several rules,
                        // cloning lets each rule own its own declarations vector.
                        declarations: declarations.clone(),
                        // Track source order for cascade
                        source_order,
                    });
                }
            }
        }

        // Return stylesheet with rules and variables
        Self { rules, variables }
    }

    pub fn from_dom(
        document: &NodePtr,
        base_url: Option<&str>,
        identity: &opus::domain::Identity,
    ) -> Self {
        // RUST FUNDAMENTAL: This helper gathers stylesheet text first, then parses once at the end.
        // Separating collection from parsing keeps each phase simpler.
        let mut source = String::new();
        collect_styles(document, base_url, identity, &mut source);
        // Pass fetch context so @import rules inside fetched CSS are also resolved.
        let fetch_ctx = base_url.map(|b| (b, identity));
        Self::do_parse(&source, fetch_ctx)
    }

    pub fn styles_for(&self, element: &ElementData, ancestors: &[ElementData]) -> StyleMap {
        // RUST FUNDAMENTAL: Starting from `Default::default()` is idiomatic when building up a map-like result incrementally.
        let mut styles = StyleMap::default();
        let mut matching_rules = self
            .rules
            .iter()
            // RUST FUNDAMENTAL: `.iter()` yields shared references to rules, so selector matching does not take ownership of the stylesheet.
            .filter(|rule| rule.selector.matches(element, ancestors))
            .collect::<Vec<_>>();

        // RUST FUNDAMENTAL: Sorting by a tuple key works because tuples implement lexicographic ordering.
        // That means specificity is compared first, then source order breaks ties.
        matching_rules.sort_by_key(|rule| (rule.selector.specificity(), rule.source_order));

        for rule in matching_rules {
            for declaration in &rule.declarations {
                // Resolve CSS variables in the value
                let resolved_value = self.resolve_variables(&declaration.value);
                // RUST FUNDAMENTAL: Accessing the tuple field `.0` exposes the inner `BTreeMap` of the newtype wrapper.
                styles.0.insert(declaration.name.clone(), resolved_value);
            }
        }

        styles
    }

    /// Resolve CSS variables (var(--name)) in a value, including fallbacks
    pub fn resolve_variables(&self, value: &str) -> String {
        // RUST FUNDAMENTAL: Creating an owned `String` here allows the function to rewrite the value in place as substitutions occur.
        let mut result = value.to_string();
        let mut iterations = 0;
        // RUST FUNDAMENTAL: A `const` inside a function is still a compile-time constant,
        // but it stays scoped to the function that uses it.
        const MAX_ITERATIONS: usize = 100; // Prevent infinite loops

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                break;
            }

            let start = match result.find("var(") {
                Some(idx) => idx,
                None => break,
            };

            // Find the matching closing paren, accounting for nested parens
            // RUST FUNDAMENTAL: A tiny manual state machine is often clearer than a complex regex for nested syntax like this.
            let mut paren_depth = 1;
            let mut end_pos = start + 4;
            for (i, ch) in result[start + 4..].chars().enumerate() {
                if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        end_pos = start + 4 + i;
                        break;
                    }
                }
            }

            if paren_depth != 0 {
                // RUST FUNDAMENTAL: Early `break` on malformed syntax is a simple recovery strategy:
                // leave the unresolved text as-is rather than panicking.
                break; // Unmatched parens
            }

            let var_content = &result[start + 4..end_pos];

            // Split on first comma to separate variable name from fallback
            // RUST FUNDAMENTAL: Optional fallbacks are modeled here as `Option<String>`,
            // which keeps the "maybe there is a fallback" case explicit.
            let (var_name, fallback) = if let Some(comma_idx) = var_content.find(',') {
                let name = var_content[..comma_idx].trim();
                let fb = var_content[comma_idx + 1..].trim();
                (name.to_string(), Some(fb.to_string()))
            } else {
                (var_content.trim().to_string(), None)
            };

            // Try to resolve the variable
            let replacement = if let Some(var_value) = self.variables.get(&var_name) {
                // RUST FUNDAMENTAL: Map lookup returns `Option<&String>`, so cloning is needed to produce an owned replacement string.
                var_value.clone()
            } else if let Some(fb) = fallback {
                // Use fallback if variable not found
                fb
            } else {
                // No variable and no fallback, keep original
                break;
            };

            // Replace var(...) with the resolved value
            // RUST FUNDAMENTAL: `replace_range` mutates the existing string buffer in place rather than building a fresh string manually.
            result.replace_range(start..end_pos + 1, &replacement);
        }

        result
    }
}

fn collect_styles(
    node_ptr: &NodePtr,
    base_url: Option<&str>,
    identity: &opus::domain::Identity,
    output: &mut String,
) {
    // RUST FUNDAMENTAL: This helper accumulates stylesheet text through a mutable output buffer passed by reference.
    // That avoids repeatedly returning and concatenating intermediate strings at each recursion step.
    let node = node_ptr.borrow();
    match &*node {
        Node::Document { children } => {
            for child in children {
                collect_styles(child, base_url, identity, output);
            }
        }
        Node::Element(element) => {
            if element.tag_name == "style" {
                for child in &element.children {
                    let child_borrow = child.borrow();
                    if let Node::Text(text) = &*child_borrow {
                        // RUST FUNDAMENTAL: `push_str` appends borrowed string contents into an owned `String` buffer.
                        output.push_str(text);
                        output.push('\n');
                    }
                }
            } else if element.tag_name == "link"
                && element.attributes.get("rel").map(String::as_str) == Some("stylesheet")
            {
                // RUST FUNDAMENTAL: Tuple-pattern `if let` is a compact way to require multiple optional values at once.
                if let (Some(base), Some(href)) = (base_url, element.attributes.get("href")) {
                    if let Ok(url) = crate::fetch::resolve_relative_url(base, href) {
                        if let Ok(css) = crate::fetch::fetch_string(&url, identity) {
                            output.push_str(&css);
                            output.push('\n');
                        }
                    }
                }
            }

            for child in &element.children {
                collect_styles(child, base_url, identity, output);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    // RUST FUNDAMENTAL: Public struct fields are often fine for plain data carrier types like parser or rule representations.
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub parts: Vec<SimpleSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub class_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
}

pub type Specificity = (u8, u8, u8);
// RUST FUNDAMENTAL: This alias documents intent: the tuple is not just any three numbers,
// it specifically represents CSS specificity components.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleMap(BTreeMap<String, String>);
// RUST FUNDAMENTAL: This is a "newtype" wrapper around `BTreeMap<String, String>`.
// Newtypes let you add domain-specific methods and trait impls without exposing the raw map as the whole API.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    pub fn zero() -> Self {
        // RUST FUNDAMENTAL: Returning `Self` instead of spelling the concrete type name keeps constructor helpers concise.
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarginValue {
    Px(f32),
    Auto,
}

impl MarginValue {
    pub fn to_px(self) -> f32 {
        // RUST FUNDAMENTAL: Taking `self` by value is cheap here because `MarginValue` is a small `Copy` enum.
        match self {
            Self::Px(px) => px,
            Self::Auto => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    pub top: f32,
    pub right: MarginValue,
    pub bottom: f32,
    pub left: MarginValue,
}

impl Margin {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: MarginValue::Px(0.0),
            bottom: 0.0,
            left: MarginValue::Px(0.0),
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left.to_px() + self.right.to_px()
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Block,
    Inline,
    InlineBlock,
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl StyleMap {
    pub fn is_empty(&self) -> bool {
        // RUST FUNDAMENTAL: Wrapper types often forward small convenience methods to their inner value.
        self.0.is_empty()
    }

    pub fn display_mode(&self) -> DisplayMode {
        // RUST FUNDAMENTAL: `.map(String::as_str)` converts `Option<&String>` into `Option<&str>`,
        // which makes matching against string literals ergonomic.
        match self.0.get("display").map(String::as_str) {
            Some("inline") | Some("inline-block") => DisplayMode::Inline,
            Some("flex") => DisplayMode::Flex,
            Some("none") => DisplayMode::None,
            _ => DisplayMode::Block,
        }
    }

    pub fn flex_direction(&self) -> FlexDirection {
        match self.0.get("flex-direction").map(String::as_str) {
            Some("column") => FlexDirection::Column,
            _ => FlexDirection::Row,
        }
    }

    pub fn justify_content(&self) -> JustifyContent {
        match self.0.get("justify-content").map(String::as_str) {
            Some("center") => JustifyContent::Center,
            Some("flex-end") => JustifyContent::FlexEnd,
            Some("space-between") => JustifyContent::SpaceBetween,
            Some("space-around") => JustifyContent::SpaceAround,
            _ => JustifyContent::FlexStart,
        }
    }

    pub fn align_items(&self) -> AlignItems {
        match self.0.get("align-items").map(String::as_str) {
            Some("flex-start") => AlignItems::FlexStart,
            Some("center") => AlignItems::Center,
            Some("flex-end") => AlignItems::FlexEnd,
            _ => AlignItems::Stretch,
        }
    }

    pub fn flex_wrap(&self) -> bool {
        matches!(self.0.get("flex-wrap").map(String::as_str), Some("wrap"))
    }

    pub fn gap_px(&self) -> f32 {
        self.get("column-gap")
            .and_then(parse_length_px)
            .or_else(|| self.get("gap").and_then(parse_length_px))
            .unwrap_or(0.0)
    }

    pub fn text_align(&self) -> TextAlign {
        match self.0.get("text-align").map(String::as_str) {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }

    pub fn box_sizing(&self) -> BoxSizing {
        match self.0.get("box-sizing").map(String::as_str) {
            Some("border-box") => BoxSizing::BorderBox,
            _ => BoxSizing::ContentBox,
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        // RUST FUNDAMENTAL: Returning borrowed `&str` slices keeps lookup cheap and avoids allocating new strings for reads.
        self.0.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        // RUST FUNDAMENTAL: Accepting `impl Into<String>` on both arguments keeps callers flexible while still storing owned strings internally.
        self.0.insert(name.into(), value.into());
    }

    pub fn margin(&self) -> Margin {
        let mut margin = parse_margin_shorthand(self.get("margin"));
        // RUST FUNDAMENTAL: Repeated `if let Some(...)` overrides are a straightforward way to express CSS shorthand-then-longhand behavior.
        if let Some(top) = self.get("margin-top").and_then(parse_length_px) {
            margin.top = top;
        }
        if let Some(right) = self.get("margin-right") {
            margin.right = parse_margin_value(right);
        }
        if let Some(bottom) = self.get("margin-bottom").and_then(parse_length_px) {
            margin.bottom = bottom;
        }
        if let Some(left) = self.get("margin-left") {
            margin.left = parse_margin_value(left);
        }
        margin
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
        self.get("background-color")
            .or_else(|| self.get("background"))
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

    /// Resolve width with support for %, rem, em, vw units.
    pub fn width_resolved(
        &self,
        available_width: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_width: f32,
    ) -> Option<f32> {
        // RUST FUNDAMENTAL: The `?` operator on `Option` means "return `None` immediately if the property is missing".
        let raw = self.get("width")?;
        // RUST FUNDAMENTAL: Returning early for special CSS keywords keeps the normal numeric path simpler.
        if raw == "auto" {
            return None;
        }
        // RUST FUNDAMENTAL: `.map(...)` transforms the successful parsed length while preserving the `Option` wrapper.
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_width,
                font_size,
                root_font_size,
                viewport_width,
                0.0,
            )
        })
    }

    /// Resolve height with support for %, rem, em, vh units.
    pub fn height_resolved(
        &self,
        available_height: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let raw = self.get("height")?;
        if raw == "auto" || raw.contains("calc(") {
            return None;
        }
        parse_length_value(raw).map(|lv| {
            lv.to_px(
                available_height,
                font_size,
                root_font_size,
                0.0,
                viewport_height,
            )
        })
    }

    /// Resolve font-size with support for rem, em, % units.
    /// parent_font_size is used for em/% resolution.
    pub fn font_size_resolved(&self, parent_font_size: f32, root_font_size: f32) -> Option<f32> {
        let raw = self.get("font-size")?;
        parse_length_value(raw)
            .map(|lv| lv.to_px(parent_font_size, parent_font_size, root_font_size, 0.0, 0.0))
    }

    pub fn font_weight(&self) -> &str {
        // RUST FUNDAMENTAL: `unwrap_or(...)` supplies a borrowed default when the option is `None`.
        self.get("font-weight").unwrap_or("normal")
    }

    pub fn is_bold(&self) -> bool {
        // RUST FUNDAMENTAL: Storing an intermediate borrowed value like `weight` avoids repeated map lookups and keeps the boolean expression readable.
        let weight = self.font_weight();
        weight == "bold" || weight == "700" || weight == "bolder"
    }

    pub fn font_style(&self) -> &str {
        self.get("font-style").unwrap_or("normal")
    }

    pub fn is_italic(&self) -> bool {
        let style = self.font_style();
        style == "italic" || style == "oblique"
    }

    pub fn line_height_px(&self) -> Option<f32> {
        self.get("line-height").and_then(parse_length_px)
    }

    pub fn text_decoration(&self) -> Option<&str> {
        self.get("text-decoration")
    }

    pub fn opacity(&self) -> f32 {
        self.get("opacity")
            // RUST FUNDAMENTAL: Parsing user input often yields `Result`, and `.ok()` discards the error while converting success into `Option`.
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
            // RUST FUNDAMENTAL: Numeric helper methods like `.clamp(min, max)` are often clearer than hand-written bounds checks.
            .clamp(0.0, 1.0)
    }

    pub fn visibility(&self) -> &str {
        self.get("visibility").unwrap_or("visible")
    }

    pub fn resolve_vars(&mut self, ancestors: &[&StyleMap]) {
        let mut resolved = BTreeMap::new();
        // RUST FUNDAMENTAL: Iterating over `&self.0` borrows the map immutably while we compute replacements into a separate map.
        // That avoids mutating a collection while iterating over it.
        for (name, value) in &self.0 {
            if value.contains("var(") {
                if let Some(new_value) = self.resolve_single_value(value, ancestors) {
                    resolved.insert(name.clone(), new_value);
                }
            }
        }
        // RUST FUNDAMENTAL: Applying updates in a second pass is a common technique when in-place mutation would conflict with active borrows.
        for (name, value) in resolved {
            self.0.insert(name, value);
        }
    }

    fn resolve_single_value(&self, value: &str, ancestors: &[&StyleMap]) -> Option<String> {
        if !value.contains("var(") {
            return Some(value.to_string());
        }

        let mut result = String::new();
        let mut last_end = 0;

        // Very basic var() parser: find "var(--name)"
        // Note: Real CSS allows fallback: var(--name, fallback)
        // RUST FUNDAMENTAL: This loop keeps slicing from `last_end` onward, which is a common incremental parsing pattern.
        while let Some(start) = value[last_end..].find("var(") {
            let start = last_end + start;
            result.push_str(&value[last_end..start]);

            let var_content_start = start + 4;
            if let Some(end) = value[var_content_start..].find(')') {
                let end = var_content_start + end;
                let var_expr = value[var_content_start..end].trim();

                // Handle fallback: var(--name, fallback)
                let (var_name, _fallback) = if let Some((name, fall)) = var_expr.split_once(',') {
                    (name.trim(), Some(fall.trim()))
                } else {
                    (var_expr, None)
                };

                if let Some(val) = self.lookup_variable(var_name, ancestors) {
                    result.push_str(&val);
                } else {
                    // If no value and no fallback, keep the var() expr or empty?
                    // Standard says it should be 'invalid at computed value time'.
                    // We'll leave it as is for now.
                    result.push_str(&value[start..end + 1]);
                }
                last_end = end + 1;
            } else {
                break;
            }
        }
        result.push_str(&value[last_end..]);
        Some(result)
    }

    fn lookup_variable(&self, name: &str, ancestors: &[&StyleMap]) -> Option<String> {
        // Look in current map
        // RUST FUNDAMENTAL: Returning early on the local match keeps the common case fast and easy to read.
        if let Some(val) = self.0.get(name) {
            return Some(val.clone());
        }
        // Look in ancestors (closest first)
        // RUST FUNDAMENTAL: `.rev()` walks the slice from the end back to the start,
        // which here models CSS inheritance from nearest ancestor outward.
        for ancestor in ancestors.iter().rev() {
            if let Some(val) = ancestor.0.get(name) {
                return Some(val.clone());
            }
        }
        None
    }

    fn edge_sizes(&self, prefix: &str) -> EdgeSizes {
        let mut edges = parse_box_shorthand(self.get(prefix));
        // RUST FUNDAMENTAL: `format!(...)` here builds property names dynamically from a prefix.
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

impl Selector {
    fn parse(source: &str) -> Option<Self> {
        let parts = source
            .split_whitespace()
            .map(SimpleSelector::parse)
            // RUST FUNDAMENTAL: Collecting into `Option<Vec<_>>` succeeds only if every item is `Some(...)`.
            // If any parsed selector part is `None`, the whole parse returns `None`.
            .collect::<Option<Vec<_>>>()?;

        if parts.is_empty() {
            return None;
        }

        Some(Self { parts })
    }

    pub fn matches(&self, element: &ElementData, ancestors: &[ElementData]) -> bool {
        // RUST FUNDAMENTAL: `split_last()` separates the rightmost item from the rest of a slice.
        // For descendant selectors, the last selector must match the current element.
        let Some((last, previous)) = self.parts.split_last() else {
            return false;
        };

        if !last.matches_data(element) {
            return false;
        }

        let mut search_index = ancestors.len();
        for selector in previous.iter().rev() {
            let mut matched_index = None;
            // RUST FUNDAMENTAL: This nested scan walks ancestors from nearest to farthest,
            // which matches how descendant selectors are typically resolved.
            while search_index > 0 {
                search_index -= 1;
                if selector.matches_data(&ancestors[search_index]) {
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

    pub fn specificity(&self) -> Specificity {
        // RUST FUNDAMENTAL: `.fold(...)` reduces an iterator to one accumulated value.
        self.parts.iter().fold((0, 0, 0), |acc, part| {
            let p = part.specificity();
            (acc.0 + p.0, acc.1 + p.1, acc.2 + p.2)
        })
    }
}

impl SimpleSelector {
    fn parse(source: &str) -> Option<Self> {
        // Strip pseudo-selectors (:hover, ::before) and attribute selectors ([type="text"])
        // instead of rejecting the whole rule — we just ignore those qualifiers
        let source = strip_pseudo_suffix(source);
        // Universal selector
        // RUST FUNDAMENTAL: Rebinding `let source = ...` shadows the previous binding with a refined value.
        // Shadowing is common in Rust when each transformation produces a cleaner next stage.
        let source = source.trim_start_matches('*');

        let mut tag_name = String::new();
        let mut id = None;
        let mut class_names = Vec::new();
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
                    // RUST FUNDAMENTAL: Type inference can often determine the collection target here from the field type.
                    id = Some(chars[start..index].iter().collect());
                }
                '.' => {
                    index += 1;
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    if start == index {
                        return None;
                    }
                    class_names.push(chars[start..index].iter().collect());
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

        if tag_name.is_empty() && id.is_none() && class_names.is_empty() {
            return None;
        }

        Some(Self {
            // RUST FUNDAMENTAL: Conditional expressions can construct optional fields directly.
            tag_name: if tag_name.is_empty() {
                None
            } else {
                Some(tag_name)
            },
            id,
            class_names,
        })
    }

    pub fn matches_data(&self, element: &ElementData) -> bool {
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

        if !self.class_names.is_empty() {
            let classes = element
                .attributes
                .get("class")
                .map(|s| s.as_str())
                .unwrap_or("");
            // RUST FUNDAMENTAL: Collecting into `Vec<&str>` here borrows slices from the original class string;
            // it does not allocate owned strings for each class token.
            let element_classes: Vec<&str> = classes.split_whitespace().collect();
            if !self
                .class_names
                .iter()
                .all(|cn| element_classes.contains(&cn.as_str()))
            {
                return false;
            }
        }

        true
    }

    fn specificity(&self) -> (u8, u8, u8) {
        (
            u8::from(self.id.is_some()),
            self.class_names.len() as u8,
            u8::from(self.tag_name.is_some()),
        )
    }
}

impl Display for Stylesheet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.rules.is_empty() {
            return writeln!(f, "(empty)");
        }

        // RUST FUNDAMENTAL: Display impls are often just structured loops that write pieces out in order.
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
            // RUST FUNDAMENTAL: `enumerate()` is useful even in formatting code when separators depend on item position.
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
        for cn in &self.class_names {
            write!(f, ".{cn}")?;
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

fn is_identifier_char(ch: char) -> bool {
    // RUST FUNDAMENTAL: Tiny predicate helpers like this keep parsing code readable by naming a repeated lexical rule once.
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

/// Strip pseudo-selectors (`:hover`, `::before`) and attribute selectors (`[type="text"]`)
/// from a simple selector string. Instead of rejecting rules that use these, we accept
/// the base selector and ignore the qualifiers.
fn strip_pseudo_suffix(s: &str) -> &str {
    let mut paren_depth: i32 = 0;
    let mut byte_pos = 0;
    // RUST FUNDAMENTAL: `char_indices()` yields both the byte offset and the decoded `char`,
    // which is exactly what string-slicing parsers need.
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ':' | '[' if paren_depth == 0 => return &s[..i],
            _ => {}
        }
        byte_pos = i + ch.len_utf8();
    }
    &s[..byte_pos]
}

/// Remove @-rule blocks (@media, @keyframes, @font-face, etc.) from CSS source.
/// These contain nested braces that break the simple `split('}')` parser.
/// When `fetch_ctx` is Some, @import URLs are fetched and prepended to the output.
/// `depth` limits recursion when fetching imported sheets (max 3 levels).
fn strip_at_rules(
    source: &str,
    fetch_ctx: Option<(&str, &opus::domain::Identity)>,
    depth: u32,
) -> String {
    // RUST FUNDAMENTAL: `String::with_capacity(...)` preallocates space when you have a decent size estimate,
    // which can reduce repeated reallocations during building.
    let mut result = String::with_capacity(source.len());
    // Accumulate imported CSS to prepend (lower specificity than the importing sheet).
    let mut imports = String::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '@' {
            // Collect the at-keyword content up to the first '{' or ';'.
            let mut keyword_buf = String::new();
            let mut found_brace = false;
            for c in chars.by_ref() {
                // RUST FUNDAMENTAL: `.by_ref()` lets the `for` loop borrow the iterator mutably
                // so the outer function can keep using the same iterator afterward.
                if c == '{' {
                    found_brace = true;
                    break;
                } else if c == ';' {
                    // Simple at-rule (e.g. @import, @charset) — no block
                    break;
                } else {
                    keyword_buf.push(c);
                }
            }
            if found_brace {
                // Skip the entire block, tracking nested braces
                let mut depth_count = 1usize;
                for c in chars.by_ref() {
                    match c {
                        '{' => depth_count += 1,
                        '}' => {
                            depth_count -= 1;
                            if depth_count == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // Simple at-rule: check for @import and fetch when context is available.
                let keyword_trimmed = keyword_buf.trim_start();
                if keyword_trimmed.to_ascii_lowercase().starts_with("import") && depth < 3 {
                    let after_import = keyword_trimmed["import".len()..].trim();
                    if let (Some(url), Some((base, identity))) =
                        (extract_import_url(after_import), fetch_ctx)
                    {
                        if let Ok(resolved) = crate::fetch::resolve_relative_url(base, &url) {
                            if let Ok(fetched) = crate::fetch::fetch_string(&resolved, identity) {
                                let inner = strip_at_rules(
                                    &fetched,
                                    Some((&resolved, identity)),
                                    depth + 1,
                                );
                                imports.push_str(&inner);
                                imports.push('\n');
                            }
                        }
                    }
                }
                // Other simple at-rules (@charset, @namespace) are discarded.
            }
        } else {
            result.push(ch);
        }
    }

    // Prepend imported rules so they have lower cascade order than the importing sheet.
    if imports.is_empty() {
        result
    } else {
        imports + &result
    }
}

/// Extract a URL string from an @import argument.
/// Handles: `url("...")`, `url('...')`, `url(...)`, `"..."`, `'...'`
fn extract_import_url(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("url(") {
        let inner = rest.trim_end_matches(')').trim();
        let inner = inner.trim_matches('"').trim_matches('\'');
        if !inner.is_empty() {
            return Some(inner.to_string());
        }
    }
    if let Some(inner) = s.strip_prefix('"') {
        if let Some(inner) = inner.strip_suffix('"') {
            return Some(inner.to_string());
        }
    }
    if let Some(inner) = s.strip_prefix('\'') {
        if let Some(inner) = inner.strip_suffix('\'') {
            return Some(inner.to_string());
        }
    }
    None
}

fn parse_margin_shorthand(value: Option<&str>) -> Margin {
    let Some(value) = value else {
        return Margin::zero();
    };

    let parts = value.split_whitespace().collect::<Vec<_>>();

    // RUST FUNDAMENTAL: Matching on `parts.as_slice()` lets the code branch by the exact number of shorthand values present.
    match parts.as_slice() {
        [all] => {
            let val = parse_margin_value(all);
            Margin {
                top: val.to_px(),
                right: val,
                bottom: val.to_px(),
                left: val,
            }
        }
        [vertical, horizontal] => {
            let v = parse_margin_value(vertical);
            let h = parse_margin_value(horizontal);
            Margin {
                top: v.to_px(),
                right: h,
                bottom: v.to_px(),
                left: h,
            }
        }
        [top, horizontal, bottom] => {
            let t = parse_margin_value(top);
            let h = parse_margin_value(horizontal);
            let b = parse_margin_value(bottom);
            Margin {
                top: t.to_px(),
                right: h,
                bottom: b.to_px(),
                left: h,
            }
        }
        [top, right, bottom, left] => {
            let t = parse_margin_value(top);
            let r = parse_margin_value(right);
            let b = parse_margin_value(bottom);
            let l = parse_margin_value(left);
            Margin {
                top: t.to_px(),
                right: r,
                bottom: b.to_px(),
                left: l,
            }
        }
        _ => Margin::zero(),
    }
}

fn parse_margin_value(value: &str) -> MarginValue {
    if value == "auto" {
        MarginValue::Auto
    } else {
        // RUST FUNDAMENTAL: `unwrap_or(0.0)` provides a forgiving fallback for malformed lengths.
        MarginValue::Px(parse_length_px(value).unwrap_or(0.0))
    }
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

    // RUST FUNDAMENTAL: Chaining `strip_suffix(...)? ... parse().ok()` is a compact parser pipeline:
    // validate the unit suffix, parse the numeric part, then convert parse failure into `None`.
    value.strip_suffix("px")?.trim().parse::<f32>().ok()
}

/// A length value that may be in various CSS units.
#[derive(Debug, Clone, Copy)]
pub enum LengthValue {
    Px(f32),
    Percent(f32),
    Rem(f32),
    Em(f32),
    Vh(f32),
    Vw(f32),
}

impl LengthValue {
    /// Resolve to pixels given context.
    pub fn to_px(
        self,
        available: f32,
        font_size: f32,
        root_font_size: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> f32 {
        // RUST FUNDAMENTAL: Enums plus `match` are a natural way to encode unit-specific behavior.
        match self {
            LengthValue::Px(v) => v,
            LengthValue::Percent(v) => available * v / 100.0,
            LengthValue::Rem(v) => root_font_size * v,
            LengthValue::Em(v) => font_size * v,
            LengthValue::Vw(v) => viewport_width * v / 100.0,
            LengthValue::Vh(v) => viewport_height * v / 100.0,
        }
    }
}

pub fn parse_length_value(value: &str) -> Option<LengthValue> {
    let value = value.trim();
    if value == "0" {
        return Some(LengthValue::Px(0.0));
    }
    // RUST FUNDAMENTAL: A sequence of `if let Some(...) = value.strip_suffix(...)` checks is a simple handwritten lexer for unit suffixes.
    if let Some(v) = value.strip_suffix("px") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Px);
    }
    if let Some(v) = value.strip_suffix('%') {
        return v.trim().parse::<f32>().ok().map(LengthValue::Percent);
    }
    if let Some(v) = value.strip_suffix("rem") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Rem);
    }
    if let Some(v) = value.strip_suffix("em") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Em);
    }
    if let Some(v) = value.strip_suffix("vw") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vw);
    }
    if let Some(v) = value.strip_suffix("vh") {
        return v.trim().parse::<f32>().ok().map(LengthValue::Vh);
    }
    None
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
        // RUST FUNDAMENTAL: `.first().copied()` turns `Option<&f32>` into `Option<f32>`,
        // which is convenient when the inner type implements `Copy`.
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
        // RUST FUNDAMENTAL: `.find(...)` returns the first matching item from an iterator and stops scanning after that.
        .find(|part| parse_length_px(part).is_none() && *part != "solid")
}
