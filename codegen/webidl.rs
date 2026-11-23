use std::collections::BTreeSet;

/// A WebIDL subset: interfaces with constants, regular attributes and
/// regular operations, plus dictionaries. Anything outside it is a parse
/// error rather than silently ignored, so unsupported IDL can't produce
/// wrong bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct Interface {
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<Member>,
    /// `interface mixin`: never installed itself, only through `includes`.
    pub mixin: bool,
    /// Mixins this interface includes, resolved by [`resolve_mixins`].
    pub includes: Vec<Interface>,
}

/// `dictionary Name [: Parent] { ... };`
#[derive(Clone, Debug, PartialEq)]
pub struct Dictionary {
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<DictionaryMember>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DictionaryMember {
    pub name: String,
    pub ty: Type,
    pub required: bool,
    pub default: Option<String>,
}

/// Everything one IDL source declares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Definitions {
    pub interfaces: Vec<Interface>,
    pub includes: Vec<Includes>,
    pub dictionaries: Vec<Dictionary>,
}

/// `A includes B;`
#[derive(Clone, Debug, PartialEq)]
pub struct Includes {
    pub interface: String,
    pub mixin: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Member {
    Const { name: String, ty: Type, value: String },
    /// `stringifier` also makes `toString()` return the attribute.
    Attribute { name: String, ty: Type, readonly: bool, ce_reactions: bool, stringifier: bool },
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
    /// `[LegacyNullToEmptyString]`: `null` converts to `""`, not `"null"`.
    pub null_to_empty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    String,
    Boolean,
    Integer { bits: u8, signed: bool },
    Float { double: bool, unrestricted: bool },
    Undefined,
    Interface(String),
    /// `NodeList`/`HTMLCollection`, returned as a fresh JS array until those
    /// interfaces exist. Only valid as a non-null read-only attribute's or
    /// operation result's type.
    NodeArray,
    /// A dictionary type, only as an operation argument. The parser reads
    /// every other capitalized name as an interface; [`resolve_dictionaries`]
    /// turns the ones naming a dictionary into this.
    Dictionary(String),
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

/// Interfaces and mixins only; `includes` statements are an error here.
#[allow(dead_code)] // build.rs reads includes through parse_definitions.
pub fn parse(source: &str) -> Result<Vec<Interface>, String> {
    let definitions = parse_definitions(source)?;
    match definitions.includes.first() {
        Some(i) => Err(format!("unexpected includes statement for {}", i.interface)),
        None => Ok(definitions.interfaces),
    }
}

pub fn parse_definitions(source: &str) -> Result<Definitions, String> {
    let tokens = tokenize(source)?;
    let mut tokens = Tokens { tokens: tokens.iter().peekable() };
    let mut interfaces = Vec::new();
    let mut includes = Vec::new();
    let mut dictionaries = Vec::new();
    while tokens.peek().is_some() {
        if tokens.eat("dictionary") {
            dictionaries.push(dictionary(&mut tokens)?);
            continue;
        }
        if tokens.peek() != Some("interface") {
            let interface = tokens.identifier()?;
            tokens.expect("includes")?;
            let mixin = tokens.identifier()?;
            tokens.expect(";")?;
            includes.push(Includes { interface, mixin });
            continue;
        }
        tokens.expect("interface")?;
        let mixin = tokens.eat("mixin");
        let name = tokens.identifier()?;
        let parent = if !mixin && tokens.eat(":") { Some(tokens.identifier()?) } else { None };
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
        interfaces.push(Interface { name, parent, members, mixin, includes: Vec::new() });
    }
    Ok(Definitions { interfaces, includes, dictionaries })
}

/// The body of `dictionary Name [: Parent] { ... };`, after the keyword.
fn dictionary(tokens: &mut Tokens) -> Result<Dictionary, String> {
    let name = tokens.identifier()?;
    let parent = if tokens.eat(":") { Some(tokens.identifier()?) } else { None };
    tokens.expect("{")?;
    let mut members: Vec<DictionaryMember> = Vec::new();
    while !tokens.eat("}") {
        let required = tokens.eat("required");
        let ty = parse_type(tokens).map_err(|e| format!("{name}: {e}"))?;
        if matches!(ty.kind, TypeKind::Undefined | TypeKind::NodeArray) {
            return Err(format!("{name}: member of type {:?}", ty.kind));
        }
        let member = tokens.identifier()?;
        let default = if tokens.eat("=") { Some(default_literal(tokens)?) } else { None };
        if required && default.is_some() {
            return Err(format!("{name}.{member}: required member with a default"));
        }
        tokens.expect(";")?;
        if members.iter().any(|m| m.name == member) {
            return Err(format!("{name}: duplicate member {member}"));
        }
        members.push(DictionaryMember { name: member, ty, required, default });
    }
    tokens.expect(";")?;
    Ok(Dictionary { name, parent, members })
}

/// A default value: one literal token, or `{}` for a dictionary.
fn default_literal(tokens: &mut Tokens) -> Result<String, String> {
    match tokens.next().ok_or("missing default value")? {
        "{" => {
            tokens.expect("}")?;
            Ok("{}".into())
        }
        literal => Ok(literal.to_string()),
    }
}

/// Turns interface-typed operation arguments and dictionary members that
/// name a dictionary into [`TypeKind::Dictionary`], and checks the
/// dictionaries: known parents, no cycles, no shadowed inherited members,
/// and dictionaries only where they are supported (non-nullable operation
/// arguments, optional ones defaulting to `{}`).
pub fn resolve_dictionaries(
    interfaces: &mut [Interface],
    dictionaries: &mut [Dictionary],
) -> Result<(), String> {
    let names: BTreeSet<String> = dictionaries.iter().map(|d| d.name.clone()).collect();
    if names.len() != dictionaries.len() {
        return Err("duplicate dictionary name".into());
    }
    let is_dictionary = |ty: &Type| matches!(&ty.kind, TypeKind::Interface(n) if names.contains(n));
    for dictionary in dictionaries.iter() {
        let mut seen: Vec<&str> = dictionary.members.iter().map(|m| m.name.as_str()).collect();
        let mut parent = dictionary.parent.as_deref();
        let mut depth = 0;
        while let Some(name) = parent {
            let found = dictionaries
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| format!("{}: unknown parent dictionary {name}", dictionary.name))?;
            depth += 1;
            if depth > dictionaries.len() {
                return Err(format!("{}: dictionary inheritance cycle", dictionary.name));
            }
            for member in &found.members {
                if seen.contains(&member.name.as_str()) {
                    return Err(format!("{}: member {} shadows {name}'s", dictionary.name, member.name));
                }
                seen.push(&member.name);
            }
            parent = found.parent.as_deref();
        }
        if let Some(member) = dictionary.members.iter().find(|m| is_dictionary(&m.ty)) {
            return Err(format!("{}.{}: nested dictionaries are unsupported", dictionary.name, member.name));
        }
    }
    for interface in interfaces.iter_mut() {
        for member in &mut interface.members {
            match member {
                Member::Const { ty, name, .. } | Member::Attribute { ty, name, .. } if is_dictionary(ty) => {
                    return Err(format!("{}.{name}: a dictionary is only valid as an argument", interface.name));
                }
                Member::Operation { name, ret, args, .. } => {
                    if is_dictionary(ret) {
                        return Err(format!("{}.{name}: dictionary return types are unsupported", interface.name));
                    }
                    for arg in args.iter_mut().filter(|a| is_dictionary(&a.ty)) {
                        if arg.ty.nullable {
                            return Err(format!("{}.{name}: a dictionary cannot be nullable", interface.name));
                        }
                        if arg.optional && arg.default.as_deref() != Some("{}") {
                            return Err(format!("{}.{name}: optional dictionary {} needs = {{}}", interface.name, arg.name));
                        }
                        if let TypeKind::Interface(dictionary) = &arg.ty.kind {
                            arg.ty.kind = TypeKind::Dictionary(dictionary.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Attaches each included mixin to its interface and drops the mixins from
/// the list. Unknown names, double inclusion and member clashes are errors.
pub fn resolve_mixins(
    definitions: Vec<Interface>,
    includes: &[Includes],
) -> Result<Vec<Interface>, String> {
    let (mixins, mut interfaces): (Vec<_>, Vec<_>) =
        definitions.into_iter().partition(|i| i.mixin);
    for include in includes {
        let mixin = mixins
            .iter()
            .find(|m| m.name == include.mixin)
            .ok_or_else(|| format!("unknown mixin: {}", include.mixin))?;
        let interface = interfaces
            .iter_mut()
            .find(|i| i.name == include.interface)
            .ok_or_else(|| format!("unknown interface: {}", include.interface))?;
        if interface.includes.iter().any(|m| m.name == mixin.name) {
            return Err(format!("{} includes {} twice", interface.name, mixin.name));
        }
        let names = interface.members.iter().chain(interface.includes.iter().flat_map(|m| &m.members));
        let names: Vec<&str> = names.map(Member::name).collect();
        if let Some(clash) = mixin.members.iter().find(|m| names.contains(&m.name())) {
            return Err(format!("{}: {} from {} clashes", interface.name, clash.name(), mixin.name));
        }
        interface.includes.push(mixin.clone());
    }
    Ok(interfaces)
}

fn member(tokens: &mut Tokens) -> Result<Member, String> {
    let mut ce_reactions = false;
    if tokens.eat("[") {
        loop {
            match tokens.identifier()?.as_str() {
                "CEReactions" => ce_reactions = true,
                // Only says the result is a fresh object; nothing to generate.
                "NewObject" => {}
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
    let stringifier = tokens.eat("stringifier");
    let readonly = tokens.eat("readonly");
    if stringifier && tokens.peek() != Some("attribute") && !readonly {
        return Err("only stringifier attributes are supported".into());
    }
    if readonly || tokens.peek() == Some("attribute") {
        tokens.expect("attribute")?;
        let ty = parse_type(tokens)?;
        if ty.kind == TypeKind::Undefined {
            return Err("attribute of type undefined".into());
        }
        if ty.kind == TypeKind::NodeArray && (!readonly || ty.nullable) {
            return Err("NodeList/HTMLCollection only as a non-null readonly attribute".into());
        }
        if stringifier && ty.kind != TypeKind::String {
            return Err("a stringifier attribute must be a string".into());
        }
        let name = tokens.identifier()?;
        tokens.expect(";")?;
        return Ok(Member::Attribute { name, ty, readonly, ce_reactions, stringifier });
    }
    let ret = parse_type(tokens)?;
    if ret.kind == TypeKind::NodeArray && ret.nullable {
        return Err("NodeList/HTMLCollection results cannot be nullable".into());
    }
    let name = tokens.identifier()?;
    tokens.expect("(")?;
    let mut args: Vec<Argument> = Vec::new();
    while !tokens.eat(")") {
        if !args.is_empty() {
            tokens.expect(",")?;
        }
        let optional = tokens.eat("optional");
        let ty = parse_type(tokens)?;
        if matches!(ty.kind, TypeKind::Undefined | TypeKind::NodeArray) {
            return Err(format!("argument of type {:?}", ty.kind));
        }
        let arg_name = tokens.identifier()?;
        let default = if optional && tokens.eat("=") { Some(default_literal(tokens)?) } else { None };
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
    let mut null_to_empty = false;
    if tokens.eat("[") {
        match tokens.identifier()?.as_str() {
            "LegacyNullToEmptyString" => null_to_empty = true,
            other => return Err(format!("unsupported type extended attribute [{other}]")),
        }
        tokens.expect("]")?;
    }
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
        Some("NodeList" | "HTMLCollection") => TypeKind::NodeArray,
        Some(name) if name.starts_with(|c: char| c.is_ascii_uppercase()) => {
            TypeKind::Interface(name.to_string())
        }
        token => return Err(format!("unsupported type {token:?}")),
    };
    let nullable = tokens.eat("?");
    if nullable && kind == TypeKind::Undefined {
        return Err("undefined cannot be nullable".into());
    }
    if null_to_empty && (nullable || kind != TypeKind::String) {
        return Err("[LegacyNullToEmptyString] needs a non-nullable string type".into());
    }
    Ok(Type { kind, nullable, null_to_empty })
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
        for member in interface.members.iter().chain(interface.includes.iter().flat_map(|m| &m.members)) {
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
                attribute [LegacyNullToEmptyString] DOMString data;
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
                ty: Type {
                    kind: TypeKind::Integer { bits: 16, signed: false },
                    nullable: false,
                    null_to_empty: false,
                },
                value: "1".into(),
            }
        );
        assert!(matches!(&members[1], Member::Attribute { readonly: true, ty, .. }
            if ty.nullable && ty.kind == TypeKind::String));
        assert!(
            matches!(&members[2], Member::Attribute { readonly: false, ce_reactions: true, ty, .. }
            if ty.kind == TypeKind::Integer { bits: 64, signed: false })
        );
        assert!(matches!(&members[3], Member::Attribute { ty, .. }
            if ty.null_to_empty && ty.kind == TypeKind::String));
        let Member::Operation { args, ce_reactions, ret, .. } = &members[5] else {
            panic!("expected operation");
        };
        assert!(*ce_reactions && ret.kind == TypeKind::Undefined);
        assert_eq!(args[0].name, "default");
        assert_eq!(args[1].default.as_deref(), Some("1.5"));
        assert!(args[2].optional && args[2].default.is_none());
    }

    #[test]
    fn resolves_mixins_into_including_interfaces() {
        let definitions = parse_definitions(
            "interface mixin M { readonly attribute HTMLCollection children; };
             interface A { readonly attribute long x; }; A includes M;",
        )
        .unwrap();
        let interfaces = resolve_mixins(definitions.interfaces, &definitions.includes).unwrap();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].includes[0].name, "M");
        for (source, error) in [
            ("interface A {}; A includes Missing;", "unknown mixin"),
            ("interface mixin M {}; B includes M;", "unknown interface"),
            ("interface mixin M { attribute long x; }; interface A { attribute long x; }; A includes M;", "clashes"),
        ] {
            let definitions = parse_definitions(source).unwrap();
            let error_text = resolve_mixins(definitions.interfaces, &definitions.includes).unwrap_err();
            assert!(error_text.contains(error), "{source}");
        }
        for source in [
            "interface A { attribute NodeList x; };",
            "interface A { NodeList? f(); };",
            "interface A { undefined f(NodeList x); };",
            "interface A {}; A includes M;",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }

    fn resolved(source: &str) -> Result<Definitions, String> {
        let mut definitions = parse_definitions(source)?;
        resolve_dictionaries(&mut definitions.interfaces, &mut definitions.dictionaries)?;
        Ok(definitions)
    }

    #[test]
    fn parses_and_resolves_dictionaries() {
        let definitions = resolved(
            "dictionary Base { boolean capture = false; };
             dictionary Options : Base { required DOMString name; Node? target = null; long count; };
             interface A { undefined f(DOMString x, optional Options options = {}); };",
        )
        .unwrap();
        let options = &definitions.dictionaries[1];
        assert_eq!(options.parent.as_deref(), Some("Base"));
        assert!(options.members[0].required);
        assert_eq!(options.members[1].default.as_deref(), Some("null"));
        assert_eq!(options.members[2].default, None);
        let Member::Operation { args, .. } = &definitions.interfaces[0].members[0] else {
            panic!("expected an operation");
        };
        assert_eq!(args[1].ty.kind, TypeKind::Dictionary("Options".into()));
        assert_eq!(args[1].default.as_deref(), Some("{}"));
        assert_eq!(args[0].ty.kind, TypeKind::String);

        for (source, error) in [
            ("dictionary D : Missing {};", "unknown parent"),
            ("dictionary D : E {}; dictionary E : D {};", "cycle"),
            ("dictionary B { long x; }; dictionary D : B { long x; };", "shadows"),
            ("dictionary D {}; dictionary D {};", "duplicate dictionary"),
            ("dictionary D {}; dictionary E { D inner; };", "nested"),
            ("dictionary D {}; interface A { attribute D x; };", "only valid as an argument"),
            ("dictionary D {}; interface A { D f(); };", "return"),
            ("dictionary D {}; interface A { undefined f(D? x); };", "nullable"),
            ("dictionary D {}; interface A { undefined f(optional D x); };", "= {}"),
        ] {
            assert!(resolved(source).unwrap_err().contains(error), "{source}");
        }
        for source in [
            "dictionary D { required long x = 1; };",
            "dictionary D { long x; long x; };",
            "dictionary D { undefined x; };",
            "dictionary D { long x }",
        ] {
            assert!(parse_definitions(source).is_err(), "{source}");
        }
    }

    #[test]
    fn parses_stringifier_attributes() {
        let interfaces = parse("interface A { stringifier attribute USVString href; };").unwrap();
        assert!(matches!(&interfaces[0].members[0], Member::Attribute { stringifier: true, .. }));
        for source in [
            "interface A { stringifier attribute long x; };",
            "interface A { stringifier DOMString f(); };",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
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
            "interface A { attribute [LegacyNullToEmptyString] DOMString? x; };",
            "interface A { attribute [LegacyNullToEmptyString] long x; };",
            "interface A { attribute [Clamp] long x; };",
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
