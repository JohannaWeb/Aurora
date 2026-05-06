use std::collections::BTreeMap;

use crate::dom::NodePtr;

use super::at_rules::strip_at_rules;
use super::dom_styles::collect_styles;
use super::variables::{find_var_content, parse_var_content};
use super::{Declaration, ElementData, Rule, Selector, StyleMap};

pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub variables: BTreeMap<String, String>,
}

impl Stylesheet {
    pub fn merge(&mut self, other: Stylesheet) {
        self.rules.extend(other.rules);
        self.variables.extend(other.variables);
    }

    pub fn user_agent_stylesheet() -> Self {
        Self::parse(
            "a, abbr, b, bdo, big, br, cite, code, dfn, em, i, img, input, kbd, label, map, object, output, q, samp, select, small, span, strong, sub, sup, textarea, time, tt, var { display: inline; } \
             b, strong { font-weight: bold; color: accent; } \
             i, em { font-style: italic; color: rust; } \
             h1 { font-size: 32px; font-weight: bold; text-align: center; color: coal; } \
             h2 { font-size: 24px; font-weight: bold; color: ink; } \
             h3 { font-size: 18px; font-weight: bold; color: ink; } \
             li { display: block; margin: 4px 0; } \
             div, section, article { display: block; } \
             head, style, script, link, meta, title, noscript, template { display: none; }",
        )
    }

    pub fn parse(source: &str) -> Self {
        Self::do_parse(source, None)
    }

    fn do_parse(source: &str, fetch_ctx: Option<(&str, &opus::domain::Identity)>) -> Self {
        let mut rules = Vec::new();
        let mut variables = BTreeMap::new();
        let stripped = strip_at_rules(source, fetch_ctx, 0);

        for (source_order, chunk) in stripped.split('}').enumerate() {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            let Some((selector_part, declarations_part)) = chunk.split_once('{') else {
                continue;
            };
            let selector_part = selector_part.trim();
            let declarations = parse_declarations(selector_part, declarations_part, &mut variables);
            if declarations.is_empty() {
                continue;
            }
            for selector_str in selector_part
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Some(selector) = Selector::parse(selector_str) {
                    rules.push(Rule {
                        selector,
                        declarations: declarations.clone(),
                        source_order,
                    });
                }
            }
        }

        Self { rules, variables }
    }

    pub fn from_dom(
        document: &NodePtr,
        base_url: Option<&str>,
        identity: &opus::domain::Identity,
    ) -> Self {
        let mut source = String::new();
        collect_styles(document, base_url, identity, &mut source);
        let fetch_ctx = base_url.map(|b| (b, identity));
        Self::do_parse(&source, fetch_ctx)
    }

    pub fn styles_for(&self, element: &ElementData, ancestors: &[ElementData]) -> StyleMap {
        let mut styles = StyleMap::default();
        let mut matching_rules = self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, ancestors))
            .collect::<Vec<_>>();
        matching_rules.sort_by_key(|rule| (rule.selector.specificity(), rule.source_order));

        for rule in matching_rules {
            for declaration in &rule.declarations {
                styles.0.insert(
                    declaration.name.clone(),
                    self.resolve_variables(&declaration.value),
                );
            }
        }

        styles
    }

    pub fn resolve_variables(&self, value: &str) -> String {
        let mut result = value.to_string();
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 100;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                break;
            }
            let Some(start) = result.find("var(") else {
                break;
            };
            let Some((end_pos, var_content)) = find_var_content(&result, start) else {
                break;
            };
            let (var_name, fallback) = parse_var_content(var_content);
            let replacement = self.variables.get(&var_name).cloned().or(fallback);
            let Some(replacement) = replacement else {
                break;
            };
            result.replace_range(start..end_pos + 1, &replacement);
        }

        result
    }
}

fn parse_declarations(
    selector_part: &str,
    declarations_part: &str,
    variables: &mut BTreeMap<String, String>,
) -> Vec<Declaration> {
    declarations_part
        .split(';')
        .filter_map(|declaration| {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                return None;
            }
            let (name, value) = declaration.split_once(':')?;
            let name = name.trim().to_string();
            let value = value
                .trim()
                .trim_end_matches("!important")
                .trim()
                .to_string();
            if name.starts_with("--") && matches!(selector_part, ":root" | "*" | "html") {
                variables.insert(name.clone(), value.clone());
            }
            Some(Declaration { name, value })
        })
        .collect()
}
