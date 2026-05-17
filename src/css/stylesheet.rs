use std::collections::BTreeMap;

use cssparser::{parse_important, Parser, ParserInput, RuleBodyParser, ToCss};

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
            // Block-level elements
            "html, body, div, p, section, article, aside, nav, header, footer, main, \
             figure, figcaption, blockquote, form, fieldset, details, summary, dialog { display: block; } \
             h1, h2, h3, h4, h5, h6 { display: block; font-weight: bold; } \
             h1 { font-size: 2em; margin-top: 0.67em; margin-bottom: 0.67em; } \
             h2 { font-size: 1.5em; margin-top: 0.83em; margin-bottom: 0.83em; } \
             h3 { font-size: 1.17em; margin-top: 1em; margin-bottom: 1em; } \
             h4 { font-size: 1em; margin-top: 1.33em; margin-bottom: 1.33em; } \
             h5 { font-size: 0.83em; margin-top: 1.67em; margin-bottom: 1.67em; } \
             h6 { font-size: 0.67em; margin-top: 2.33em; margin-bottom: 2.33em; } \
             ul, ol { display: block; padding-left: 40px; margin-top: 1em; margin-bottom: 1em; } \
             li { display: list-item; } \
             dl { display: block; } \
             dt { display: block; font-weight: bold; } \
             dd { display: block; margin-left: 40px; } \
             pre { display: block; white-space: pre; font-family: monospace; \
                   margin-top: 1em; margin-bottom: 1em; } \
             hr { display: block; margin-top: 0.5em; margin-bottom: 0.5em; } \
             table { display: table; } \
             tr { display: table-row; } \
             td, th { display: table-cell; } \
             thead, tbody, tfoot { display: table-row-group; } \
             col { display: table-column; } \
             colgroup { display: table-column-group; } \
             caption { display: table-caption; } \
             th { font-weight: bold; } \
             \
             head, style, script, link, meta, title, noscript, template { display: none; } \
             \
             a, abbr, acronym, b, bdo, big, br, button, cite, code, dfn, em, i, img, \
             input, kbd, label, map, object, q, s, samp, select, small, span, strong, \
             sub, sup, textarea, time, tt, u, var { display: inline; } \
             \
             b, strong { font-weight: bold; } \
             i, em, cite, dfn, var { font-style: italic; } \
             small { font-size: 0.8em; } \
             code, kbd, samp, tt { font-family: monospace; } \
             \
             a { color: #0000ee; text-decoration: underline; } \
             a:visited { color: #551a8b; } \
             \
             :link { color: #0000ee; } \
             ",
        )
    }

    pub fn parse(source: &str) -> Self {
        Self::do_parse(source, None)
    }

    fn do_parse(source: &str, fetch_ctx: Option<(&str, &opus::domain::Identity)>) -> Self {
        let mut rules = Vec::new();
        let mut variables = BTreeMap::new();
        let stripped = strip_at_rules(source, fetch_ctx, 0);

        for (source_order, (selector_part, declarations_part)) in
            iter_qualified_rules(&stripped).into_iter().enumerate()
        {
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

        apply_declarations(&mut styles, matching_rules, self);
        styles
    }

    #[allow(dead_code)]
    pub(crate) fn inline_styles(&self, declarations: &[Declaration]) -> StyleMap {
        let mut styles = StyleMap::default();
        for declaration in declarations {
            styles.0.insert(
                declaration.name.clone(),
                self.resolve_variables(&declaration.value),
            );
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

pub(crate) fn parse_declarations_for_style_attribute(source: &str) -> Vec<Declaration> {
    parse_declarations("*", source, &mut BTreeMap::new())
}

fn iter_qualified_rules(source: &str) -> Vec<(&str, &str)> {
    let mut rules = Vec::new();
    let mut selector_start = 0;
    let mut block_start = None;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;

    for (index, ch) in source.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if let Some(open_quote) = quote {
            if ch == open_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }

        match ch {
            '{' => {
                if depth == 0 {
                    block_start = Some(index);
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = block_start.take() {
                        let selector = source[selector_start..start].trim();
                        let body = source[start + 1..index].trim();
                        if !selector.is_empty() && !body.is_empty() {
                            rules.push((selector, body));
                        }
                    }
                    selector_start = index + ch.len_utf8();
                }
            }
            _ => {}
        }
    }

    rules
}

fn parse_declarations(
    selector_part: &str,
    declarations_part: &str,
    variables: &mut BTreeMap<String, String>,
) -> Vec<Declaration> {
    let mut input = ParserInput::new(declarations_part);
    let mut parser = Parser::new(&mut input);
    let mut declaration_parser = AuroraDeclarationParser {
        selector_part,
        variables,
    };

    RuleBodyParser::new(&mut parser, &mut declaration_parser)
        .filter_map(Result::ok)
        .collect()
}

fn apply_declarations(styles: &mut StyleMap, rules: Vec<&Rule>, stylesheet: &Stylesheet) {
    let mut normal = Vec::new();
    let mut important = Vec::new();
    for rule in rules {
        for declaration in &rule.declarations {
            if declaration.important {
                important.push(declaration);
            } else {
                normal.push(declaration);
            }
        }
    }

    for declaration in normal.into_iter().chain(important) {
        styles.0.insert(
            declaration.name.clone(),
            stylesheet.resolve_variables(&declaration.value),
        );
    }
}

struct AuroraDeclarationParser<'a, 'b> {
    selector_part: &'a str,
    variables: &'b mut BTreeMap<String, String>,
}

impl<'i> cssparser::DeclarationParser<'i> for AuroraDeclarationParser<'_, '_> {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value<'t>(
        &mut self,
        name: cssparser::CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _state: &cssparser::ParserState,
    ) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> {
        let name = name.to_ascii_lowercase();
        let mut value = input
            .parse_until_before(cssparser::Delimiter::Bang, |input| {
                let mut value = String::new();
                while let Ok(token) = input.next_including_whitespace_and_comments() {
                    value.push_str(&token.to_css_string());
                }
                Ok::<_, cssparser::ParseError<'i, Self::Error>>(value)
            })?
            .trim()
            .to_string();
        let important = input.try_parse(parse_important).is_ok();
        value = value.trim().to_string();
        if name.starts_with("--") && matches!(self.selector_part, ":root" | "*" | "html") {
            self.variables.insert(name.clone(), value.clone());
        }
        Ok(Declaration {
            name: name.to_string(),
            value,
            important,
        })
    }
}

impl<'i> cssparser::AtRuleParser<'i> for AuroraDeclarationParser<'_, '_> {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = ();
}

impl<'i> cssparser::QualifiedRuleParser<'i> for AuroraDeclarationParser<'_, '_> {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = ();
}

impl<'i> cssparser::RuleBodyItemParser<'i, Declaration, ()> for AuroraDeclarationParser<'_, '_> {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}
