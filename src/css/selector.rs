use super::{AttrOp, AttrSel, Combinator, ElementData, PseudoClass, Selector, SelectorPart,
            SimpleSelector, Specificity};

// ─── Selector ────────────────────────────────────────────────────────────────

impl Selector {
    pub fn parse(source: &str) -> Option<Self> {
        let source = source.trim();
        if source.is_empty() {
            return None;
        }
        let parts = parse_selector_parts(source)?;
        (!parts.is_empty()).then_some(Self { parts })
    }

    /// Test whether this selector matches `element` given its `ancestors` slice
    /// (`ancestors[0]` = outermost, `ancestors[last]` = immediate parent).
    pub fn matches(&self, element: &ElementData, ancestors: &[ElementData]) -> bool {
        if self.parts.is_empty() {
            return false;
        }

        // The rightmost part must match the current element.
        if !self
            .parts
            .last()
            .unwrap()
            .simple
            .matches_data(element, Some(ancestors))
        {
            return false;
        }

        // Walk backwards through the remaining parts.
        // `search_end` is the exclusive upper bound of the ancestor slice to search.
        let mut search_end = ancestors.len();

        for i in (0..self.parts.len() - 1).rev() {
            // parts[i+1].combinator describes how parts[i+1] relates to parts[i].
            let combinator = self.parts[i + 1].combinator;
            let simple = &self.parts[i].simple;

            match combinator {
                Combinator::Descendant => {
                    let mut found = false;
                    for j in (0..search_end).rev() {
                        if simple.matches_data(&ancestors[j], Some(&ancestors[..j])) {
                            search_end = j;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
                Combinator::Child => {
                    if search_end == 0
                        || !simple.matches_data(
                            &ancestors[search_end - 1],
                            Some(&ancestors[..search_end - 1]),
                        )
                    {
                        return false;
                    }
                    search_end -= 1;
                }
                Combinator::Adjacent | Combinator::Sibling => {
                    // Sibling info not available in the cascade context.
                    return false;
                }
            }
        }

        true
    }

    pub fn specificity(&self) -> Specificity {
        self.parts.iter().fold((0u8, 0u8, 0u8), |acc, part| {
            let p = part.simple.specificity();
            (
                acc.0.saturating_add(p.0),
                acc.1.saturating_add(p.1),
                acc.2.saturating_add(p.2),
            )
        })
    }
}

// ─── SimpleSelector ───────────────────────────────────────────────────────────

impl SimpleSelector {
    pub fn matches_data(&self, element: &ElementData, ancestors: Option<&[ElementData]>) -> bool {
        // Tag name (case-insensitive).
        if let Some(tag) = &self.tag_name {
            if !element.tag_name.eq_ignore_ascii_case(tag) {
                return false;
            }
        }
        // ID.
        if let Some(id) = &self.id {
            if element.attributes.get("id").map(String::as_str) != Some(id.as_str()) {
                return false;
            }
        }
        // Classes.
        let class_attr = element
            .attributes
            .get("class")
            .map(String::as_str)
            .unwrap_or("");
        let element_classes: Vec<&str> = class_attr.split_whitespace().collect();
        if !self
            .class_names
            .iter()
            .all(|cn| element_classes.contains(&cn.as_str()))
        {
            return false;
        }
        // Attribute selectors.
        for attr in &self.attributes {
            let actual = element.attributes.get(&attr.name);
            if !attr_op_matches(attr.op, actual.map(String::as_str), attr.value.as_deref()) {
                return false;
            }
        }
        // Pseudo-classes.
        for pc in &self.pseudo_classes {
            match pc {
                PseudoClass::Not(inner) => {
                    if inner.matches_data(element, ancestors) {
                        return false;
                    }
                }
                PseudoClass::Root => {
                    let is_root = ancestors.map_or(true, |a| a.is_empty());
                    if !is_root {
                        return false;
                    }
                }
                // Structural pseudo-classes need sibling info — not available in cascade.
                PseudoClass::FirstChild
                | PseudoClass::LastChild
                | PseudoClass::NthChild(_, _)
                | PseudoClass::FirstOfType
                | PseudoClass::LastOfType => return false,
                // State pseudo-classes can't be matched statically.
                PseudoClass::Hover
                | PseudoClass::Focus
                | PseudoClass::Active
                | PseudoClass::Checked
                | PseudoClass::Disabled
                | PseudoClass::Enabled
                | PseudoClass::Visited => return false,
                PseudoClass::Unknown(_) => return false,
            }
        }
        true
    }

    pub fn specificity(&self) -> Specificity {
        let ids = u8::from(self.id.is_some());
        let mut classes = (self.class_names.len() + self.attributes.len()) as u8;

        for pc in &self.pseudo_classes {
            match pc {
                PseudoClass::Not(inner) => {
                    // :not() contributes its argument's specificity, not its own.
                    let (i, c, t) = inner.specificity();
                    return (
                        ids.saturating_add(i),
                        classes.saturating_add(c),
                        u8::from(self.tag_name.is_some()).saturating_add(t),
                    );
                }
                _ => classes = classes.saturating_add(1),
            }
        }

        (ids, classes, u8::from(self.tag_name.is_some()))
    }
}

fn attr_op_matches(op: AttrOp, actual: Option<&str>, expected: Option<&str>) -> bool {
    match op {
        AttrOp::Exists => actual.is_some(),
        AttrOp::Equals => actual == expected,
        AttrOp::Includes => {
            let val = expected.unwrap_or("");
            actual
                .map(|a| a.split_whitespace().any(|w| w == val))
                .unwrap_or(false)
        }
        AttrOp::DashPrefix => {
            let val = expected.unwrap_or("");
            actual
                .map(|a| a == val || a.starts_with(&format!("{val}-")))
                .unwrap_or(false)
        }
        AttrOp::Prefix => actual.map(|a| a.starts_with(expected.unwrap_or(""))).unwrap_or(false),
        AttrOp::Suffix => actual.map(|a| a.ends_with(expected.unwrap_or(""))).unwrap_or(false),
        AttrOp::Substring => actual.map(|a| a.contains(expected.unwrap_or(""))).unwrap_or(false),
    }
}

// ─── Parsing ─────────────────────────────────────────────────────────────────

/// Parse a single selector string (no commas) into a sequence of `SelectorPart`s.
fn parse_selector_parts(source: &str) -> Option<Vec<SelectorPart>> {
    let chars: Vec<char> = source.chars().collect();
    let mut parts: Vec<SelectorPart> = Vec::new();
    let mut pending: Option<Combinator> = None;
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            c if c.is_whitespace() => {
                if pending.is_none() {
                    pending = Some(Combinator::Descendant);
                }
                i += 1;
            }
            '>' => {
                pending = Some(Combinator::Child);
                i += 1;
            }
            '+' => {
                pending = Some(Combinator::Adjacent);
                i += 1;
            }
            '~' => {
                pending = Some(Combinator::Sibling);
                i += 1;
            }
            _ => {
                let (simple, consumed) = parse_simple_selector(&chars[i..])?;
                let combinator = if parts.is_empty() {
                    Combinator::Descendant // ignored for first part
                } else {
                    pending.unwrap_or(Combinator::Descendant)
                };
                parts.push(SelectorPart { combinator, simple });
                pending = None;
                i += consumed;
            }
        }
    }

    if parts.is_empty() { None } else { Some(parts) }
}

/// Parse one compound selector from the start of `chars`.
/// Returns `(SimpleSelector, chars_consumed)`.
fn parse_simple_selector(chars: &[char]) -> Option<(SimpleSelector, usize)> {
    let mut tag_name: Option<String> = None;
    let mut id: Option<String> = None;
    let mut class_names: Vec<String> = Vec::new();
    let mut attributes: Vec<AttrSel> = Vec::new();
    let mut pseudo_classes: Vec<PseudoClass> = Vec::new();
    let mut i = 0;

    // Universal selector or tag name.
    if i < chars.len() {
        if chars[i] == '*' {
            i += 1; // universal — no tag constraint
        } else if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            tag_name = Some(chars[start..i].iter().collect::<String>().to_lowercase());
        }
    }

