use super::{ElementData, Selector, SimpleSelector, Specificity};

impl Selector {
    pub(super) fn parse(source: &str) -> Option<Self> {
        let parts = source
            .split_whitespace()
            .map(SimpleSelector::parse)
            .collect::<Option<Vec<_>>>()?;
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

impl SimpleSelector {
    fn parse(source: &str) -> Option<Self> {
        let source = strip_pseudo_suffix(source).trim_start_matches('*');
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
            tag_name: (!tag_name.is_empty()).then_some(tag_name),
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
