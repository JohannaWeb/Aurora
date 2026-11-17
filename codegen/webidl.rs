use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub parent: Option<String>,
}

/// P2.1 accepts member-free interface declarations; unsupported IDL is an error.
pub fn parse(source: &str) -> Result<Vec<Interface>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err("unterminated comment".into());
                }
            }
            ':' | '{' | '}' | ';' => tokens.push(c.to_string()),
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = c.to_string();
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    name.push(chars.next().unwrap());
                }
                tokens.push(name);
            }
            _ => return Err(format!("unsupported IDL character: {c}")),
        }
    }
    let mut tokens = tokens.iter().map(String::as_str).peekable();
    let mut interfaces = Vec::new();
    while tokens.peek().is_some() {
        expect(&mut tokens, "interface")?;
        let name = identifier(&mut tokens)?;
        let parent = if tokens.peek() == Some(&":") {
            tokens.next();
            Some(identifier(&mut tokens)?)
        } else {
            None
        };
        expect(&mut tokens, "{")?;
        expect(&mut tokens, "}")?;
        expect(&mut tokens, ";")?;
        interfaces.push(Interface { name, parent });
    }
    Ok(interfaces)
}

fn expect<'a>(tokens: &mut impl Iterator<Item = &'a str>, expected: &str) -> Result<(), String> {
    match tokens.next() {
        Some(token) if token == expected => Ok(()),
        token => Err(format!("expected {expected:?}, got {token:?}")),
    }
}

fn identifier<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<String, String> {
    match tokens.next() {
        Some(token) if token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') => {
            Ok(token.into())
        }
        token => Err(format!("expected interface name, got {token:?}")),
    }
}

pub fn inheritance_order(mut pending: Vec<Interface>) -> Result<Vec<Interface>, String> {
    let mut names = BTreeSet::new();
    for interface in &pending {
        if !names.insert(interface.name.clone()) {
            return Err(format!("duplicate interface: {}", interface.name));
        }
    }
    for interface in &pending {
        if let Some(parent) = &interface.parent {
            if !names.contains(parent) {
                return Err(format!("unknown parent: {parent}"));
            }
        }
    }
    let mut ordered: Vec<Interface> = Vec::new();
    while !pending.is_empty() {
        let index = pending
            .iter()
            .position(|i| {
                i.parent
                    .as_ref()
                    .is_none_or(|parent| ordered.iter().any(|i| &i.name == parent))
            })
            .ok_or("interface inheritance cycle")?;
        ordered.push(pending.remove(index));
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_and_orders_cross_file_inheritance() {
        let mut interfaces =
            parse("// child\ninterface Element : Node {}; /* root later */").unwrap();
        interfaces
            .extend(parse("interface Node : EventTarget {}; interface EventTarget {};").unwrap());
        assert_eq!(
            inheritance_order(interfaces)
                .unwrap()
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            ["EventTarget", "Node", "Element"]
        );
    }

    #[test]
    fn rejects_invalid_or_unsupported_idl() {
        for source in [
            "interface {};",
            "interface A {}",
            "interface A { attribute long x; };",
            "/* unfinished",
            "partial interface A {};",
            "interface A {}; garbage",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
        for source in [
            "interface A {}; interface A {};",
            "interface A : Missing {};",
            "interface A : B {}; interface B : A {};",
        ] {
            assert!(
                inheritance_order(parse(source).unwrap()).is_err(),
                "{source}"
            );
        }
    }
}