    // Qualifiers: #id  .class  [attr]  :pseudo
    loop {
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            '#' => {
                i += 1;
                let start = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                if start < i && id.is_none() {
                    id = Some(chars[start..i].iter().collect());
                }
            }
            '.' => {
                i += 1;
                let start = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                if start < i {
                    class_names.push(chars[start..i].iter().collect());
                }
            }
            '[' => {
                i += 1;
                let (attr_sel, consumed) = parse_attr_selector(&chars[i..])?;
                attributes.push(attr_sel);
                i += consumed;
            }
            ':' => {
                i += 1;
                if i < chars.len() && chars[i] == ':' {
                    i += 1; // pseudo-element — treat as unknown pseudo-class
                }
                let (pc, consumed) = parse_pseudo_class(&chars[i..]);
                pseudo_classes.push(pc);
                i += consumed;
            }
            _ => break,
        }
    }

    if i == 0 {
        return None;
    }

    Some((
        SimpleSelector { tag_name, id, class_names, attributes, pseudo_classes },
        i,
    ))
}

/// Parse the body of an attribute selector after the opening `[`, up to and including `]`.
/// Returns `(AttrSel, chars_consumed_including_])`.
fn parse_attr_selector(chars: &[char]) -> Option<(AttrSel, usize)> {
    let mut i = 0;

    // Attribute name (may be namespaced — skip namespace prefix).
    while i < chars.len()
        && chars[i] != '='
        && chars[i] != ']'
        && chars[i] != '~'
        && chars[i] != '|'
        && chars[i] != '^'
        && chars[i] != '$'
        && chars[i] != '*'
    {
        i += 1;
    }
    let raw_name: String = chars[..i].iter().collect::<String>().trim().to_lowercase();
    // Strip any namespace prefix (e.g. `xml:lang` → `lang`).
    let name = raw_name
        .rsplit_once(':')
        .map(|(_, local)| local.to_string())
        .unwrap_or(raw_name);
    if name.is_empty() {
        return None;
    }

    if i >= chars.len() || chars[i] == ']' {
        let consumed = if i < chars.len() { i + 1 } else { i };
        return Some((AttrSel { name, op: AttrOp::Exists, value: None }, consumed));
    }

    // Operator.
    let op = if chars[i] == '=' {
        i += 1;
        AttrOp::Equals
    } else if i + 1 < chars.len() && chars[i + 1] == '=' {
        let op = match chars[i] {
            '~' => AttrOp::Includes,
            '|' => AttrOp::DashPrefix,
            '^' => AttrOp::Prefix,
            '$' => AttrOp::Suffix,
            '*' => AttrOp::Substring,
            _ => AttrOp::Equals,
        };
        i += 2;
        op
    } else {
        i += 1;
        AttrOp::Equals
    };

    // Optional whitespace before value.
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }

    // Value (quoted or unquoted).
    let value = if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
        let quote = chars[i];
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != quote {
            i += 1;
        }
        let v: String = chars[start..i].iter().collect();
        if i < chars.len() {
            i += 1; // closing quote
        }
        Some(v)
    } else {
        let start = i;
        while i < chars.len() && chars[i] != ']' && !chars[i].is_whitespace() {
            i += 1;
        }
        let v: String = chars[start..i].iter().collect();
        Some(v).filter(|s| !s.is_empty())
    };

    // Skip to `]`.
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    if i < chars.len() {
        i += 1; // consume `]`
    }

    Some((AttrSel { name, op, value }, i))
}

