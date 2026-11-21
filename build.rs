#[path = "codegen/webidl.rs"]
mod webidl;
#[path = "codegen/generate.rs"]
mod generate;

fn main() {
    println!("cargo:rerun-if-changed=idl");
    println!("cargo:rerun-if-changed=codegen");
    let mut paths = std::fs::read_dir("idl")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "idl"))
        .collect::<Vec<_>>();
    paths.sort();
    let (mut definitions, mut includes) = (Vec::new(), Vec::new());
    for path in paths {
        let (found, found_includes) =
            webidl::parse_definitions(&std::fs::read_to_string(&path).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        definitions.extend(found);
        includes.extend(found_includes);
    }
    let mixins: Vec<_> = definitions.iter().filter(|d| d.mixin).cloned().collect();
    let interfaces = webidl::resolve_mixins(definitions, &includes).expect("invalid includes");
    let interfaces = webidl::inheritance_order(interfaces).expect("invalid interface inheritance");
    let mut output = String::from(
        "// Generated from idl/*.idl; do not edit.\n\
        fn install_interfaces(scope: &mut v8::PinScope<'_, '_>, registry: &NodeRegistry) {\n",
    );
    let mut members: String = mixins.iter().map(generate::mixin_trait).collect();
    for (index, interface) in interfaces.iter().enumerate() {
        let parent = interface.parent.as_ref().map_or("None".into(), |parent| {
            let index = interfaces.iter().position(|i| &i.name == parent).unwrap();
            format!("Some(interface_{index})")
        });
        output.push_str(&format!(
            "let interface_{index} = define_interface(scope, registry, {name:?}, {parent}, \
             install_members_{name});\n",
            name = interface.name
        ));
        members.push_str(&generate::members(interface));
        output.push_str(&format!("let _ = interface_{index};\n"));
    }
    output.push_str("}\n");
    output.push_str(&members);
    output.push_str("pub(super) fn parent_interface(name: &str) -> Option<&'static str> { match name {\n");
    for interface in &interfaces {
        if let Some(parent) = &interface.parent {
            output.push_str(&format!("{:?} => Some({parent:?}),\n", interface.name));
        }
    }
    output.push_str("_ => None, } }\n");
    output.push_str("pub(super) fn html_interface(tag: &str) -> &'static str { match tag {\n");
    let mut tags = std::collections::BTreeSet::new();
    for line in std::fs::read_to_string("idl/html-tags.txt")
        .unwrap()
        .lines()
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() || words[0].starts_with('#') {
            continue;
        }
        assert_eq!(words.len(), 2, "expected tag and interface: {line}");
        let (tag, interface) = (words[0], words[1]);
        assert!(tags.insert(tag), "duplicate HTML tag: {tag}");
        assert!(
            interfaces.iter().any(|i| i.name == interface),
            "unknown interface: {interface}"
        );
        output.push_str(&format!("{tag:?} => {interface:?},\n"));
    }
    output
        .push_str("_ if tag.contains('-') => \"HTMLElement\", _ => \"HTMLUnknownElement\", } }\n");
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(path.join("web_interfaces.rs"), output).unwrap();
}
