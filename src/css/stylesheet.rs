use std::collections::{BTreeMap, HashMap};

use cssparser::{
    parse_important, AtRuleParser, BasicParseErrorKind, CowRcStr, ParseError, Parser,
    ParserInput, ParserState, QualifiedRuleParser, RuleBodyParser, StyleSheetParser, ToCss,
};
use selectors::parser::{ParseRelative, SelectorList};

use crate::dom::NodePtr;

use super::dom_styles::collect_styles;
use super::selectors_impl::{
    element_matches, AurSelectorParser, AuroraSelectorImpl,
};
use super::{Declaration, ElementData, Rule, Selector, StyleMap};

// ─── Stylesheet ───────────────────────────────────────────────────────────────

pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub variables: BTreeMap<String, String>,
    /// Bucket index: maps "#id", ".class", "tag", or "*" → rule indices.
    index: HashMap<String, Vec<usize>>,
}

impl Stylesheet {
    pub fn merge(&mut self, other: Stylesheet) {
        let offset = self.rules.len();
        self.rules.extend(other.rules);
        self.variables.extend(other.variables);
        for (key, indices) in other.index {
            self.index
                .entry(key)
                .or_default()
                .extend(indices.into_iter().map(|i| i + offset));
        }
    }

    pub fn user_agent_stylesheet() -> Self {
        Self::parse(
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
             head, style, script, link, meta, title, noscript, template { display: none; } \
             a, abbr, acronym, b, bdo, big, br, button, cite, code, dfn, em, i, img, \
             input, kbd, label, map, object, q, s, samp, select, small, span, strong, \
             sub, sup, textarea, time, tt, u, var, video { display: inline; } \
             b, strong { font-weight: bold; } \
             i, em, cite, dfn, var { font-style: italic; } \
             small { font-size: 0.8em; } \
             code, kbd, samp, tt { font-family: monospace; } \
             a { color: #0000ee; text-decoration: underline; } \
             a:visited { color: #551a8b; } \
             :link { color: #0000ee; } \
             sup { vertical-align: super; font-size: 0.75em; } \
             sub { vertical-align: sub; font-size: 0.75em; } \
             img, video { display: inline-block; } \
             input, button, select, textarea { display: inline-block; font-family: inherit; font-size: inherit; } \
             button { cursor: pointer; } \
             textarea { white-space: pre-wrap; } \
             ol { list-style-type: decimal; } \
             ul { list-style-type: disc; } \
             ",
        )
    }

    pub fn parse(source: &str) -> Self {
        do_parse(source, None)
    }

    pub fn from_dom(
        document: &NodePtr,
        base_url: Option<&str>,
        identity: &crate::identity::Identity,
    ) -> Self {
        let mut source = String::new();
        collect_styles(document, base_url, identity, &mut source);
        let fetch_ctx = base_url.map(|b| (b, identity));
        do_parse(&source, fetch_ctx)
    }

    pub fn styles_for(
        &self,
        element: &ElementData,
        ancestors: &[ElementData],
        siblings: &[ElementData],
        sibling_index: usize,
    ) -> StyleMap {
        // Collect candidates from buckets.
        let mut seen = vec![false; self.rules.len()];
        let mut candidate_indices: Vec<usize> = Vec::new();

        let mut add = |key: &str| {
            if let Some(bucket) = self.index.get(key) {
                for &i in bucket {
                    if !seen[i] {
                        seen[i] = true;
                        candidate_indices.push(i);
                    }
                }
            }
        };

        if let Some(id) = element.attributes.get("id") {
            add(&format!("#{id}"));
        }
        if let Some(class) = element.attributes.get("class") {
            for cls in class.split_whitespace() {
                add(&format!(".{cls}"));
            }
        }
        add(&element.tag_name.to_ascii_lowercase());
        add("*");

        let mut matching: Vec<&Rule> = candidate_indices
            .into_iter()
            .map(|i| &self.rules[i])
            .filter(|rule| {
                element_matches(&rule.selector, element, ancestors, siblings, sibling_index)
            })
            .collect();

        matching.sort_by_key(|rule| (rule.selector.specificity(), rule.source_order));

        let mut styles = StyleMap::default();
        apply_declarations(&mut styles, matching, self);
        styles
    }

    pub fn resolve_variables(&self, value: &str) -> String {
        resolve_vars_with_map(value, &self.variables)
    }
}

// ─── StyleSheetParser integration ────────────────────────────────────────────

fn do_parse(source: &str, fetch_ctx: Option<(&str, &crate::identity::Identity)>) -> Stylesheet {
    let mut variables = BTreeMap::new();
    let mut source_order = 0usize;
    let rules = parse_rules(source, fetch_ctx, &mut variables, &mut source_order);
    let index = build_index(&rules);
    Stylesheet { rules, variables, index }
}

/// Parse CSS source into a flat `Vec<Rule>` using cssparser's `StyleSheetParser`.
pub(super) fn parse_rules(
    source: &str,
    fetch_ctx: Option<(&str, &crate::identity::Identity)>,
    variables: &mut BTreeMap<String, String>,
    source_order: &mut usize,
) -> Vec<Rule> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut rule_parser = AuroraStyleParser { fetch_ctx, variables, source_order };
    let sheet_parser = StyleSheetParser::new(&mut parser, &mut rule_parser);
    sheet_parser.filter_map(|r| r.ok()).flatten().collect()
}

/// Returns true when the @media condition describes print-only output.
fn is_print_only(condition: &str) -> bool {
    let c = condition.trim().to_ascii_lowercase();
    c == "print"
        || c == "only print"
        || (c.contains("print") && !c.contains("screen") && !c.contains("all")
            && !c.contains("not print"))
}

// ─── The combined AtRule + QualifiedRule parser ───────────────────────────────

enum AtRulePrelude {
    Media(String),
    Supports,
    Layer,
    Import(String),
}

struct AuroraStyleParser<'a> {
    fetch_ctx: Option<(&'a str, &'a crate::identity::Identity)>,
    variables: &'a mut BTreeMap<String, String>,
    source_order: &'a mut usize,
}

impl<'i> AtRuleParser<'i> for AuroraStyleParser<'_> {
    type Prelude = AtRulePrelude;
    type AtRule = Vec<Rule>;
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<AtRulePrelude, ParseError<'i, ()>> {
        match name.as_ref().to_ascii_lowercase().as_str() {
            "media" => {
                let condition = collect_prelude_as_string(input);
                Ok(AtRulePrelude::Media(condition))
            }
            "supports" => {
                // Drain the prelude.
                while input.next().is_ok() {}
                Ok(AtRulePrelude::Supports)
            }
            "layer" => {
                while input.next().is_ok() {}
                Ok(AtRulePrelude::Layer)
            }
            "import" => {
                let url = match input.next() {
                    Ok(cssparser::Token::QuotedString(s)) => s.to_string(),
                    Ok(cssparser::Token::UnquotedUrl(s)) => s.to_string(),
                    Ok(cssparser::Token::Function(f)) if f.eq_ignore_ascii_case("url") => {
                        input
                            .parse_nested_block(|p| -> Result<String, cssparser::ParseError<'_, ()>> {
                                Ok(p.expect_string_cloned()?.as_ref().to_string())
                            })
                            .unwrap_or_default()
                    }
                    _ => String::new(),
                };
                // Drain the rest of the prelude (media condition after the URL).
                while input.next().is_ok() {}
                Ok(AtRulePrelude::Import(url))
            }
            _ => Err(input.new_error(BasicParseErrorKind::AtRuleInvalid(name))),
        }
    }

    fn rule_without_block(
        &mut self,
        prelude: AtRulePrelude,
        _start: &ParserState,
    ) -> Result<Vec<Rule>, ()> {
        // Only @import ends with `;` (no block).
        if let AtRulePrelude::Import(url) = prelude {
            if url.is_empty() {
                return Ok(vec![]);
            }
            if let Some((base, identity)) = self.fetch_ctx {
                if let Ok(resolved) = crate::fetch::resolve_relative_url(base, &url) {
                    if let Ok(css) = crate::fetch::fetch_string(&resolved, identity) {
                        let rules = parse_rules(
                            &css,
                            Some((&resolved, identity)),
                            self.variables,
                            self.source_order,
                        );
                        return Ok(rules);
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn parse_block<'t>(
        &mut self,
        prelude: AtRulePrelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Vec<Rule>, ParseError<'i, ()>> {
        match prelude {
            AtRulePrelude::Media(ref condition) => {
                if is_print_only(condition) {
                    while input.next().is_ok() {}
                    return Ok(vec![]);
                }
                Ok(parse_nested_block(input, self.fetch_ctx, self.variables, self.source_order))
            }
            AtRulePrelude::Supports | AtRulePrelude::Layer => {
                Ok(parse_nested_block(input, self.fetch_ctx, self.variables, self.source_order))
            }
            _ => {
                while input.next().is_ok() {}
                Ok(vec![])
            }
        }
    }
}

impl<'i> QualifiedRuleParser<'i> for AuroraStyleParser<'_> {
    type Prelude = SelectorList<AuroraSelectorImpl>;
    type QualifiedRule = Vec<Rule>;
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<SelectorList<AuroraSelectorImpl>, ParseError<'i, ()>> {
        SelectorList::parse(&AurSelectorParser, input, ParseRelative::No)
            .map_err(|_| input.new_error(BasicParseErrorKind::QualifiedRuleInvalid))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: SelectorList<AuroraSelectorImpl>,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Vec<Rule>, ParseError<'i, ()>> {
        let declarations = parse_declaration_block(input, self.variables);
        if declarations.is_empty() {
            return Ok(vec![]);
        }

        let rules: Vec<Rule> = prelude
            .slice()
            .iter()
            .map(|selector| {
                let order = *self.source_order;
                *self.source_order += 1;
                Rule { selector: selector.clone(), declarations: declarations.clone(), source_order: order }
            })
            .collect();

        Ok(rules)
    }
}

// ─── Declaration parsing ──────────────────────────────────────────────────────

fn parse_declaration_block<'i>(
    input: &mut Parser<'i, '_>,
    variables: &mut BTreeMap<String, String>,
) -> Vec<Declaration> {
    let mut decl_parser = AuroraDeclarationParser { variables };
    RuleBodyParser::new(input, &mut decl_parser)
        .filter_map(Result::ok)
        .collect()
}

