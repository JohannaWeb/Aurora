//! Emits the Rust side of interface members: a trait per interface that the
//! hand-written `Dom` type implements, plus V8 callbacks that brand-check
//! `this`, convert arguments per WebIDL and convert the result back.

use super::webidl::{Argument, Dictionary, Interface, Member, Type, TypeKind};
use std::fmt::Write;

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "box", "break", "const", "continue", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe",
    "use", "where", "while", "yield",
];

/// `compareDocumentPosition` → `compare_document_position`,
/// `innerHTML` → `inner_html`.
pub fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev_lower = chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit();
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev_lower || (next_lower && chars[i - 1].is_ascii_uppercase()) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    if KEYWORDS.contains(&out.as_str()) {
        out.insert_str(0, "r#");
    }
    out
}

fn base_type(kind: &TypeKind) -> String {
    match kind {
        TypeKind::String => "String".into(),
        TypeKind::Boolean => "bool".into(),
        TypeKind::Integer { bits, signed } => format!("{}{bits}", if *signed { "i" } else { "u" }),
        TypeKind::Float { double, .. } => if *double { "f64" } else { "f32" }.into(),
        TypeKind::Undefined => "()".into(),
        TypeKind::Interface(_) => "NodePtr".into(),
        TypeKind::NodeArray => "Vec<NodePtr>".into(),
        TypeKind::Dictionary(name) => name.clone(),
    }
}

fn rust_type(ty: &Type) -> String {
    if ty.nullable {
        format!("Option<{}>", base_type(&ty.kind))
    } else {
        base_type(&ty.kind)
    }
}

fn arg_type(arg: &Argument) -> String {
    // An optional argument without a default can be absent: `Option`. When it
    // is also nullable, null and absent both map to `None`.
    if arg.optional && arg.default.is_none() && !arg.ty.nullable {
        format!("Option<{}>", base_type(&arg.ty.kind))
    } else {
        rust_type(&arg.ty)
    }
}

/// Expression converting the JS value `v` to the (non-nullable) Rust type,
/// evaluating to `Fallible<T>`.
fn convert(kind: &TypeKind) -> String {
    match kind {
        TypeKind::String => "conv::string(scope, v)".into(),
        TypeKind::Boolean => "Ok(v.boolean_value(scope))".into(),
        TypeKind::Integer { bits, signed } => {
            format!("conv::integer(scope, v, {bits}, {signed}).map(|x| x as {})", base_type(kind))
        }
        TypeKind::Float { double, unrestricted } => {
            format!("conv::float(scope, v, {double}, {unrestricted}).map(|x| x as {})", base_type(kind))
        }
        TypeKind::Interface(name) => format!("conv::interface(scope, registry, v, {name:?})"),
        TypeKind::Dictionary(name) => format!("convert_dictionary_{name}(scope, registry, v)"),
        TypeKind::Undefined | TypeKind::NodeArray => unreachable!("rejected by the parser"),
    }
}

fn default_value(arg: &Argument, literal: &str) -> String {
    match (&arg.ty.kind, literal) {
        (_, "null") => {
            assert!(arg.ty.nullable, "{}: null default on non-nullable type", arg.name);
            "None".into()
        }
        (TypeKind::String, s) if s.starts_with('"') => wrap(arg, &format!("String::from({s})")),
        (TypeKind::Boolean, "true" | "false") => wrap(arg, literal),
        (TypeKind::Integer { .. } | TypeKind::Float { .. }, n) => {
            let n: f64 = n.parse().unwrap_or_else(|_| panic!("{}: bad default {n}", arg.name));
            wrap(arg, &format!("{n:?} as {}", base_type(&arg.ty.kind)))
        }
        (_, other) => panic!("{}: unsupported default {other}", arg.name),
    }
}

fn wrap(arg: &Argument, value: &str) -> String {
    if arg.ty.nullable { format!("Some({value})") } else { value.into() }
}

/// Statement binding `name` to the converted value of `v`.
fn convert_arg(arg: &Argument, name: &str) -> String {
    let conversion = convert(&arg.ty.kind);
    // A dictionary converts `undefined` to its defaults, which is also what
    // the only default the parser accepts for one (`{}`) means.
    if matches!(arg.ty.kind, TypeKind::Dictionary(_)) {
        return format!("let {name}: {} = {conversion}?;\n", arg_type(arg));
    }
    let value = if arg.ty.nullable {
        format!("if v.is_null_or_undefined() {{ None }} else {{ Some({conversion}?) }}")
    } else if arg.ty.null_to_empty {
        format!("if v.is_null() {{ String::new() }} else {{ {conversion}? }}")
    } else if arg.optional && arg.default.is_none() {
        format!("if v.is_undefined() {{ None }} else {{ Some({conversion}?) }}")
    } else {
        format!("{conversion}?")
    };
    let value = match &arg.default {
        Some(default) => format!(
            "if v.is_undefined() {{ {} }} else {{ {value} }}",
            default_value(arg, default)
        ),
        None => value,
    };
    format!("let {name}: {} = {value};\n", arg_type(arg))
}

