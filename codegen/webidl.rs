use std::collections::BTreeSet;

/// A WebIDL subset: interfaces with constants, regular attributes and
/// regular operations. Anything outside it is a parse error rather than
/// silently ignored, so unsupported IDL can't produce wrong bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct Interface {
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Member {
    Const { name: String, ty: Type, value: String },
    Attribute { name: String, ty: Type, readonly: bool, ce_reactions: bool },
    Operation { name: String, ret: Type, args: Vec<Argument>, ce_reactions: bool },
}

impl Member {
    pub fn name(&self) -> &str {
        match self {
            Member::Const { name, .. }
            | Member::Attribute { name, .. }
            | Member::Operation { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub nullable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    String,
    Boolean,
    Integer { bits: u8, signed: bool },
    Float { double: bool, unrestricted: bool },
    Undefined,
    Interface(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
    pub default: Option<String>,
}

fn tokenize(source: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '/' if chars.peek() == Some(&'/') => {
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
            ':' | '{' | '}' | ';' | '(' | ')' | '[' | ']' | ',' | '=' | '?' => {
                tokens.push(c.to_string())
            }
            '"' => {
                let mut literal = String::from('"');
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(c) => literal.push(c),
                        None => return Err("unterminated string".into()),
                    }
                }
                literal.push('"');
                tokens.push(literal);
            }
            c if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') => {
                let mut word = c.to_string();
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
                {
                    word.push(chars.next().unwrap());
                }
                tokens.push(word);
            }
            _ => return Err(format!("unsupported IDL character: {c}")),
        }
    }
    Ok(tokens)
}

struct Tokens<'a> {
    tokens: std::iter::Peekable<std::slice::Iter<'a, String>>,
}

impl<'a> Tokens<'a> {
    fn peek(&mut self) -> Option<&'a str> {
        self.tokens.peek().map(|t| t.as_str())
    }

    fn next(&mut self) -> Option<&'a str> {
        self.tokens.next().map(|t| t.as_str())
    }

    fn eat(&mut self, expected: &str) -> bool {
        let found = self.peek() == Some(expected);
        if found {
            self.next();
        }
        found
    }

    fn expect(&mut self, expected: &str) -> Result<(), String> {
        match self.next() {
            Some(token) if token == expected => Ok(()),
            token => Err(format!("expected {expected:?}, got {token:?}")),
        }
    }

    fn identifier(&mut self) -> Result<String, String> {
        match self.next() {
            Some(token) if token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') => {
                // A leading underscore escapes a keyword (WebIDL §2.2).
                Ok(token.strip_prefix('_').unwrap_or(token).into())
            }
            token => Err(format!("expected identifier, got {token:?}")),
        }
    }
}

pub fn parse(source: &str) -> Result<Vec<Interface>, String> {
    let tokens = tokenize(source)?;
    let mut tokens = Tokens { tokens: tokens.iter().peekable() };
    let mut interfaces = Vec::new();
    while tokens.peek().is_some() {
        tokens.expect("interface")?;
        let name = tokens.identifier()?;
        let parent = if tokens.eat(":") { Some(tokens.identifier()?) } else { None };
        tokens.expect("{")?;
        let mut members: Vec<Member> = Vec::new();
        while !tokens.eat("}") {
            let member = member(&mut tokens).map_err(|e| format!("{name}: {e}"))?;
            if members.iter().any(|m| m.name() == member.name()) {
                return Err(format!(
                    "{name}: duplicate member {} (overloads are unsupported)",
                    member.name()
                ));
            }
            members.push(member);
        }
        tokens.expect(";")?;
        interfaces.push(Interface { name, parent, members });
    }
    Ok(interfaces)
}

fn member(tokens: &mut Tokens) -> Result<Member, String> {
    let mut ce_reactions = false;
    if tokens.eat("[") {
        loop {
            match tokens.identifier()?.as_str() {
                "CEReactions" => ce_reactions = true,
                other => return Err(format!("unsupported extended attribute [{other}]")),
            }
            if tokens.eat("]") {
                break;
            }
            tokens.expect(",")?;
        }
    }
    if tokens.eat("const") {
        if ce_reactions {
            return Err("[CEReactions] on a constant".into());
        }
        let ty = parse_type(tokens)?;
        if ty.nullable || !matches!(ty.kind, TypeKind::Integer { .. } | TypeKind::Boolean) {
            return Err("constants must be integer or boolean".into());
        }
        let name = tokens.identifier()?;
        tokens.expect("=")?;
        let value = tokens.next().ok_or("missing constant value")?.to_string();
        tokens.expect(";")?;
        return Ok(Member::Const { name, ty, value });
    }
    let readonly = tokens.eat("readonly");
    if readonly || tokens.peek() == Some("attribute") {
        tokens.expect("attribute")?;
        let ty = parse_type(tokens)?;
        if ty.kind == TypeKind::Undefined {
            return Err("attribute of type undefined".into());
        }
        let name = tokens.identifier()?;
        tokens.expect(";")?;
        return Ok(Member::Attribute { name, ty, readonly, ce_reactions });
    }
    let ret = parse_type(tokens)?;
    let name = tokens.identifier()?;
    tokens.expect("(")?;
    let mut args: Vec<Argument> = Vec::new();
    while !tokens.eat(")") {
        if !args.is_empty() {
            tokens.expect(",")?;
        }
        let optional = tokens.eat("optional");
        let ty = parse_type(tokens)?;
        if ty.kind == TypeKind::Undefined {
            return Err("argument of type undefined".into());
        }
        let arg_name = tokens.identifier()?;
        let default = if optional && tokens.eat("=") {
            Some(tokens.next().ok_or("missing default value")?.to_string())
        } else {
            None
        };
        if !optional && args.iter().any(|a| a.optional) {
            return Err(format!("required argument {arg_name} follows an optional one"));
        }
        args.push(Argument { name: arg_name, ty, optional, default });
    }
    tokens.expect(";")?;
    Ok(Member::Operation { name, ret, args, ce_reactions })
}