struct AuroraDeclarationParser<'a> {
    variables: &'a mut BTreeMap<String, String>,
}

impl<'i> cssparser::DeclarationParser<'i> for AuroraDeclarationParser<'_> {
    type Declaration = Declaration;
    type Error = ();

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _state: &ParserState,
    ) -> Result<Declaration, ParseError<'i, ()>> {
        let name = name.to_ascii_lowercase();
        let mut value = input
            .parse_until_before(cssparser::Delimiter::Bang, |input| {
                let mut value = String::new();
                while let Ok(token) = input.next_including_whitespace_and_comments() {
                    value.push_str(&token.to_css_string());
                }
                Ok::<_, ParseError<'i, ()>>(value)
            })?
            .trim()
            .to_string();
        let important = input.try_parse(parse_important).is_ok();
        value = value.trim().to_string();

        if name.starts_with("--") {
            self.variables.insert(name.clone(), value.clone());
        }

        Ok(Declaration { name: name.to_string(), value, important })
    }
}

impl<'i> cssparser::AtRuleParser<'i> for AuroraDeclarationParser<'_> {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = ();
}

impl<'i> cssparser::QualifiedRuleParser<'i> for AuroraDeclarationParser<'_> {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = ();
}

impl<'i> cssparser::RuleBodyItemParser<'i, Declaration, ()> for AuroraDeclarationParser<'_> {
    fn parse_declarations(&self) -> bool { true }
    fn parse_qualified(&self) -> bool { false }
}