/// One callback: an outer V8 entry point and a typed inner function.
#[allow(clippy::too_many_arguments)]
fn callback(
    out: &mut String,
    interface: &str,
    trait_name: &str,
    fn_name: &str,
    trait_call: &str,
    args: &[Argument],
    ret: &str,
    ce_reactions: bool,
    error_context: &str,
) {
    let required = args.iter().filter(|a| !a.optional).count();
    let mut body = format!("let this = unwrap_this(scope, registry, args.this(), {interface:?})?;\n");
    if required > 0 {
        writeln!(body, "check_arity(args, {required}, {error_context:?})?;").unwrap();
    }
    let mut names = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let name = format!("arg{i}");
        writeln!(body, "let v = args.get({i});").unwrap();
        body.push_str(&convert_arg(arg, &name));
        names.push(name);
    }
    if ce_reactions {
        body.push_str("let _reactions = CeReactionsGuard::new(scope, registry);\n");
    }
    write!(
        body,
        "<Dom as {trait_name}Members>::{trait_call}(scope, registry, &this{})",
        names.iter().map(|n| format!(", {n}")).collect::<String>()
    )
    .unwrap();
    writeln!(
        out,
        "fn {fn_name}_impl<'s>(scope: &mut v8::PinScope<'s, '_>, registry: &Rc<NodeRegistry>, \
         args: &v8::FunctionCallbackArguments<'s>) -> Fallible<{ret}> {{\n{body}\n}}\n\
         fn {fn_name}<'s>(scope: &mut v8::PinScope<'s, '_>, args: v8::FunctionCallbackArguments<'s>, \
         mut rv: v8::ReturnValue) {{\n\
         let Some(registry) = scope.get_current_context().get_slot::<NodeRegistry>() else {{ return; }};\n\
         let result = {fn_name}_impl(scope, &registry, &args);\n\
         finish(scope, &registry, result, &mut rv);\n}}"
    )
    .unwrap();
}

/// Trait, callbacks and the template installer for one interface. Members
/// of included mixins get callbacks brand-checked against this interface
/// that call the mixin's trait, which [`mixin_trait`] emits once.
pub fn members(interface: &Interface) -> String {
    let name = &interface.name;
    let mut out = String::new();
    let mut install = format!(
        "#[allow(non_snake_case)]\nfn install_members_{name}(scope: &mut v8::PinScope<'_, '_>, \
         template: v8::Local<v8::FunctionTemplate>) {{\n\
         let proto = template.prototype_template(scope);\n"
    );
    let trait_fns = emit(&mut out, &mut install, name, name, &interface.members);
    for mixin in &interface.includes {
        emit(&mut out, &mut install, name, &mixin.name, &mixin.members);
    }
    install.push_str("let _ = (scope, proto);\n}\n");
    if !trait_fns.is_empty() {
        writeln!(out, "pub(super) trait {name}Members {{\n{trait_fns}}}").unwrap();
    }
    out.push_str(&install);
    out
}

/// The `<Mixin>Members` trait that every including interface's callbacks call.
pub fn mixin_trait(mixin: &Interface) -> String {
    let trait_fns = emit(&mut String::new(), &mut String::new(), &mixin.name, &mixin.name, &mixin.members);
    format!("pub(super) trait {}Members {{\n{trait_fns}}}\n", mixin.name)
}