fn parse_type(tokens: &mut Tokens) -> Result<Type, String> {
    let integer = |bits, signed| TypeKind::Integer { bits, signed };
    let float = |double, unrestricted| TypeKind::Float { double, unrestricted };
    let kind = match tokens.next() {
        Some("DOMString" | "USVString") => TypeKind::String,
        Some("boolean") => TypeKind::Boolean,
        Some("undefined") => TypeKind::Undefined,
        Some("byte") => integer(8, true),
        Some("octet") => integer(8, false),
        Some("short") => integer(16, true),
        Some("long") if tokens.eat("long") => integer(64, true),
        Some("long") => integer(32, true),
        Some("unsigned") => match tokens.next() {
            Some("short") => integer(16, false),
            Some("long") if tokens.eat("long") => integer(64, false),
            Some("long") => integer(32, false),
            token => return Err(format!("expected short or long after unsigned, got {token:?}")),
        },
        Some("float") => float(false, false),
        Some("double") => float(true, false),
        Some("unrestricted") => match tokens.next() {
            Some("float") => float(false, true),
            Some("double") => float(true, true),
            token => {
                return Err(format!("expected float or double after unrestricted, got {token:?}"));
            }
        },
        Some(name) if name.starts_with(|c: char| c.is_ascii_uppercase()) => {
            TypeKind::Interface(name.to_string())
        }
        token => return Err(format!("unsupported type {token:?}")),
    };
    let nullable = tokens.eat("?");
    if nullable && kind == TypeKind::Undefined {
        return Err("undefined cannot be nullable".into());
    }
    Ok(Type { kind, nullable })
}

pub fn inheritance_order(mut pending: Vec<Interface>) -> Result<Vec<Interface>, String> {
    let mut names = BTreeSet::new();
    for interface in &pending {
        if !names.insert(interface.name.clone()) {
            return Err(format!("duplicate interface: {}", interface.name));
        }
    }
    for interface in &pending {
        if let Some(parent) = &interface.parent
            && !names.contains(parent)
        {
            return Err(format!("unknown parent: {parent}"));
        }
        for member in &interface.members {
            let types: Vec<&Type> = match member {
                Member::Const { .. } => Vec::new(),
                Member::Attribute { ty, .. } => vec![ty],
                Member::Operation { ret, args, .. } => {
                    std::iter::once(ret).chain(args.iter().map(|a| &a.ty)).collect()
                }
            };
            for ty in types {
                if let TypeKind::Interface(name) = &ty.kind
                    && !names.contains(name)
                {
                    return Err(format!(
                        "{}.{}: unknown type {name}",
                        interface.name,
                        member.name()
                    ));
                }
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
    fn parses_members() {
        let interfaces = parse(
            r#"interface Node {
                const unsigned short ELEMENT_NODE = 1;
                readonly attribute DOMString? nodeValue;
                [CEReactions] attribute unsigned long long size;
                boolean contains(Node? other);
                [CEReactions] undefined set(DOMString _default,
                    optional unrestricted double x = 1.5, optional boolean flag);
            };"#,
        )
        .unwrap();
        let members = &interfaces[0].members;
        assert_eq!(
            members[0],
            Member::Const {
                name: "ELEMENT_NODE".into(),
                ty: Type { kind: TypeKind::Integer { bits: 16, signed: false }, nullable: false },
                value: "1".into(),
            }
        );
        assert!(matches!(&members[1], Member::Attribute { readonly: true, ty, .. }
            if ty.nullable && ty.kind == TypeKind::String));
        assert!(
            matches!(&members[2], Member::Attribute { readonly: false, ce_reactions: true, ty, .. }
            if ty.kind == TypeKind::Integer { bits: 64, signed: false })
        );
        let Member::Operation { args, ce_reactions, ret, .. } = &members[4] else {
            panic!("expected operation");
        };
        assert!(*ce_reactions && ret.kind == TypeKind::Undefined);
        assert_eq!(args[0].name, "default");
        assert_eq!(args[1].default.as_deref(), Some("1.5"));
        assert!(args[2].optional && args[2].default.is_none());
    }

    #[test]
    fn rejects_invalid_or_unsupported_idl() {
        for source in [
            "interface {};",
            "interface A {}",
            "/* unfinished",
            "partial interface A {};",
            "interface A {}; garbage",
            "interface A { [SameObject] readonly attribute long x; };",
            "interface A { any x(); };",
            "interface A { undefined f(long a); undefined f(); };",
            "interface A { undefined f(optional long a, long b); };",
            "interface A { const DOMString X = \"x\"; };",
            "interface A { attribute undefined x; };",
            "interface A { undefined f(long... rest); };",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
        for source in [
            "interface A {}; interface A {};",
            "interface A : Missing {};",
            "interface A : B {}; interface B : A {};",
            "interface A { attribute Missing x; };",
        ] {
            assert!(
                inheritance_order(parse(source).unwrap()).is_err(),
                "{source}"
            );
        }
    }
}