// ─── Variable resolution ──────────────────────────────────────────────────────

fn resolve_vars_with_map(value: &str, vars: &BTreeMap<String, String>) -> String {
    if !value.contains("var(") {
        return value.to_string();
    }
    let mut result = String::new();
    let mut i = 0;
    while i < value.len() {
        if value[i..].starts_with("var(") {
            let content_start = i + 4;
            let (end, inner) = find_matching_close(value, content_start);
            let (var_name, fallback) = split_var_args(inner);
            let var_name = var_name.trim();
            if let Some(val) = vars.get(var_name) {
                let resolved = resolve_vars_with_map(val, vars);
                result.push_str(&resolved);
            } else if let Some(fb) = fallback {
                let resolved = resolve_vars_with_map(fb.trim(), vars);
                result.push_str(&resolved);
            } else {
                result.push_str(&value[i..=end]);
            }
            i = end + 1;
        } else {
            let ch = value[i..].chars().next().unwrap_or('\0');
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

fn find_matching_close(s: &str, start: usize) -> (usize, &str) {
    let mut depth = 1usize;
    let mut i = start;
    for ch in s[start..].chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (i, &s[start..i]);
                }
            }
            _ => {}
        }
        i += ch.len_utf8();
    }
    (s.len().saturating_sub(1), &s[start..])
}