/// Callbacks and install statements for `members`, declared by `owner` (the
/// interface itself or a mixin) and installed on `interface`. Returns the
/// owner's trait items.
fn emit(out: &mut String, install: &mut String, interface: &str, owner: &str, members: &[Member]) -> String {
    let name = interface;
    let prefix = if owner == interface {
        snake_case(interface)
    } else {
        format!("{}_{}", snake_case(interface), snake_case(owner))
    };
    let mut trait_fns = String::new();
    let receiver = "scope: &mut v8::PinScope<'_, '_>, registry: &Rc<NodeRegistry>, this: &NodePtr";
    for member in members {
        match member {
            Member::Const { name: const_name, ty, value } => {
                let value = match ty.kind {
                    TypeKind::Boolean => format!("v8::Boolean::new(scope, {value})"),
                    _ => {
                        let n = if let Some(hex) = value.strip_prefix("0x") {
                            i64::from_str_radix(hex, 16).unwrap()
                        } else {
                            value.parse::<i64>().unwrap()
                        };
                        format!("v8::Number::new(scope, {n}_f64)")
                    }
                };
                writeln!(
                    install,
                    "define_constant(scope, template, proto, {const_name:?}, {value}.into());"
                )
                .unwrap();
            }
            Member::Attribute { name: attr, ty, readonly, ce_reactions } => {
                let getter = format!("{prefix}_get_{}", snake_case(attr).trim_start_matches("r#"));
                let rust_name = snake_case(attr);
                writeln!(trait_fns, "fn {rust_name}({receiver}) -> Fallible<{}>;", rust_type(ty))
                    .unwrap();
                let context = format!("Failed to read the '{attr}' property from '{name}'");
                callback(out, name, owner, &getter, &rust_name, &[], &rust_type(ty), false, &context);
                let setter = if *readonly {
                    "None".to_string()
                } else {
                    let setter = format!("{prefix}_set_{}", rust_name.trim_start_matches("r#"));
                    let set_name = format!("set_{}", rust_name.trim_start_matches("r#"));
                    writeln!(trait_fns, "fn {set_name}({receiver}, value: {}) -> Fallible<()>;", rust_type(ty))
                        .unwrap();
                    let value = Argument { name: "value".into(), ty: ty.clone(), optional: false, default: None };
                    let context = format!("Failed to set the '{attr}' property on '{name}'");
                    callback(out, name, owner, &setter, &set_name, &[value], "()", *ce_reactions, &context);
                    format!("Some(accessor(scope, {setter}, \"set {attr}\", 1))")
                };
                writeln!(
                    install,
                    "let getter = accessor(scope, {getter}, \"get {attr}\", 0);\n\
                     proto.set_accessor_property(v8_str(scope, {attr:?}).into(), Some(getter), {setter}, \
                     v8::PropertyAttribute::NONE);"
                )
                .unwrap();
            }
            Member::Operation { name: op, ret, args, ce_reactions } => {
                let rust_name = snake_case(op);
                let fn_name = format!("{prefix}_{}", rust_name.trim_start_matches("r#"));
                let params: String = args
                    .iter()
                    .map(|a| format!(", {}: {}", snake_case(&a.name), arg_type(a)))
                    .collect();
                writeln!(trait_fns, "fn {rust_name}({receiver}{params}) -> Fallible<{}>;", rust_type(ret))
                    .unwrap();
                let context = format!("Failed to execute '{op}' on '{name}'");
                callback(out, name, owner, &fn_name, &rust_name, args, &rust_type(ret), *ce_reactions, &context);
                let length = args.iter().filter(|a| !a.optional).count();
                writeln!(
                    install,
                    "let function = accessor(scope, {fn_name}, {op:?}, {length});\n\
                     proto.set_with_attr(v8_str(scope, {op:?}).into(), function.into(), \
                     v8::PropertyAttribute::NONE);"
                )
                .unwrap();
            }
        }
    }
    trait_fns
}

/// A dictionary's members in WebIDL order: inherited dictionaries' first,
/// each dictionary's own sorted lexicographically.
fn dictionary_members<'d>(dictionary: &'d Dictionary, all: &'d [Dictionary]) -> Vec<&'d super::webidl::DictionaryMember> {
    let mut members = match &dictionary.parent {
        Some(parent) => dictionary_members(all.iter().find(|d| &d.name == parent).unwrap(), all),
        None => Vec::new(),
    };
    let mut own: Vec<_> = dictionary.members.iter().collect();
    own.sort_by(|a, b| a.name.cmp(&b.name));
    members.extend(own);
    members
}

/// A struct for the dictionary and the function converting a JS value to it
/// (<https://webidl.spec.whatwg.org/#js-dictionary>). A member that is not
/// required and has no default is an `Option`, `None` when absent.
pub fn dictionary(dictionary: &Dictionary, all: &[Dictionary]) -> String {
    let name = &dictionary.name;
    let members = dictionary_members(dictionary, all);
    let mut fields = String::new();
    let mut body = String::new();
    for member in &members {
        let field = snake_case(&member.name);
        let arg = Argument {
            name: member.name.clone(),
            ty: member.ty.clone(),
            optional: !member.required,
            default: member.default.clone(),
        };
        writeln!(fields, "    pub(super) {field}: {},", arg_type(&arg)).unwrap();
        writeln!(
            body,
            "let v = match object {{ Some(object) => {{ let key = v8_str(scope, {key:?}); \
             object.get(scope, key.into()).ok_or(Error::Pending)? }} None => v8::undefined(scope).into() }};",
            key = member.name
        )
        .unwrap();
        if member.required {
            writeln!(
                body,
                "if v.is_undefined() {{ return Err(Error::Type(\"Failed to read the '{}' property from '{name}': \
                 Required member is undefined.\".into())); }}",
                member.name
            )
            .unwrap();
        }
        body.push_str(&convert_arg(&arg, &format!("member_{}", field.trim_start_matches("r#"))));
    }
    let construct: String = members
        .iter()
        .map(|m| {
            let field = snake_case(&m.name);
            format!("{field}: member_{}, ", field.trim_start_matches("r#"))
        })
        .collect();
    format!(
        "#[derive(Debug)]\n#[allow(dead_code)]\npub(super) struct {name} {{\n{fields}}}\n\
         #[allow(non_snake_case, unused_variables)]\n\
         fn convert_dictionary_{name}(scope: &mut v8::PinScope<'_, '_>, registry: &Rc<NodeRegistry>, \
         value: v8::Local<v8::Value>) -> Fallible<{name}> {{\n\
         let object = if value.is_null_or_undefined() {{ None }} else {{ match v8::Local::<v8::Object>::try_from(value) {{\n\
         Ok(object) => Some(object),\n\
         Err(_) => return Err(Error::Type(\"Failed to convert value to '{name}'.\".into())) }} }};\n\
         {body}Ok({name} {{ {construct}}})\n}}\n"
    )
}
