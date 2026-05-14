use super::{ElementData, Selector, SimpleSelector, Specificity};

impl Selector {
    pub(super) fn parse(source: &str) -> Option<Self> {
        // Strip and validate the selector string.
        let source = source.trim();
        if source.is_empty() {
            return None;
        }

        // Split by comma for grouped selectors, then parse each one.
        // For now, we only support the first selector in a group
        // (proper grouping requires rebuilding the Rule structure).
        let selector_str = source.split(',').next().unwrap_or(source).trim();

        // Use the custom descent-based parser (simple, proven).
        let parts = parse_selector_sequence(selector_str)?;
        (!parts.is_empty()).then_some(Self { parts })
    }

    pub fn matches(&self, element: &ElementData, ancestors: &[ElementData]) -> bool {
        let Some((last, previous)) = self.parts.split_last() else {
            return false;
        };
        if !last.matches_data(element) {
            return false;
        }

        let mut search_index = ancestors.len();
        for selector in previous.iter().rev() {
            let mut matched = false;
            while search_index > 0 {
                search_index -= 1;
                if selector.matches_data(&ancestors[search_index]) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return false;
            }
        }

        true
    }

    pub fn specificity(&self) -> Specificity {
        self.parts.iter().fold((0, 0, 0), |acc, part| {
            let p = part.specificity();
            (acc.0 + p.0, acc.1 + p.1, acc.2 + p.2)
        })
    }
}

/// Parse a selector sequence ("div p.foo #bar") into SimpleSelector parts.
/// Whitespace-separated parts are descendant combinators.
fn parse_selector_sequence(source: &str) -> Option<Vec<SimpleSelector>> {
    source
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| SimpleSelector::parse(s))
        .collect::<Option<Vec<_>>>()
}

impl SimpleSelector {
    fn parse(source: &str) -> Option<Self> {
        // Strip pseudo-classes and pseudo-elements (Aurora doesn't support them yet).
        let source = strip_pseudo_suffix(source);
        if source.is_empty() {
            return None;
        }

        // Strip leading universal selector.
        let source = source.trim_start_matches('*');

        let mut tag_name = None;
        let mut id = None;
        let mut class_names = Vec::new();
        let mut attributes = Vec::new();

        let chars: Vec<char> = source.chars().collect();
        let mut index = 0;

        // Parse tag name if present and not starting with . or #.
        if !chars.is_empty() && chars[0].is_ascii_alphabetic() {
            let start = index;
            while index < chars.len() && is_identifier_char(chars[index]) {
                index += 1;
            }
            tag_name = Some(chars[start..index].iter().collect::<String>().to_lowercase());
        }

        // Parse id, classes, and attributes.
        while index < chars.len() {
            match chars[index] {
                '#' => {
                    index += 1;
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    if start < index && id.is_none() {
                        id = Some(chars[start..index].iter().collect());
                    }
                }
                '.' => {
                    index += 1;
                    let start = index;
                    while index < chars.len() && is_identifier_char(chars[index]) {
                        index += 1;
                    }
                    if start < index {
                        class_names.push(chars[start..index].iter().collect());
                    }
                }
                '[' => {
                    // Parse attribute selector [name], [name=value], [name~=value], etc.
                    index += 1;
                    let attr_start = index;
                    while index < chars.len() && chars[index] != ']' {
                        index += 1;
                    }
                    if index < chars.len() && chars[index] == ']' {
                        let attr_str: String = chars[attr_start..index].iter().collect();
                        attributes.push(attr_str);
                        index += 1;
                    }
                }
                _ => {
                    index += 1;
                }
            }
        }

        if tag_name.is_none() && id.is_none() && class_names.is_empty() && attributes.is_empty() {
            return None;
        }

        Some(Self {
            tag_name,
            id,
            class_names,
        })
    }

    pub fn matches_data(&self, element: &ElementData) -> bool {
        if let Some(tag_name) = &self.tag_name {
            if element.tag_name.to_lowercase() != *tag_name {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if element.attributes.get("id") != Some(id) {
                return false;
            }
        }
        let classes = element
            .attributes
            .get("class")
            .map(String::as_str)
            .unwrap_or("");
        let element_classes = classes.split_whitespace().collect::<Vec<_>>();
        self.class_names
            .iter()
            .all(|cn| element_classes.contains(&cn.as_str()))
    }

    fn specificity(&self) -> Specificity {
        (
            u8::from(self.id.is_some()),
            self.class_names.len() as u8,
            u8::from(self.tag_name.is_some()),
        )
    }
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

fn strip_pseudo_suffix(s: &str) -> &str {
    let mut paren_depth = 0i32;
    let mut byte_pos = 0;
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
