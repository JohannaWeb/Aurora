use super::*;

pub(in crate::js_boa) struct SimpleSel {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<(String, Option<String>)>,
    universal: bool,
}

pub(in crate::js_boa) fn parse_simple(s: &str) -> Option<SimpleSel> {
    let mut sel = SimpleSel {
        tag: None,
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
        universal: false,
    };
    let mut chars = s.chars().peekable();
    let mut acc = String::new();
    let mut mode = 't'; // 't' tag, 'i' id, 'c' class
    let flush = |mode: char, acc: &mut String, sel: &mut SimpleSel| {
        if acc.is_empty() {
            return;
        }
        match mode {
            't' => {
                if acc == "*" {
                    sel.universal = true;
                } else {
                    sel.tag = Some(acc.to_lowercase());
                }
            }
            'i' => sel.id = Some(std::mem::take(acc)),
            'c' => sel.classes.push(std::mem::take(acc)),
            _ => {}
        }
        acc.clear();
    };
    while let Some(&ch) = chars.peek() {
        match ch {
            '#' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                mode = 'i';
            }
            '.' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                mode = 'c';
            }
            '[' => {
                flush(mode, &mut acc, &mut sel);
                chars.next();
                let mut attr = String::new();
                let mut val: Option<String> = None;
                let mut in_val = false;
                while let Some(c) = chars.next() {
                    if c == ']' {
                        break;
                    } else if c == '=' {
                        in_val = true;
                        val = Some(String::new());
                    } else if in_val {
                        if c == '"' || c == '\'' {
                            continue;
                        }
                        val.as_mut().unwrap().push(c);
                    } else if c == '~' || c == '|' || c == '^' || c == '$' || c == '*' {
                        // Treat prefix operators as "present with value" — lossy but safe.
                        continue;
                    } else {
                        attr.push(c);
                    }
                }
                sel.attrs.push((attr, val));
            }
            ':' => {
                // Pseudo-class: skip everything up to next space/comma/combinator.
                chars.next();
                // Skip nested `(...)`.
                let mut depth = 0;
                while let Some(&c) = chars.peek() {
                    if c == '(' {
                        depth += 1;
                        chars.next();
                    } else if c == ')' {
                        if depth > 0 {
                            depth -= 1;
                        }
                        chars.next();
                    } else if depth == 0
                        && (c == ' '
                            || c == ','
                            || c == '>'
                            || c == '+'
                            || c == '~'
                            || c == '.'
                            || c == '#'
                            || c == '[')
                    {
                        break;
                    } else {
                        chars.next();
                    }
                }
            }
            _ if ch.is_whitespace() => {
                break;
            }
            _ => {
                acc.push(ch);
                chars.next();
            }
        }
    }
    flush(mode, &mut acc, &mut sel);
    Some(sel)
}

pub(in crate::js_boa) fn simple_matches(node: &NodePtr, sel: &SimpleSel) -> bool {
    let b = node.borrow();
    let el = match &*b {
        Node::Element(e) => e,
        _ => return false,
    };
    if let Some(t) = &sel.tag {
        if !el.tag_name.eq_ignore_ascii_case(t) {
            return false;
        }
    }
    if let Some(id) = &sel.id {
        if el.attributes.get("id").map(|s| s.as_str()) != Some(id.as_str()) {
            return false;
        }
    }
    for cls in &sel.classes {
        let present = el
            .attributes
            .get("class")
            .map(|s| s.split_whitespace().any(|c| c == cls))
            .unwrap_or(false);
        if !present {
            return false;
        }
    }
    for (k, v) in &sel.attrs {
        match v {
            Some(val) => {
                if el.attributes.get(k).map(|s| s.as_str()) != Some(val.as_str()) {
                    return false;
                }
            }
            None => {
                if !el.attributes.contains_key(k) {
                    return false;
                }
            }
        }
    }
    true
}