/// Parse a pseudo-class (or pseudo-element) starting after the `:` or `::`.
/// Returns `(PseudoClass, chars_consumed)`.
fn parse_pseudo_class(chars: &[char]) -> (PseudoClass, usize) {
    let mut i = 0;

    // Pseudo-class name.
    while i < chars.len() && (is_ident_char(chars[i]) || chars[i] == '-') {
        i += 1;
    }
    let name: String = chars[..i].iter().collect::<String>().to_lowercase();

    // Optional function argument `(...)`.
    let arg = if i < chars.len() && chars[i] == '(' {
        i += 1; // consume `(`
        let mut depth = 1usize;
        let arg_start = i;
        while i < chars.len() && depth > 0 {
            match chars[i] {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        // chars[arg_start..i-1] is the argument (i-1 points to the closing `)`)
        let inner: String = chars[arg_start..i - 1].iter().collect();
        Some(inner)
    } else {
        None
    };

    let pc = match name.as_str() {
        "not" => {
            let inner = arg.as_deref().unwrap_or("").trim();
            if let Some(parts) = parse_selector_parts(inner) {
                if parts.len() == 1 {
                    PseudoClass::Not(Box::new(parts.into_iter().next().unwrap().simple))
                } else {
                    PseudoClass::Unknown(format!("not({inner})"))
                }
            } else {
                PseudoClass::Unknown("not(?)".to_string())
            }
        }
        "root" => PseudoClass::Root,
        "first-child" => PseudoClass::FirstChild,
        "last-child" => PseudoClass::LastChild,
        "first-of-type" => PseudoClass::FirstOfType,
        "last-of-type" => PseudoClass::LastOfType,
        "nth-child" => {
            let (a, b) = parse_nth(arg.as_deref().unwrap_or(""));
            PseudoClass::NthChild(a, b)
        }
        "hover" => PseudoClass::Hover,
        "focus" => PseudoClass::Focus,
        "active" => PseudoClass::Active,
        "checked" => PseudoClass::Checked,
        "disabled" => PseudoClass::Disabled,
        "enabled" => PseudoClass::Enabled,
        "visited" => PseudoClass::Visited,
        other => PseudoClass::Unknown(other.to_string()),
    };

    (pc, i)
}

/// Parse an+b notation for `:nth-child` etc.
fn parse_nth(s: &str) -> (i32, i32) {
    let s = s.trim();
    match s {
        "even" => return (2, 0),
        "odd" => return (1, 1),
        _ => {}
    }
    if let Some(n_pos) = s.find('n') {
        let a_str = s[..n_pos].trim();
        let a = match a_str {
            "" | "+" => 1,
            "-" => -1,
            other => other.parse().unwrap_or(1),
        };
        let b_str = s[n_pos + 1..].trim();
        let b = if b_str.is_empty() { 0 } else { b_str.parse().unwrap_or(0) };
        (a, b)
    } else {
        (0, s.parse().unwrap_or(0))
    }
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii()
}