fn split_var_args(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

// ─── apply_declarations ───────────────────────────────────────────────────────

fn apply_declarations(styles: &mut StyleMap, rules: Vec<&Rule>, stylesheet: &Stylesheet) {
    let mut normal = Vec::new();
    let mut important = Vec::new();
    for rule in rules {
        for decl in &rule.declarations {
            if decl.important { important.push(decl); } else { normal.push(decl); }
        }
    }
    for decl in normal.into_iter().chain(important) {
        styles.0.insert(
            decl.name.clone(),
            stylesheet.resolve_variables(&decl.value),
        );
    }
}

// ─── Bucket index ─────────────────────────────────────────────────────────────

fn build_index(rules: &[Rule]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rule) in rules.iter().enumerate() {
        let key = bucket_key_for(&rule.selector);
        index.entry(key).or_default().push(i);
    }
    index
}

fn bucket_key_for(selector: &Selector) -> String {
    use selectors::parser::Component;
    for component in selector.iter() {
        match component {
            Component::ID(id) => return format!("#{}", id.0),
            Component::Class(cls) => return format!(".{}", cls.0),
            Component::LocalName(ln) => return ln.name.0.to_ascii_lowercase(),
            Component::Combinator(_) => break,
            _ => {}
        }
    }
    "*".to_string()
}

// ─── Helpers for collecting unparsed CSS text ─────────────────────────────────

/// Drain remaining tokens in a prelude parser into a raw string.
fn collect_prelude_as_string<'i>(input: &mut Parser<'i, '_>) -> String {
    let mut s = String::new();
    while let Ok(token) = input.next_including_whitespace_and_comments() {
        s.push_str(&token.to_css_string());
    }
    s
}

/// Parse a CSS block (e.g. @media body) directly as nested rules without string roundtrip.
fn parse_nested_block<'i, 't>(
    input: &mut Parser<'i, 't>,
    fetch_ctx: Option<(&str, &crate::identity::Identity)>,
    variables: &mut BTreeMap<String, String>,
    source_order: &mut usize,
) -> Vec<Rule> {
    let mut nested = AuroraStyleParser { fetch_ctx, variables, source_order };
    StyleSheetParser::new(input, &mut nested)
        .filter_map(|r| r.ok())
        .flatten()
        .collect()
}

/// Exposed for headless/inline use — parses declarations from a style attribute.
pub(crate) fn parse_declarations_for_style_attribute(source: &str) -> Vec<Declaration> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut variables = BTreeMap::new();
    let mut decl_parser = AuroraDeclarationParser { variables: &mut variables };
    RuleBodyParser::new(&mut parser, &mut decl_parser)
        .filter_map(Result::ok)
        .collect()
}
