use std::collections::BTreeMap;

pub fn parse_style_text(css: &str) -> BTreeMap<String, String> {
    parse_inline_declarations(css)
        .into_iter()
        .map(|declaration| (declaration.name, declaration.value))
        .collect()
}

pub(crate) fn parse_inline_declarations(css: &str) -> Vec<crate::css::Declaration> {
    crate::css::stylesheet::parse_declarations_for_style_attribute(css)
}
